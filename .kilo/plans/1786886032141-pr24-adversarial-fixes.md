# PR #24 Adversarial Review — Implementation Plan

## Decisions Locked

| Decision | Choice |
|---|---|
| Task 1: `MokaCache::delete_prefix` non-empty prefix | **Option C** — always call `invalidate_all()`, return `Err(CacheError::UnsupportedPrefix)` |
| Task 3: `MokaCache::shake` non-empty prefix | **Option C** — always call `invalidate_all()`, return `Err(CacheError::UnsupportedPrefix)` |
| `_superedge_invalidate` error handling | **Option B** — propagate both `delete_prefix` and `shake` errors to caller |
| Task 4: `Generation` enum at application layer | **Option A** — keep enum, compare `Generation` directly in `query()` |
| Task 5: FFI double-free guard | **Option A** — live handle tracking via `Mutex<HashSet<usize>>` |

## ADR: Moka Tier Degraded-Invalidation Policy

**Status:** Accepted

**Context:** `MokaCache` has no key index and cannot perform prefix-scoped invalidation or shake. The `Cache` trait requires `delete_prefix(prefix)` and `shake(prefix)` for arbitrary prefixes.

**Decision:** `MokaCache` will always call `invalidate_all()` for any non-empty prefix, then return `Err(CacheError::UnsupportedPrefix)` describing the degraded operation. Empty-prefix calls (`prefix == ""`) continue to return `Ok(())` after `invalidate_all()`.

**Consequence:** With `HierarchicalCache`, Moka errors propagate through `_superedge_invalidate`, which now returns `Err` to `put`/`delete` callers. This means a successful repository mutation can be reported as failed after the fact — the broken transaction boundary the adversarial review flagged. The team accepts this trade-off because explicit errors are preferred over silent degradation for debuggability.

**Rationale:** Silent degradation (returning `Ok(())` without clearing entries) caused the stale-L2 bug. Returning an error without clearing entries leaves stale data in L2 indefinitely. Clearing all entries and signaling the degradation is the only option that is both correct and observable.

---

## Task 1 (P0): `MokaCache::delete_prefix` — Option C

**File:** `crates/daf-cache/src/moka.rs`

Add `CacheError::UnsupportedPrefix` variant to `daf-core/src/lib.rs` if it doesn't exist (current definition is `pub struct CacheError(pub String)` — use `CacheError::new(...)`).

Change `delete_prefix`:
```rust
async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
    self.inner.invalidate_all();
    if prefix.is_empty() {
        Ok(())
    } else {
        Err(CacheError::new(format!(
            "MokaCache does not support prefix-scoped invalidation; full tier invalidated for prefix '{}'",
            prefix
        )))
    }
}
```

Add doc comment to `MokaCache` explaining the behavior.

---

## Task 2 (P0): `HierarchicalCache::delete_prefix` — propagate tier errors

**File:** `crates/daf-cache/src/hierarchical.rs`

Change from `let _ = tier.delete_prefix(...)` to `?`-propagation for all four tiers:
```rust
async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
    self.l1.delete_prefix(prefix).await?;
    self.l2.delete_prefix(prefix).await?;
    self.l3.delete_prefix(prefix).await?;
    self.l4.delete_prefix(prefix).await?;
    Ok(())
}
```

---

## Task 3 (P1): `MokaCache::shake` — Option C

**File:** `crates/daf-cache/src/moka.rs`

Change to always invalidate all and return the count, with error for non-empty prefix:
```rust
async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
    let count = self.inner.entry_count() as usize;
    self.inner.invalidate_all();
    if prefix.is_empty() {
        Ok(count)
    } else {
        Err(CacheError::new(format!(
            "MokaCache does not support prefix-scoped shake; full tier invalidated for prefix '{}'",
            prefix
        )))
    }
}
```

---

## Task 3b (P0): `HierarchicalCache::shake` — propagate all tier errors

**File:** `crates/daf-cache/src/hierarchical.rs`

Currently L2-L4 shake errors are silently swallowed (`if let Ok(r2) = ...`). Make all tiers authoritative:
```rust
async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
    let mut total: usize = 0;
    let r1 = self.l1.shake(prefix).await?;
    total += r1;
    let r2 = self.l2.shake(prefix).await?;
    total += r2;
    let r3 = self.l3.shake(prefix).await?;
    total += r3;
    let r4 = self.l4.shake(prefix).await?;
    total += r4;
    Ok(total)
}
```

---

## Task 4 (P0): `_superedge_invalidate` — propagate errors

**File:** `crates/daf-application/src/lib.rs`

