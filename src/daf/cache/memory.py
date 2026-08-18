"""Cache implementations."""

from __future__ import annotations

import builtins
import copy
import heapq
import logging
from collections import OrderedDict
from itertools import count
from typing import Any

from daf.cache._trie import (
    _trie_collect,
    _trie_delete,
    _trie_delete_prefix,
    _trie_insert,
    _TrieNode,
)
from daf.utils._recursion import TreeCollector

logger = logging.getLogger(__name__)

_DFS_COLLECTOR = TreeCollector(
    key_extractor=lambda node: node.key,
    children_extractor=lambda node: node.children.values(),
    strategy="dfs",
)


class MemoryCache:
    """In-memory cache implementation.

    Values must support ``copy.deepcopy()``. Non-deepcopy-able values
    (e.g. open file handles, locks) are not supported.

    ``max_size`` controls the maximum number of entries. ``0`` (the default)
    means unbounded, which is appropriate for development and testing.
    Bounded mode uses LRU eviction: when the cache is at capacity, the
    least-recently-used entry is evicted on the next ``set()``.

    Prefix operations (``shake``, ``delete_prefix``) are
    O(prefix_length + subtree_nodes) where K is the number of matching
    entries, via a terminal-only prefix trie.
    """

    def __init__(self, max_size: int = 0) -> None:
        """Initialize the in-memory cache.

        Args:
            max_size: Maximum number of cached entries. ``0`` disables the
                bound (unbounded cache, backward-compatible default).

        Raises:
            ValueError: If ``max_size`` is negative.
        """
        if max_size < 0:
            raise ValueError("max_size must be non-negative (0 = unbounded)")
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
        assert isinstance(key, str), "key must be a string"
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
        _trie_insert(self._trie, key)

    async def delete(self, key: str) -> None:
        """Delete a value from cache.

        Args:
            key: The cache key to delete.
        """
        logger.debug("cache delete", extra={"key": key})
        if key in self._cache:
            _trie_delete(self._trie, key)
            del self._cache[key]
            if self._max_size > 0:
                self._lru.pop(key, None)

    async def delete_prefix(self, prefix: str) -> None:
        """Delete all values with keys starting with the given prefix.

        Args:
            prefix: The key prefix to match.
        """
        logger.debug("cache delete_prefix", extra={"prefix": prefix})
        keys_to_delete = _trie_delete_prefix(self._trie, prefix)
        for key in keys_to_delete:
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
        keys_to_delete = _trie_delete_prefix(self._trie, prefix)
        for key in keys_to_delete:
            del self._cache[key]
            if self._max_size > 0:
                self._lru.pop(key, None)
        return len(keys_to_delete)

    def _evict_oldest(self) -> None:
        """Evict the least-recently-used entry from the cache."""
        key, _ = self._lru.popitem(last=False)
        _trie_delete(self._trie, key)
        del self._cache[key]

    def _dfs_collect(self, node: _TrieNode | None) -> builtins.set[str]:
        return _DFS_COLLECTOR.collect(node) if node is not None else builtins.set()

    def _bfs_collect(self, node: _TrieNode | None) -> builtins.set[str]:
        """Level-order terminal key collection.

        **Experimental** — no production consumer yet.
        """
        if node is None:
            return builtins.set()
        collector = TreeCollector(
            key_extractor=lambda n: n.key,
            children_extractor=lambda n: n.children.values(),
            strategy="bfs",
        )
        return collector.collect(node)

    def _astar_collect(self, node: _TrieNode | None, target: str) -> builtins.set[str]:
        """Best-first terminal key collection returning keys with longest
        common prefix against ``target``.

        **Experimental** — no production consumer yet.
        """
        if node is None:
            return builtins.set()
        best_keys: builtins.set[str] = builtins.set()
        best_match_len = 0
        heap: list[tuple[int, int, _TrieNode, int, int]] = [(0, 0, node, 0, 0)]
        assert heap, "heap initialized with root"
        counter = count()
        while heap:
            _neg_match, _cnt, current, depth, match_len = heapq.heappop(heap)
            if match_len > 0 and current.key is not None:
                if match_len > best_match_len:
                    best_match_len = match_len
                    best_keys = {current.key}
                elif match_len == best_match_len:
                    best_keys.add(current.key)
            for ch, child in current.children.items():
                child_depth = depth + 1
                if (
                    match_len < len(target)
                    and match_len == depth
                    and ch == target[match_len]
                ):
                    child_match = match_len + 1
                else:
                    child_match = match_len
                heapq.heappush(
                    heap,
                    (-child_match, next(counter), child, child_depth, child_match),
                )
        return best_keys

    def _trie_collect(self, prefix: str) -> builtins.set[str]:
        return _trie_collect(self._trie, prefix)

    def _trie_delete_prefix(self, prefix: str) -> builtins.set[str]:
        """Detach the subtree rooted at ``prefix`` and return its terminal keys.

        Walks to the prefix node, collects all terminal keys via DFS,
        removes the prefix node from its parent's children, and returns the
        collected keys for bulk ``_cache`` cleanup. Complexity is
        O(prefix_length + subtree_nodes) where K is the number of matching entries.
        """
        return _trie_delete_prefix(self._trie, prefix)

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
