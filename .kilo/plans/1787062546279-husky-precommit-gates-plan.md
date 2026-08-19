# Husky Precommit + CI Gates Plan

## Goal

Add local precommit gates and harden CI so that the 12 issues found in PR30 (and similar future issues) are caught before merge, not after.

## Scope

- **In scope:** Husky precommit setup, CI workflow hardening, new/updated lint scripts, documentation.
- **Out of scope:** Fixing the issues themselves (already tracked as #31–#42). This plan only prevents recurrence.

## Problem Summary

PR30 review found issues across 4 categories:
1. **Soundness** — unsound FFI lifetimes, dead code with panicking debug asserts
2. **API drift** — breaking trait-bound changes, missing validation on constructors
3. **Logic** — no-op functions, meaningless assertions, unconditional invalidation
4. **CI gaps** — missing `--all-features`, silent test skips, per-command runtime allocation

Existing CI already runs ruff, mypy, clippy, fmt, pytest, cargo test, power-of-ten, and parity tests. The gap is **local enforcement** — developers can push broken code and only discover it in CI.

## Proposed Solution: Tiered Gates

### Tier 1 — Precommit (must be < 10s or devs will bypass)

| Gate | Command | Catches |
|------|---------|---------|
| Python lint | `uv run ruff check src/ tests/` | Style, unused imports, obvious bugs |
| Rust fmt | `cargo fmt -- --check` | Formatting drift |
| Typecheck | `uv run mypy src/ --strict` | Type errors, missing stubs |
| Staged-file scope | Run only on changed `.py` / `.rs` files | Speed |

**Rationale:** These are fast, deterministic, and already configured in the project. Running them locally catches the majority of trivial issues before they reach CI.

### Tier 2 — Pre-push (can be 30–60s)

| Gate | Command | Catches |
|------|---------|---------|
| Rust clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Logic bugs, dead code, unsound patterns |
| Power of Ten (Python) | `python scripts/power_of_ten.py src/` | Recursion, function length, assertion density |
| Power of Ten (Rust) | `python scripts/power_of_ten_rust.py` | Same for Rust |
| Targeted pytest | `uv run pytest tests/unit/ -q` | Unit regressions |

**Rationale:** Slower but still acceptable before push. Catches the structural issues PR30 found (e.g., meaningless `debug_assert!`, no-op functions, recursion).

### Tier 3 — CI-only (heavy, runs on PR)

| Gate | Command | Catches |
|------|---------|---------|
| Full test suite | `cargo test --workspace --all-features` + `pytest tests/` | Integration regressions |
| Parity differential | Build `daf-parity`, run `test_differential_parity.py` | Python/Rust behavioral divergence |
| Build | `uv build` | Packaging regressions |

**Rationale:** These are too slow for local hooks and must stay in CI.

## New / Updated Checks

### 1. Fix CI `rust-test` to use `--all-features`
- **File:** `.github/workflows/ci.yml`
- **Change:** `cargo test --workspace` → `cargo test --workspace --all-features`
- **Why:** PR30 found that feature-gated modules (`redis`, `postgres`) are not compiled in CI.

### 2. Parity binary should fail CI on build error
- **File:** `tests/unit/test_differential_parity.py`
- **Change:** Replace `pytest.skip()` on binary build failure with `pytest.fail()` or re-raise the subprocess error.
- **Why:** Silent skips mask Rust build regressions.

### 3. Parity binary runtime reuse
- **File:** `crates/daf-ffi/src/bin/parity.rs`
- **Change:** Create one `tokio::runtime::Runtime` at the top of `main()`, reuse for all commands.
- **Why:** Per-command allocation is a performance defect that should be caught by benchmarks or code review.

### 4. Remove unused `tracing` dependency
- **File:** `crates/daf-application/Cargo.toml`
- **Change:** Remove `tracing = "0.1"` or add instrumentation.
- **Why:** Unused deps bloat build time and Cargo.lock.

### 5. Fix `daf-runtime` dead `RUNTIME` static
- **File:** `crates/daf-runtime/src/lib.rs`
- **Change:** Remove `RUNTIME` static and spurious debug asserts, or initialize properly.
- **Why:** Dead code that panics in debug builds is a landmine.

## Implementation Steps

1. **Add husky to `package.json`** and initialize `.husky/`
2. **Create `.husky/pre-commit`** with Tier 1 commands
3. **Create `.husky/pre-push`** with Tier 2 commands
4. **Update `.github/workflows/ci.yml`**:
   - Add `--all-features` to `rust-test`
   - Ensure parity build failures fail the job
5. **Add `Makefile`** with lint/test shortcuts for developers who prefer `make lint` over husky
6. **Document** in `CONTRIBUTING.md` or `README.md`: how to bypass hooks (`--no-verify`), how to run checks manually, expected timing

## Rollout

1. Add husky and hooks to feature branch
2. Verify all hooks pass on current `main`
3. Open PR with note: "If hooks are too slow, tell us — we will tune thresholds"
4. Merge and announce to team

## Validation

- `git commit` on a dirty tree with intentional ruff violation → blocked
- `git commit` with intentional clippy warning → blocked at pre-push
- `cargo test --workspace --all-features` passes in CI
- Parity test fails if `daf-parity` binary is broken

## Open Question

**Q:** Should Tier 2 (clippy + power-of-ten + unit tests) run on every `git push`, or only on `git commit`?

**Recommended answer:** Run Tier 2 on `pre-push`, not `pre-commit`. Developers commit frequently; pushing is the natural "ready for review" gate. This keeps the commit experience fast while still catching issues before they reach CI.
