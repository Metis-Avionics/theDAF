# Cache Hierarchy Architecture Alignment Plan

**Spec contract:** `cache-hierarchy.toml` + companion coding-agent prompt  
**Goal:** Eliminate all tier-contract violations in `daf-cache` and `daf-application`, ensure L1 uses the actual Cachelito primitive (not DashMap), and add the full test matrix.

---

## 1. Current State Assessment

| Tier | Current Implementation | Spec Requirement | Status |
|------|----------------------|-----------------|--------|
| L1 | `AsyncGlobalCache` (Cachelito primitive) with leaked DashMap backing | Actual Cachelito primitive | ✅ Primitive correct; ⚠️ lib.rs doc comment still says "via DashMap" |
| L2 | `moka::future::Cache` | Actual Moka primitive | ✅ Primitive correct; ❌ `delete_prefix`/`shake` do full `invalidate_all()` |
| L3 | Stub — returns `"redis feature not enabled"` | Valkey/Redis operations | ❌ Stub |
| L4 | Stub — returns `"postgres feature not enabled"` | Postgres/HelixDB operations | ❌ Stub |
| `CacheEntry` | `{ value, origin_tier }` | Must carry `generation` | ❌ Missing |
| `HierarchicalCache::get()` | Promotes L2/L3/L4 hits directly to L1; no generation validation; propagates tier errors | L4→L3→L2→L1 promotion; generation-aware; degrade on failure | ❌ Wrong direction, no validation, no degradation |
| `DataAccess::query()` | Generation embedded in JSON value; lock added in working tree | Generation in `CacheEntry`; coherent read | ⚠️ Coherence fixed; ⚠️ Still embeds gen in JSON |

---

## 2. Architecture Decisions

### D1: `CacheEntry` carries generation

Add `generation: Generation` to `CacheEntry` in `daf-core`. This makes every tier generation-aware by construction. The data-access layer (generation owner) populates and validates this field.

### D2: HierarchicalCache promotion with degradation

`HierarchicalCache::get()` will:
1. Probe L1 → L2 → L3 → L4 in order
2. On tier hit: validate entry generation against current generation before returning
3. On tier error: degrade to next tier (do not propagate)
4. On L4 hit: populate L3, L2, L1 in that order, validating generation at each step
5. Never promote a stale entry

`HierarchicalCache` takes `Arc<GenerationRegistry>` in its constructor to read the current generation. The resource_id is derived from the cache key using the same SHA-256 namespace function as `DataAccess`.

### D3: Generation read order (coherence guarantee)

Within `HierarchicalCache::get()`:
1. Read current generation from `GenerationRegistry` first (under per-resource lock)
2. Probe tiers; compare each entry's `generation` against the observed current generation
3. Entry is valid iff `entry.generation == current_generation`

This eliminates the race: a writer cannot advance the generation between the generation read and the cache observation because the generation is read under the same lock that writers hold when advancing.

### D4: Moka prefix invalidation

Replace `invalidate_all()` in `MokaCache::delete_prefix` and `shake` with `invalidate_entries_if(|key| key.starts_with(prefix))`. This gives prefix-scoped invalidation using Moka's native predicate API.

### D5: L3/L4 real implementations

Implement `RedisCache` using the `redis` crate (tokio-comp, connection-manager) and `PostgresCache` using `sqlx` (runtime-tokio, postgres). Both remain behind feature flags. On connection failure, return `CacheError` so `HierarchicalCache` degrades to the next tier.

### D6: Cachelito L1 invalidation

`AsyncGlobalCache` does not expose `delete`/`clear`/`shake`. Accept this as a primitive limitation. `CachelitoCache::delete`, `delete_prefix`, `shake`, `clear` remain no-ops. Stale-entry rejection is handled by generation validation in `HierarchicalCache` and `DataAccess`. This satisfies the spec's `physical_deletion_optional = true` / `logical_invalidation_required = true`.

---

## 3. Detailed Implementation Tasks

### Task 1 (P0): Add `generation` to `CacheEntry`

**File:** `crates/daf-core/src/lib.rs`

```rust
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub value: Arc<dyn Any + Send + Sync>,
    pub origin_tier: Tier,
    pub generation: Generation,
}
```

Update all `CacheEntry { ... }` construction sites across the workspace.

### Task 2 (P0): Fix `HierarchicalCache` promotion, degradation, and generation validation

**File:** `crates/daf-cache/src/hierarchical.rs`

