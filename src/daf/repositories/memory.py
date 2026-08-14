"""Base repository implementations."""

import logging
import uuid

logger = logging.getLogger(__name__)


class MemoryRepository[T]:
    """In-memory repository implementation."""

    def __init__(self) -> None:
        """Initialize the in-memory repository."""
        self._store: dict[str, T] = {}

    async def get(self, key: str) -> T | None:
        """Retrieve an item by key.
        
        Args:
            key: The key to retrieve.
            
        Returns:
            The value if found, None otherwise.
        """
        logger.debug("repository get: key=%s", key)
        return self._store.get(key)

    async def save(self, key: str, value: T) -> None:
        """Save an item with the given key.
        
        Args:
            key: The key to save under.
            value: The value to save.
        """
        logger.debug("repository save: key=%s", key)
        self._store[key] = value

    async def delete(self, key: str) -> None:
        """Delete an item by key.
        
        Args:
            key: The key to delete.
        """
        logger.debug("repository delete: key=%s", key)
        if key in self._store:
            del self._store[key]

    async def create(self, value: T) -> str:
        """Create a new item and return its generated resource ID.
        
        Args:
            value: The value to store.
            
        Returns:
            The generated resource ID.
        """
        logger.debug("repository create")
        resource_id = str(uuid.uuid4())
        self._store[resource_id] = value
        return resource_id
