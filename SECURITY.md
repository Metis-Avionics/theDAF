# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in `daf`, please report it responsibly.

### How to Report

- **Email**: security@example.com (replace with actual maintainer email)
- **GitHub**: Open a private security advisory at https://github.com/Metis-Avionics/theDAF-LLVM/security/advisories

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
- ~~**Trie memory amplification**~~ — FIXED. Terminal-only trie nodes.
- ~~**Cache downcast panic**~~ — FIXED. Cache entry downcast uses `serde_json::Value` + `.as_object()`.
- ~~**Superedge double-delete panic**~~ — FIXED. Removed redundant `cache.delete(gen_key)`.
- ~~**Trie ancestor removal panic**~~ — FIXED. Off-by-one cleanup in `trie_delete_prefix`.
- ~~**Async blocking in tests**~~ — FIXED. `_trie_collect` made async.

---

## Security Invariants (Phase 2)

These invariants were added to prevent second-order defects from interacting incorrectly:

- Cache invalidation now covers all derived projections (prefix-based invalidation)
- Authorization is atomic with mutation reads (prevents TOCTOU race)
- Unknown algorithms return validation errors (prevents silent raw data exposure)

## Rust Security Considerations

### Memory Safety

- `daf-core` contains no `unsafe` code. All shared state uses `Arc` + immutable borrows or `Mutex`/`RwLock` for interior mutability.
- `daf-cache`’s prefix trie uses `unsafe` raw pointers for path tracking during deletion. This is confined to `trie_delete` and `trie_delete_prefix` and is covered by adversarial invariant tests.
- No `static mut` state. The `daf-ffi` crate uses `OnceLock` / `Mutex` for global error state instead of mutable statics.

### Concurrency

- Per-resource lock striping (`GenerationLocks`) bounds the number of live `tokio::sync::Mutex` allocations via LRU eviction (max 256 by default). This prevents unbounded memory growth from unique resource IDs.
- `MemoryRepository` uses `tokio::sync::RwLock` for concurrent reads and exclusive writes. `try_update` / `try_delete` perform compare-and-swap under the write lock.
- `MemoryCache` updates `_cache`, `_trie`, and `_lru` without `await` between them, preserving the multi-index atomicity invariant.

### Authorization

- The `Authorizer` trait is optional. When absent, `DataAccess` allows all operations. Production deployments must provide an implementor.
- Authorization runs after repository read on cache miss, and re-runs on cache hit with the cached raw data. This prevents stale grants from bypassing revoked access.
- Empty `resource_id` is rejected before authorization, avoiding unnecessary authorizer calls with invalid input.

### Cache Isolation

- Cache keys include `resource_id`, canonical `filters`, `algorithm`, and `user_id`. Changing any of these produces a distinct key, preventing cross-user and cross-projection leakage.
- Generation keys (`_daf_gen:{namespace}`) are stored in the same cache namespace as query entries. Bounded LRU eviction may evict generation metadata, which is handled by treating missing generation as a cache miss.

### FFI Boundaries

- `daf-ffi` exposes `extern "C"` functions with opaque pointers. No Rust panics, `Result`, or `String` cross the boundary. Errors are returned as `i32` error codes.
- `UserId` and `ResourceId` are passed as `const char*` and copied into Rust `String` to avoid lifetime issues across the FFI boundary.

### Algorithm Execution

- Custom `Algorithm` implementations execute arbitrary code. Only use trusted algorithms in production. The `daf-algorithms` crate ships only `FibonacciDP` as a reference implementation.

### Error Handling

- `DataAccessError` is a `thiserror` enum with typed sub-errors. The Rust API returns `Result<T, DataAccessError>`.
- `QueryResult` / `MutationResult` preserve the external `Option<String>` envelope for ABI compatibility with the Python contract and FFI consumers.

## Security Best Practices for Users

### Do Not Expose DAF Without Authorization

The core `DataAccess` layer does not enforce authorization. Always provide an `Authorizer` when exposing DAF through any interface:

```rust
use daf_application::DataAccess;
use daf_core::Authorizer;

let daf = DataAccess::new(
    repo,
    cache,
    algorithms,
    Some(my_authorizer), // REQUIRED for production
);
```

When using the Axum adapter, always provide `get_current_user`:

```rust
use daf_http::DataAccessRouter;

let router = DataAccessRouter::new(daf, get_current_user)?;
// GET /query/{id} returns 403/404/500 as appropriate
```

### Input Validation

Validate input at the boundary before passing to `DataAccess`:

```rust
use daf_core::QueryInfo;
use daf_core::ResourceId;

let info = QueryInfo {
    resource_id: ResourceId::new("user:1"),
    filters: None,
    algorithm: None,
};
```

### Error Handling

Never expose raw exception messages to clients. Wrap DAF operations to sanitize errors in production:

```rust
match daf.query(info, Some(user)).await {
    Ok(result) => ...,
    Err(daf_core::DataAccessError::Authorization(_)) => (StatusCode::FORBIDDEN, "Forbidden").into_response(),
    Err(daf_core::DataAccessError::NotFound(_)) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response(),
}
```

### Cache Invalidation

Be aware that mutations use per-resource prefix invalidation (`delete_prefix`). In high-throughput systems, prefix invalidation can still cause cache stampedes. Consider implementing per-key invalidation or TTL-based expiration at the repository or cache layer if your workload requires it.

### Rate Limiting

Use the built-in rate limiting in the Axum adapter. Do not disable rate limiting in production.

### Secrets Management

Never store secrets in the repository:

```rust
// Bad
repo.save(&ResourceId::new("db_password"), "super_secret").await;

// Good
let db_password = std::env::var("DB_PASSWORD")?;
```

### Dependency Scanning

Regularly scan dependencies for vulnerabilities:

```bash
cargo audit
```

## Known Security Considerations

### In-Memory Components

`MemoryRepository` and `MemoryCache` are reference implementations for development and testing. They do not provide:

- Persistence across restarts
- Access control
- Encryption at rest
- Audit logging

Do not use them in production with sensitive data.

### Bounded LRU Can Evict Generation Metadata

`_daf_gen:*` keys share the same cache namespace as query entries. Evicting generation metadata forces a cache miss, which is correct but may increase repository load. The `GenerationLocks` LRU is bounded to 256 entries to prevent unbounded memory growth from unique resource IDs.

### Rate Limiting

Rate limiting is implemented at the Axum adapter layer only. If you expose `DataAccess` directly (without the adapter), you must implement your own rate limiting.

### Algorithm Execution

Custom `Algorithm` implementations execute arbitrary code. Only use trusted algorithms in production.

### Authorization Model

The built-in authorizer is a simple ownership check (`owner_id == user.id`). It does not support roles, scopes, tenant boundaries, or administrative access. Implement a custom `Authorizer` for production use.

### Trie Traversal Complexity

`MemoryCache._trie_collect()` and `_trie_delete_prefix()` operate in O(prefix_length + K) time where K is the number of matching entries. The internal prefix trie stores keys only at terminal nodes, eliminating the O(N × L) memory amplification present in naive implementations.

### Rust FFI Safety

`daf-ffi` uses `extern "C"` with opaque pointers. Callers must not free returned pointers; the FFI layer owns all allocations. Error codes are `i32`; a return value of `0` indicates success, non-zero indicates failure.
