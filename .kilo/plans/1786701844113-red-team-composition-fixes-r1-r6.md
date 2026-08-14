# Red Team Composition Fixes: R1-R6

## Context

PR #15 at `f6bfac15` passes CI (92 tests, mypy strict, ruff, power-of-ten) but fails red-team review on 6 interaction-level issues. The core problem: error handling has two competing semantic layers (exceptions internally + envelopes externally), and several component interactions don't preserve their documented invariants.

## Verified Baseline

- HEAD: `f6bfac15` (fix: satisfy power-of-ten checks)
- 92 tests pass
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- pytest: pass

---

## Design Decisions (locked)

| Finding | Decision |
|---------|----------|
| R1 — HTTP 403 | Core raises `AuthorizationError`; FastAPI maps to 403. Eliminates envelope layer for errors. |
| R2 — Query read composition | Read resource first, then authorize with data snapshot. Single read per query. |
| R3 — Cache invalidation scope | Use prefix-based keys (`query:{resource_id}:{digest}`) and `delete_prefix()`. Remove `_cache_key_map`. |
| R4 — Mutable values | `MemoryRepository` and `MemoryCache` return deep copies of dict values. Document contract in protocols. |
| R5 — `Any` identity | Document `user.id` contract; deprecate `str(user)` fallback with a warning log. |
| R6 — POST policy | Document that POST authorizer receives `data=info.data` with `resource_id=None`; authorizer decides creation policy. |

---

## R1: AuthorizationError → HTTP 403 (High severity)

**Files:** `src/daf/core/access.py`, `src/daf/adapters/fastapi.py`, `tests/integration/test_fastapi_adapter.py`, `tests/integration/test_security_invariants.py`

**Root cause:** `DataAccess.query()`, `.post()`, `.put()`, `.delete()` catch `AuthorizationError` and return `QueryResult(success=False, error_type="authorization")`. FastAPI's `except DataAccessError` handler maps to 500, but auth errors never reach it.

**Fix:**
1. Remove `_query_auth_error`, `_post_auth_error`, `_mutation_auth_error` helper methods.
2. In `query()`, `post()`, `put()`, `delete()`: remove `except AuthorizationError` blocks. Let `AuthorizationError` propagate.
3. In FastAPI adapter `_setup_query_route`, `_setup_post_route`, `_setup_put_route`, `_setup_delete_route`:
   ```python
   try:
       return await self._daf.query(info, user=current_user)
   except AuthorizationError:
       raise HTTPException(status_code=403, detail="Forbidden") from None
   except NotFoundError:
       raise HTTPException(status_code=404, detail="Not found") from None
   except DataAccessError:
       raise HTTPException(status_code=500, detail="Internal server error") from None
   ```
4. Import `AuthorizationError`, `NotFoundError` in FastAPI adapter.

**Test changes:**
- `test_user_gets_403_for_other_resource`: `assert response.status_code == 403`
- `test_unauthenticated_user_gets_403`: `assert response.status_code == 403`
- `test_nonexistent_resource_returns_404`: `assert response.status_code == 404` (was incorrectly asserting authorization error)
- `test_authorization_error_preserves_type`: change to `with pytest.raises(AuthorizationError)`
- `test_cache_hit_reauthorizes`: change to `with pytest.raises(AuthorizationError)`

---

## R2: Query Authorization/Read Composition (Medium-High severity)

**Files:** `src/daf/core/access.py`, `tests/integration/test_security_invariants.py`

**Root cause:** `query()` pre-authorizes before `_execute_query()` reads from repository. FastAPI authorizer does its own `repository.get()` when `data is None`. Two reads per cache miss; cache hits get pre-authorized without data.

**Fix:**
1. In `query()`, remove the pre-authorization block:
   ```python
   # REMOVE:
   if self._authorizer is not None:
       await self._check_authorization("query", info.resource_id, user)
   ```
2. In `_execute_query()`, add authorization after repository read:
   ```python
   data = await self._repository.get(info.resource_id)
   if data is None:
       raise NotFoundError(f"Resource '{info.resource_id}' not found")
   
   if self._authorizer is not None:
       await self._check_authorization("query", info.resource_id, user, data=data)
   ```
3. For cache hits, authorization already happens with `data=cached` — keep as-is.

**Test changes:**
- `test_put_uses_single_repository_read` / `test_delete_uses_single_repository_read`: add analogous `test_query_uses_single_repository_read` for cache-miss path
- `test_authorized_user_can_query_own_resource`: update to expect `pytest.raises(AuthorizationError)` for unauthorized user (matches R1 change)
- `test_nonexistent_resource_returns_not_found_for_authenticated_user`: update to `pytest.raises(NotFoundError)`

---

## R3: Cache Invalidation Scope (Medium severity)

**Files:** `src/daf/core/access.py`, `tests/integration/test_security_invariants.py`

**Root cause:** `_cache_key_map: dict[str, set[str]] = {}` is local to each `DataAccess` instance. Shared caches across instances invalidate incompletely.

**Fix:**
1. Remove `self._cache_key_map: dict[str, set[str]] = {}` from `__init__`.
2. Remove `_invalidate_cache_for_resource()` method entirely.
3. Change `_cache_key()` format:
   ```python
   return f"query:{info.resource_id}:{digest}"
   ```
4. Replace all `_invalidate_cache_for_resource(resource_id)` calls with:
   ```python
   await self._cache.delete_prefix(f"query:{resource_id}:")
   ```
   Affected: `post()`, `_execute_put()`, `_execute_delete()`.

