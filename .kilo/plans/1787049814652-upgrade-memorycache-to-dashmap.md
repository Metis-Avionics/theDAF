# ADR-003: L1 Hot Cache Strategy — Cachelito + DashMap-Upgraded MemoryCache

## Status

Accepted

## Date

2026-08-18

## Context

The architectural translation plan (`1786810915934`) specifies Cachelito as the planned L1 backend:
```rust
pub struct CachelitoCache {
    inner: cachelito::Cache<String, Arc<dyn Any + Send + Sync>>,
}
```

Current L1 is `MemoryCache` (`Arc<RwLock<MemoryCacheInner>>`), which serializes all reads with a global write lock.

The `cachelito` crate (v0.16.0) exposes two instance types:

| Type | Concurrency model | Methods available | Methods missing |
|---|---|---|---|
| `AsyncGlobalCache<'a, R>` | DashMap-based, lock-free async | `get`, `insert`, `insert_with_memory`, `stats` | `delete`, `delete_prefix`, `shake`, `clear` |
| `GlobalCache<R>` | `parking_lot::RwLock`-based sync | `get`, `insert`, `insert_with_memory`, `stats`, `clear` | `delete`, `delete_prefix`, `shake` |

Invalidation is handled via a global `InvalidationRegistry` that invokes callbacks registered per cache. `AsyncGlobalCache` does **not** expose its internal DashMap, so a caller cannot write a removal callback. The registry is designed for macro-generated caches (`#[cache_async]`), not manually-created instances.

**Neither Cachelito variant can fully implement the `daf_core::Cache` trait without degraded semantics for `delete`, `delete_prefix`, `shake`, and `clear`.**

## Decision

Adopt a **dual-track L1 strategy**:

1. **Cachelito as the planned L1 backend** — adopt `AsyncGlobalCache` with degraded/no-op semantics for operations it cannot support natively (`delete`, `delete_prefix`, `shake`, `clear`). This matches the existing Moka degraded-tier pattern and is acceptable for an L1 hot cache where fast reads are the primary goal and invalidation is advisory.

2. **DashMap-upgraded MemoryCache as the production L1** — replace `MemoryCache`'s `Arc<RwLock<HashMap>>` with `DashMap` while preserving the trie and LRU. This gives us a concurrent-capable L1 with full `Cache` trait compliance (including `delete_prefix` and `shake`), without depending on Cachelito's incomplete API surface.

Both implementations satisfy the `daf_core::Cache` trait. The system can choose which L1 to use based on configuration or deployment profile.

## Rationale

- **Cachelito provides the planned architectural evolution.** Adopting it validates the architectural translation plan and makes the team's stated intent real.
- **Degraded semantics are acceptable for L1.** The Pass 3 review established that caches are advisory and generation validation is the correctness guarantee. An L1 that cannot delete or shake does not introduce stale-data risk that generation validation cannot catch.
- **DashMap-upgraded MemoryCache provides a fallback.** It achieves concurrent reads via per-shard locking, preserves O(prefix_length + K) prefix operations via the trie, and keeps LRU eviction. This is the implementation path that can ship today with full correctness guarantees.
- **The `Cache` trait abstraction is preserved.** Neither approach requires changing trait signatures or flattening the hierarchy.

## Consequences

### CachelitoCache

- `get` — concurrent reads via DashMap shards
- `set` — per-shard write lock
- `delete` — **degraded**: returns `Ok(())` no-op (mirrors `GlobalCache` which also lacks `delete`)
- `delete_prefix` — **degraded**: returns `Ok(())` no-op
- `shake` — **degraded**: returns `Ok(0)` no-op
- `clear` — **degraded**: returns `Ok(())` no-op (also missing from `AsyncGlobalCache`)
- All tier operations remain async-compatible via `async-trait`

### DashMap-upgraded MemoryCache

- `get` — `DashMap::get` (lock-free concurrent read per shard)
- `set` — `DashMap::insert` (per-shard write lock)
- `delete` — `DashMap::remove` (per-shard write lock)
- `delete_prefix` — trie traversal + `DashMap::remove` per key (O(prefix_length + K), per-shard locks)
- `shake` — trie traversal + `DashMap::remove` per key, returns count (O(prefix_length + K))
- `clear` — `DashMap::clear()` (per-shard write locks)
- `has` — `DashMap::contains_key` (lock-free concurrent read per shard)
- LRU eviction preserved via `lru::LruCache`

### Tiered invalidation behavior

```rust
// HierarchicalCache::delete_prefix() calls each tier in sequence.
// L1 (Cachelito) → no-op
// L2 (MokaCache) → full invalidation + Err (existing degraded behavior)
// L3 (RedisCache) → stub error
// L4 (PostgresCache) → stub error
// DataAccess::_invalidate_caches logs advisory warning and continues.
```

Generation validation remains the authoritative stale-data guard regardless of which L1 is in use.

### Dependency changes

- Add `dashmap = "7"` to `crates/daf-cache/Cargo.toml`
- Remove `tokio::sync::RwLock` from `MemoryCache` (no longer needed for L1)
- Add `cachelito` workspace dependency (version TBD based on `AsyncGlobalCache` stability)

### Naming

- `CachelitoCache` — new file `crates/daf-cache/src/cachelito.rs`
- `MemoryCache` — renamed internals to use `DashMap`; keeps existing name to minimize downstream changes, or renamed to `ConcurrentMemoryCache` if team prefers explicit naming

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Use `GlobalCache<R>` instead of `AsyncGlobalCache` | Sync API conflicts with async `Cache` trait; requires `parking_lot::RwLock` (coarser than DashMap); still lacks `delete`, `delete_prefix`, `shake` |
| Patch Cachelito to expose internal DashMap | Adds upstream dependency; not in scope for this upgrade |
| Replace Cachelito with DashMap entirely | Defeats the purpose of adopting Cachelito as the planned L1 backend |
| Keep MemoryCache with `RwLock` | Does not solve the concurrent-read serialization problem |
| Use Moka as L1 | Already designated as L2; moving it would require rearchitecting the tier hierarchy |

## Related Decisions

- ADR-001: Semantic Freeze & Reference Boundary
- ADR-002: Generation State Model
- PR24 Adversarial Pass 3 (`.kilo/plans/PR24_ADVERSARIAL_PASS3.toml`) — concurrency-compatibility validation

## Validation

1. `cargo check --workspace` passes with both `CachelitoCache` and `DashMap`-upgraded `MemoryCache` compiled
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
3. `cargo test --workspace` passes
4. Prefix invalidation and generation coherence tests pass with both L1 implementations
5. Concurrency stress test confirms independent L1 reads proceed without global serialization with both implementations
6. Degraded semantics are documented in `Cache` trait implementor docs
