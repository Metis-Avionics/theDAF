# theDAF → theDAF-LLVM: Architectural Translation Plan

## Status

Implementation-ready. No outstanding blocking decisions.

## Goal

Translate theDAF's stabilized Python semantic architecture into a modular Rust workspace while preserving externally observable semantics, modularity, and architectural boundaries. Rust should make implicit Python guarantees explicit through ownership, borrowing, traits, enums, typed errors, explicit concurrency, and bounded resources.

## Critical Guardrail

The Python code is the **semantic reference**, not the target. Do not optimize Python during migration. When a Python bug or sentinel-value pattern is discovered, record it, model the correct Rust semantics, and create a migration issue. Modify Python only when explicitly required.

## Phase 0: Semantic Freeze

**Decision**: The Python implementation is frozen as the semantic reference. No further Python changes without a linked migration ADR.

**Action**: Create ADR-001 "Semantic Freeze & Reference Boundary" documenting:
- Python is the semantic reference for all observable behavior
- No Python changes without explicit migration ADR
- Test suite is the ground truth for equivalence

## Phase 1: Extract Semantic Model

Produce the following extraction artifacts **before** writing Rust:

| Artifact | Source | Purpose |
|---|---|---|
| `docs/adr/adr-001-semantic-freeze.md` | team decision | Reference boundary |
| `docs/adr/adr-002-generation-state-model.md` | `access.py` | Generation semantics discrepancy |
| `docs/contracts/function-matrix.md` | `access.py`, protocols | Every public method's inputs/outputs/errors/side-effects |
| `docs/contracts/polymorphism-matrix.md` | `protocols.py` | Python Protocol → Rust trait mapping |
| `docs/contracts/error-taxonomy.md` | `errors.py` | Typed error hierarchy |
| `docs/contracts/cache-semantics.md` | `memory.py`, `access.py` | L1-L4 hierarchy, hit/miss, invalidation, generation |
| `docs/architecture/crate-dependency-graph.md` | `theDAF-LLVM.toml` | Dependency direction validation |

### Key Semantic Extraction: Generation State Model

**Discrepancy found**: In Python, `_execute_cache_miss` catches `GenerationKeyError` and falls back to `current_generation = 0`, writing `0` to the cache. The TOML spec states `missing_generation_must_not_equal = 0`.

**Resolution**: Document this as ADR-002. In Rust, generation state is modeled as an explicit enum:

```rust
enum Generation {
    Missing,
    Valid(u64),
}
```

`Generation::Missing` semantically means "no generation established yet." `Generation::Valid(0)` means "generation is 0." These are distinct states. The cache miss path initializes to `Generation::Missing`, then on first write advances to `Generation::Valid(0)`. The Rust type system prevents the conflation.

### Key Semantic Extraction: Error Taxonomy

Python `QueryResult` / `MutationResult` carry `error: str | None` and `error_type: str | None`. These are stringly-typed.

**Resolution**: Rust result types carry a proper enum:

```rust
enum QueryError {
    NotFound,
    AuthorizationFailed,
    ValidationFailed { message: String },
    UnknownAlgorithm { name: String },
    CacheError(CacheError),
    RepositoryError(RepositoryError),
}
```

`MutationResult` has `Ok` and `Conflict` variants instead of `success: bool` + stringly error fields.

### Key Semantic Extraction: Deep-Copy Isolation

Python uses `copy.deepcopy()` at every ownership boundary (repo get/set, cache get/set).

**Resolution**: Rust uses ownership and `Clone` at boundaries. For the in-memory tier (`daf-cache` L1, `daf-repository`), values are stored as `Arc<T>` and cloned on read (cheap `Arc` clone, not deep copy). For persistence tiers, serialization round-trips enforce isolation. The semantic contract—"callers must not mutate returned values in-place"—is enforced by Rust's borrow checker: returned references are either immutable `&T` or owned `T`.

## Phase 2: Workspace Setup

Create a Rust workspace at the repo root:

```
/workspaces/theDAF/
├── Cargo.toml          # workspace root
├── crates/
│   ├── daf-core/
│   ├── daf-application/
│   ├── daf-cache/
│   ├── daf-repository/
│   ├── daf-algorithms/
│   ├── daf-runtime/
│   ├── daf-messaging/
│   ├── daf-http/
│   └── daf-ffi/
└── tests/              # workspace-level integration tests
```

