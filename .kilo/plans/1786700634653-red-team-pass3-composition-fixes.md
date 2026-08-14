# Red Team Pass #3: Composition-Correctness Fixes

## Context

PR #15 (S1-S6 fixes) is mergeable but not safe to merge. Red-team pass #3 found 5 interaction-level flaws that local-correctness tests do not exercise. This plan addresses all 5.

## Verified Baseline

- 92 tests pass on `fix/remaining-bugs-security`
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- Branch: `fix/remaining-bugs-security`, PR #15 open

---

## Design Decisions (locked)

| Finding | Decision |
|---------|----------|
| R10 — non-dict auth bypass | Fail closed: authorizer denies when `data` is not a dict it can inspect |
| R11 — POST ownership | `post()` passes `data=info.data` to `_check_authorization()`; authorizer decides; DAF stays generic |
| R12 — TOCTOU race | Add `try_update(key, expected, update)` and `try_delete(key, expected)` to `Repository` protocol; `DataAccess.put()`/`delete()` use them |
| R13 — cache-key collision | Replace delimiter string with canonical JSON + SHA-256 hash |
| R14 — stale auth on cache hit | Re-authorize on cache hit: pass `cached` data to `_check_authorization()` before returning |

---

## R10: Fail-Closed Authorization for Non-Dict Resources

**Files:** `src/daf/adapters/fastapi.py`

**Current behavior:** `_make_authorizer` skips ownership check when `data` is not a dict, granting access.

**Fix:** Invert the condition. If `data` is not a dict (or is None after fallback read), raise `AuthorizationError("Cannot establish ownership for resource")`.

```python
# fastapi.py _Authorizer.authorize():
if data is None:
    data = await repository.get(resource_id)
if not isinstance(data, dict):
    raise AuthorizationError("Cannot establish ownership for resource")
owner_id = data.get("owner_id")
if owner_id != user.id:
    raise AuthorizationError(...)
```

---

## R11: POST Passes Proposed Data to Authorizer

**Files:** `src/daf/core/access.py`, `tests/integration/test_authorization.py`

**Current behavior:** `post()` calls `_check_authorization("post", None, user)` — authorizer receives no data and returns immediately.

**Fix:** Pass `data=info.data` so the authorizer can inspect or reject creation payloads. The core `DataAccess` stays generic — it does not inject `owner_id`.

```python
# access.py post():
if self._authorizer is not None:
    await self._check_authorization("post", None, user, data=info.data)
```

**Test changes:**
- Add `test_post_authorizer_rejects_creation` — authorizer that rejects `data` containing certain keys
- Add `test_post_authorizer_receives_data` — verify `authorize()` is called with `data=info.data`

---

## R12: CAS Primitives on Repository Protocol

**Files:** `src/daf/core/protocols.py`, `src/daf/core/access.py`, `src/daf/repositories/memory.py`, `tests/integration/test_authorization.py`, `tests/integration/test_security_invariants.py`

### Protocol changes

Add to `Repository[T]`:

```python
async def try_update(self, key: str, expected: T, update: Callable[[T], T]) -> T | None:
    """Conditionally update if current value equals expected. Returns new value or None."""
    ...

async def try_delete(self, key: str, expected: T) -> bool:
    """Conditionally delete if current value equals expected. Returns True if deleted."""
    ...
```

### DataAccess.put() changes

```python
existing = await self._repository.get(info.resource_id)
if existing is None:
    raise NotFoundError(...)
if self._authorizer is not None:
    await self._check_authorization("put", info.resource_id, user, data=existing)

result = await self._repository.try_update(
    info.resource_id,
    expected=existing,
    update=lambda e: {**e, **info.data},
)
if result is None:
    # State changed between auth and mutation — reject
    return MutationResult(success=False, resource_id=info.resource_id, ...)
```

### DataAccess.delete() changes

