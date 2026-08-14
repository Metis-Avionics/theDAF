# IDOR Authorization Plan

## Goal

Prevent Insecure Direct Object Reference (IDOR) violations by enforcing authorization in the `DataAccess` layer, returning HTTP 403 from the FastAPI adapter when a user accesses a resource they do not own.

## Current State

- `DataAccess` performs no authorization checks
- FastAPI adapter passes `resource_id` from URL directly to `DataAccess`
- No concept of "current user" exists anywhere in the codebase
- `MemoryRepository` stores arbitrary `dict[str, T]` with no ownership metadata

## Proposed Design (Option B: Pluggable Authorizer Protocol)

### 1. New `Authorizer` Protocol

Add to `src/daf/core/protocols.py`:

```python
class Authorizer(Protocol):
    async def authorize(self, operation: str, resource_id: str, user: Any) -> None:
        ...
```

- `operation`: `"query"`, `"post"`, `"put"`, or `"delete"`
- `resource_id`: The resource being accessed
- `user`: The authenticated user context (framework-agnostic)
- Raises `AuthorizationError` if access is denied
- Returns `None` if access is allowed

### 2. New `AuthorizationError`

Add to `src/daf/core/errors.py`:

```python
class AuthorizationError(DataAccessError):
    """Raised when a user is not authorized to access a resource."""
    pass
```

### 3. Wire Authorizer into `DataAccess`

Update `src/daf/core/access.py`:

```python
class DataAccess:
    def __init__(
        self,
        repository: Repository[Any],
        cache: Cache,
        algorithm: Algorithm | None = None,
        authorizer: Authorizer | None = None,
    ) -> None:
        ...
        self._authorizer = authorizer

    async def _check_authorization(self, operation: str, resource_id: str, user: Any) -> None:
        if self._authorizer is not None:
            await self._authorizer.authorize(operation, resource_id, user)
```

Call `_check_authorization` at the top of `query`, `post`, `put`, `delete`.

**For `post`**: No `resource_id` yet; authorize with `resource_id=None` or skip auth (creation is typically allowed for any authenticated user).

### 4. FastAPI Adapter Implementation

Update `src/daf/adapters/fastapi.py`:

```python
class FastAPIAuthorizer:
    """FastAPI-specific authorizer that checks resource ownership."""
    
    def __init__(self, daf: DataAccess) -> None:
        self._daf = daf

    async def authorize(self, operation: str, resource_id: str, user: Any) -> None:
        if user is None:
            raise AuthorizationError("Unauthenticated")
        if resource_id is None:
            return  # Allow creation without resource check
        
        data = await self._daf._repository.get(resource_id)
        if data is None:
            raise NotFoundError(f"Resource '{resource_id}' not found")
        
        owner_id = data.get("owner_id") if isinstance(data, dict) else None
        if owner_id != user.id:
            raise AuthorizationError(f"Access denied to resource '{resource_id}'")
```

**Problem**: `FastAPIAuthorizer` needs access to the repository, but `Authorizer.authorize` signature doesn't receive it. Two options:

- **Option A**: Add `repository` parameter to `authorize` (breaks protocol simplicity)
- **Option B**: Have `DataAccessRouter` hold the repository reference and pass it to a closure-based authorizer

**Recommended: Option B** — keep the protocol minimal. The adapter can capture the repository in a closure:

```python
class DataAccessRouter:
    def __init__(self, daf: DataAccess, get_current_user=None) -> None:
        self._daf = daf
        self._get_current_user = get_current_user
        ...
    
    def _make_authorizer(self) -> Authorizer:
        repository = self._daf._repository
        
        async def authorize(operation: str, resource_id: str, user: Any) -> None:
            if user is None:
                raise AuthorizationError("Unauthenticated")
            if resource_id is None:
                return
            data = await repository.get(resource_id)
            if data is None:
                raise NotFoundError(f"Resource '{resource_id}' not found")
            owner_id = data.get("owner_id") if isinstance(data, dict) else None
            if owner_id != user.id:
                raise AuthorizationError(f"Access denied to resource '{resource_id}'")
        
        return authorize
```

### 5. Update `DataAccessFactory`

```python
class DataAccessFactory:
    def __init__(self, ..., authorizer: Authorizer | None = None):
        self._authorizer = authorizer
    
    def create(self) -> DataAccess:
        return DataAccess(
            repository=self._repository,
            cache=self._cache,
            algorithm=self._algorithm,
            authorizer=self._authorizer,
        )
```

### 6. FastAPI Route Changes