**Dependencies**: None in `daf-core`. `daf-application` depends on `daf-core`. `daf-cache` and `daf-repository` depend on `daf-core`. `daf-http` and `daf-ffi` depend on `daf-core` + `daf-application`.

**Stack**: Tokio (async), Axum (HTTP), Rayon (CPU parallelism), Apalis (messaging), Cachelito (L1), Moka (L2), Redis/Valkey (L3), Postgres/HelixDB (L4). All framework dependencies are in interface/infrastructure crates only.

## Phase 3: Core Crate (`daf-core`)

Implement first. No framework dependencies.

### Types

```rust
// ResourceId — newtype around String for type safety
pub struct ResourceId(pub String);

// UserId — newtype; FFI boundary accepts opaque pointer
pub struct UserId(pub String);
```

### Traits (from Python Protocols)

```rust
#[async_trait]
pub trait Repository<T>: Send + Sync {
    async fn get(&self, key: &ResourceId) -> Result<Option<Arc<T>>, RepositoryError>;
    async fn save(&self, key: &ResourceId, value: T) -> Result<(), RepositoryError>;
    async fn delete(&self, key: &ResourceId) -> Result<(), RepositoryError>;
    async fn create(&self, value: T) -> Result<ResourceId, RepositoryError>;
    async fn try_update(
        &self,
        key: &ResourceId,
        expected: &T,
        update: impl FnOnce(T) -> T + Send,
    ) -> Result<Option<T>, RepositoryError>;
    async fn try_delete(&self, key: &ResourceId, expected: &T) -> Result<bool, RepositoryError>;
}

#[async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Arc<dyn Any + Send + Sync>>, CacheError>;
    async fn set(&self, key: String, value: Arc<dyn Any + Send + Sync>) -> Result<(), CacheError>;
    async fn delete(&self, key: &str) -> Result<(), CacheError>;
    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError>;
    async fn shake(&self, prefix: &str) -> Result<usize, CacheError>;
    async fn clear(&self) -> Result<(), CacheError>;
}

#[async_trait]
pub trait Algorithm: Send + Sync {
    async fn execute(&self, input: Arc<dyn Any + Send + Sync>) -> Result<Arc<dyn Any + Send + Sync>, AlgorithmError>;
    async fn get_stats(&self) -> Result<AlgorithmStats, AlgorithmError>;
}

#[async_trait]
pub trait Authorizer: Send + Sync {
    async fn authorize(
        &self,
        operation: &str,
        resource_id: Option<&ResourceId>,
        user: &UserId,
        data: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Result<(), AuthorizationError>;
}
```

### Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum DataAccessError {
    #[error("resource not found")]
    NotFound(#[from] NotFoundError),
    #[error("validation failed: {message}")]
    Validation { message: String },
    #[error("repository error")]
    Repository(#[from] RepositoryError),
    #[error("cache error")]
    Cache(#[from] CacheError),
    #[error("generation key missing or malformed")]
    GenerationKeyError,
    #[error("algorithm error")]
    Algorithm(#[from] AlgorithmError),
    #[error("authorization failed")]
    Authorization(#[from] AuthorizationError),
}
```

Sub-types: `RepositoryError`, `CacheError`, `AlgorithmError`, `AuthorizationError`, `NotFoundError`, `ValidationError`.

### Contracts

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryInfo {
    pub resource_id: ResourceId,
    pub filters: Option<HashMap<String, serde_json::Value>>,
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryResult {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub error_type: Option<String>,
    pub cache_hit: bool,
    pub algorithm_stats: Option<AlgorithmStats>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

Note: `error` / `error_type` fields are preserved as `Option<String>` for ABI compatibility, but the Rust API uses `Result<T, QueryError>`.

## Phase 4: Cache Crate (`daf-cache`)

### L1 — Cachelito (process-local, ultra-low-latency)

```rust
pub struct CachelitoCache {
    inner: cachelito::Cache<String, Arc<dyn Any + Send + Sync>>,
}
```

Implements `daf_core::Cache`. No prefix trie needed—cachelito provides its own eviction. Prefix operations are delegated to L2 or handled via iteration.

### L2 — Moka (process-local, bounded)

```rust
pub struct MokaCache {
    inner: moka::future::Cache<String, Arc<dyn Any + Send + Sync>>,
}
```

### L3 — Redis/Valkey

```rust
pub struct RedisCache {
    client: redis::Client,
}
```

Implements prefix delete via `SCAN` + `DEL` pipeline.

### L4 — Postgres/HelixDB

```rust
pub struct PostgresCache {
    pool: sqlx::PgPool,
}
```

Authoritative persistent store. Cache hierarchy rule: `L1 -> L2 -> L3 -> L4`, miss continues down, persistent store is authoritative.

### Trie (L2 MemoryCache prefix optimization)

Direct translation of `_trie.py` to Rust `HashMap`-based trie. Standalone, no `daf` imports. Used by `MemoryCache` for O(prefix_length + K) prefix ops.

```rust
pub struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub key: Option<String>,
}
```

## Phase 5: Repository Crate (`daf-repository`)

### MemoryRepository

```rust
pub struct MemoryRepository<T: Clone + Send + Sync + 'static> {
    store: Arc<RwLock<HashMap<String, T>>>,
}
```

Uses `Arc<RwLock<HashMap>>` for concurrent access. `try_update` / `try_delete` use equality comparison under write lock. Deep-copy isolation is achieved via `Clone` on `T` (caller receives clone, stored value is not shared).

### Abstractions

`Repository` trait from `daf-core`. Implementations for memory and (later) persistence backends.

## Phase 6: Algorithms Crate (`daf-algorithms`)

### FibonacciDP

```rust
pub struct FibonacciDp {
    memo: Option<std::collections::HashMap<u64, u64>>,
    iterations: u64,
    cache_hits: u64,
}

#[async_trait]
impl Algorithm for FibonacciDp {
    async fn execute(&self, input: Arc<dyn Any + Send + Sync>) -> Result<Arc<dyn Any + Send + Sync>, AlgorithmError> {
        // ... translate execute logic
    }
    async fn get_stats(&self) -> Result<AlgorithmStats, AlgorithmError> {
        // ...
    }
}
```

Memoization uses `HashMap` instead of custom `Memo` class. Stats are a typed struct.

## Phase 7: Application Crate (`daf-application`)

`DataAccess` orchestrator. This is the core translation target.

### Key Design Decisions

1. **Concurrency**: Per-resource locks use `tokio::sync::Mutex` keyed by `ResourceId`. A bounded LRU (from `lru` crate or custom) provides lazy-init lock caching (replaces `ResourceMemo`). Max size: 256 (matches Python).

2. **Generation model**: Explicit `Generation` enum, not sentinel `0`. Missing generation is a distinct state.

3. **Cache key**: SHA-256 of canonical JSON payload + resource namespace. Deterministic—no memoization needed. Translation of `_cached_key` is a pure function.

4. **Authorization**: `Authorizer` trait with `UserId` parameter. The closure-based ownership authorizer from `fastapi.py` becomes an implementor of `Authorizer`.

5. **Error handling**: Rust API returns `Result<T, DataAccessError>`. The `QueryResult` / `MutationResult` structs carry `Option<String>` for error fields to preserve the external contract shape.

### DataAccess structure

```rust
pub struct DataAccess<R, C, A> {
    repository: Arc<R>,
    cache: Arc<C>,
    algorithms: Arc<HashMap<String, Arc<dyn Algorithm>>>,
    authorizer: Option<Arc<dyn Authorizer>>,
    generation_locks: LruCache<ResourceId, Arc<tokio::sync::Mutex<()>>>, // or custom bounded LRU
}
```

### Invariants to enforce in Rust

- **Cache-correctness**: Query result served from cache only if stored generation == current generation. Rust type system enforces this via `Generation` enum.
- **Single-read**: Repository read exactly once per cache miss. Enforced by control flow.
- **Re-auth on cache hit**: Raw data re-authorized before return. Enforced by `_handle_cache_hit` logic.
- **Atomic invalidation**: `_superedge_invalidate` performs delete-prefix + generation advance under per-resource lock.

## Phase 8: Runtime / Messaging / HTTP / FFI Crates

These are interface/infrastructure crates. Implement only after `daf-core` + `daf-application` are stable.

### `daf-runtime`

Tokio runtime configuration, task spawning adapters.

### `daf-messaging`

Apalis job/message processing. Implements `Algorithm` trait for async job execution.

### `daf-http`

Axum router translating HTTP to `DataAccess` operations. Error translation: `AuthorizationError` → 403, `NotFoundError` → 404.

### `daf-ffi`

C-compatible ABI. Uses `extern "C"` functions. Opaque pointers for `DataAccess`, `ResourceId`, `UserId`. Errors returned as `int` error codes. No panics cross FFI boundary.

## Phase 9: Tests & Validation

### Test Strategy: Semantic Equivalence

For every Python test, produce a Rust test that demonstrates equivalent behavior.

| Python test | Rust equivalent |
|---|---|
| `test_query_cache_miss_then_hit` | Same scenario in Rust async test |
| `test_concurrent_post` | Same concurrent mutation test |
| `test_generation_monotonicity` | Same invariant test |
| `test_prefix_invalidation` | Same trie prefix test |
| `test_authorization_x_cache_isolation` | Same security invariant test |

### Required tests (from TOML spec)

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cargo check --workspace`

### Architecture tests

Prove:
- No circular crate dependencies (enforced by workspace resolver)
- `daf-core` has no framework dependencies (Cargo.toml `[dependencies]` = empty)
- Infrastructure does not leak upward (compilation error if violated)
- No global mutable state (`static mut` is forbidden, glob usage is reviewed)
- Typed errors are used (compiler enforces)
- ABI boundaries are explicit (FFI tests + doc comments)

## Phase 10: Documentation

Produce:
- `docs/adr/` — All ADRs extracted during Phase 1
- `docs/contracts/function-matrix.md`
- `docs/contracts/polymorphism-matrix.md`
- `docs/contracts/error-taxonomy.md`
- `docs/contracts/cache-semantics.md`
- `docs/architecture/crate-dependency-graph.md` (generated from Cargo.toml)
- `docs/abi/ffi-contract.md`

## Rollout / Migration Path

1. **Week 1**: Workspace setup + `daf-core` (traits, errors, contracts)
2. **Week 2**: `daf-cache` + `daf-repository` (L1/L2 + Memory)
3. **Week 3**: `daf-algorithms` + `daf-application` (DataAccess orchestrator)
4. **Week 4**: `daf-http` + `daf-ffi` + test equivalence
5. **Week 5**: Validation, clippy, documentation, semantic equivalence proof

## Risks

| Risk | Mitigation |
|---|---|
| Deep-copy isolation semantic gap in Rust | Define `Clone` + `Arc` ownership contracts explicitly; document that `Arc::clone` is the Rust equivalent of deep-copy isolation for immutable data |
| Authorization user type generalization | Define `UserId` trait; implementors provide stable identity |
| Generation state conflation (Python bug) | Document as ADR-002; Rust type system prevents conflation via `Generation` enum |
| Async trait performance | Use `async-trait` crate; document that `daf-core` traits are `Send + Sync` |
| FFI ABI stability | Use `extern "C"`, opaque pointers, explicit error codes; never expose Rust panics or `Result` across boundary |

## Validation Checklist

- [ ] `theDAF-LLVM.toml` is the authoritative spec (copied to repo root)
- [ ] Python semantic freeze documented (ADR-001)
- [ ] Generation state semantics documented and modeled as enum (ADR-002)
- [ ] Function contracts extracted for all public APIs
- [ ] Polymorphism matrix maps Python Protocols → Rust traits
- [ ] Error taxonomy maps Python exceptions → Rust enums
- [ ] Cache hierarchy L1-L4 documented with semantics
- [ ] Workspace Cargo.toml created with correct dependency direction
- [ ] `daf-core` compiles with zero dependencies
- [ ] All 9 crates compile with `cargo check --workspace`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo fmt --check` passes
- [ ] Rust tests demonstrate semantic equivalence with Python test suite
- [ ] No circular dependencies
- [ ] `daf-core` has no framework dependencies
- [ ] C ABI boundary is explicit and documented
- [ ] No Python changes without migration ADR
