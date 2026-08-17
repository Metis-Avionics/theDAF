# Rust Power of Ten Violation Remediation

## Context
Session 018 added `scripts/power_of_ten_rust.py`. First run found 78 violations across 12 files. This plan resolves all real violations.

## Violation Summary

| Rule | Count | Files |
|------|-------|-------|
| 1 (unsafe outside FFI) | 4 blocks | `daf-cache/src/trie.rs` |
| 4 (>60 line fn) | 2 fn | `daf-application/src/lib.rs` |
| 5 (<2 assertions) | all fn | all 12 files (codebase-wide) |
| 7 (.unwrap()/.expect()) | 8 calls | `daf-cache/src/lib.rs`, `daf-cache/src/trie.rs`, `daf-runtime/src/lib.rs` |
| 9 (raw pointers outside FFI) | 4 uses | `daf-cache/src/trie.rs` |
| 6 (mutable scope) | 0 | — (false positives only) |

Rule 6 violations are all false positives (`static` + `OnceLock`/`RefCell` is correct Rust interior mutability).

## Execution Order

### Task 1: Refactor `trie.rs` to safe Rust (Rules 1, 7, 9)
**File:** `crates/daf-cache/src/trie.rs`

Replace all `unsafe` pointer manipulation with safe Rust using `Vec<usize>` indices into a `Vec<TrieNode>` arena, or use safe `HashMap` traversal without raw pointers.

Concretely:
- `trie_delete`: replace `Vec<*mut TrieNode>` path with `Vec<usize>` indices into arena, or restructure to avoid backtracking with raw pointers
- `trie_delete_prefix`: same refactor
- Remove all `.unwrap()` calls that are logically guarded (lines 13, 33, 49, 87, 105, 186, 188) — replace with `expect()` with descriptive message or safe access pattern
- Remove `unsafe` blocks entirely

### Task 2: Decompose long functions in `daf-application/src/lib.rs` (Rule 4)
**File:** `crates/daf-application/src/lib.rs`

Extract helpers from:
- `_execute_cache_miss` (66 lines) → extract serialization logic into `_serialize_generation` and cache-key construction into `_build_cache_entry`
- `put` (62 lines) → extract `_execute_put_impl` or `_build_put_info`

### Task 3: Add assertions across all crates (Rule 5)
Add `debug_assert!` or `assert!` to non-trivial functions (>35 lines) that lack them. Priority:
- Constructors: add `debug_assert!` on input validity
- Core logic: add `debug_assert!` on invariants
- At minimum, add assertion-density to `daf-core`, `daf-application`, `daf-cache`, `daf-algorithms`, `daf-repository`, `daf-http`, `daf-runtime`, `daf-messaging`

### Task 4: Fix unguarded `.expect()` (Rule 7)
**File:** `crates/daf-runtime/src/lib.rs:32`
- Replace `Runtime::new().expect(...)` with `Runtime::new().map_err(...)?` or propagate error

**File:** `crates/daf-cache/src/lib.rs:45`
- Replace `NonZeroUsize::new(max_size).unwrap()` with `NonZeroUsize::new(max_size).expect("max_size must be > 0")` (already guarded by `if max_size > 0`, but use `expect` for clarity)

**File:** `crates/daf-cache/src/trie.rs`
- Replace guarded `.unwrap()` calls with `.expect("descriptive message")` after Tasks 1 refactor

### Task 5: Validation
- Run `python scripts/power_of_ten_rust.py` — expect 0 violations
- Run `cargo test --workspace` — expect all tests pass
- Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` — expect 0 warnings

## Out of Scope
- Rule 6: no real violations to fix
- Rule 2/3/8: no violations found
- Rule 10 (zero warnings): delegated to clippy CI job
