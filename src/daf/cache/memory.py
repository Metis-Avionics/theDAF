"""Cache implementations."""

import logging
from typing import Any

logger = logging.getLogger(__name__)


class MemoryCache:
    """In-memory cache implementation."""

    def __init__(self) -> None:
        """Initialize the in-memory cache."""
        self._cache: dict[str, Any] = {}

    async def get(self, key: str) -> Any | None:
        """Retrieve a value from cache.
        
        Args:
            key: The cache key.
            
        Returns:
            The cached value if found, None otherwise.
        """
        logger.debug("cache get", extra={"key": key})
        return self._cache.get(key)

    async def set(self, key: str, value: Any) -> None:
        """Store a value in cache.
        
        Args:
            key: The cache key.
            value: The value to cache.
        """
        logger.debug("cache set", extra={"key": key})
        self._cache[key] = value

    async def delete(self, key: str) -> None:
        """Delete a value from cache.
        
        Args:
            key: The cache key to delete.
        """
        logger.debug("cache delete", extra={"key": key})
        if key in self._cache:
            del self._cache[key]

    async def delete_prefix(self, prefix: str) -> None:
        """Delete all values with keys starting with the given prefix.
        
        Args:
            prefix: The key prefix to match.
        """
        logger.debug("cache delete_prefix", extra={"prefix": prefix})
        keys_to_delete = [key for key in self._cache if key.startswith(prefix)]
        for key in keys_to_delete:
            del self._cache[key]

    async def clear(self) -> None:
        """Clear all values from cache."""
        logger.debug("cache clear")
        self._cache.clear()

    async def has(self, key: str) -> bool:
        """Check if a key exists in cache.
        
        Args:
            key: The cache key.
            
        Returns:
            True if the key exists, False otherwise.
        """
        logger.debug("cache has", extra={"key": key})
        return key in self._cache
