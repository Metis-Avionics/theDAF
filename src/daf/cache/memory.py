"""Cache implementations."""

from __future__ import annotations

import builtins
import copy
import logging
from collections import OrderedDict
from typing import Any, cast

logger = logging.getLogger(__name__)


class _TrieNode:
    __slots__ = ("children", "keys")

    def __init__(self) -> None:
        self.children: dict[str, _TrieNode] = {}
        self.keys: set[str] = set()


class MemoryCache:
    """In-memory cache implementation.

    Values must support ``copy.deepcopy()``. Non-deepcopy-able values
    (e.g. open file handles, locks) are not supported.

    ``max_size`` controls the maximum number of entries. ``0`` (the default)
    means unbounded, which is appropriate for development and testing.
    Bounded mode uses LRU eviction: when the cache is at capacity, the
    least-recently-used entry is evicted on the next ``set()``.
    """

    def __init__(self, max_size: int = 0) -> None:
        """Initialize the in-memory cache.

        Args:
            max_size: Maximum number of cached entries. ``0`` disables the
                bound (unbounded cache, backward-compatible default).
        """
        self._cache: dict[str, Any] = {}
        self._trie = _TrieNode()
        self._max_size = max_size
        self._lru: OrderedDict[str, None] = OrderedDict()

    async def get(self, key: str) -> Any | None:
        """Retrieve a value from cache.

        Args:
            key: The cache key.

        Returns:
            An independent copy of the cached value if found, None otherwise.
            Callers must not mutate the returned value in-place.
        """
        logger.debug("cache get", extra={"key": key})
        value = self._cache.get(key)
        if value is not None:
            if self._max_size > 0:
                self._lru.move_to_end(key)
            return copy.deepcopy(value)
        return None

    async def set(self, key: str, value: Any) -> None:
        """Store a value in cache.

        The implementation stores an independent copy; the caller's
        reference is not retained.

        Args:
            key: The cache key.
            value: The value to cache.
        """
        logger.debug("cache set", extra={"key": key})
        if key in self._cache:
            if self._max_size > 0:
                self._lru.move_to_end(key)
        else:
            if self._max_size > 0 and len(self._cache) >= self._max_size:
                self._evict_oldest()
            if self._max_size > 0:
                self._lru[key] = None
        self._cache[key] = copy.deepcopy(value)
        self._trie_insert(key)

    async def delete(self, key: str) -> None:
        """Delete a value from cache.

        Args:
            key: The cache key to delete.
        """
        logger.debug("cache delete", extra={"key": key})
        if key in self._cache:
            self._trie_delete(key)
            del self._cache[key]
            if self._max_size > 0:
                self._lru.pop(key, None)

    async def delete_prefix(self, prefix: str) -> None:
        """Delete all values with keys starting with the given prefix.

        Args:
            prefix: The key prefix to match.
        """
        logger.debug("cache delete_prefix", extra={"prefix": prefix})
        keys_to_delete = self._trie_delete_prefix(prefix)
        for key in keys_to_delete:
            self._trie_delete(key)
            del self._cache[key]
            if self._max_size > 0:
                self._lru.pop(key, None)

    async def shake(self, prefix: str) -> int:
        """Delete all values with keys starting with the given prefix.

        Returns the count of removed keys. This is the same operation as
        ``delete_prefix`` but returns the number of keys removed, which is
        useful for observability and testing.

        Args:
            prefix: The key prefix to match.

        Returns:
            Number of keys removed.
        """
        logger.debug("cache shake", extra={"prefix": prefix})
        keys_to_delete = self._trie_delete_prefix(prefix)
        for key in keys_to_delete:
            self._trie_delete(key)
            del self._cache[key]
            if self._max_size > 0:
                self._lru.pop(key, None)
        return len(keys_to_delete)

    def _evict_oldest(self) -> None:
        """Evict the least-recently-used entry from the cache."""
        key, _ = self._lru.popitem(last=False)
        self._trie_delete(key)
        del self._cache[key]

    def _trie_insert(self, key: str) -> None:
        self._trie.keys.add(key)
        node = self._trie
        for ch in key:
            node.children.setdefault(ch, _TrieNode())
            node = node.children[ch]
            node.keys.add(key)

    def _trie_delete(self, key: str) -> None:
        self._trie.keys.discard(key)
        path: list[tuple[_TrieNode, str]] = []
        node: _TrieNode | None = self._trie
        for ch in key:
            if node is None:
                return
            path.append((node, ch))
            node = node.children.get(ch)
            if node is None:
                return
            node.keys.discard(key)
        for parent, ch in reversed(path):
            child = parent.children.get(ch)
            if child is not None and not child.keys and not child.children:
                del parent.children[ch]

    def _trie_collect(self, prefix: str) -> builtins.set[str]:
        node: _TrieNode | None = self._trie
        for ch in prefix:
            node = node.children.get(ch) if node is not None else None
            if node is None:
                return builtins.set()
        return builtins.set(cast(_TrieNode, node).keys)

    def _trie_delete_prefix(self, prefix: str) -> builtins.set[str]:
        """Detach the subtree rooted at ``prefix`` and return its terminal keys.

        Walks to the prefix node, collects all keys stored there via DFS,
        removes the prefix node from its parent's children, and returns the
        collected keys for bulk ``_cache`` cleanup.
        """
        if prefix == "":
            keys = builtins.set(self._trie.keys)
            self._trie = _TrieNode()
            return keys
        path: list[tuple[_TrieNode, str]] = []
        node: _TrieNode | None = self._trie
        for ch in prefix:
            if node is None:
                return builtins.set()
            path.append((node, ch))
            node = node.children.get(ch)
            if node is None:
                return builtins.set()
        parent, ch = path[-1]
        keys = builtins.set(cast(_TrieNode, node).keys)
        del parent.children[ch]
        return keys

    async def clear(self) -> None:
        """Clear all values from cache."""
        logger.debug("cache clear")
        self._cache.clear()
        self._trie = _TrieNode()
        self._lru.clear()

    async def has(self, key: str) -> bool:
        """Check if a key exists in cache.

        Args:
            key: The cache key.

        Returns:
            True if the key exists, False otherwise.
        """
        logger.debug("cache has", extra={"key": key})
        return key in self._cache