Changes:
1. Add `generation_registry: Arc<GenerationRegistry>` field
2. Add constructor parameter
3. Add `fn resource_namespace(key: &str) -> String` to extract the resource-id namespace from a cache key (SHA-256 of the portion between `query:` and the next `:`)
4. Rewrite `get()`:
   - Read current generation under per-resource lock
   - Probe L1: if hit and `entry.generation == current_gen`, return
   - On L1 error: continue to L2
   - Probe L2: if hit and `entry.generation == current_gen`, promote to L1, return
   - On L2 error: continue to L3
   - Probe L3: if hit and `entry.generation == current_gen`, promote L3→L2→L1, return
   - On L3 error: continue to L4
   - Read L4: if hit, promote L4→L3→L2→L1 (all with generation = current_gen), return
   - Return `Ok(None)` if all tiers miss
5. Fix `delete`, `delete_prefix`, `shake`, `clear` to degrade (use `if let Err(_) = tier.op()` continue)

### Task 3 (P0): Fix `MokaCache` prefix invalidation

**File:** `crates/daf-cache/src/moka.rs`

Replace `self.inner.invalidate_all()` in `delete_prefix` and `shake` with:
```rust
self.inner.invalidate_entries_if(|key| key.starts_with(prefix)).await;
```
Remove the error return for non-empty prefixes. Return the count of removed entries for `shake`.

### Task 4 (P1): Implement `RedisCache` (L3)

**File:** `crates/daf-cache/src/redis.rs`

- Accept `redis::Client` or `redis::ConnectionManager` in constructor
- `get`: `cmd.GET` → deserialize `CacheEntry`
- `set`: `cmd.SETEX` with TTL derived from generation metadata
- `delete`: `cmd.DEL`
- `delete_prefix`: `SCAN` + `DEL` (or `UNLINK`) for matching keys
- `shake`: same as `delete_prefix` but returns count
- `clear`: `FLUSHDB` or `FLUSHALL`
- On connection error, return `CacheError` (so HierarchicalCache degrades)

### Task 5 (P1): Implement `PostgresCache` (L4)

**File:** `crates/daf-cache/src/postgres.rs`

- Accept `sqlx::PgPool` in constructor
- `get`: `SELECT value, generation FROM cache WHERE key = $1`
- `set`: `INSERT INTO cache (key, value, generation) VALUES ($1, $2, $3) ON CONFLICT (key) DO UPDATE SET value = $2, generation = $3`
- `delete`: `DELETE FROM cache WHERE key = $1`
- `delete_prefix`: `DELETE FROM cache WHERE key LIKE $1`
- `shake`: `DELETE FROM cache WHERE key LIKE $1` returning count
- `clear`: `TRUNCATE cache`
- On query error, return `CacheError`

Requires adding a `CREATE TABLE cache` migration or ensuring the table exists.

### Task 6 (P1): Update `DataAccess` to use `CacheEntry.generation`

**File:** `crates/daf-application/src/lib.rs`

1. Replace embedded JSON generation with `CacheEntry.generation`:
   - `_build_cache_value()`: remove generation from JSON payload
   - `_handle_cache_hit()`: remove generation extraction from JSON
   - `query()`: validate with `cached_entry.generation == current_gen`
2. Keep the per-resource lock in `query()` for coherence (already in working tree)
3. `_resolve_current_generation()` already acquires lock — keep as-is

### Task 7 (P1): Fix `CachelitoCache` documentation and tests

**File:** `crates/daf-cache/src/lib.rs`

Update module doc comment:
```rust
//! - `CachelitoCache` (L1): async concurrent cache via Cachelito `AsyncGlobalCache` primitive.
//!   Invalidation operations (`delete`, `delete_prefix`, `shake`, `clear`) are no-ops
//!   because `AsyncGlobalCache` does not expose them. Stale-entry rejection is handled
//!   by generation validation in `HierarchicalCache` and `DataAccess`.
```

### Task 8 (P1): Fix `HierarchicalCache::set` to write to correct tier

Currently `set()` writes only to L1. Per the spec, writes should go to L4 (authoritative) and be promoted upward. Change `set()` to:
1. Write to L4
2. On success, write to L3, L2, L1 (fire-and-forget, log errors)
3. Return error only if L4 write fails

### Task 9 (P2): Add `daf-cache` tests

**File:** `crates/daf-cache/tests/cache_tests.rs` (new)

Cover all 22 spec-required test dimensions:

