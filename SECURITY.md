# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in `daf`, please report it responsibly.

### How to Report

- **Email**: security@example.com (replace with actual maintainer email)
- **GitHub**: Open a private security advisory at https://github.com/RAliane-REBORN/theDAF/security/advisories

### What to Include

1. Description of the vulnerability
2. Steps to reproduce
3. Affected versions
4. Suggested fix (if available)

### Response Timeline

- Acknowledgment: Within 48 hours
- Initial assessment: Within 7 days
- Fix release: Depends on severity
  - Critical: 1-7 days
  - High: 7-30 days
  - Medium: 30-90 days
  - Low: Next scheduled release

## Known Vulnerabilities

Detailed technical findings are documented in [BUGS.md](./BUGS.md). Critical and high-severity issues include:

- ~~**Authorization can be silently disabled**~~ — FIXED. `DataAccessRouter` now requires `get_current_user` and raises `ValueError` if missing.
- ~~**Cache is not authorization-aware**~~ — FIXED. Cache keys now include user context, filters, and algorithm.
- ~~**Filters are dead data**~~ — FIXED. `QueryInfo.filters` is now applied in-memory.
- ~~**Algorithm selection is broken**~~ — FIXED. `DataAccess` now accepts an algorithm registry and looks up algorithms by name.
- ~~**Race-prone ID generation**~~ — FIXED. `Repository.create()` owns identity generation.
- ~~**Destructive cache invalidation**~~ — FIXED. Mutations now use per-resource `cache.delete()`.
- ~~**Exception flattening**~~ — FIXED. Expected errors return typed results with `error_type`. Unexpected errors propagate as exceptions. The FastAPI adapter maps them to HTTP 500 without leaking diagnostics.
- ~~**Resource enumeration via authorizer**~~ — FIXED. `_make_authorizer` no longer raises `NotFoundError`. It only checks ownership for dict resources. Non-existent resources fall through to `_execute_query()`.
- ~~**GET ignores query parameters**~~ — FIXED. `filters` and `algorithm` are now read from query parameters.
- ~~**Filter edge case with non-dict data**~~ — FIXED. Returns `{}` when filters are present but data is not a dict.
- ~~**Non-serializable filters crash cache key**~~ — FIXED. `_cache_key` raises `ValidationError` for non-JSON-serializable filters.
- ~~**Missing input validation**~~ — FIXED. `resource_id`, `data`, and `resource_type` are validated on all operations.
- ~~**POST drops resource_type**~~ — FIXED. `resource_type` is included in `MutationResult.data`.
- ~~**Adapter reaches into private state**~~ — FIXED. Uses `daf.get_components()` to extract dependencies.
- ~~**PUT mutates validated model**~~ — FIXED. Constructs new `PutInfo` instead of mutating in-place.
- ~~**No structured logging**~~ — FIXED. Logging added with structured `extra` dicts across core components.

---

## Security Invariants (Phase 2)

These invariants were added to prevent second-order defects from interacting incorrectly:

- Cache invalidation now covers all derived projections (prefix-based invalidation)
- Authorization is atomic with mutation reads (prevents TOCTOU race)
- Unknown algorithms return validation errors (prevents silent raw data exposure)

## Security Best Practices for Users

### Do Not Expose DAF Without Authorization

The core `DataAccess` layer does not enforce authorization. Always provide an `Authorizer` when exposing DAF through any interface:

```python
from daf.core.access import DataAccess
from daf.core.factory import DataAccessFactory

factory = DataAccessFactory(
    repository=repo,
    cache=cache,
    authorizer=my_authorizer,  # REQUIRED for production
)
daf = factory.create()
```

When using the FastAPI adapter, always provide `get_current_user`:

```python
from daf.adapters.fastapi import DataAccessRouter

router = DataAccessRouter(
    daf=daf,
    get_current_user=get_current_user,  # REQUIRED for production
)
```

### Input Validation

Validate input at the boundary before passing to `DataAccess`:

```python
import re
from daf.contracts.query import QueryInfo

if not re.match(r'^[a-zA-Z0-9_-]+$', resource_id):
    raise ValueError("Invalid resource_id format")

info = QueryInfo(resource_id=resource_id)
```

### Error Handling

Never expose raw exception messages to clients. The framework currently propagates error strings in result envelopes; wrap DAF operations to sanitize errors in production:

```python
from daf.core.errors import DataAccessError

try:
    result = await daf.query(info)
except DataAccessError as e:
    logger.error(f"DataAccess error: {e}")
    raise HTTPException(status_code=500, detail="Internal server error")
```

### Cache Invalidation

Be aware that `POST` performs global cache invalidation. In high-throughput systems, this can cause cache stampedes. Consider implementing per-resource invalidation at the repository or cache layer until this is fixed in the framework.

### Rate Limiting

Use the built-in rate limiting in the FastAPI adapter. Do not disable rate limiting in production.

### Secrets Management

Never store secrets in the repository:

```python
# Bad
repo.save("db_password", "super_secret")

# Good
import os
db_password = os.environ["DB_PASSWORD"]
```

### Dependency Scanning

Regularly scan dependencies for vulnerabilities:

```bash
uv pip list --outdated
pip-audit
```

## Known Security Considerations

### In-Memory Components

`MemoryRepository` and `MemoryCache` are reference implementations for development and testing. They do not provide:

- Persistence across restarts
- Access control
- Encryption at rest
- Audit logging

Do not use them in production with sensitive data.

### Rate Limiting

Rate limiting is implemented at the FastAPI adapter layer only. If you expose `DataAccess` directly (without the adapter), you must implement your own rate limiting.

### Algorithm Execution

Custom `Algorithm` implementations execute arbitrary code. Only use trusted algorithms in production.

### Authorization Model

The built-in authorizer is a simple ownership check (`owner_id == user.id`). It does not support roles, scopes, tenant boundaries, or administrative access. Implement a custom `Authorizer` for production use.

### HTTP Test Client

The test suite uses `httpx2` (the actively maintained successor to `httpx`) for HTTP integration tests. `httpx2` resolves `StarletteDeprecationWarning` seen with older `httpx` versions in combination with Starlette's `TestClient`.

### Trie Traversal Complexity

`MemoryCache._trie_delete_prefix()` and `_trie_collect()` operate in O(prefix_length + K) time where K is the number of matching entries. The internal prefix trie stores keys only at terminal nodes, eliminating the O(N × L) memory amplification present in naive implementations that store key references at every node along each key's path.

### Base SHA Validation

`scripts/graphify_affected.py` validates the base ref exists locally via `git rev-parse --verify` before running `git diff`. This prevents CI from producing false-green results when the base ref is missing or the clone is shallow.

### Graph JSON Schema Validation

`scripts/graphify_affected.py` validates the loaded graph JSON contains a top-level `nodes` list and that each node has `source_file` and `id` fields. Malformed graph output now produces a clear error instead of silently producing "no impacted test files detected".

## Security Updates

Security updates will be released as patch versions (e.g., 0.1.1) and announced via:

- GitHub Security Advisories
- CHANGELOG.md
- GitHub Releases

Subscribe to releases for notifications: https://github.com/RAliane-REBORN/theDAF/releases
