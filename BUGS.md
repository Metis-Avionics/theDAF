# Known Bugs and Structural Defects

> Red-team assessment findings. Issues are ordered by severity.
> Last updated: 2026-08-14

## 🔴 Critical

### 1. ~~Authorization can be silently disabled~~ FIXED

`DataAccessRouter` now requires `get_current_user` at construction time. Missing callback raises `ValueError`. The adapter composes the authorizer from the provided callback rather than mutating `DataAccess._authorizer` after construction.

**Fix:**
- `src/daf/adapters/fastapi.py:48` — `get_current_user` is a required parameter
- `src/daf/adapters/fastapi.py:60` — raises `ValueError` if missing
- `src/daf/adapters/fastapi.py:72` — constructs a new `DataAccess` with the authorizer pre-set

---

### 2. ~~Cache is not authorization-aware~~ FIXED

Cache key now includes canonical representation of all query inputs: `resource_id` + serialized `filters` + `algorithm` + `user_id`. The cache key is built by `DataAccess._cache_key()` and includes the user identity.

**Fix:**
- `src/daf/core/access.py:56` — `_cache_key()` includes `filters`, `algorithm`, and `user_id`
- `src/daf/core/access.py:97` — cache key used in `_execute_query`

---

### 3. ~~Filters are dead data~~ FIXED

`_execute_query()` now applies `info.filters` in-memory against the retrieved resource before returning.

**Fix:**
- `src/daf/core/access.py:64` — `_apply_filters()` method added
- `src/daf/core/access.py:112` — filters applied after repository lookup

---

### 4. ~~Algorithm selection is semantically broken~~ FIXED

`DataAccess` now accepts an `algorithms: dict[str, Algorithm]` registry. `info.algorithm` is used to look up the algorithm by name from the registry. Cache key includes the algorithm name.

**Fix:**
- `src/daf/core/access.py:35` — `algorithms` parameter in `__init__`
- `src/daf/core/access.py:114` — algorithm lookup by name from registry

---

## 🟠 High

### 5. ~~Resource ID generation is race-prone~~ FIXED

`post()` now delegates ID generation to `repository.create(info.data)`, which returns the generated resource ID. `MemoryRepository.create()` uses `uuid.uuid4()`.

**Fix:**
- `src/daf/core/protocols.py:25` — `create(self, value: T) -> str` added to protocol
- `src/daf/core/access.py:139` — `await self._repository.create(info.data)` replaces `list_all()` + manual ID
- `src/daf/repositories/memory.py:37` — `create()` uses UUID

---
### 6. ~~POST cache invalidation is unnecessarily destructive~~ FIXED

`post()` now calls `cache.delete(cache_key)` for the specific resource instead of `cache.clear()`.

**Fix:**
- `src/daf/core/access.py:145` — per-resource `cache.delete(cache_key)` replaces `cache.clear()`

---

### 7. ~~Exceptions are flattened into opaque error strings~~ FIXED

`DataAccess` no longer catches broad `Exception`. Expected errors (`NotFoundError`, `ValidationError`, `AuthorizationError`) are returned as typed `QueryResult`/`MutationResult` with `error_type` preserved. Unexpected errors propagate as exceptions.

**Fix:**
- `src/daf/contracts/query.py` — `error_type: str | None` added to `QueryResult` and `MutationResult`
- `src/daf/core/access.py:69` — `query()` catches only `AuthorizationError`, `NotFoundError`, `ValidationError`
- `src/daf/core/access.py:123` — `post()` catches only `ValidationError`, `AuthorizationError`

---

### 8. ~~Error messages can leak internal information~~ FIXED

`DataAccess` returns sanitized error messages (`"Not found"`, `"Validation error"`, `"Unauthorized"`). Raw `str(error)` is no longer included in external responses. The FastAPI adapter catches `DataAccessError` subclasses and maps to HTTP 500 with `"Internal server error"`.

**Fix:**
- `src/daf/core/access.py:76` — error messages are user-safe strings
- `src/daf/adapters/fastapi.py:144` — `HTTPException(status_code=500, detail="Internal server error")`

---

### 9. ~~FastAPI adapter mutates private implementation detail~~ FIXED

`DataAccessRouter` no longer assigns to `self._daf._authorizer`. Instead, it constructs a new `DataAccess` instance with the authorizer pre-set at construction time.

**Fix:**
- `src/daf/adapters/fastapi.py:72` — `DataAccessRouter.__init__` builds authorizer and constructs new `DataAccess`

---

## 🟡 Medium

### 10. `datetime.utcnow()` deprecated territory FIXED