```python
existing = await self._repository.get(info.resource_id)
if existing is None:
    raise NotFoundError(...)
if self._authorizer is not None:
    await self._check_authorization("delete", info.resource_id, user, data=existing)

deleted = await self._repository.try_delete(info.resource_id, expected=existing)
if not deleted:
    return MutationResult(success=False, resource_id=info.resource_id, ...)
```

### MemoryRepository implementation

`MemoryRepository` has no real concurrency, so `try_update`/`try_delete` use identity comparison (`is`) under the hood, with a `threading.Lock` for coarse correctness. Document that this is a best-effort implementation.

### Tests

- `test_put_detects_concurrent_modification` — modify repo between auth and mutation, expect failure
- `test_delete_detects_concurrent_modification` — same for delete

---

## R13: Canonical JSON + SHA-256 Cache Keys

**Files:** `src/daf/core/access.py`, `tests/integration/test_security_invariants.py`

**Current behavior:**
```python
return f"query:{info.resource_id}:{filters_json}:{algorithm}:{user_id}"
```

**Fix:**
```python
import hashlib

def _cache_key(self, info: QueryInfo, user: Any) -> str:
    payload = {
        "resource_id": info.resource_id,
        "filters": info.filters or {},
        "algorithm": info.algorithm or "",
        "user_id": self._user_id(user),
    }
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    return f"query:{digest}"
```

**Test changes:**
- Update `TestCacheKeyCanonicalization` — keys are now `query:<hex>`, not colon-delimited
- Add `test_cache_key_no_delimiter_collision` — resource_id containing `:` produces distinct keys

---

## R14: Re-authorize on Cache Hit

**Files:** `src/daf/core/access.py`

**Current behavior:** Cache hit returns data without re-checking authorization.

**Fix:** In `_execute_query`, after `cached is not None`, call `_check_authorization("query", info.resource_id, user, data=cached)` before returning.

```python
cached = await self._cache.get(cache_key)
if cached is not None:
    if self._authorizer is not None:
        await self._check_authorization("query", info.resource_id, user, data=cached)
    logger.debug("cache hit", extra={"key": cache_key})
    return QueryResult(...)
```

**Important:** `_check_authorization` can raise `AuthorizationError` or `NotFoundError`. These are already caught by `query()`'s try/except, so no new error handling is needed. If the cached resource no longer authorizes, the caller gets the appropriate error envelope.

**Test changes:**
- `test_cache_hit_reauthorizes` — cache a result, change ownership, verify query fails

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/daf/core/protocols.py` | Add `try_update`, `try_delete` to `Repository` |
| `src/daf/core/access.py` | Fail-closed auth, POST passes data, CAS put/delete, SHA-256 cache keys, re-auth on cache hit |
| `src/daf/adapters/fastapi.py` | Fail-closed non-dict auth in `_make_authorizer` |
| `src/daf/repositories/memory.py` | Implement `try_update`, `try_delete` with coarse lock |
| `src/daf/cache/memory.py` | No changes |
| `tests/integration/test_authorization.py` | Update FakeAuthorizer, add POST auth tests, CAS mutation tests |
| `tests/integration/test_fastapi_adapter.py` | No changes needed |
| `tests/integration/test_security_invariants.py` | Update cache key tests, add re-auth-on-cache-hit test, add delimiter-collision test |

---

## Validation

- pytest: ≥92 passing (add ~5 new tests)
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors

---

## Risks

| Risk | Mitigation |
|------|-----------|
| `try_update`/`try_delete` are not truly atomic in `MemoryRepository` | Document as best-effort; real repositories (SQL) can implement with transactions |
| SHA-256 keys are opaque in logs/debug | Acceptable — structured `extra={"key": cache_key}` preserves debuggability |
| Re-authorizing on cache hit adds latency | Single dict-type check + optional authorizer call; acceptable for security invariant |
| POST authorizer receiving `data` is a protocol change | `data` has a default of `None`, so existing authorizers are backward-compatible |

---

## Out of Scope (next phase)

- Transactional consistency contract beyond CAS primitives
- Cache stampede protection
- Repository `create()` failure semantics
- Non-dict authorization bypass formalization (ownership model redesign)
