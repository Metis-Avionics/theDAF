# Test Caching + Manifest-Based Test Plan

## Goal

Reduce test runtime and maintenance burden by adding caching and introducing a manifest-based test format for parity tests.

## Current State

- Python tests use pytest with no caching (`asyncio_mode = "auto"`, no xdist, no cacheprovider)
- Rust tests use `cargo test --workspace` without `--all-features`
- `tests/unit/test_differential_parity.py` rebuilds Rust binary on every run and silently skips on failure
- `crates/daf-ffi/src/bin/parity.rs` creates new Tokio runtime per command
- CI has 8 jobs; no test result caching

## Proposed Changes

### 1. pytest-cacheprovider for Python tests

**File:** `pyproject.toml`
- Add `pytest-cacheprovider` to dev dependencies
- Enable `cacheprovider` in `[tool.pytest.ini_options]`

**Benefit:** Re-run only failed/changed tests within a session; cache expensive fixtures.

### 2. Manifest-based parity tests

**File:** `tests/unit/test_differential_parity.py`
- Extract test cases into a JSON manifest (`tests/unit/parity_manifest.json`)
- Each entry: `{ "name": "...", "ops": [ { "op": "post", ... }, ... ], "assertions": [...] }`
- Single parametrized test loads manifest, runs ops, checks assertions
- **Benefit:** Add new parity cases without touching Python test logic; easier diff/review.

### 3. Fix parity binary build caching

**File:** `tests/unit/test_differential_parity.py`
- Build binary once per session, not per test
- Reuse `parity_proc` fixture across tests in a class
- Fail CI on build error instead of `pytest.skip()`

### 4. Fix parity runtime reuse

**File:** `crates/daf-ffi/src/bin/parity.rs`
- Create one `tokio::runtime::Runtime` at top of `main()`
- Reuse for all commands instead of per-command `Runtime::new()`

### 5. CI rust-test fix

**File:** `.github/workflows/ci.yml`
- Change `cargo test --workspace` → `cargo test --workspace --all-features`

## Implementation Steps

1. Add `pytest-cacheprovider` to `pyproject.toml` dev dependencies
2. Enable cacheprovider in pytest config
3. Create `tests/unit/parity_manifest.json` with existing test cases
4. Refactor `test_differential_parity.py` to use manifest
5. Fix parity binary build caching and error handling
6. Fix `parity.rs` runtime reuse
7. Fix CI `rust-test` job
8. Validate all tests pass

## Out of Scope

- Fixing other PR30 issues (already tracked as #31-#42)
- Husky precommit hooks (separate plan)

## Risks

- Manifest format adds a file to maintain; keep it minimal
- pytest-cacheprovider is lightweight but adds a dependency

## Validation

- `uv run pytest tests/unit/test_differential_parity.py -v` passes
- `cargo test --workspace --all-features` passes
- CI passes with updated workflow
