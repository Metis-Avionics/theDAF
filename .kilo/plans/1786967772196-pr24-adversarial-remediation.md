# Plan: PR24 Adversarial Remediation

## Context
PR24 is at adversarial score 7.9/10. Two P0 architectural blockers and several P1/P2 issues prevent merge. This plan resolves all blocking and material findings.

---

## P0 Blockers (must fix before merge)

### Task 1: Restructure mutation flow — repository + generation first, cache advisory
**File:** `crates/daf-application/src/lib.rs`

**Current broken sequence in `put` and `delete`:**
```text
repository.try_update() / try_delete()
→ _superedge_invalidate()
    → delete_prefix()?  ← if Moka returns Err, stops here
    → shake()?
    → advance generation  ← never reached
    → set generation
→ return success
```

**Required sequence (Option A from review):**
```text
repository.try_update() / try_delete()  ← mutation is authoritative
→ advance generation                    ← must succeed regardless of cache
→ delete_prefix (best-effort)
→ shake (best-effort)
→ return success
```

**Implementation:**
- Split `_superedge_invalidate` into:
  - `_advance_generation` (already exists, called by `post`)
  - `_invalidate_caches` (delete_prefix + shake, errors swallowed/logged as advisory)
- In `put` and `delete`, call `_advance_generation` before `_invalidate_caches`
- If `_invalidate_caches` fails, log the degradation but do not fail the mutation
- Generation must advance even if cache tiers are broken

**Why this fixes the bug:** Generation becomes the freshness authority. Even if L2/L3/L4 survive invalidation, their cached generation will be stale and queries will reject them.

---

### Task 2: Make HierarchicalCache error semantics intentional and consistent
**File:** `crates/daf-cache/src/hierarchical.rs`

**Current state:**
- `delete` / `clear`: swallow errors (`let _ = ...`)
- `delete_prefix` / `shake`: propagate errors (`?`)

**Required state:** Choose one policy and apply it consistently.

**Recommended policy (propagate):** All tier mutations should propagate errors. The caller decides whether to treat cache errors as fatal or advisory. This matches the architectural correction in Task 1 where cache invalidation is already best-effort at the DataAccess level.

**Implementation:**
- Replace `let _ = self.l1.delete(key).await;` with `self.l1.delete(key).await?;` for all four tiers in `delete`
- Replace `let _ = self.l1.clear().await;` with `self.l1.clear().await?;` for all four tiers in `clear`
- Update `HANDOVER.md` to document the consistent propagation policy

---

## P1 Issues

### Task 3: Rename `CacheEntry.tier` to `origin_tier`
**Files:**
- `crates/daf-core/src/lib.rs` (struct definition)
- `crates/daf-cache/src/hierarchical.rs` (promotion code)
- `crates/daf-cache/src/memory.rs` (set creates `Tier::L1`)
- `crates/daf-cache/src/moka.rs` (set creates `Tier::L2`)

**Rationale:** After promotion, `tier` no longer reflects current residency. Renaming to `origin_tier` makes provenance explicit and prevents misuse.

---

### Task 4: Document Moka degraded semantics explicitly
**Files:**
- `crates/daf-cache/src/moka.rs` (doc comment already exists, strengthen it)
- `HANDOVER.md` (add explicit degraded-cache section)

**Add to MokaCache doc comment:**
> MokaCache is a **degraded tier**. It cannot perform prefix-scoped invalidation or shake. All non-empty prefix operations trigger a full tier invalidation followed by an error. Callers must treat L2 as advisory when Moka is in use.

---

### Task 5: Remove redundant `delete(gen_key)` from Python reference
**File:** `src/daf/core/access.py`

**Current code (line 225–227):**
```python
await self._cache.delete_prefix(f"query:{namespace}:")
await self._cache.delete(f"_daf_gen:{namespace}")  # redundant
await self._cache.shake(f"_daf_gen:{namespace}")
```

**Fix:** Remove the `delete` call. The Rust implementation already omits it. This brings parity to the invalidation sequence.

---

### Task 6: Add Python↔Rust differential parity tests
**File:** `tests/unit/test_rust_parity.py` (or new `tests/unit/test_differential_parity.py`)

**Current state:** Tests run Python and Rust test suites independently. CI runs both but never compares outputs.

**Required:** For the core contract surface, execute equivalent operations in both runtimes and compare normalized results:
- `post` → compare `MutationResult`
- `put` → compare `MutationResult` and generation advancement
- `delete` → compare `MutationResult` and generation advancement
- `query` cache miss → compare `QueryResult`
- `query` cache hit → compare `QueryResult`
- Generation round-trip → serialize/deserialize and compare
- Cache invalidation → verify stale rejection in both runtimes

**Implementation approach:**
- Use `pyo3` or `subprocess` to call Rust from Python, or vice versa
- Normalize timestamps and `cache_hit` fields before comparison
- Add as a new CI job `parity-differential` that depends on both `test` and `rust-test`

---

### Task 7: FFI stale-handle ABA prevention (P1 hardening)
**File:** `crates/daf-ffi/src/lib.rs`

**Current:** `LIVE_HANDLES` guards against double-free but not pointer reuse after free.

