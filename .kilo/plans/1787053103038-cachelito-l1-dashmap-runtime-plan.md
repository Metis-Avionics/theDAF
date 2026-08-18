# Cachelito L1 + DashMap Runtime State Plan

## Status

Implementation-ready. One blocking question remains: whether `delete_prefix`/`shake` should be removed from the mutation path or kept as belt-and-suspenders fallback. Recommended answer: remove from mutation path; rely on generation validation + Cachelito eviction. See §4.

## Goal

Replace the current `MemoryCache` (DashMap-backed bespoke L1) with **Cachelito as the sole L1 cache**. Use **DashMap exclusively for concurrent runtime metadata** (`GenerationRegistry`). Eliminate duplicated ownership of cached values.

## Current Problem

`MemoryCache` in `crates/daf-cache/src/lib.rs` uses `DashMap<String, CacheEntry>` as the actual L1 store, plus `lru::LruCache` and `TrieNode` for eviction and prefix ops. This recreates the cache machinery (storage, eviction, synchronization, lifecycle, cache semantics) that Cachelito is supposed to replace.

## Target Architecture

```text
                    DataAccess
                        │
                        ▼
                 ┌─────────────┐
                 │ L1 Cache    │
                 │  Cachelito  │
                 └──────┬──────┘
                        │ miss
                        ▼
                 ┌─────────────┐
                 │ concurrent  │
                 │ structures  │
                 │  DashMap    │
                 │ (GenRegistry)│
                 └──────┬──────┘
                        │
                        ▼
                 lower persistence
```

**Separation of concerns:**
- Cachelito: cached query results (values only)
- DashMap: runtime metadata (current generation per resource)

**Invariant:** No duplicated ownership of cached values.

## Implementation Steps

### Step 1: Add Cachelito dependency

**File:** `crates/daf-cache/Cargo.toml`

Add `cachelito` workspace dependency. Use `cachelito` v0.16+ with `AsyncGlobalCache`. The `Cache` trait is async, so `AsyncGlobalCache` is the correct variant.

```toml
cachelito = { version = "0.16", features = ["async"] }
```

Verify `cargo check --workspace` passes.

### Step 2: Create `CachelitoCache`

**New file:** `crates/daf-cache/src/cachelito.rs`

```rust
use std::sync::Arc;
use async_trait::async_trait;
use cachelito::AsyncGlobalCache;
use daf_core::{Cache, CacheEntry, CacheError, Tier};

pub struct CachelitoCache {
    inner: AsyncGlobalCache<String, Arc<dyn std::any::Any + Send + Sync>>,
}

impl CachelitoCache {
    pub fn new() -> Self {
        Self {
            inner: AsyncGlobalCache::new(Default::default()),
        }
    }
}

#[async_trait]
impl Cache for CachelitoCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        let entry = self.inner.get(key).await;
        Ok(entry.map(|value| CacheEntry {
            value,
            origin_tier: Tier::L1,
        }))
    }

    async fn set(&self, key: String, value: Arc<dyn std::any::Any + Send + Sync>) -> Result<(), CacheError> {
        self.inner.insert(key, value).await;
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn delete_prefix(&self, _prefix: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn shake(&self, _prefix: &str) -> Result<usize, CacheError> {
        Ok(0)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        Ok(())
    }
}
```

**Degraded semantics:** `delete`, `delete_prefix`, `shake`, `clear` are no-ops. This is acceptable because generation validation is the authoritative stale-data guard.

### Step 3: Create `GenerationRegistry`

**New file:** `crates/daf-core/src/generation_registry.rs`

```rust
use std::sync::Arc;
use dashmap::DashMap;
use daf_core::{Generation, ResourceId};

pub struct GenerationRegistry {
    inner: Arc<DashMap<ResourceId, Generation>>,
}

impl GenerationRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub async fn current(&self, resource_id: &ResourceId) -> Generation {
        self.inner
            .get(resource_id)
            .map(|entry| *entry)
            .unwrap_or(Generation::Missing)
    }

    pub async fn advance(&self, resource_id: &ResourceId) -> Generation {
        let mut entry = self.inner.entry(*resource_id).or_insert(Generation::Missing);
        let next = entry.advance();
        *entry = next;
        next
    }
}
```

