# Adversarial Review Remediation Plan

## Context

PR #24 received adversarial review with 4 P0, 5 P1, and 3 P2 findings. The PR is **REQUEST CHANGES**. This plan addresses all findings with minimal architectural disruption while preserving the tier-aware cache hierarchy and parity test work.

## Key Design Decisions

### D1: Thread-local FFI error state
**Decision**: Replace `static mut LAST_ERROR` with `thread_local!` storage.
**Rationale**: C libraries conventionally use thread-local error state. This eliminates the data race while preserving the `daf_last_error_message()` ABI. Each OS thread gets its own error slot, matching caller expectations for FFI boundaries.

### D2: Cache invalidation is best-effort, never fails mutation
**Decision**: `HierarchicalCache::delete_prefix`, `delete`, and `clear` swallow backend errors (log only) and never propagate to caller.
**Rationale**: Repository mutation must not be rolled back by cache invalidation failure. The application already uses generation-based invalidation (`_superedge_invalidate` increments generation), so a failed prefix delete leaves the system in a correct state (old entries will be rejected by generation mismatch). Destructive invalidation is an optimization, not a correctness requirement.

### D3: Add lower-tier promotion on cache hit
**Decision**: `HierarchicalCache::get` promotes L2/L3/L4 hits into L1 before returning.
**Rationale**: Without promotion, L2/L3/L4 are never populated by normal writes (set writes L1 only). Promotion makes the hierarchy a useful read-through cache rather than a mere multiplexer. The `CacheEntry.tier` field preserves the originating tier metadata for observability.

### D4: Global generation lock registry via lock striping
**Decision**: Replace per-`DataAccess` `GenerationLocks` with a global fixed-size lock array (N=16 stripes), keyed by `resource_id` hash.
**Rationale**: Two independent `DataAccess` instances must synchronize on the same resource. Lock striping bounds memory (fixed array) while providing cross-instance coordination. This matches the original architecture from earlier plans.

### D5: Moka prefix ops are best-effort with accurate shake count
**Decision**: 
- `MokaCache::delete_prefix("")` → full invalidation (supported)
- `MokaCache::delete_prefix(non-empty)` → no-op, return `Ok(())` (best-effort)
- `MokaCache::shake` returns actual removed count for empty prefix, `0` for non-empty prefix (documented as best-effort)
**Rationale**: Moka does not support arbitrary prefix invalidation. Making it best-effort preserves `Cache` trait compatibility while documenting the limitation. `HierarchicalCache` delegates to L1 for precise prefix ops.

### D6: Feature-gated modules at compile time
**Decision**: Use `#[cfg(feature = "redis")]` and `#[cfg(feature = "postgres")]` at `pub mod` declarations.
**Rationale**: When features are disabled, the modules are not compiled at all. This makes the capability boundary explicit and prevents accidental use of non-functional backends.

### D7: FFI pointer validation contract
**Decision**: All FFI entrypoints validate `ptr != NULL`, `resource_id != NULL`, and `user_id` before unsafe dereference. Invalid pointers return `DafErrorCode::InvalidArgument`.
**Rationale**: `catch_unwind` does not make UB safe. The FFI boundary must either be `unsafe extern "C"` (caller guarantees) or fully self-validating. We choose self-validating for robustness.

### D8: Parity CI gate
**Decision**: Add `parity` to `build` job's `needs` list.
**Rationale**: Green build with failed parity is a false signal. Parity is part of the correctness gate.

## Task Breakdown

### Task 1: FFI Safety (P0)
- [ ] Replace `static mut LAST_ERROR` with `thread_local!` in `daf-ffi/src/lib.rs`
- [ ] Add null pointer validation to all `daf_*` entrypoints
- [ ] Add UTF-8 validation for C string inputs
- [ ] Update `SECURITY.md` FFI section with thread-local error semantics

### Task 2: Cache Invalidation Atomicity (P0)
- [ ] Make `HierarchicalCache::delete_prefix`, `delete`, `clear` swallow backend errors (log only)
- [ ] Document best-effort invalidation in `Cache` trait docs
- [ ] Update `DataAccess::_superedge_invalidate` to handle cache errors gracefully

### Task 3: Lower-Tier Promotion (P0)
- [ ] Add promotion logic to `HierarchicalCache::get`: on L2/L3/L4 hit, write entry to L1 before returning
- [ ] Preserve original `CacheEntry.tier` in promoted entry for observability
- [ ] Add tests for promotion behavior

### Task 4: Global Generation Lock Registry (P1)
- [ ] Create `daf-core/src/lock_registry.rs` with `LockRegistry` using fixed-size lock striping (N=16)
- [ ] Replace `DataAccess::generation_locks` with reference to global `LockRegistry`
- [ ] Update `_current_generation`, `_advance_generation`, `_superedge_invalidate` to use global registry
- [ ] Add `tokio::join!` to `test_concurrent_mutations_generation_monotonic`
- [ ] Add property test for concurrent mutations across multiple `DataAccess` instances

### Task 5: Moka Cache Accuracy (P1)
- [ ] Fix `MokaCache::shake` to return actual count for empty prefix
- [ ] Document non-empty prefix behavior as best-effort in trait docs
- [ ] Add test for shake count accuracy

### Task 6: Feature-Gated Modules (P1)
- [ ] Add `#[cfg(feature = "redis")]` and `#[cfg(feature = "postgres")]` to module declarations in `daf-cache/src/lib.rs`
- [ ] Update `daf-cache/Cargo.toml` to ensure features control compilation

### Task 7: CI Parity Gate (P1)
- [ ] Add `parity` to `build` job's `needs` list in `.github/workflows/ci.yml`

### Task 8: Cross-Implementation Parity Tests (P2)
- [ ] Add tests that run equivalent queries through Python and Rust and compare outputs
- [ ] Document parity methodology in `README.md`

### Task 9: Generation Enum End-to-End (P2)
- [ ] Keep `Generation` enum in cache key generation instead of converting to `u64`
- [ ] Update `_current_generation`, `_advance_generation`, `_superedge_invalidate` to use `Generation` type

### Task 10: Narrow FFI Claims (P2)
- [ ] Update `README.md` FFI section to describe control-plane ABI scope
- [ ] Add `ABI.md` documenting supported operations and limitations

## Validation

1. `cargo fmt --check` passes
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
3. `cargo test --workspace` passes
4. `uv run pytest tests/ -q` passes
5. FFI thread-safety verified with concurrent test
6. Cache invalidation atomicity verified with failure injection test

## Risks

| Risk | Mitigation |
|------|-----------|
| Best-effort invalidation masks real errors | Log all cache errors; add observability counter |
| Global lock registry contention | Lock striping with N=16 reduces contention |
| Promotion increases L1 memory pressure | L1 LRU eviction handles overflow |
| Moka prefix limitation surprises users | Document in `Cache` trait and `HierarchicalCache` docs |
| `thread_local!` error state not shared across threads | Document thread-affinity in FFI ABI docs |

## Out of Scope

- Full transactional cache invalidation (requires distributed transaction or MVCC)
- Parameterized `Cache<K, V>` (requires major trait redesign)
- Complete FFI ABI (filters, algorithms, payloads)
- Real Redis/Postgres backend implementations
