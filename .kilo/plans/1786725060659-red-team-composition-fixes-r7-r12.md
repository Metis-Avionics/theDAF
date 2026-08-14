# Red Team Composition Fixes: R7-R12

## Context

PR #16 at `eb6300a` (fix: R1-R6 red-team composition fixes) passes local CI (105 tests, mypy --strict, ruff, power-of-ten). This plan addresses the remaining 6 composition-level findings from the red-team pass on PR #16.

All 105 tests currently pass locally. The red team noted CI was failing at the time of assessment; current HEAD passes all checks.

---

## Design Decisions (locked)

| Finding | Decision |
|---------|----------|
| R7 | `copy.deepcopy()` at all ownership boundaries (get/set in MemoryRepository/MemoryCache); `save()` and `create()` also deepcopy the stored value |
| R8 | Document the authorization-after-read model as a **security model decision**. No code change; the single-read invariant is intentional. |
| R9 | Authorizer receives raw repository data on both cache-miss and cache-hit paths. Cache stores the post-algorithm result; authorization always sees the raw `repository.get()` data. |
| R10 | Remove the no-op `delete_prefix` from `post()`. |
| R11 | Consolidate FastAPI error translation into a single `_handle_daf_error` helper method. |
| R12 | Already resolved — CI passes locally. No code change needed. |

---

## R7: Complete Memory Isolation (Medium severity)

**Files:** `src/daf/repositories/memory.py`, `src/daf/cache/memory.py`, `tests/unit/test_components.py`, `tests/integration/test_security_invariants.py`

**Root cause:** `MemoryRepository.get()` and `MemoryCache.get()` only deepcopy `dict` values. `save()` and `create()` store the caller's original object reference. Non-dict mutable types (list, set, custom objects) and nested structures inside non-dict containers are shared.

**Fix:**
1. In `MemoryRepository`:
   - `get()`: change `if isinstance(value, dict)` → always `copy.deepcopy(value)` when value is not None
   - `save()`: `self._store[key] = copy.deepcopy(value)` 
   - `create()`: `self._store[resource_id] = copy.deepcopy(value)`
2. In `MemoryCache`:
   - `get()`: change `if isinstance(value, dict)` → always `copy.deepcopy(value)` when value is not None
   - `set()`: `self._cache[key] = copy.deepcopy(value)`
3. Update protocol docstrings to reflect the new invariant:
   - `Repository`: "Implementations own their stored values. Callers must not mutate values returned by `get()` in-place."
   - `Cache`: "Implementations own their cached values. Callers must not mutate values returned by `get()` in-place."

**Test changes:**
- Add to `TestMemoryRepository`:
  - `test_get_returns_independent_copy_for_list`: save a list, fetch it, mutate the fetched copy, verify repository unaffected
  - `test_save_does_not_retain_caller_reference`: save a dict, mutate the original, verify repository unaffected
  - `test_create_does_not_retain_caller_reference`: create with a dict, mutate the original, verify repository unaffected
- Add to `TestMemoryCache`:
  - `test_get_returns_independent_copy_for_list`: cache a list, fetch it, mutate the fetched copy, verify cache unaffected
  - `test_set_does_not_retain_caller_reference`: cache a dict, mutate the original, verify cache unaffected
- Update `TestMutableValueIsolation` in `test_security_invariants.py`:
  - `test_repository_returns_independent_copy`: add a list mutation sub-test
  - `test_cache_returns_independent_copy`: add a list mutation sub-test

---

## R8: Document Authorization-After-Read Model (Medium severity)

**Files:** `src/daf/core/access.py`, `README.md` or `docs/`

**Root cause:** Authorization happens after `repository.get()` on cache miss. The authorizer prevents **use** of data, not **retrieval** of data. This is intentional (R2 single-read invariant) but undocumented as a security model decision.

**Fix:**
1. In `DataAccess.__init__()` docstring, document:
   > Security model: authorization is evaluated after data retrieval on cache miss. The authorizer receives the raw repository data and decides whether the caller may use it. The repository is treated as a trusted internal data source; authorization enforces usage policy, not access policy.
2. In `_execute_query()` docstring, document:
   > On cache miss, the raw repository value is retrieved first, then authorization is checked against that value. This preserves the single-read invariant.
3. In `README.md` or a new `docs/security-model.md`, add a section:
   > **Authorization boundary**: DataAccess performs authorization after reading from the repository on cache miss. This means the repository is considered a trusted data source. If your deployment requires authorization-before-read (e.g., for audited data sources or multi-tenant isolation at the storage layer), implement this at the repository level.

No test changes required.

---

## R9: Authorization Representation Consistency (High severity)

**Files:** `src/daf/core/access.py`

**Root cause:** On cache miss, the authorizer receives raw `repository.get()` data (with `owner_id`). On cache hit, the authorizer receives the cached value, which may be the post-algorithm transformation (potentially missing fields like `owner_id`). The authorizer is operating on different data representations depending on cache state.

**Fix:**
1. On cache hit in `_execute_query()`, store the raw repository data alongside the cached result, or pass the raw data to the authorizer instead of the cached data.
2. Preferred approach: **always pass raw repository data to the authorizer**, even on cache hit.
   - This requires either: (a) storing raw data alongside cached result, or (b) re-fetching from repository on cache hit (violates single-read invariant), or (c) storing raw data in the cache entry.
