"""Cache implementations."""

from typing import Any


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
        return self._cache.get(key)

    async def set(self, key: str, value: Any) -> None:
        """Store a value in cache.
        
        Args:
            key: The cache key.
            value: The value to cache.
        """
        self._cache[key] = value

    async def delete(self, key: str) -> None:
        """Delete a value from cache.
        
        Args:
            key: The cache key to delete.
        """
        if key in self._cache:
            del self._cache[key]

    async def clear(self) -> None:
        """Clear all values from cache."""
        self._cache.clear()

    async def has(self, key: str) -> bool:
        """Check if a key exists in cache.
        
        Args:
            key: The cache key.
            
        Returns:
            True if the key exists, False otherwise.
        """
        return key in self._cache
