# Second Red-Team Fixes: Invariant Composition

## Context

PR #15 fixed R1-R9 but introduced second-order defects caused by new invariants
interacting incorrectly. This plan addresses the 4 must-fix issues before merge.

## Verified Baseline

- 87 tests pass locally
- mypy --strict: 0 errors
- ruff: 0 errors
- **CI is red**: `uv sync --frozen --group dev` fails with "Missing workspace member `thedaf`"
  - Lockfile doesn't include project package entry
  - Fix: `uv lock` then commit updated `uv.lock`

## Critical Fixes (must merge blockers)

### S1. Cache invalidation must cover all derived projections

**Problem**: PUT/DELETE delete only `query:{id}:{}:{algo}:{user}` but cache contains
`query:{id}:{filters}:{algo}:{user}` for every filter/algorithm/user combination.

**Fix**:
1. Add `delete_prefix(prefix: str) -> None` to `Cache` protocol in `src/daf/core/protocols.py`
2. Implement in `MemoryCache` (`src/daf/cache/memory.py`) using `str.startswith` scan
3. In `DataAccess.put()` and `DataAccess.delete()`, replace single `cache.delete(cache_key)`
   with `self._cache.delete_prefix(f"query:{info.resource_id}:")`
4. Remove the now-unused `_cache_key()` call from put/delete

### S2. Authorization must be atomic with mutation read

**Problem**: `put()`/`delete()` do two separate repository reads:
- Authorizer reads to check ownership
- Mutation reads again to get data
This is a TOCTOU race between auth state and mutation state.

**Fix**:
1. Add optional `data: Any = None` parameter to `Authorizer.authorize()` protocol method
   (backward-compatible default)
2. In `DataAccess._check_authorization()`, pass `data` through
3. In `DataAccess.put()`:
   - Read resource first
   - Call `_check_authorization("put", id, user, data=existing)`
   - If authorized, mutate the same `existing` data
   - Eliminate the second `repository.get()` call
4. In `DataAccess.delete()`:
   - Read resource first
   - Call `_check_authorization("delete", id, user, data=existing)`
   - If authorized, delete
5. Update FastAPI `_make_authorizer` to use `data` parameter instead of own read
6. Update test `FakeAuthorizer` classes to accept optional `data` parameter

### S3. Unknown algorithms must return validation error

**Problem**: `algorithm=nonexistent` silently returns raw repository data with `success=True`.

**Fix**:
1. In `DataAccess._execute_query()`, after `self._algorithms.get(info.algorithm)` returns None,
   raise `ValidationError("Unknown algorithm: {info.algorithm}")`
2. This flows through existing `except ValidationError` in `query()`
3. Update test in `test_fastapi_adapter.py` to expect `error_type="validation"` for unknown algorithm

### S4. Fix CI reproducibility

**Problem**: `uv sync --frozen --group dev` fails because lockfile is missing project package entry.

**Fix**:
1. Run `uv lock` to regenerate lockfile with project package
2. Commit updated `uv.lock`

## Important Fixes (should fix before merge)

### S5. Make HTTP contract explicit in docs

**Problem**: README shows 403/404 HTTP codes but adapter returns 200 with error envelopes.

**Decision**: Keep the envelope-over-HTTP-200 pattern (it's internally consistent and
used by GraphQL-style APIs). Update README to document actual behavior:

```python
# All responses are HTTP 200 with QueryResult/MutationResult envelopes
# Check result.error_type for: "authorization", "not_found", "validation"
```

Remove the misleading 403/404 example from README.

### S6. Structured logging with `extra` dict

**Problem**: String interpolation loses field structure for log aggregators.

**Fix**: Replace `logger.debug("msg %s", field)` with `logger.debug("msg", extra={...})`
in `DataAccess`, `DataAccessRouter`, `MemoryRepository`, `MemoryCache`.

## Out of Scope (next phase)

- Cache stampede protection (requires new concurrency primitive)
- Transactional consistency contract (requires new protocol methods)
- Repository `create()` failure semantics (requires protocol extension)
- Non-dict authorization bypass formalization (requires ownership model redesign)
- Filter no-match semantics change (would break existing contract)

## Test Changes

- Add tests for prefix-based cache invalidation:
  - PUT with cached filtered projections invalidates all variants
  - DELETE with cached algorithm projections invalidates all variants
- Add tests for atomic auth+read:
  - Verify single repository read for PUT/DELETE with authorizer
  - Verify auth uses data from mutation read, not stale read
- Update unknown algorithm test to expect `error_type="validation"`
- All existing tests must continue passing

## Validation

- pytest: ≥87 passing (add ~5 new tests for S1, S2, S3)
- mypy --strict: 0 errors
- ruff check: 0 errors
- CI: `uv lock` committed, all 3 validation jobs pass

## File Changes

| File | Change |
|------|--------|
| `src/daf/core/protocols.py` | Add `delete_prefix` to Cache, `data` to Authorizer |
| `src/daf/core/access.py` | Atomic auth+read, prefix invalidation, unknown algorithm error |
| `src/daf/adapters/fastapi.py` | Authorizer uses `data` param, no own read |
| `src/daf/cache/memory.py` | Implement `delete_prefix` |
| `tests/integration/test_authorization.py` | Update FakeAuthorizer, add atomic auth tests |
| `tests/integration/test_fastapi_adapter.py` | Fix unknown algorithm test |
| `tests/integration/test_security_invariants.py` | Add prefix invalidation tests |
| `uv.lock` | Regenerate with `uv lock` |
| `README.md` | Fix HTTP semantics documentation |