3. **Selected approach (c)**: Cache entry is a tuple/dict with both `raw` and `transformed` data:
   ```python
   # On cache set:
   await self._cache.set(cache_key, {"raw": data, "transformed": transformed_data})
   # On cache hit:
   cached_entry = await self._cache.get(cache_key)
   if cached_entry is not None:
       if self._authorizer is not None:
           await self._check_authorization("query", info.resource_id, user, data=cached_entry["raw"])
       return QueryResult(data=cached_entry["transformed"], ...)
   ```

**Alternative (simpler) approach:** On cache hit, pass `data=None` to the authorizer, which triggers the FastAPI authorizer to do its own `repository.get()` — but this violates the single-read invariant.

**Selected: Store raw+transformed in cache entry.** This preserves the single-read invariant while giving the authorizer consistent data.

**Test changes:**
- Add `test_authorizer_receives_raw_data_on_cache_hit`: create a resource with `owner_id`, query it (populates cache), then query again as a different user. Verify authorization is checked against raw data (with `owner_id`) even on cache hit.
- Add `test_cache_hit_authorization_sees_raw_owner_id`: algorithm that removes `owner_id` from output — verify authorizer still sees `owner_id` in the raw data on cache hit.

---

## R10: Remove No-Op POST Cache Invalidation (Low severity)

**Files:** `src/daf/core/access.py`

**Root cause:** `post()` calls `await self._cache.delete_prefix(f"query:{resource_id}:")` after `create()`, but the resource was just created and has no prior query cache entries.

**Fix:**
1. Remove `await self._cache.delete_prefix(f"query:{resource_id}:")` from `post()` (lines 230-234).
2. Add a comment explaining why: "No query cache entries exist for a newly created resource; invalidation is unnecessary."

**Test changes:**
- `test_post_cache_invalidated`: change assertion from "cache invalidated" to "no prior cache entries exist for new resource" or remove the test entirely.

---

## R11: Consolidate FastAPI Error Translation (Low/Medium severity)

**Files:** `src/daf/adapters/fastapi.py`

**Root cause:** Each of the 4 route handlers (`_setup_query_route`, `_setup_post_route`, `_setup_put_route`, `_setup_delete_route`) repeats the same error translation pattern. `# noqa: C901` is used to suppress complexity warning.

**Fix:**
1. Extract a single `_handle_daf_error` method:
   ```python
   def _handle_daf_error(self, error: DataAccessError, context: dict[str, Any]) -> None:
       if isinstance(error, AuthorizationError):
           raise HTTPException(status_code=403, detail="Forbidden") from None
       if isinstance(error, NotFoundError):
           raise HTTPException(status_code=404, detail="Not found") from None
       logger.error("data access error", extra=context)
       raise HTTPException(status_code=500, detail="Internal server error") from None
   ```
2. Replace all 4 try/except blocks with:
   ```python
   try:
       return await self._daf.query(info, user=current_user)
   except DataAccessError as e:
       self._handle_daf_error(e, {"resource_id": resource_id})
   ```
3. Remove `# noqa: C901` from `_setup_query_route`.
4. Note: `ValidationError` (a subclass of `DataAccessError`) currently maps to 500. If desired, add explicit `except ValidationError: raise HTTPException(422)` — but this is a separate design decision (out of scope for R11 unless the team wants it now).

**Test changes:** None required (behavior is identical).

---

## R12: CI Discrepancy (Low severity)

**Files:** N/A

**Root cause:** Red team assessment noted CI was failing at the time of review. Local validation shows all 105 tests pass, mypy --strict clean, ruff clean.

**Fix:**
- No code changes required. The discrepancy was temporal — CI has been green since commit `eb6300a`.
- Optionally: add a CI status badge to README to make CI state visible to future reviewers.

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/daf/repositories/memory.py` | `get()`: deepcopy all non-None values; `save()`: deepcopy stored value; `create()`: deepcopy stored value |
| `src/daf/cache/memory.py` | `get()`: deepcopy all non-None values; `set()`: deepcopy stored value |
| `src/daf/core/access.py` | R8: document auth-after-read model; R9: cache entry stores `{"raw": ..., "transformed": ...}`; R10: remove no-op `delete_prefix` from `post()` |
| `src/daf/core/protocols.py` | Update `Repository` and `Cache` docstrings for new copy contract |
| `src/daf/adapters/fastapi.py` | R11: extract `_handle_daf_error` helper |
| `tests/unit/test_components.py` | Add list/copy isolation tests for repo and cache |
| `tests/integration/test_security_invariants.py` | R9: add cache-hit authorization raw-data test; R10: update post invalidation test |

---

## Validation

- pytest: ≥105 passing (target: 108–110 with new tests)
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- New tests added: ~5 (R7 list/copy tests, R9 cache-hit auth tests)

---

## Risks

| Risk | Mitigation |
|------|-----------|
| `copy.deepcopy()` on all values may fail for non-picklable objects | Guard with `if value is not None`; deepcopy only on concrete stored values at boundary |
| Cache entry format change (`{"raw": ..., "transformed": ...}`) breaks existing cached data | Cache is in-memory only; no persistent cache format to migrate |
| Authorizer receives `data=None` on cache hit for R9 alternative | Not selected; selected approach always passes raw data |
| Removing `delete_prefix` from `post()` removes future-proofing | Document that invalidation should be added if query-by-new-resource-id becomes a valid use case before any mutation |

---

## Out of Scope (next phase)

- `UserIdentity` protocol replacing `Any` user parameter
- Transactional consistency contract beyond CAS primitives
- Cache stampede protection
- Non-dict authorization bypass formalization (ownership model redesign)
- `ValidationError` → HTTP 422 mapping (separate from R11 consolidation)
- Response model consistency (FastAPI error response format)
