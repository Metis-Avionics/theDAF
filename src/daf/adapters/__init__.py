"""FastAPI and other framework adapters."""


def _public(*names: str) -> list[str]:
    return list(names)


__all__ = _public(
    "fastapi",
)