**DashMap is used ONLY here.** This is the runtime metadata store. No cached values.

### Step 4: Update `DataAccess` to use `GenerationRegistry`

**File:** `crates/daf-application/src/lib.rs`

Changes:
1. Add `generation_registry: Arc<GenerationRegistry>` field to `DataAccess`
2. Update constructor and factory
3. Replace `_current_generation` to read from `GenerationRegistry` instead of cache key `_daf_gen:{namespace}`
4. Replace `_advance_generation` to write to `GenerationRegistry` instead of cache key
5. **Remove `_invalidate_caches`** from the mutation path (`put`, `delete`). Generation advancement alone is sufficient; stale cache entries are rejected by generation validation and eventually evicted by Cachelito.

**Key behavioral change:**
- `put` and `delete` now only call `_advance_generation`
- `_invalidate_caches` is removed entirely
- Old cache entries remain in Cachelito but are rejected by `query()`'s generation check

### Step 5: Remove `MemoryCache` as L1

**File:** `crates/daf-cache/src/lib.rs`

- Remove `MemoryCache` struct and its `impl Cache` block
- Remove `dashmap` and `lru` dependencies from `daf-cache/Cargo.toml` (keep `lru` only if still needed elsewhere; check)
- Remove `pub use` of `MemoryCache` from `lib.rs`
- Keep `trie` module if needed by other tiers or tests; otherwise remove

**Note:** If `MemoryCache` is needed for tests or as a non-L1 fallback, keep it but do not use it as the default L1. Preferred: remove it entirely to eliminate the temptation to use DashMap as a cache store.

### Step 6: Update `HierarchicalCache` construction

**File:** wherever `HierarchicalCache::new` is called (likely `daf-http` or `daf-application` tests)

Update to use `CachelitoCache::new()` as L1 instead of `MemoryCache::new()`.

### Step 7: Update tests

1. **Integration tests** in `crates/daf-application/tests/` — verify `post`/`put`/`delete` still advance generation correctly
2. **Cache tests** in `crates/daf-cache/tests/` — update or remove `MemoryCache` tests; add `CachelitoCache` tests
3. **Parity tests** — ensure Rust behavior matches Python reference

### Step 8: Validate

```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo fmt --check
```

## Unresolved Question

**Should `delete_prefix` and `shake` be removed from `_invalidate_caches` entirely, or kept as a fallback?**

- **Recommended answer:** Remove from mutation path. Generation validation + Cachelito eviction is sufficient and architecturally cleaner. Prefix deletion recreates the coupling between cache structure and mutation logic that the generation model is designed to eliminate.
- **Alternative:** Keep `_invalidate_caches` as an advisory/best-effort operation (log warning on failure, don't block mutation). This preserves the existing pattern but adds complexity without correctness benefit.

## Risks

| Risk | Mitigation |
|---|---|
| Cachelito eviction timing leaves stale entries longer than prefix deletion | Generation validation rejects stale entries; eviction is eventual, not correctness-critical |
| Cachelito API changes | Pin version in Cargo.toml; degraded ops are no-ops so API breakage is low-impact |
| DashMap guard retained across await | `GenerationRegistry::current` returns `Generation` by value (Copy), no guard retention possible |
| Tests depend on `MemoryCache` | Update or remove dependent tests |
| `GenerationRegistry` becomes a global singleton | Keep it injected via `DataAccess` constructor for testability; promote to global only if cross-crate access is needed |

## Validation Criteria

1. `CachelitoCache` is the sole L1 implementation
2. `MemoryCache` is removed from production path
3. `DashMap` appears in exactly one place: `GenerationRegistry`
4. `_invalidate_caches` is removed from mutation path
5. Generation advancement is the sole mutation-side invalidation mechanism
6. `query()` still rejects stale entries via generation comparison
7. All existing tests pass
8. No `daf-core` dependency on `cachelito` or `dashmap`
