# AST Tree Shaking and Superedge Collapse

## Goal

Introduce graph-native cache invalidation semantics into theDAF:
- **Superedge collapse**: consolidate mutation invalidation into a single atomic operation.
- **AST tree shaking**: proactively prune stale cache branches after mutations.
- **graphifyy layer**: add graphifyy as a dev dependency and run it in CI with shake/collapse post-processing on `graph.json`.

## Current State

- `src/daf/core/access.py`: mutations call `delete_prefix(...)` then `_advance_generation(...)` as two separate uncorrelated steps.
- `src/daf/cache/memory.py`: `delete_prefix` scans keys linearly and deletes matching entries.
- `pyproject.toml`: version `0.2.0`; no graphifyy dependency.
- CI: `.github/workflows/ci.yml` runs lint, tests, power-of-ten, and build.

## Implementation Steps

### 1. Superedge collapse in `DataAccess` (native)

**File:** `src/daf/core/access.py`

Add a `_superedge_invalidate(resource_id: str)` helper that atomically:
1. Calls `self._cache.delete_prefix(f"query:{namespace}:")`
2. Calls `self._cache.delete_prefix(f"_daf_gen:{namespace}:")`  — ensures generation keys are also swept under the same logical invalidation event
3. Advances the generation counter via `_advance_generation(resource_id)`

Replace the two-step pattern in `_execute_put` and `_execute_delete`:
```python
# Before:
await self._cache.delete_prefix(f"query:{namespace}:")
await self._advance_generation(info.resource_id)

# After:
await self._superedge_invalidate(info.resource_id)
```

This makes the resource-to-all-projections invalidation a single named operation ("superedge collapse").

### 2. AST tree shaking in `MemoryCache` (native)

**File:** `src/daf/cache/memory.py`

Add `async def shake(self, prefix: str) -> int` that:
- Walks all keys under `prefix`
- Returns count of removed keys
- Leaves no partial-branch state (all-or-nothing per prefix)

This gives `DataAccess` the ability to proactively prune stale branches rather than relying solely on lazy generation checks on read.

**Optional optimization:** If `delete_prefix` and `shake` share the same scan logic, factor it into a private `_delete_prefix_impl` to avoid double iteration.

### 3. Expose shake through `Cache` protocol

**File:** `src/daf/core/protocols.py`

Add `shake(self, prefix: str) -> int` to the `Cache` protocol alongside `delete_prefix`.

Update `MemoryCache` implementation (step 2). Other cache backends may leave `shake` as a no-op or raise `NotImplementedError` if they cannot efficiently implement prefix tree traversal.

### 4. Integrate tree shaking into mutation path

**File:** `src/daf/core/access.py`

In `_superedge_invalidate`, after the superedge collapse, optionally call `self._cache.shake(...)` to prune any orphaned sub-branches that `delete_prefix` may have missed due to hash collisions or legacy key formats.

### 5. graphifyy dev dependency and CI step

**Files:** `pyproject.toml`, `.github/workflows/ci.yml`

- Add `graphifyy` to `[project.optional-dependencies] dev` or `[dependency-groups] dev`.
- Add a new CI job `graphify` that runs after build:
  ```bash
  uv run python -m graphify extract . --code-only --no-cluster
  uv run python -m graphify diagnose multigraph --graph graphify-out/graph.json --json
  ```
- The diagnose step reports same-endpoint edge collapse risk; fail CI if `directed_same_endpoint_collapsed_edges` exceeds a configurable threshold (start at 0, relax to project-specific baseline after first run).

### 6. Add graphifyy-generated artifacts to `.gitignore`

**File:** `.gitignore`

Add:
```
graphify-out/
graph.json
```

These are CI/development artifacts and should not be committed.

## Validation

- `pytest tests/ -q` — existing 121 tests must pass; add 2–4 new tests for `shake` and `_superedge_invalidate`.
- `mypy src/ --strict` — clean.
- `ruff check src/ tests/` — clean.
- `python scripts/power_of_ten.py src/` — clean.
- New tests to add:
  - `test_superedge_invalidate_advances_generation_and_clears_prefix`
  - `test_shake_removes_all_keys_under_prefix`
  - `test_shake_returns_count_of_removed_keys`
  - `test_concurrent_mutations_with_superedge_do_not_lose_invalidations`

## Rollout

1. Implement steps 1–4 (native library changes), commit as `feat: superedge collapse and AST tree shaking`.
2. Implement step 5 (graphifyy CI), commit as `chore: add graphifyy dev dependency and CI job`.
3. Bump patch version in `pyproject.toml`, tag, push.

## Risks

- `shake` on `MemoryCache` is O(N) over the entire cache; document that it is intended for bounded in-memory caches only.
- Distributed cache backends implementing `Cache` may not support efficient prefix tree walks; `shake` should be optional/unsupported for those backends.
- graphifyy `extract --code-only` produces different graphs depending on graphifyy version; pin the version in `pyproject.toml`.
