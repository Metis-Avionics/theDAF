"""Memoization primitives.

Provides:
- Memo: key→value cache with iteration and hit tracking (for algorithms).
- ResourceMemo: lazy-init memo under a global lock (for generation locks).
"""

from __future__ import annotations

import asyncio
import logging
import time
from collections.abc import Callable
from typing import Any, TypeVar

logger = logging.getLogger(__name__)

T = TypeVar("T")


class Memo:
    """Key→value cache with iteration and hit tracking.

    Designed to back Algorithm implementations that need memoization
    plus observability. Exposes stats in the shape
    {"iterations": int, "cache_hits": int, "memo_size": int} expected
    by DataAccess._run_algorithm.
    """

    __slots__ = ("_store", "_iterations", "_cache_hits")

    def __init__(self) -> None:
        self._store: dict[Any, Any] = {}
        self._iterations: int = 0
        self._cache_hits: int = 0

    def has(self, key: Any) -> bool:
        return key in self._store

    def get(self, key: Any) -> Any:
        if key in self._store:
            self._cache_hits += 1
            return self._store[key]
        self._iterations += 1
        raise KeyError(key)

    def set(self, key: Any, value: Any) -> None:
        self._store[key] = value

    def clear(self) -> None:
        self._store.clear()
        self._iterations = 0
        self._cache_hits = 0

    def stats(self) -> dict[str, int]:
        return {
            "iterations": self._iterations,
            "cache_hits": self._cache_hits,
            "memo_size": len(self._store),
        }


class ResourceMemo:
    """Lazy-init resource memo under a global lock.

    Encapsulates the pattern: lazily create a value keyed by a derived
    key, cache it, and return the cached value on subsequent calls.
    Creation is serialised by a global lock so two concurrent callers
    for the same key receive the same resource object.

    Used for generation-lock objects in DataAccess.
    """

    def __init__(
        self,
        key_fn: Callable[[Any], str],
        factory: Callable[[Any], T],
        ttl_seconds: float | None = None,
    ) -> None:
        self._key_fn = key_fn
        self._factory = factory
        self._ttl_seconds = ttl_seconds
        self._store: dict[str, T] = {}
        self._timestamps: dict[str, float] = {}
        self._lock = asyncio.Lock()

    def _is_expired(self, key: str) -> bool:
        if self._ttl_seconds is None:
            return False
        age = time.monotonic() - self._timestamps.get(key, 0.0)
        return age > self._ttl_seconds

    async def get(self, key_arg: Any) -> T:
        key = self._key_fn(key_arg)
        async with self._lock:
            if key in self._store and not self._is_expired(key):
                return self._store[key]
            value = self._factory(key_arg)
            self._store[key] = value
            self._timestamps[key] = time.monotonic()
            return value

    async def clear(self) -> None:
        async with self._lock:
            self._store.clear()
            self._timestamps.clear()
