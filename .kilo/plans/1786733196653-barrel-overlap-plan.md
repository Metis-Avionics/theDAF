# Barrel Overlap Plan

## Current State

Two barrel files share 7 names in their `__all__` lists:

| Name | `src/daf/__init__.py` | `src/daf/core/__init__.py` |
|---|---|---|
| `DataAccess` | ✓ | ✓ |
| `DataAccessFactory` | ✓ | ✓ |
| `AuthorizationError` | ✓ | ✓ |
| `DataAccessError` | ✓ | ✓ |
| `NotFoundError` | ✓ | ✓ |
| `RepositoryError` | ✓ | ✓ |
| `ValidationError` | ✓ | ✓ |

`src/daf/__init__.py` is a strict subset of `src/daf/core/__init__.py`. The top-level `daf` package is the documented public API (`from daf import DataAccess`); `daf.core` is used internally and in tests.

## Goal

Reduce mechanical duplication and add a safety net so the two barrels stay consistent without merging them.

## Changes

### 1. Add a `_public` helper to each barrel

Replace `__all__ = [...]` with `__all__ = _public(...)` in both barrel files.

`src/daf/__init__.py` and `src/daf/core/__init__.py` each get:

```python
def _public(*names: str) -> list[str]:
    return list(names)
```

This is a zero-cost readability helper. It makes `__all__` definitions one-liners, reduces diff noise when names are reordered, and is a recognizable pattern across the ecosystem.

### 2. Add a barrel-consistency test

Add `tests/unit/test_barrels.py` with two assertions:

- Every name in `daf.__all__` is present in `daf.core.__all__` (subset invariant).
- Every name in `daf.__all__` is actually importable from the `daf` package (import invariant).

This catches the real failure mode: someone adds a new public name to `daf.core` but forgets to expose it in `daf`.

### 3. Document the design intent

Add a one-line comment in `src/daf/__init__.py`:

```python
# daf is a curated public subset of daf.core. When adding a new
# public name, update both this file and daf/core/__init__.py.
```

## What We Are Not Changing

- We are **not** merging the barrels. `daf` and `daf.core` serve different audiences.
- We are **not** auto-deriving `daf.__all__` from `daf.core.__all__`. Explicit is better than implicit for a public API surface.
- We are **not** removing any names from either barrel.

## Validation

```bash
pytest tests/unit/test_barrels.py -q
ruff check src/daf/__init__.py src/daf/core/__init__.py tests/unit/test_barrels.py
mypy src/ --strict
```

## Risks

- The `_public` helper is stylistic. If the team prefers plain `__all__ = [...]`, skip step 1 and keep only the test.
- The subset test will fail if someone intentionally expands `daf.__all__` beyond `daf.core.__all__`. In that case, the test should be updated to reflect the new design decision.
