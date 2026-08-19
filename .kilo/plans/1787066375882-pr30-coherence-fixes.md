# PR30 Head: Adversarial Review — Implementation Plan

## Scope

Address all findings from the PR30 adversarial review. P0/P1 items are in scope. P2 items are deferred unless the implementer has capacity.

---

## Task 1 (P0): Fix generation/cache coherence race in `DataAccess::query()`

**File:** `crates/daf-application/src/lib.rs`

**Problem:** `query()` reads `cache.get()` then `generation_registry.current()` as two independent async calls. `_advance_generation()` holds a per-resource lock while advancing. Between the generation read and cache read, a mutation can advance the generation, causing a stale cache hit.

**Fix:** Acquire the generation lock in `query()` before reading generation and checking cache. Hold the lock through the cache check. Drop the lock before the repository fetch (cache miss path).

**Also fix:** `_resolve_current_generation()` currently does not acquire the lock. Change it to acquire the lock (or inline the locked read), since it's the only caller of the unlocked generation read in the query miss path.

**Concrete changes:**
1. In `query()` (line 330), add `let lock = self.generation_lock(&info.resource_id).await; let _guard = lock;` before line 341.
2. After the cache-hit check block (line 358), add `drop(_guard);` before calling `_execute_cache_miss`.
3. In `_resolve_current_generation()` (line 183), add lock acquisition identical to `_current_generation()` (line 135-143). Remove the now-unused `_current_generation()` if it's not called elsewhere (it is only called from Python tests, not from Rust query paths).

---

## Task 2 (P0): Fix CI workflow trigger typo

**File:** `.github/workflows/ci.yml`

**Problem:** Line 6 uses `pull_requests:` (plural) instead of `pull_request:` (singular). GitHub Actions does not recognize the plural form, so PR triggers are silently ignored.

**Fix:** Change line 6 from `pull_requests:` to `pull_request:`.

---

## Task 3 (P1): Rename `CachelitoCache` to `DashMapCache`

**Problem:** The struct is named `CachelitoCache` but uses `DashMap` directly, with no dependency on the `cachelito` crate. The architectural argument for this change was validated against a DashMap-shaped wrapper, not Cachelito.

**Fix:** Rename `CachelitoCache` to `DashMapCache` throughout the codebase.

**Files to change:**
- `crates/daf-cache/src/cachelito.rs` — rename struct, impl blocks, and consider renaming file to `dashmap_cache.rs`
- `crates/daf-cache/src/lib.rs` — update module declaration and `pub use`
- `crates/daf-application/tests/integration_tests.rs` — update imports and instantiations (8 occurrences)
- `crates/daf-application/tests/factory_tests.rs` — update imports and instantiations (3 occurrences)
- `crates/daf-cache/tests/traversal_tests.rs` — update imports and instantiations (3 occurrences)
- `crates/daf-ffi/src/lib.rs` — update imports and instantiations (2 occurrences)
- `crates/daf-ffi/src/bin/parity.rs` — update imports and instantiations (2 occurrences)
- Documentation files: `CHANGELOG.md`, `HANDOVER.md`, `README.md`, `SECURITY.md` — replace `CachelitoCache` with `DashMapCache`

---

## Task 4 (P1): Implement bounded eviction in L1 (`DashMapCache`)

**Problem:** `DashMapCache` (née `CachelitoCache`) has no eviction. `delete()`, `delete_prefix()`, `shake()`, and `clear()` are all no-ops. This is an unbounded memory leak.

**Fix:** Add capacity-bounded LRU-like eviction to `DashMapCache`.

**Concrete changes in `crates/daf-cache/src/cachelito.rs`:**
1. Add `capacity: usize` and `order: parking_lot::Mutex<Vec<String>>` (or `std::sync::Mutex`) fields to `DashMapCache`.
2. In `new(capacity: usize)`, initialize fields.
3. In `set()`, after insert, if `inner.len() > capacity`, evict oldest entries from `order` and `inner` until under capacity.
4. Implement `delete()` using `inner.remove(key)` and `order` cleanup.
5. Implement `delete_prefix()` by iterating `inner` and removing matching keys, then rebuilding `order`.
6. Implement `shake()` by removing expired/prefix-matched entries and returning count.
7. Implement `clear()` by clearing both `inner` and `order`.

