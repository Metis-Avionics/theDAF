"""Protocol definitions for repository, cache, and algorithm abstractions."""

from collections.abc import Callable
from typing import Any, Protocol


class Repository[T](Protocol):
    """Abstract repository protocol for data access."""

    async def get(self, key: str) -> T | None:
        """Retrieve an item by key. Returns None if not found."""
        ...

    async def save(self, key: str, value: T) -> None:
        """Save an item with the given key."""
        ...

    async def delete(self, key: str) -> None:
        """Delete an item by key."""
        ...

    async def create(self, value: T) -> str:
        """Create a new item and return its generated resource ID."""
        ...

    async def try_update(
        self, key: str, expected: T, update: Callable[[T], T]
    ) -> T | None:
        """Conditionally update if current value equals expected.

        Returns the new value if successful, or None if the current value
        does not match the expected value (e.g. due to concurrent modification).
        """
        ...

    async def try_delete(self, key: str, expected: T) -> bool:
        """Conditionally delete if current value equals expected.

        Returns True if deleted, False if the current value does not match
        the expected value.
        """
        ...


class Cache(Protocol):
    """Abstract cache protocol."""

    async def get(self, key: str) -> Any | None:
        """Retrieve a value from cache. Returns None if not found or expired."""
        ...

    async def set(self, key: str, value: Any) -> None:
        """Store a value in cache."""
        ...

    async def delete(self, key: str) -> None:
        """Delete a value from cache."""
        ...

    async def delete_prefix(self, prefix: str) -> None:
        """Delete all values with keys starting with the given prefix."""
        ...

    async def clear(self) -> None:
        """Clear all values from cache."""
        ...


class Algorithm(Protocol):
    """Abstract algorithm protocol."""

    async def execute(self, input_data: Any) -> Any:
        """Execute the algorithm with the given input."""
        ...

    async def get_stats(self) -> dict[str, Any]:
        """Get execution statistics."""
        ...


class Authorizer(Protocol):
    """Abstract authorizer protocol for access control."""

    async def authorize(
        self,
        operation: str,
        resource_id: str | None,
        user: Any,
        data: Any = None,
    ) -> None:
        """Authorize an operation on a resource for a given user.

        Args:
            operation: The operation being performed.
            resource_id: The resource being accessed, or None for creation.
            user: The authenticated user context.
            data: Optional data of the resource, for atomic authorization decisions.

        Raises:
            AuthorizationError: If the user is not authorized.
            NotFoundError: If the resource does not exist.
        """
        ...