**Test changes:**
- `test_put_invalidates_all_filtered_projections`: update `_expected_cache_key` to use new format `query:{resource_id}:{digest}`
- `test_delete_invalidates_all_algorithm_projections`: same
- `test_cache_key_includes_all_query_semantics`: same
- `test_cache_key_no_delimiter_collision`: same
- Remove `_cache_key_map` assertions (e.g., checking `len(keys) == 2` before invalidation)

---

## R4: Mutable Repository/Cache Values (Medium severity)

**Files:** `src/daf/repositories/memory.py`, `src/daf/cache/memory.py`, `src/daf/core/protocols.py`

**Root cause:** `MemoryRepository.get()` and `MemoryCache.get()` return direct references. Mutating a returned dict mutates the cache/repository state.

**Fix:**
1. In `MemoryRepository.get()`, return a deep copy for dict values:
   ```python
   import copy
   async def get(self, key: str) -> T | None:
       value = self._store.get(key)
       if isinstance(value, dict):
           return copy.deepcopy(value)
       return value
   ```
2. In `MemoryCache.get()`, return a deep copy for dict values:
   ```python
   async def get(self, key: str) -> Any | None:
       value = self._cache.get(key)
       if isinstance(value, dict):
           return copy.deepcopy(value)
       return value
   ```
3. In `Repository` protocol docstring, add:
   > Implementations should return independent copies or snapshots. Callers must not mutate returned values in-place.
4. In `Cache` protocol docstring, add:
   > Implementations should return independent copies. Callers must not mutate cached values in-place.

**Test changes:**
- Add `test_repository_returns_independent_copy` — mutate returned dict, verify repository unaffected
- Add `test_cache_returns_independent_copy` — mutate cached dict, verify cache unaffected

---

## R5: `Any` as Identity (Low-Medium severity)

**Files:** `src/daf/core/access.py`, `src/daf/core/protocols.py`, `tests/integration/test_security_invariants.py`

**Root cause:** `_user_id(user)` falls back to `str(user)` which has no stability or uniqueness guarantee.

**Fix:**
1. In `_user_id()`, add a deprecation warning for `str(user)` fallback:
   ```python
   import warnings
   def _user_id(self, user: Any) -> str:
       if user is None:
           return "anonymous"
       user_id = getattr(user, "id", None)
       if user_id is None:
           warnings.warn(
               "user object has no .id attribute; falling back to str(user). "
               "This is deprecated and will be removed in a future version. "
               "Pass a user object with a stable .id attribute.",
               DeprecationWarning,
               stacklevel=2,
           )
           return str(user)
       return str(user_id)
   ```
2. In `Authorizer` protocol docstring, document:
   > `user` must have a stable `.id` attribute for cache identity. `str(user)` fallback is deprecated.

**No test changes required** (warning is informational).

---

## R6: POST Authorization Policy (Low severity)

**Files:** `src/daf/core/access.py`, `src/daf/adapters/fastapi.py`, `README.md` or `docs/`

**Root cause:** `post()` passes `resource_id=None` to authorizer. Default FastAPI authorizer returns immediately, making POST always-allowed for authenticated users.

**Fix:**
1. In `access.py` `post()` docstring, document:
   > POST authorization receives `resource_id=None` and `data=info.data`. The authorizer decides creation policy based on the proposed data. Core does not inject `owner_id`.
2. In FastAPI `_make_authorizer`, document:
   > For POST, `resource_id` is None. Authorizers should inspect `data` to enforce creation policies.
3. Add a `docs/authorization.md` or update README with:
   > POST creation policy is authorizer-defined. The default FastAPI authorizer allows all authenticated POSTs. Override `_make_authorizer` to restrict creation.

**No code changes required** — this is a documentation/policy fix.

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/daf/core/access.py` | Remove envelope helpers for auth; let exceptions propagate; prefix cache keys; remove `_cache_key_map`; single-read query auth; deprecation warning in `_user_id` |
| `src/daf/adapters/fastapi.py` | Map `AuthorizationError`→403, `NotFoundError`→404 in all route handlers |
| `src/daf/repositories/memory.py` | Return deep copies for dict values |
| `src/daf/cache/memory.py` | Return deep copies for dict values |
| `src/daf/core/protocols.py` | Document value isolation contract in `Repository` and `Cache` docstrings; document `user.id` contract in `Authorizer` |
| `tests/integration/test_fastapi_adapter.py` | Update auth tests to expect 403; update not-found test to expect 404 |
| `tests/integration/test_security_invariants.py` | Update error-type tests to use `pytest.raises`; update cache key tests for new prefix format; add single-read query test; add mutable-value isolation tests |

---

## Validation

- pytest: ≥92 passing (some tests change assertion style, no test count change expected)
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- New tests added: ~5 (mutable-value isolation, single-read query)

---

## Risks

| Risk | Mitigation |
|------|-----------|
| Core API break: `query()`/`post()`/`put()`/`delete()` now raise instead of returning envelopes | Documented breaking change; envelope pattern was the root cause of R1 |
| `copy.deepcopy()` on non-dict values may fail | Guard with `isinstance(value, dict)` check; non-dict values returned as-is |
| `delete_prefix` matches keys by string prefix | Cache key format is now `query:{resource_id}:{digest}`; resource_id is part of namespace, so prefix deletion is resource-scoped |
| `warnings.warn` may fail in strict environments | Use `warnings` module with `stacklevel`; no runtime failure |

---

## Out of Scope (next phase)

- `UserIdentity` protocol replacing `Any` user parameter
- Transactional consistency contract beyond CAS primitives
- Cache stampede protection
- Non-dict authorization bypass formalization (ownership model redesign)
- Response model consistency (FastAPI error response format)
