# Four-Tier Cache Hierarchy — CacheManager Implementation Plan

**Scope:** Replace `HierarchicalCache` + `GenerationRegistry` with a unified `CacheManager` that owns tier instances and uses DashMap only for Cachelito-incompatible orchestration. Cachelito is the L1 primitive; no reimplementation of cache semantics.

---

## 1. Current State

| Component | Status | Notes |
|-----------|--------|-------|
| `CacheEntry` with `generation` | ✅ | `daf-core/src/lib.rs` |
| `CachelitoCache` (L1) | ✅ | Wraps `AsyncGlobalCache<'static, CacheEntry>` |
| `MokaCache` (L2) | ✅ | Wraps `moka::future::Cache` |
| `RedisCache` (L3) | ❌ | 7 compilation errors |
| `PostgresCache` (L4) | ✅ | Compiles behind `postgres` feature |
| `HierarchicalCache` | ❌ | To be replaced by `CacheManager` |
| `GenerationRegistry` | ❌ | To be inlined into `CacheManager` |
| `DataAccess` constructor | ⚠️ | Takes `cache` + `generation_registry` separately |
| `integration_tests.rs` | ❌ | 10 `let` lines have unparenthesized `as Arc<dyn>` casts |

---

## 2. Architecture: `CacheManager`

**File:** `crates/daf-cache/src/cache_manager.rs` (new)

`CacheManager` replaces both `HierarchicalCache` and `GenerationRegistry`. It:
- Owns the four tier instances directly (concrete types, not `dyn Cache`)
- Contains an internal `DashMap<String, Generation>` for the generation registry (Cachelito has no key-value store for this; DashMap is the correct primitive here)
- Contains an internal `DashMap<Tier, TierStats>` for per-tier health/error tracking (orchestration metadata only)
- Exposes the `Cache` trait so `DataAccess` sees no change in its `cache` parameter type
- Has no `dyn Cache` indirection; tiers are concrete fields

```rust
pub struct CacheManager {
    l1: CachelitoCache,
    l2: MokaCache,
    l3: Option<RedisCache>,
    l4: Option<PostgresCache>,
    generations: DashMap<String, Generation>,
    stats: DashMap<Tier, TierStats>,
}
```

**DashMap usage is limited to:**
1. `generations`: the generation registry (Cachelito cannot do key-value generation tracking)
2. `stats`: lightweight per-tier counters for observability

No DashMap is used as a cache backing store. Cachelito is the sole L1 cache primitive.

**Constructor:**
```rust
impl CacheManager {
    pub fn new(l1: CachelitoCache, l2: MokaCache, l3: Option<RedisCache>, l4: Option<PostgresCache>) -> Self {
        Self { l1, l2, l3, l4, generations: DashMap::new(), stats: DashMap::new() }
    }
}
```

**`get` behavior (mirrors existing `HierarchicalCache::get` with degradation fix):**
1. Read current generation from `self.generations` under per-resource lock (`LockRegistry` — already in `daf-core`)
2. Probe L1: on error → `None`; on hit with `entry.generation == current_gen` → return
3. Probe L2: on error → `None`; on hit with `entry.generation == current_gen` → promote to L1, return
4. Probe L3 (if present): on error → `None`; on hit with `entry.generation == current_gen` → promote L3→L2→L1, return
5. Probe L4 (if present): on error → propagate `CacheError` (authoritative, no lower tier); on hit with `entry.generation == current_gen` → promote L4→L3→L2→L1, return
6. Return `Ok(None)` if all tiers miss

**`set` behavior:**
1. Write to L4 (if present), then L3 (if present), then L2, then L1
2. Return error only if L4 write fails

**`delete`/`delete_prefix`/`shake`/`clear`:**
- Propagate to all present tiers; L4 errors propagate, others degrade

---

## 3. `DataAccess` Constructor Change

**File:** `crates/daf-application/src/lib.rs`

Replace the two-parameter `(cache: Arc<dyn Cache>, generation_registry: Arc<GenerationRegistry>)` with a single `cache: Arc<CacheManager>` parameter.

`CacheManager` implements `Cache`, so the `Cache` trait bound remains unchanged. Generation reads go through `CacheManager`'s internal DashMap rather than a separate `GenerationRegistry`.

**Remove:** `daf-cache/src/generation_registry.rs` (module and `pub use`)

---

## 4. Bug Fixes (carried from previous plan)

