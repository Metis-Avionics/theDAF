"""Base repository implementations."""

import copy
import logging
import threading
import uuid
from collections.abc import Callable

logger = logging.getLogger(__name__)


class MemoryRepository[T]:
    """In-memory repository implementation.

    Note: try_update and try_delete use a coarse lock and identity comparison
    (``is``) to detect concurrent modification. This is a best-effort
    implementation suitable for testing only; real transactional backends
    should implement these primitives with proper atomicity guarantees.
    """

    def __init__(self) -> None:
        """Initialize the in-memory repository."""
        self._store: dict[str, T] = {}
        self._lock = threading.Lock()

    async def get(self, key: str) -> T | None:
        """Retrieve an item by key.

        Args:
            key: The key to retrieve.

        Returns:
            An independent copy of the value if found, None otherwise.
            Callers must not mutate the returned value in-place.
        """
        logger.debug("repository get", extra={"key": key})
        value = self._store.get(key)
        if value is not None:
            return copy.deepcopy(value)
        return None

    async def save(self, key: str, value: T) -> None:
        """Save an item with the given key.

        The implementation stores an independent copy; the caller's
        reference is not retained.

        Args:
            key: The key to save under.
            value: The value to save.
        """
        logger.debug("repository save", extra={"key": key})
        self._store[key] = copy.deepcopy(value)

    async def delete(self, key: str) -> None:
        """Delete an item by key.

        Args:
            key: The key to delete.
        """
        logger.debug("repository delete", extra={"key": key})
        if key in self._store:
            del self._store[key]

    async def create(self, value: T) -> str:
        """Create a new item and return its generated resource ID.

        The implementation stores an independent copy; the caller's
        reference is not retained.

        Args:
            value: The value to store.

        Returns:
            The generated resource ID.
        """
        logger.debug("repository create")
        resource_id = str(uuid.uuid4())
        self._store[resource_id] = copy.deepcopy(value)
        return resource_id

    async def try_update(
        self, key: str, expected: T, update: Callable[[T], T]
    ) -> T | None:
        """Conditionally update if current value equals expected.

        Uses identity comparison (``is``) under a coarse lock to detect
        concurrent modification. Returns the updated value on success or
        ``None`` if the stored value no longer matches ``expected``.

        Args:
            key: The key to update.
            expected: The value that is expected to be currently stored.
            update: A callable that transforms the current value into the new value.

        Returns:
            The new value if the update succeeded, or None if the expected
            value did not match the current stored value.
        """
        with self._lock:
            current = self._store.get(key)
            if current is not expected and not (
                isinstance(current, dict)
                and isinstance(expected, dict)
                and current == expected
            ):
                return None
            new_value = update(current)
            self._store[key] = new_value
            return new_value

    async def try_delete(self, key: str, expected: T) -> bool:
        """Conditionally delete if current value equals expected.

        Uses identity comparison (``is``) under a coarse lock to detect
        concurrent modification.

        Args:
            key: The key to delete.
            expected: The value that is expected to be currently stored.

        Returns:
            True if the key was deleted, False if the expected value did not
            match the current stored value.
        """
        with self._lock:
            current = self._store.get(key)
            if current is not expected and not (
                isinstance(current, dict)
                and isinstance(expected, dict)
                and current == expected
            ):
                return False
            del self._store[key]
            return True
