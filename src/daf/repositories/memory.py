"""Base repository implementations."""


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
        return self._store.get(key)

    async def save(self, key: str, value: T) -> None:
        """Save an item with the given key.
        
        Args:
            key: The key to save under.
            value: The value to save.
        """
        self._store[key] = value

    async def delete(self, key: str) -> None:
        """Delete an item by key.
        
        Args:
            key: The key to delete.
        """
        if key in self._store:
            del self._store[key]

    async def list_all(self) -> dict[str, T]:
        """List all items in the repository.
        
        Returns:
            A copy of all items in the repository.
        """
        return dict(self._store)
