"""Repository implementations and abstractions."""

from daf.repositories.memory import MemoryRepository  # noqa: F401


def _public(*names: str) -> list[str]:
    return list(names)


__all__ = _public(
    "MemoryRepository",
)