Change `let _ =` to `?` for both `delete_prefix` and `shake`:
```rust
async fn _superedge_invalidate(&self, resource_id: &str) -> Result<(), DataAccessError> {
    let lock = self.generation_lock(resource_id).await;
    let _guard = lock;
    let namespace = self.resource_namespace(resource_id);
    let gen_key = format!("_daf_gen:{namespace}");
    let current = match self.cache.get(&gen_key).await? {
        Some(e) => e.value.downcast_ref::<Generation>().copied(),
        None => None,
    };
    self.cache.delete_prefix(&format!("query:{namespace}:")).await?;
    self.cache.shake(&gen_key).await?;
    let next = current.unwrap_or(Generation::Missing).advance();
    self.cache.set(gen_key, Arc::new(next)).await?;
    Ok(())
}
```

**⚠️ Consequence:** `put` and `delete` now return `Err` after the repository has already mutated when L2 is `MokaCache`. This is the broken transaction boundary the review flagged. Accepted per ADR.

---

## Task 5 (P2): `Generation` enum comparison in `query()`

**File:** `crates/daf-application/src/lib.rs`

In `query()` (around line 283), replace:
```rust
if cached_gen == current_gen.as_u64() {
```
with:
```rust
if let Some(cached_gen_enum) = cached_value.get("generation").and_then(|g| g.as_u64()).map(Generation::Valid) {
    if cached_gen_enum == current_gen {
```

Wait — `current_gen` is already `Generation`. The cached JSON stores `generation` as a `u64`. To compare without losing the `Missing`/`Valid` distinction, compare the stored `u64` against `current_gen.as_u64()` *only if* we also check whether the cache entry was stored with a `Missing` generation. But the JSON serialization loses that distinction.

Better approach: store the `Generation` in the cache entry's typed value, not just in JSON. But `CacheEntry.value` is `Arc<dyn Any>`, and the current code stores `Arc<serde_json::Value>`. Changing this requires changing `_execute_cache_miss`.

Alternative: keep the JSON comparison but document that `Missing` is serialized as `null` or absent, and `Valid(n)` as the integer `n`. Then in `query()`:
```rust
let cached_gen = cached_map.get("generation").and_then(|g| {
    if g.is_null() { Some(Generation::Missing) } else { g.as_u64().map(Generation::Valid) }
});
if cached_gen == Some(current_gen) {
```

This preserves the enum semantics through the JSON transport. The `_execute_cache_miss` should serialize `Generation::Missing` as `serde_json::Value::Null` and `Generation::Valid(n)` as `serde_json::Value::Number(n.into())`.

Changes:
1. In `_execute_cache_miss` line 220: replace `current_generation.as_u64()` with explicit enum serialization.
2. In `query()` line 283: deserialize back to `Generation` and compare directly.

---

## Task 6 (P2): FFI double-free guard

**File:** `crates/daf-ffi/src/lib.rs`

Add:
```rust
use std::sync::Mutex;
use std::collections::HashSet;

static LIVE_HANDLES: Mutex<HashSet<usize>> = Mutex::new(HashSet::new());
```

In `daf_data_access_new`: after `Box::into_raw`, insert `ptr as usize` into `LIVE_HANDLES`.
In `daf_data_access_free`: check membership; if absent, return `InvalidArgument`; otherwise remove and drop.

---

## Task 7 (tests)

New tests in `crates/daf-application/tests/integration_tests.rs` and `crates/daf-cache/tests/`:

1. `moka_delete_prefix_non_empty_returns_error_and_clears` — assert `Err` returned, cache empty after.
2. `moka_shake_non_empty_returns_error_and_clears` — assert `Err` returned, cache empty after.
3. `hierarchical_delete_prefix_propagates_moka_error` — mock L2 returns `Err`; assert `HierarchicalCache::delete_prefix` returns `Err`.
4. `generation_enum_comparison_in_query` — post → gen `Valid(1)`, put → gen `Valid(2)`, query with stale cached gen `Valid(1)` returns miss.
5. `ffi_double_free_returns_invalid_argument` — allocate, free, free again → `InvalidArgument`.
6. **Adversarial:** `put_with_moka_l2_returns_err_after_repo_mutation` — assert repository is mutated even when `_superedge_invalidate` returns `Err`. Documents the accepted broken transaction boundary.

---

## Execution Order

```
Task 1 (P0) → Task 2 (P0) → Task 3 (P1) + Task 3b (P0) → Task 4 (P0) → Task 5 (P2) → Task 6 (P2) → Task 7 (tests) → ADR writeup
```

`_superedge_invalidate` change (Task 4) must come after Tasks 1-3 so the new `CacheError::UnsupportedPrefix` exists. Generation comparison (Task 5) is independent but touches `query()` near Task 4.

---

## Risks

- **Moka as L2 makes `put`/`delete` always fail.** This is accepted. Documented in ADR. If this becomes blocking, the mitigation is to use `MemoryCache` as L2 or implement prefix tracking in Moka (Option C from original analysis).
- **`HierarchicalCache::shake` error propagation.** If L1 shake fails, L2-L4 are not attempted. This is a behavioral change from current "best-effort" semantics.
