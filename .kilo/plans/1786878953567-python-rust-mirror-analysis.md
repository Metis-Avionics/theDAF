# Python to Rust Mirror Analysis Plan

## Goal
Systematically map every Python symbol, behavior, contract, and test to its Rust counterpart, identify divergences, and produce a structured report of function lists, polymorphism models, shared contracts, and behavioral gaps.

## Scope
- **Source of truth**: Python `src/daf/` (primary API + implementations)
- **Target**: Rust `crates/` (parallel reimplementation)
- **Out of scope**: `src/thedaf/`, `scripts/graphify_*`, Rust-only crates (`daf-runtime`, `daf-ffi`, `daf-messaging`) unless they expose parity-relevant APIs.

## Analysis Steps

1. **Module-to-Crate Mapping**
   - Enumerate every Python module path under `src/daf/` and identify the matching Rust crate/file.
   - Produce a table: `Python path → Rust crate/file`.

2. **Public API Surface (Symbol Inventory)**
   - For each Python module, list all public classes, functions, type aliases, and constants.
   - For each, locate the equivalent Rust item (struct, enum, trait, function, type alias).
   - Note any Python items with no Rust equivalent and any Rust items with no Python equivalent.

3. **Polymorphism / Trait-Protocol Comparison**
   - Compare Python protocols (`Repository`, `Cache`, `Algorithm`, `Authorizer`) against Rust traits.
   - Document signature-by-signature differences: parameter types, return types, async handling, bounds (`Send + Sync`), generics.
   - Note Python's structural subtyping (Protocol) vs Rust's explicit trait implementation.

4. **Contracts / Types Comparison**
   - Compare Python Pydantic models (`QueryInfo`, `PostInfo`, `PutInfo`, `DeleteInfo`, `QueryResult`, `MutationResult`, `AlgorithmStats`) against Rust structs.
   - Document field-level mapping, default values, serialization, and newtype wrappers (`ResourceId`, `UserId`).

5. **Behavioral Semantics Comparison**
   - Ownership model: Python `copy.deepcopy()` vs Rust `Arc` / `Clone`.
   - Concurrency primitives: Python `asyncio.Lock` + `asyncio.Lock` memo vs Rust `tokio::sync::Mutex` + `lru::LruCache`.
   - Cache keying: Python `_cached_key` (functools `@cache`) vs Rust `cache_key` (SHA-256 inline).
   - Error hierarchy: Python exception classes vs Rust `thiserror::Error` enum + structs.
   - Generation tracking: Python inline in `DataAccess` vs Rust `Generation` enum.

6. **DataAccess Orchestration Layer**
   - Compare method-by-method: `__init__`, `query`, `post`, `put`, `delete`, and all private helpers (`_execute_cache_miss`, `_handle_cache_hit`, `_superedge_invalidate`, `_advance_generation`, `_run_algorithm`, `_apply_filters`, `_user_id`, `_resource_namespace`, etc.).
   - Document any control-flow or security-model differences.

7. **Utility / Infrastructure Comparison**
   - `Memo` / `ResourceMemo` (Python) vs any Rust equivalent.
   - `TreeCollector`, `collect_tree`, `walk_tree` (Python) vs Rust trie traversal functions.
   - Trie implementations: Python `_TrieNode` + helpers vs Rust `TrieNode` + helpers.

8. **Implementation Divergences**
   - Record Rust-only additions (e.g., `AlgorithmStats`, `Generation` enum, `UserId`, `ResourceId`, FFI layer, Axum adapter).
   - Record Python-only additions (e.g., `_barrel._public`, FastAPI adapter, `src/thedaf/`).
   - Note any semantic differences (e.g., Rust `MemoryRepository` uses `ulid`, Python uses `uuid4`; Rust `values_equal` serializes to JSON, Python uses `is` + dict equality).

9. **Test Parity Check**
   - Map Python tests in `tests/` to Rust tests in `crates/*/tests/` and inline tests.
   - List Python tests with no Rust equivalent and vice versa.

10. **Output Document**
    - Write the analysis to `/workspaces/theDAF/.kilo/plans/1786878953567-python-rust-mirror-analysis.md`.
    - Sections: Module Map, Public API Table, Polymorphism Table, Contracts Table, Behavioral Gaps, Orchestration Method Map, Test Parity Table, Divergences.

## Validation
- Cross-check every Rust trait method against the Python `Protocol` method.
- Cross-check every Rust struct field against the Python `BaseModel` field.
- Verify that all orchestration methods in `DataAccess` exist on both sides.

## Risks / Unknowns
- Python `MemoryRepository.try_update` uses identity comparison (`is`) for dicts; Rust `MemoryRepository.values_equal` serializes to JSON. **Resolved in step 8.**
- Python `ResourceMemo` is a general lazy-init cache; Rust `GenerationLocks` is specialized for mutex generation locks. **Resolved in step 5.**
- Python `Cache.set` does `copy.deepcopy`; Rust `MemoryCache.set` clones the `Arc` without deep cloning. **Resolved in step 5.**