### B1: `HierarchicalCache::get` error degradation → applies to `CacheManager::get`

L1/L2/L3 errors return `None` (degrade). L4 errors propagate.

### B2: `MokaCache::shake` return value

`invalidate_entries_if` returns `u64` count of removed entries. Use it directly.

### B3: `RedisCache` compilation errors

- Add `use serde::{Deserialize, Serialize};`
- Remove non-existent `RedisError::Io` / `ConnectionNotFound` pattern matching; use generic `.map_err(|e| CacheError::new(format!("redis ... error: {e}")))`
- Collect `scan_match` `AsyncIter` manually with `while let Some(key) = iter.next_item().await`

### B4: `integration_tests.rs` `as Arc<dyn ...>` syntax

Wrap all `let`-statement casts in parentheses: `let x = (Arc::new(...) as Arc<dyn ...>);`

---

## 5. Test Updates

All tests referencing `HierarchicalCache` or `GenerationRegistry` must be updated.

**Files to update:**
- `crates/daf-application/tests/integration_tests.rs`
- `crates/daf-application/tests/factory_tests.rs`
- `crates/daf-cache/tests/traversal_tests.rs` (if any CacheManager references)

**New tests needed in `crates/daf-cache/tests/cache_manager_tests.rs`:**
- L2→L1 promotion with generation validation
- L3→L2→L1 promotion
- L4→L3→L2→L1 population
- Stale entry rejection at each tier
- Error degradation (mock tier error → next tier consulted)
- L4 error propagation (authoritative)
- Generation advancement + stale rejection under concurrency

---

## 6. Module Structure After Changes

```
crates/daf-cache/src/
├── lib.rs              ← exports CacheManager, CachelitoCache, MokaCache, RedisCache, PostgresCache
├── cache_manager.rs    ← NEW: unified tier orchestrator
├── cachelito.rs        ← L1 (unchanged)
├── moka.rs             ← L2 (unchanged except B2 fix)
├── redis.rs            ← L3 (B3 fixes)
├── postgres.rs         ← L4 (unchanged)
├── generation_registry.rs ← REMOVED
├── hierarchical.rs     ← REMOVED
└── trie.rs             ← unchanged
```

`lib.rs` changes:
- Remove `pub mod hierarchical;` and `pub mod generation_registry;`
- Remove `pub use crate::hierarchical::HierarchicalCache;` and `pub use crate::generation_registry::GenerationRegistry;`
- Add `pub mod cache_manager;` and `pub use crate::cache_manager::CacheManager;`

---

## 7. `daf-cache/Cargo.toml` Changes

No new dependencies. `dashmap` is already present (used by `CachelitoCache` backing and now also by `CacheManager` for generation/stats).

---

## 8. Rollout Order

1. Create `cache_manager.rs` with `CacheManager` struct + `Cache` impl
2. Fix `moka.rs` shake return value (B2)
3. Fix `redis.rs` compilation errors (B3)
4. Update `lib.rs`: remove `hierarchical`/`generation_registry` modules, add `cache_manager`
5. Delete `hierarchical.rs` and `generation_registry.rs`
6. Update `daf-application/src/lib.rs`: single `cache: Arc<CacheManager>` parameter
7. Fix `integration_tests.rs` syntax (B4)
8. Add `cache_manager_tests.rs`
9. Update all test call sites
10. Validate

---

## 9. Validation

```bash
cargo fmt --all
cargo check --workspace
cargo check --workspace --all-features
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

---

## 10. Invariants

| Invariant | How Verified |
|-----------|-------------|
| I1: L1 uses Cachelito primitive | `CachelitoCache` wraps `AsyncGlobalCache<'static, CacheEntry>` |
| I2: L2 uses Moka primitive | `MokaCache` wraps `moka::future::Cache` |
| I3: L3 uses Redis primitive | `RedisCache` wraps `redis::Client` |
| I4: L4 is Postgres authoritative | `PostgresCache` wraps `sqlx::PgPool` |
| I5: No DashMap as cache backing | DashMap only in `CacheManager` for generations + stats |
| I6: Stale values never returned | `entry.generation == current_gen` check |
| I7: Stale values never promoted | Promotion validates generation before writing to higher tier |
| I8: Tier errors degrade toward L4 | L1/L2/L3 errors in `get()` return `None` |
| I9: L4 errors propagate | L4 `get()` uses `?` |
| I10: Single orchestrator | `CacheManager` is sole owner of tier promotion/degradation logic |
