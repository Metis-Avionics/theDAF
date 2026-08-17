# Tier-aware Cache Hierarchy, Parity Tests, and CI Plan

## Goal
Close the remaining Python–Rust parity gaps: implement the chosen Tier-aware Cache hierarchy (Option C), add `DataAccessFactory`, align/document `try_update` equality semantics, add Python-side parity tests, update CI for Rust, and port remaining Python test patterns.

## Constraints
- Python is the semantic reference; Rust must preserve externally observable semantics.
- `daf-core` has zero framework dependencies.
- Cache hierarchy: L1 → L2 → L3 → L4; miss continues down; persistent store is authoritative.
- Do not modify Python implementation; add Python tests only.

---

## Task 1: Tier-aware Cache Hierarchy (Option C)

### 1.1 Define `Tier` and `CacheEntry` in `daf-core`

Add to `crates/daf-core/src/lib.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    L1,
    L2,
    L3,
    L4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub value: Arc<dyn Any + Send + Sync>,
    pub tier: Tier,
}
```

### 1.2 Update `Cache` trait return type

Change `get` signature:
```rust
async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError>;
```

All other methods (`set`, `delete`, `delete_prefix`, `shake`, `clear`) keep their current signatures.

### 1.3 Update `MemoryCache` (`daf-cache`)

- `get` returns `Ok(Some(CacheEntry { value, tier: Tier::L1 }))` on hit, `Ok(None)` on miss.
- `set` wraps incoming `Arc<dyn Any>` in `CacheEntry` with `Tier::L1` before storing.
- Update all internal reads/writes to work with `CacheEntry` instead of raw `Arc<dyn Any>`.

### 1.4 Implement L2 `MokaCache`

New file `crates/daf-cache/src/moka.rs`:
- Wraps `moka::future::Cache<String, CacheEntry>`
- `get` returns `Ok(Some(entry))` with `tier: Tier::L2` on hit
- `set` stores with `Tier::L2`

### 1.5 Implement L3 `RedisCache` (stub)

New file `crates/daf-cache/src/redis.rs`:
- Stub with `redis::Client` field; compile without `redis` feature by default.
- `get`/`set` return `CacheEntry { tier: Tier::L3 }` when feature `redis` is enabled; otherwise return `CacheError`.

### 1.6 Implement L4 `PostgresCache` (stub)

New file `crates/daf-cache/src/postgres.rs`:
- Stub with `sqlx::PgPool` field; compile without `postgres` feature by default.
- `get`/`set` return `CacheEntry { tier: Tier::L4 }` when feature `postgres` is enabled; otherwise return `CacheError`.

### 1.7 Implement `HierarchicalCache`

New file `crates/daf-cache/src/hierarchical.rs`:
- Fields: `l1: Arc<dyn Cache>`, `l2: Arc<dyn Cache>`, `l3: Arc<dyn Cache>`, `l4: Arc<dyn Cache>`
- `get(key)` queries L1 → L2 → L3 → L4 in order, returns first hit with its tier preserved.
- `set(key, value)` writes to L1 only (write-through policy: caller decides which tier to populate).
- `delete`/`delete_prefix`/`shake` propagate to all tiers.
- `clear` propagates to all tiers.

### 1.8 Update `DataAccess` (`daf-application`)

- `_current_generation`, `_advance_generation`, `_superedge_invalidate` handle `CacheEntry` via `.value` field.
- `_execute_cache_miss` and `_handle_cache_hit` unwrap `CacheEntry.value`.
- `cache_key` generation is unchanged.

### 1.9 Update `daf-http` and `daf-ffi`

- `daf-http` handlers unchanged (they call `DataAccess` which now handles `CacheEntry` internally).
- `daf-ffi` unchanged.

---

## Task 2: `DataAccessFactory`

Add to `crates/daf-application/src/lib.rs`:

```rust
pub struct DataAccessFactory {
    repository: Arc<dyn Repository<JsonValue>>,
    cache: Arc<dyn Cache>,
    algorithms: Option<HashMap<String, Arc<dyn Algorithm>>>,
    authorizer: Option<Arc<dyn Authorizer>>,
}

impl DataAccessFactory {
    pub fn new(
        repository: Arc<dyn Repository<JsonValue>>,
        cache: Arc<dyn Cache>,
        algorithms: Option<HashMap<String, Arc<dyn Algorithm>>>,
        authorizer: Option<Arc<dyn Authorizer>>,
    ) -> Self { ... }

    pub fn create(self) -> DataAccess { ... }
}
```

---

## Task 3: `try_update` Equality Semantics

### 3.1 Current state
- Python: identity (`is`) + dict equality for dicts under `threading.Lock`.
- Rust: JSON serialization round-trip equality in `MemoryRepository::values_equal`.

