"""Repository implementations and abstractions."""

from daf._barrel import _public
from daf.repositories.memory import MemoryRepository  # noqa: F401

__all__ = _public(
    "MemoryRepository",
)
