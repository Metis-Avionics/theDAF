"""Cache implementations."""

from __future__ import annotations

import builtins
import copy
import logging
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
    """

    def __init__(self) -> None:
        """Initialize the in-memory cache."""
        self._cache: dict[str, Any] = {}
        self._trie = _TrieNode()

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

    async def delete_prefix(self, prefix: str) -> None:
        """Delete all values with keys starting with the given prefix.
        
        Args:
            prefix: The key prefix to match.
        """
        logger.debug("cache delete_prefix", extra={"prefix": prefix})
        for key in self._delete_prefix_impl(prefix):
            self._trie_delete(key)
            del self._cache[key]

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
        keys_to_delete = self._delete_prefix_impl(prefix)
        for key in keys_to_delete:
            self._trie_delete(key)
            del self._cache[key]
        return len(keys_to_delete)

    def _delete_prefix_impl(self, prefix: str) -> builtins.set[str]:
        """Collect keys matching the given prefix for removal.
        
        Args:
            prefix: The key prefix to match.
            
        Returns:
            Set of keys to delete.
        """
        return self._trie_collect(prefix)

    def _trie_insert(self, key: str) -> None:
        node = self._trie
        for ch in key:
            node.children.setdefault(ch, _TrieNode())
            node = node.children[ch]
            node.keys.add(key)

    def _trie_delete(self, key: str) -> None:
        node: _TrieNode | None = self._trie
        for ch in key:
            node = node.children.get(ch) if node is not None else None
            if node is None:
                return
            node.keys.discard(key)

    def _trie_collect(self, prefix: str) -> builtins.set[str]:
        node: _TrieNode | None = self._trie
        for ch in prefix:
            node = node.children.get(ch) if node is not None else None
            if node is None:
                return builtins.set()
        return builtins.set(cast(_TrieNode, node).keys)

    async def clear(self) -> None:
        """Clear all values from cache."""
        logger.debug("cache clear")
        self._cache.clear()
        self._trie = _TrieNode()

    async def has(self, key: str) -> bool:
        """Check if a key exists in cache.
        
        Args:
            key: The cache key.
            
        Returns:
            True if the key exists, False otherwise.
        """
        logger.debug("cache has", extra={"key": key})
        return key in self._cache