Each endpoint must:
1. Extract current user from request (via dependency or direct lookup)
2. Pass user to `DataAccess` operations

Two approaches:
- **A. Add `user` parameter to `DataAccess` methods** (breaks existing API)
- **B. Set `request.state.user` before calling `DataAccess`** (non-breaking)

**Recommended: Option B** — adapter sets `request.state.user` before calling `DataAccess`, and `_check_authorization` reads from there. But this couples `DataAccess` to FastAPI's `request.state`... 

**Better: Option C** — `DataAccess` methods accept an optional `user` parameter defaulting to `None`:

```python
async def query(self, info: QueryInfo, user: Any = None) -> QueryResult:
    if self._authorizer:
        await self._check_authorization("query", info.resource_id, user)
    ...
```

This is backward-compatible and keeps the core framework-agnostic.

### 7. HTTP 403 Translation

In the FastAPI adapter, wrap `DataAccess` calls:

```python
try:
    return await self._daf.query(info, user=current_user)
except AuthorizationError as e:
    raise HTTPException(status_code=403, detail=str(e))
```

### 8. Test Plan

**New test file: `tests/integration/test_authorization.py`**

Scenarios:
1. ✅ User can access their own resource (200)
2. ✅ User gets 403 when accessing another user's resource
3. ✅ Unauthenticated user gets 403
4. ✅ Authorization checked on `query`, `put`, `delete` (not `post`)
5. ✅ `NotFoundError` takes precedence over `AuthorizationError` (resource doesn't exist = 404, not 403)
6. ✅ Works with fake authorizer in unit tests

**Updated test files:**
- `tests/integration/test_data_access.py` — pass `user` to `DataAccess` methods
- `tests/integration/test_fastapi_adapter.py` — add authorization test cases
- `tests/unit/test_components.py` — add `Authorizer` protocol tests

### 9. Migration Path

1. Add `Authorizer` protocol and `AuthorizationError` (non-breaking)
2. Add optional `authorizer` parameter to `DataAccessFactory` and `DataAccess` (non-breaking)
3. Add optional `user` parameter to `DataAccess` methods (non-breaking, defaults to `None`)
4. Update FastAPI adapter to extract user and call authorizer
5. Update tests
6. Update documentation

## Open Questions

**Q1: What represents the "current user"?**

The existing codebase has no `User` model or authentication. Options:
- **A. Simple string `user_id`** — minimal, but loses type safety
- **B. New `User` dataclass** — `User(id: str, ...)` — more extensible
- **C. Accept any object with `.id` attribute** — duck-typing, flexible

**Recommended: A** — keep it minimal with `str` for now. The `Authorizer` protocol accepts `Any`, so users can pass whatever they want.

**Q2: How should the FastAPI adapter extract the current user?**

- **A. Require caller to pass `get_current_user` dependency** — flexible, testable
- **B. Hardcode a simple header-based extractor** — less flexible but works out of the box
- **C. Use FastAPI's `Depends` with a default** — most idiomatic FastAPI

**Recommended: A** — `DataAccessRouter.__init__(self, daf, get_current_user=None)` where `get_current_user` is a callable `Request -> User | None`.

**Q3: Should `post` require authorization?**

- **A. Yes** — only certain user roles can create resources
- **B. No** — creation is open, authorization only on existing resources

**Recommended: B** — creation doesn't target an existing resource, so IDOR doesn't apply. Authorizer receives `resource_id=None` for `post`.

## Validation

```bash
uv run ruff check
uv run mypy src/ --strict
uv run pytest tests/ -q
uv run python scripts/power_of_ten.py
```

## Files Changed

| File | Change |
|------|--------|
| `src/daf/core/protocols.py` | Add `Authorizer` protocol |
| `src/daf/core/errors.py` | Add `AuthorizationError` |
| `src/daf/core/access.py` | Add `authorizer` param, `user` param to methods, `_check_authorization` |
| `src/daf/core/factory.py` | Add `authorizer` to factory |
| `src/daf/adapters/fastapi.py` | Add `FastAPIAuthorizer`, route auth, 403 translation |
| `tests/unit/test_components.py` | Authorizer protocol tests |
| `tests/integration/test_authorization.py` | New: IDOR prevention tests |
| `tests/integration/test_data_access.py` | Update for `user` parameter |
| `tests/integration/test_fastapi_adapter.py` | Add 403 test cases |
| `SESSION.md` | Log authorization work |
| `HANDOVER.md` | Update state |
| `CHANGELOG.md` | Add authorization section |