**Note:** Use `parking_lot::Mutex` for the order tracking to avoid Tokio Mutex overhead on a synchronous data structure. If `parking_lot` is not in `Cargo.toml`, add it.

---

## Task 5 (P1): Remove stale promotion from `HierarchicalCache::get()`

**Problem:** `HierarchicalCache::get()` promotes L2/L3/L4 hits to L1 without generation validation. This creates a stale-resurrection mechanism in the hot tier.

**Fix:** Remove promotion side effects from `HierarchicalCache::get()`. Return the found entry without writing to L1. Let `DataAccess` (or an explicit promotion path) handle L1 writes with generation awareness.

**File:** `crates/daf-cache/src/hierarchical.rs`

**Concrete changes:**
1. In `get()` (lines 78-104), remove the `let promoted = ...; let _ = self.l1.set(...)` blocks for L2, L3, and L4 hits. Return the entry directly without promotion.
2. `set()` already only writes to L1 — keep this behavior.
3. `delete()` already deletes from all tiers — keep this behavior.

**Rationale:** Promotion should be an explicit, generation-aware action, not an automatic side effect of a tier probe. If promotion is needed, it should happen in `DataAccess` after a cache miss is resolved and the generation is verified.

---

## Task 6 (P1): Move `GenerationRegistry` from `daf-cache` to `daf-application`

**Problem:** `GenerationRegistry` is in `daf-cache`, but generation is application-level coherence state. The cache crate should not own the mechanism that determines whether cached data is valid.

**Fix:** Move `GenerationRegistry` to `daf-application`.

**Files to change:**
- `crates/daf-cache/src/lib.rs` — remove `pub mod generation_registry` and `pub use`
- `crates/daf-cache/src/generation_registry.rs` — move to `crates/daf-application/src/generation_registry.rs`
- `crates/daf-application/src/lib.rs` — add `pub mod generation_registry` and update internal imports
- `crates/daf-application/tests/integration_tests.rs` — update import path
- `crates/daf-application/tests/factory_tests.rs` — update import path
- `crates/daf-ffi/src/lib.rs` — update import path
- `crates/daf-ffi/src/bin/parity.rs` — update import path
- `crates/daf-cache/Cargo.toml` — remove `daf-core` from dependencies if no longer needed (GenerationRegistry was the only daf-core user in daf-cache besides the Cache trait)
- `crates/daf-application/Cargo.toml` — ensure `daf-core` is listed (it already is)

---

## Task 7 (P2): Reconcile documentation inconsistencies

**Files:** `CHANGELOG.md`, `README.md`, `SECURITY.md`, `HANDOVER.md`, `ADR/*.md`

**Action:** Update all docs to state the post-PR30 architecture consistently:
- L1: `DashMapCache` (bounded, concurrent, no LRU, explicit eviction)
- L2: `MokaCache`
- L3/L4: stubs
- `HierarchicalCache`: tier probe without automatic promotion
- `GenerationRegistry`: owned by `daf-application`
- Generation read + cache read are serialized via per-resource lock in `DataAccess::query()`

---

## Task 8 (P2): Remove tautological `debug_assert!` statements

**Files:** `crates/daf-cache/src/*.rs`, `crates/daf-application/src/lib.rs`

**Action:** Remove assertions whose conditions are always true (e.g., `debug_assert!(node.is_some() || true, ...)`). Keep assertions that enforce real invariants.

---

## Validation

After all changes:
1. `cargo fmt --check` — formatting
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — lint
3. `cargo test --workspace --all-features` — tests
4. `uv run ruff check src/ tests/` — Python lint
5. `uv run mypy src/ --strict` — Python types
6. `uv run pytest tests/ -q` — Python tests
7. `uv run python scripts/power_of_ten.py src/` — Power of Ten
8. `python scripts/power_of_ten_rust.py` — Power of Ten Rust
9. `uv run pytest tests/unit/test_differential_parity.py -v` — parity tests

---

## Rollout Order

1. Task 1 (P0 coherence) — must be first, unblocks correctness review
2. Task 2 (CI typo) — trivial, parallel-safe
3. Task 6 (GenerationRegistry move) — do before Task 3 to avoid renaming a soon-to-be-moved type
4. Task 3 (rename CachelitoCache) — after Task 6
5. Task 4 (bounded eviction) — after Task 3
6. Task 5 (remove promotion) — after Task 3
7. Tasks 7, 8 (P2 docs/assertions) — last