`QueryResult` and `MutationResult` now use `datetime.now(UTC)` instead of `datetime.utcnow()`.

**Fix:**
- `src/daf/contracts/query.py:98` — `default_factory=lambda: datetime.now(UTC)`

---

### 11. Repository abstraction forces `list_all()` FIXED

`list_all()` has been removed from the `Repository` protocol and replaced with `create(self, value: T) -> str`. The repository owns identity generation.

**Fix:**
- `src/daf/core/protocols.py:25` — `create()` added, `list_all()` removed
- `src/daf/repositories/memory.py:37` — `create()` uses UUID

---

### 12. Query cache now includes query semantics

The cache key is now a canonical representation of all inputs: `resource_id` + serialized `filters` + `algorithm` + `user_id`.

**Fix:**
- `src/daf/core/access.py:56` — `_cache_key()` builds canonical key from all query semantics

---

### 13. POST uses per-resource cache invalidation

`post()` now calls `cache.delete(cache_key)` for the specific resource instead of `cache.clear()`.

**Fix:**
- `src/daf/core/access.py:145` — per-resource invalidation

---

### 14. Authorizer leaks resource existence via enumeration FIXED

`_make_authorizer` no longer calls `repository.get()` and raises `NotFoundError`. Instead, it only checks ownership for existing dict resources. Non-existent resources fall through to `_execute_query()`, which raises `NotFoundError` after the authorization check. This removes the timing side channel that allowed attackers to distinguish missing resources from forbidden ones.

**Fix:**
- `src/daf/adapters/fastapi.py:108` — authorizer skips existence check, only validates ownership for dict data

---

### 15. GET endpoint ignores query parameters FIXED

`_setup_query_route` now reads `filters` and `algorithm` from `Request.query_params`. Filters are parsed as JSON if provided.

**Fix:**
- `src/daf/adapters/fastapi.py:137` — `filters` and `algorithm` extracted from query parameters

---

### 16. `_apply_filters` returns non-dict data when filters are present FIXED

When filters are provided but data is not a dict, `_apply_filters` now returns `{}` to indicate no match, instead of silently returning the raw data.

**Fix:**
- `src/daf/core/access.py:65` — returns `{}` when filters present and data is not a dict

---

### 17. `_cache_key` crashes on non-JSON-serializable filters FIXED

`_cache_key` now wraps `json.dumps` in try/except and raises `ValidationError` with a user-safe message when filters contain non-serializable objects.

**Fix:**
- `src/daf/core/access.py:58` — catches `TypeError`/`ValueError` and raises `ValidationError`

---

### 18. Missing input validation on operations FIXED

`DataAccess` methods now validate inputs:
- `resource_id` must be a non-empty string
- `post()` and `put()` `data` must be a dict
- Raises `ValidationError` with user-safe messages on bad input

**Fix:**
- `src/daf/core/access.py:84` — `query()` validates `resource_id`
- `src/daf/core/access.py:168` — `post()` validates `resource_type` and `data`
- `src/daf/core/access.py:207` — `put()` validates `resource_id` and `data`
- `src/daf/core/access.py:265` — `delete()` validates `resource_id`

---

### 19. `post()` drops `resource_type` FIXED

`MutationResult.data` now includes `resource_type` in the returned data.

**Fix:**
- `src/daf/core/access.py:199` — `data={"id": resource_id, "resource_type": info.resource_type, **info.data}`

---

### 20. `DataAccessRouter.__init__` reaches into `daf` private state FIXED

`DataAccessRouter` now uses `daf.get_components()` to extract repository, cache, and algorithms, instead of reading `daf._repository`, `daf._cache`, and `daf._algorithms` directly.

**Fix:**
- `src/daf/core/access.py:52` — `get_components()` method added
- `src/daf/adapters/fastapi.py:71` — uses `daf.get_components()`

---

### 21. `put_endpoint` mutates validated Pydantic model FIXED

`_setup_put_route` now constructs a new `PutInfo` instance instead of mutating the validated model in-place.

**Fix:**
- `src/daf/adapters/fastapi.py:172` — `info = PutInfo(resource_id=resource_id, data=info.data)`

---

### 22. ~~No structured logging~~ FIXED

`logging.getLogger(__name__)` added to `DataAccess`, `DataAccessRouter`, `MemoryRepository`, and `MemoryCache`. Logs at DEBUG for normal flow, WARNING for recoverable errors, and ERROR for failures. Structured logging uses `extra={...}` dict instead of string interpolation.

