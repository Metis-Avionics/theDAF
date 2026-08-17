# Red Team Review Remediation — Remaining Fixes

## Context
Session 016 implemented most of `.kilo/plans/1786880865005-adversarial-review-remediation.md` but crashed before updating all integration tests. Two tests are currently failing, and one Moka accounting bug remains.

## Current State
- **Branch**: `feat/tier-aware-cache-and-parity`
- **Python**: 212/212 passing
- **Rust**: 27/29 passing (2 failures)
- **Uncommitted**: All red-team remediation changes in working tree

## Failing Tests
1. `test_concurrent_mutations_generation_monotonic` — downcasts cached generation to `u64`, but cache now stores `Generation` enum; assertion `gen >= 2` is wrong for CAS-based implementation (only one concurrent put succeeds).
2. `test_generation_advances_on_delete` — same downcast mismatch; `Generation::Valid(1)` downcasts to `u64` as `None` → `0`, so `0 == 0 + 1` fails.

## Root Cause
Session 016 changed `_current_generation`, `_advance_generation`, and `_superedge_invalidate` to store/read `Generation` enum in the cache, and updated most tests to downcast to `Generation` then call `.as_u64()`. Six call sites were missed:
- `crates/daf-application/tests/integration_tests.rs:310`
- `crates/daf-application/tests/integration_tests.rs:663`
- `crates/daf-application/tests/integration_tests.rs:679`
- `crates/daf-application/tests/integration_tests.rs:959`
- `crates/daf-application/tests/integration_tests.rs:973`
- `crates/daf-application/tests/integration_tests.rs:1207`

## Fixes

### Fix 1: Update all generation downcasts in integration tests
Replace every:
```rust
.and_then(|v| v.value.downcast_ref::<u64>().copied())
.unwrap_or(0)
```
with:
```rust
.and_then(|v| {
    v.value
        .downcast_ref::<daf_core::Generation>()
        .copied()
        .and_then(|g| g.as_u64())
})
.unwrap_or(0)
```

### Fix 2: Correct concurrent mutation test assertion
In `test_concurrent_mutations_generation_monotonic`, change:
```rust
assert!(gen >= 2);
```
to:
```rust
assert!(gen >= 1);
```
Rationale: `MemoryRepository::try_update` uses CAS; only one of two concurrent `put` calls with the same expected value can succeed. The successful mutation advances generation by exactly 1. The test name validates monotonicity, not count.

### Fix 3: Fix Moka `shake` empty-prefix accounting
In `crates/daf-cache/src/moka.rs`, change `shake` to return the pre-invalidation entry count for empty prefix:
```rust
async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
    if prefix.is_empty() {
        let count = self.inner.entry_count() as usize;
        self.inner.invalidate_all();
        Ok(count)
    } else {
        Ok(0)
    }
}
```
Rationale: Red team flagged `invalidate_all(); return 0` as false accounting. Moka 0.12 provides `entry_count()`.

### Fix 4: Verify CI parity gate wiring
`.github/workflows/ci.yml` already has `parity` in `build.needs`. No change needed, but verify after edits.

## Validation
1. `cargo test --workspace` — 29/29 passing
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — 0 warnings
3. `cargo fmt --check` — passes
4. `uv run pytest tests/ -q` — 212/212 passing

## Out of Scope
- Full transactional cache invalidation (design decision: best-effort with generation check)
- Complete FFI ABI (filters, algorithms, payloads)
- Real Redis/Postgres backends
- Additional adversarial property tests (beyond what session 016 added)
