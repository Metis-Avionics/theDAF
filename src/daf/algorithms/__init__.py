"""Algorithm implementations."""

from daf.algorithms.dynamic_programming import FibonacciDP  # noqa: F401


def _public(*names: str) -> list[str]:
    return list(names)


__all__ = _public(
    "FibonacciDP",
)