**Fix:**
- `src/daf/core/access.py` — logs query, mutation, cache, and algorithm events with `extra` dict
- `src/daf/adapters/fastapi.py` — logs route handling and errors with `extra` dict
- `src/daf/repositories/memory.py` — logs get/save/delete/create with `extra` dict
- `src/daf/cache/memory.py` — logs get/set/delete/delete_prefix/has/clear with `extra` dict

---

## Phase 2 Fixes (Invariant Composition)

### 23. ~~Cache invalidation does not cover all derived projections~~ FIXED

`put()` and `delete()` previously deleted only a single exact cache key, leaving stale entries for other filter/algorithm/user combinations. Now they use `cache.delete_prefix(f"query:{id}:")` to invalidate all derived projections atomically.

**Fix:**
- `src/daf/core/protocols.py:41` — `delete_prefix(self, prefix: str)` added to `Cache` protocol
- `src/daf/cache/memory.py:48` — `delete_prefix()` implemented with `str.startswith` scan
- `src/daf/core/access.py:305` — `put()` calls `delete_prefix` instead of single `delete`
- `src/daf/core/access.py:388` — `delete()` calls `delete_prefix` instead of single `delete`

---

### 24. ~~Authorization and mutation reads are not atomic~~ FIXED

`put()` and `delete()` previously performed two separate repository reads: one in the authorizer and another to fetch data for mutation. This created a TOCTOU race condition. Now the resource is read once, passed to the authorizer via `data` parameter, and the same object is used for mutation.

**Fix:**
- `src/daf/core/protocols.py:64` — `Authorizer.authorize()` gained optional `data` parameter
- `src/daf/core/access.py:48` — `_check_authorization()` passes `data` through
- `src/daf/core/access.py:264` — `put()` reads resource first, authorizes with data, then mutates
- `src/daf/core/access.py:376` — `delete()` reads resource first, authorizes with data, then deletes
- `src/daf/adapters/fastapi.py:96` — `_make_authorizer` uses `data` when provided, falls back to repository read for queries
- `tests/integration/test_security_invariants.py:403` — atomic auth+read tests added

---

### 25. ~~Unknown algorithm silently returns raw data~~ FIXED

Querying with an algorithm name not present in the registry silently returned raw repository data with `success=True`. Now `_execute_query()` raises `ValidationError("Unknown algorithm: {name}")`, which flows through the existing `except ValidationError` handler and returns `error_type="validation"`.

**Fix:**
- `src/daf/core/access.py:196` — raises `ValidationError` for unknown algorithm names
- `tests/integration/test_fastapi_adapter.py:214` — updated to expect `error_type="validation"`
- `tests/integration/test_security_invariants.py:515` — new `TestUnknownAlgorithmValidation`

---

## Missing Test Dimensions

The existing tests validate component behavior well, but the following interaction dimensions are now covered:

- ✅ Authorization × cache (different users get different cache entries)
- ✅ Algorithm × cache (different algorithms produce different cache keys)
- ✅ Filters × cache (different filters produce different cache keys)
- ✅ Concurrency × POST (ID collision)
- ✅ Cache × non-dict data (filters on non-dict return empty)
- ✅ Cache × non-serializable filters (returns validation error)
- ✅ Auth × non-existent resource (returns not_found for authenticated users)
- ✅ Input validation (empty resource_id, malformed data)
- ✅ GET query parameters (filters and algorithm passed via query string)
- ✅ PUT mutation of request model (new instance instead of in-place mutation)
- ✅ Repository failure × cache
- ✅ Cache failure × mutation
- ✅ Concurrent PUT × query
- ✅ Concurrent DELETE × query
- ✅ Multiple users × same resource
- ✅ Multiple tenants × same resource ID
- ✅ Prefix cache invalidation (PUT/DELETE invalidate all projections)
- ✅ Atomic auth+read (single repository read for mutation operations)
- ✅ Unknown algorithm validation (returns validation error)
- ✅ Stale query interleaving after mutation (stale entry rejected by generation comparison)
- ✅ Concurrent mutation generation monotonicity (per-resource lock serialization)

---

## Core Finding

The repository has a clean conceptual decomposition (Repository / Cache / Algorithm / Authorizer → DataAccess → FastAPI adapter), but the state-space represented by the interfaces is larger than the state-space represented by the cache and orchestration invariants.

Inputs include `resource_id`, `filters`, `algorithm`, and `user`, but the cache only models `resource_id`. Failures include `NotFound`, `Validation`, `Repository`, `Cache`, `Algorithm`, `Authorization`, and unexpected, but query orchestration largely models `success / error string`. Operations are concurrent and async, but the repository contract does not model atomicity or transactions.

The system is modular structurally, but not yet invariant-complete operationally.
