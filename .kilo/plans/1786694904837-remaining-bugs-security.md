# Remaining Bugs and Security Vulnerabilities

## Summary

All critical and high-severity findings from the original red-team assessment have been fixed. This plan addresses remaining medium/low-severity bugs and security gaps discovered during post-fix audit.

---

## Remaining Issues

### R1 [Critical] FastAPI adapter leaks resource existence via authorizer

**Location**: `src/daf/adapters/fastapi.py:108-112`

`_make_authorizer` calls `repository.get(resource_id)` and raises `NotFoundError` when the resource doesn't exist. `DataAccess.query()` catches `NotFoundError` separately from `AuthorizationError`, so an attacker can distinguish:
- "Resource exists but you're not authorized" → `AuthorizationError` → `"Unauthorized"`
- "Resource doesn't exist" → `NotFoundError` → `"Not found"`

This is a resource-enumeration vulnerability.

**Fix**: The authorizer should NOT check resource existence. It should only check ownership. If the resource doesn't exist, let `_execute_query()` raise `NotFoundError` after the authorization check passes. This ensures both cases return the same `"Unauthorized"` or `"Not found"` envelope without leaking existence.

Change `_make_authorizer` to skip the existence check:
```python
data = await repository.get(resource_id)
# Don't raise NotFoundError here — let query() handle it after auth passes
owner_id = data.get("owner_id") if isinstance(data, dict) else None
if owner_id != user.id:
    raise AuthorizationError(...)
```

Then in `DataAccess.query()`, ensure authorization runs before the repository `get()` so that unauthorized users always get `AuthorizationError` regardless of resource existence. Currently the auth check is already before `_execute_query()`, so this is consistent.

---

### R2 [High] FastAPI GET endpoint ignores query parameters

**Location**: `src/daf/adapters/fastapi.py:137-141`

`_setup_query_route` hardcodes `filters=None, algorithm=None`:
```python
info = QueryInfo(
    resource_id=resource_id,
    filters=None,
    algorithm=None,
)
```

The REST API cannot actually filter or apply algorithms, even though `DataAccess.query()` supports both.

**Fix**: Read `filters` and `algorithm` from query parameters. Use `Request.query_params` to extract them. Parse filters as JSON if provided.

---

### R3 [Medium] `_apply_filters` returns non-dict data when filters are present

**Location**: `src/daf/core/access.py:65-72`

```python
def _apply_filters(self, data: Any, filters: dict[str, Any] | None) -> Any:
    if not filters or not isinstance(data, dict):
        return data
```

If `filters` is provided but `data` is not a dict (e.g., an int from an algorithm), filters are silently ignored and the raw data is returned. This is inconsistent — filters should either be applied or indicate no-match.

**Fix**: When filters are present and data is not a dict, return `{}` to indicate no match (consistent with the "filter didn't match" path).

---

### R4 [Medium] `_cache_key` crashes on non-JSON-serializable filters

**Location**: `src/daf/core/access.py:61`

```python
filters_json = json.dumps(info.filters or {}, sort_keys=True)
```

If `filters` contains non-serializable objects (e.g., datetime, custom classes), `json.dumps` raises `TypeError` and the entire query fails.

**Fix**: Wrap in try/except and fall back to `str(filters)` or raise `ValidationError` with a user-safe message.

---

### R5 [Medium] Missing input validation on all operations

**Locations**:
- `src/daf/core/access.py:74` — `query()` doesn't validate `info.resource_id`
- `src/daf/core/access.py:158` — `post()` only validates non-empty, not structure
- `src/daf/core/access.py:207` — `put()` doesn't validate `info.resource_id` or `info.data`
- `src/daf/core/access.py:265` — `delete()` doesn't validate `info.resource_id`

**Fix**: Add validation guards:
- `resource_id` must be a non-empty string
- `post()` and `put()` `data` must be a dict
- Raise `ValidationError` with user-safe messages on bad input

---

### R6 [Low] `post()` drops `resource_type`

**Location**: `src/daf/core/access.py:199-205`

`PostInfo.resource_type` is validated as non-empty but never stored or returned. The `MutationResult.data` is `{"id": resource_id, **info.data}` without `resource_type`.

**Fix**: Include `resource_type` in the returned data: `{"id": resource_id, "resource_type": info.resource_type, **info.data}`.

---

### R7 [Low] `DataAccessRouter.__init__` reaches into `daf` private state

**Location**: `src/daf/adapters/fastapi.py:71-79`

```python
repository = daf._repository
authorizer = self._make_authorizer(repository)
self._daf = DataAccess(
    repository=repository,
    cache=daf._cache,
    algorithms=getattr(daf, "_algorithms", None),
    authorizer=authorizer,
)
```

This couples the adapter to `DataAccess` internals. If internal attribute names change, the adapter breaks.

**Fix**: Add a public composition method or constructor parameter to `DataAccess` that allows extracting the components, or accept components directly in `DataAccessRouter.__init__`.

---

### R8 [Low] `put_endpoint` mutates validated Pydantic model

**Location**: `src/daf/adapters/fastapi.py:172`

```python
info.resource_id = resource_id
```

Mutating `PutInfo` after FastAPI has validated it is a code smell and could cause confusion if the model is reused.

**Fix**: Construct a new `PutInfo` instance instead of mutating:
```python
info = PutInfo(resource_id=resource_id, data=info.data)
```

---

### R9 [Low] No structured logging

No logging exists anywhere in the codebase. Debugging production issues, auditing access patterns, and tracing errors is impossible without instrumented logs.

**Fix**: Add `logging.getLogger(__name__)` to `DataAccess`, `DataAccessRouter`, `MemoryRepository`, and `MemoryCache`. Log at DEBUG for normal flow, WARNING for recoverable errors, ERROR for failures.

---

## Test Dimension Gaps

The existing tests cover component behavior but lack interaction coverage for:

- **Cache × non-dict data**: What happens when an algorithm returns a non-dict and filters are applied?
- **Cache × non-serializable filters**: What happens when filters contain objects that can't be JSON-serialized?
- **Auth × non-existent resource**: Both authorized and unauthorized users hitting a missing resource should return consistent envelopes (addresses R1)
- **Input validation**: Empty `resource_id`, malformed `data`, missing `resource_type`
- **GET query parameters**: `filters` and `algorithm` passed via query string
- **PUT mutation of request model**: Ensure `PutInfo` is not mutated in-place (addresses R8)

---

## Implementation Order

1. **R1** (Critical) — Fix authorizer existence check
2. **R2** (High) — Wire up GET query parameters
3. **R3** (Medium) — Fix `_apply_filters` edge case
4. **R4** (Medium) — Harden `_cache_key`
5. **R5** (Medium) — Add input validation
6. **R6** (Low) — Preserve `resource_type`
7. **R7** (Low) — Decouple adapter from private state
8. **R8** (Low) — Avoid Pydantic model mutation
9. **R9** (Low) — Add structured logging
10. **Tests** — Fill interaction gaps

---

## Validation

- `pytest` — all 79 tests pass + new interaction tests for R1–R9
- `mypy --strict` — 0 errors
- `ruff check` — 0 errors
- Manual verification: `GET /data/123?filters={"status":"active"}&algorithm=fibonacci` returns filtered/algorithmed result
- Manual verification: probing non-existent resource with/without auth returns same envelope shape