| # | Test | Tier |
|---|------|------|
| 1 | L1 hit | L1 |
| 2 | L2 hit | L2 |
| 3 | L3 hit | L3 |
| 4 | L4 authoritative read | L4 |
| 5 | L2 → L1 promotion | Hierarchical |
| 6 | L3 → L2 promotion | Hierarchical |
| 7 | L4 → L3 population | Hierarchical |
| 8 | Generation advancement | App |
| 9 | Stale L1 rejection | Hierarchical |
| 10 | Stale L2 rejection | Hierarchical |
| 11 | Stale L3 rejection | Hierarchical |
| 12 | Stale promotion rejection | Hierarchical |
| 13 | Concurrent reads | L1/L2 |
| 14 | Concurrent reads + mutation | App |
| 15 | Cache failure fallback | Hierarchical |
| 16 | L1 eviction/boundedness | L1 |
| 17 | L2 eviction/boundedness | L2 |
| 18 | L3 failure | Hierarchical |
| 19 | L4 failure | Hierarchical |
| 20 | All-features compilation | CI |
| 21 | Parity tests | CI |
| 22 | Parity binary failure fails CI | CI |

Concurrency tests must explicitly exercise interleavings (e.g., `tokio::join!`, `tokio::spawn`).

---

## 4. Test Plan

### Unit tests (`crates/daf-cache/tests/`)

- `cachelito_eviction`: fill L1 past capacity, assert bounded
- `moka_eviction`: fill L2 past capacity, assert bounded
- `moka_prefix_invalidation`: set keys with prefix, call `delete_prefix`, assert only matching keys removed
- `redis_prefix_invalidation`: (behind `redis` feature, requires test Redis instance)
- `postgres_prefix_invalidation`: (behind `postgres` feature, requires test Postgres)
- `hierarchical_promotion_direction`: mock tiers, verify L4→L3→L2→L1 order
- `hierarchical_degradation`: make L1 return error, verify L2 is consulted
- `hierarchical_stale_rejection`: inject stale entry into L2, verify miss
- `generation_validation_coherence`: concurrent mutation + read, assert no stale return

### Integration tests (`crates/daf-application/tests/`)

- Extend existing `test_stale_cache_entry_rejected_after_mutation` to use HierarchicalCache
- Add `test_hierarchical_l3_fallback_on_l1_l2_failure`
- Add `test_l4_failure_propagates_error`
- Add `test_concurrent_reads_never_return_stale` (stress test with many concurrent readers + 1 writer)

### Parity / CI tests

- `cargo test --workspace --all-features` must pass
- Parity binary (`daf-ffi/src/bin/parity.rs`) must build; CI must fail if it doesn't (use `std::process::exit(1)` on build failure, not `skip`)

---

## 5. Validation Commands

```bash
cargo fmt --all
cargo check --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

---

## 6. Invariants Checklist

| Invariant | How Verified |
|-----------|-------------|
| I1: L1 uses actual Cachelito | `CachelitoCache` wraps `AsyncGlobalCache<'static, ...>` from `cachelito_core` |
| I2: L2 uses actual Moka | `MokaCache` wraps `moka::future::Cache` |
| I3: L3 uses Valkey/Redis | `RedisCache` wraps `redis::Client` |
| I4: L4 is Postgres/HelixDB and authoritative | `PostgresCache` wraps `sqlx::PgPool` |
| I5: Caches never become authoritative | `HierarchicalCache::get()` always reads L4 on miss |
| I6: Stale values never returned | `entry.generation == current_gen` check in `HierarchicalCache::get()` and `DataAccess::query()` |
| I7: Stale values never promoted | Promotion validates generation before writing to higher tier |
| I8: Generation validation correct under concurrent mutation | Per-resource lock around generation read + cache observation |
| I9: Cache primitives not reimplemented | No `DashMap`/`HashMap` used as cache backing in L1/L2 |
| I10: Cache lifecycle bounded | Cachelito LRU + Moka LRU + bounded `AsyncGlobalCache` |
| I11: Cache failures degrade toward L4 | `if let Err(_) = tier.get()` continues to next tier |
| I12: Tests cannot silently skip | Parity binary build failure exits 1 |

---

## 7. Rollout Order

1. **Task 1** — `CacheEntry` generation field (unblocks all downstream work)
2. **Task 2** — `HierarchicalCache` rewrite (promotion direction, degradation, generation validation)
3. **Task 6** — `DataAccess` uses `CacheEntry.generation`
4. **Task 3** — Moka prefix invalidation
5. **Task 4** — RedisCache implementation
6. **Task 5** — PostgresCache implementation
7. **Task 7** — L1 documentation fix
8. **Task 8** — HierarchicalCache `set()` writes to L4
9. **Task 9** — Full test matrix

---

## 8. Confirmation: L1 Primitive

`CachelitoCache` wraps **`cachelito_core::AsyncGlobalCache`** directly. The `DashMap` and `parking_lot::Mutex` are leaked backing storage required by Cachelito's own primitive API — they are not a reimplementation or wrapper substituting for Cachelito. No `DashMap`-shaped wrapper exists in L1.