**Required:** Replace raw pointer handle with an indirection table or generation-tagged handle.

**Recommended minimal fix:**
- Change `LIVE_HANDLES: HashSet<usize>` to `HashMap<usize, u64>` where the value is a generation counter
- On `new`: insert `(handle, 0)`
- On `free`: remove entry, reject if absent
- On `use`: the FFI entry points already dereference `&*ptr` — this is safe as long as the handle is in the map

**Note:** Full opaque handle with token is better but requires changing the C ABI. The generation-tagged registry is a minimal improvement that prevents stale-handle reuse within a single process.

---

### Task 8: Fix documentation contradictions
**Files:**
- `HANDOVER.md`
- `BUGS.md`
- `VERIFICATION_CHECKLIST.md`
- `INDEX.md`

**Required fixes:**
- `HANDOVER.md`: Correct `delete`/`clear` error semantics claim (they swallow, not propagate)
- `HANDOVER.md`: Update Rust test count from 71 to 77
- `HANDOVER.md`: Remove "all planned bugs and security issues resolved" claim; replace with accurate status
- `HANDOVER.md`: Document the "accepted broken transaction boundary" as fixed (referencing Task 1)
- `BUGS.md`: Update last-modified date to current; ensure all items marked FIXED match code reality
- `VERIFICATION_CHECKLIST.md`: Update test counts, version numbers, and package names
- `INDEX.md`: Resolve 85% vs 100% parity contradiction; update to reflect actual differential parity plan

---

### Task 9: Remove ceremonial Power-of-Ten assertions
**Files:** All crates (8 total)

**Current:** `debug_assert!(true, "new invariant")` and similar meaningless assertions throughout.

**Required:**
- Remove all `debug_assert!(true, "...")` calls
- Replace with assertions that encode actual invariants:
  - Non-empty keys where required
  - Generation state machine transitions (`Missing → Valid(1)`, `Valid(n) → Valid(n+1)`)
  - Lock acquisition before mutation
  - Cache entry tier validity after promotion
- Update `scripts/power_of_ten_rust.py` to check for meaningful assertions, not just presence

---

### Task 10: Remove redundant `daf-core-contract` CI job or add differentiation
**File:** `.github/workflows/ci.yml`

**Current:** `daf-core-contract` runs `cargo test -p daf-core`, which is a subset of `cargo test --workspace` in `rust-test`.

**Required:** Either remove `daf-core-contract` or make it run a distinct subset (e.g., contract tests only, with `--test contract_tests`). If removed, update `build` job `needs`.

---

### Task 11: Restructure `parity` CI job into differential parity
**File:** `.github/workflows/ci.yml`

**Current:** `parity` re-runs `pytest` and `cargo test` after waiting on both.

**Required:** Replace with `parity-differential` that:
1. Builds both Python and Rust artifacts
2. Runs a dedicated differential test script
3. Fails if normalized outputs diverge

Remove or deprecate the old `parity` job.

---

### Task 12: Clarify L3/L4 feature coverage in docs
**Files:** `HANDOVER.md`, `INDEX.md`, `crates/daf-cache/src/lib.rs` (feature-gated modules)

**Current:** Docs imply L3/L4 are implemented backends. Redis/Postgres stubs return `CacheError::new("redis feature not enabled")` for every operation.

**Required:** Document that L3/L4 are **stub implementations** behind feature flags. Feature compilation (verified by `--all-features clippy`) proves stub compilation, not backend behavior.

---

## P2 Issues

### Task 13: Narrow PR scope documentation
**File:** `HANDOVER.md`, `INDEX.md`

PR24 title suggests "tier-aware cache hierarchy" but the diff includes:
- Rust architectural translation (all 9 crates)
- FFI implementation
- HTTP runtime
- CI hardening
- Parity tests
- Power-of-Ten compliance
- Living docs updates

**Required:** In documentation, explicitly list PR24 as the **Rust architectural milestone** with cache hierarchy as one component. This manages review scope expectations for future PRs.

---

## Validation Plan

After all tasks complete:
1. `cargo test --workspace` → all 77+ tests pass
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean
3. `uv run pytest tests/ -q` → all Python tests pass
4. `uv run ruff check src/ tests/` → clean
5. `uv run mypy src/ --strict` → clean
6. New `parity-differential` CI job passes
7. `HANDOVER.md`, `BUGS.md`, `VERIFICATION_CHECKLIST.md`, `INDEX.md` are consistent with code reality
8. Manual verification: `put` with Moka L2 returns success and advances generation even when `delete_prefix` would fail

---

## Execution Order

1. Task 1 (P0 — architectural fix)
2. Task 2 (P0 — error semantics)
3. Task 5 (Python parity — simple delete removal)
4. Task 3 (Tier rename — mechanical but wide blast radius)
5. Task 4 (Moka docs — accompanies Task 1)
6. Task 6 (Differential parity tests)
7. Task 7 (FFI handle hardening)
8. Task 8 (Documentation sync)
9. Task 9 (Power-of-Ten real invariants)
10. Task 10 (CI job cleanup)
11. Task 11 (CI parity redesign)
12. Task 12 (L3/L4 docs)
13. Task 13 (Scope docs)