### 3.2 Decision
For `JsonValue` specifically, `serde_json::Value` already implements `PartialEq`. Update `MemoryRepository` to use `PartialEq` directly when `T: PartialEq`, falling back to JSON serialization only when `T` does not implement `PartialEq`.

Add a trait bound check:
```rust
fn values_equal<T: PartialEq + Serialize>(a: &T, b: &T) -> bool { a == b }
```

For non-PartialEq types, keep JSON fallback. Document that Rust equality is structural (`PartialEq`) while Python is identity-first.

---

## Task 4: Python-side Parity Tests

Add `tests/unit/test_rust_parity.py`:

1. **Contract round-trip**: Serialize/deserialize every `daf.contracts.query` model and assert field preservation.
2. **Trie traversal**: Mirror Rust `traversal_tests.rs` cases using `daf.cache.memory.MemoryCache._trie_collect` and `_trie_delete_prefix`.
3. **FibonacciDP with Arc-like inputs**: Verify `FibonacciDP.execute` accepts `int` and returns correct result; verify `get_stats` shape matches Rust `AlgorithmStats`.
4. **Generation advancement**: Verify post/put/delete advance generation counter in cache.
5. **Cache invalidation**: Verify `delete_prefix` clears query projections after mutation.

---

## Task 5: CI Updates

Add Rust jobs to `.github/workflows/ci.yml`:

```yaml
  rust-lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  rust-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace

  parity:
    runs-on: ubuntu-latest
    needs: [rust-test, test]
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: uv run pytest tests/ -q
      - run: cargo test --workspace
```

---

## Task 6: Port Python Test Patterns to Rust

Add Rust tests mirroring Python unit tests:

| Python file | Rust target | Tests to port |
|---|---|---|
| `test_contracts.py` | `daf-core/tests/contract_tests.rs` | Serde round-trip for all models, field assertions |
| `test_recursion.py` | `daf-cache/tests/traversal_tests.rs` | DFS/BFS collect, walk_tree equivalent via trie DFS |
| `test_memoize.py` | `daf-algorithms/tests/fibonacci_tests.rs` | Memo hit/miss, stats shape, clear resets |

### 6.1 `daf-core/tests/contract_tests.rs` — extend
- Add `Generation::Missing` serialization round-trip.
- Add `QueryInfo` with empty filters/algorithm defaults.
- Add `AlgorithmStats` serde round-trip.

### 6.2 `daf-cache/tests/traversal_tests.rs` — extend
- Add `MemoryCache::set` + `get` round-trip for `CacheEntry` with `Tier::L1`.
- Add `delete_prefix` integration with `MemoryCache`.
- Add `shake` count verification.

### 6.3 `daf-algorithms/tests/fibonacci_tests.rs` — extend
- Add `execute` with `Arc<i64>` input (matching `Arc<dyn Any>` trait signature).
- Add `get_stats` after multiple executes.
- Add `execute` with `n=0` and `n=1` edge cases.

### 6.4 `daf-application/tests/integration_tests.rs` — extend
- Add `DataAccessFactory::create` test.
- Add `test_post_then_query_returns_fresh_data` (cache miss after mutation).
- Add `test_concurrent_queries_share_cache_hit`.
- Add `test_generation_missing_initializes_to_zero_on_miss`.

### 6.5 New file `daf-application/tests/factory_tests.rs`
- Test factory stores deps and `create()` returns `DataAccess` with those deps.
- Test `get_components()` round-trip.

---

## Task 7: `daf-core` Contract Tests in CI

- Add a `daf-core-contract` job to CI that runs only `cargo test -p daf-core` to catch field drift early.

---

## Validation Steps

1. `cargo fmt --check` passes.
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
3. `cargo test --workspace` passes (target: all existing + new tests).
4. `cargo test -p daf-core` passes contract tests in isolation.
5. `uv run pytest tests/ -q` passes (Python tests including new parity tests).
6. CI pipeline with all new jobs passes on a test branch.

---

## Risks

| Risk | Mitigation |
|---|---|
| `CacheEntry` wrapping breaks downstream consumers | Update all internal consumers in one pass; add compile-fail test if any `Cache::get` consumer is missed |
| L3/L4 stubs increase binary size | Gate behind Cargo features (`redis`, `postgres`); default off |
| `PartialEq` fallback may silently diverge for complex `T` | Document that `MemoryRepository` equality is best-effort; production backends should use proper CAS |
| `DataAccessFactory` is trivial; risk is over-engineering | Keep minimal: store deps, single `create()` method |

---

## Out of Scope
- Implementing real Redis/Postgres cache backends (stubs only).
- Rate limiting in `daf-http` (Python uses slowapi; Rust does not).
- `src/thedaf/`, `daf-runtime`, `daf-messaging`, `daf-ffi` enhancements beyond compilation.
- Python implementation changes.
