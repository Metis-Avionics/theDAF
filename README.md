# theDAF — Data Access Factory (Python + theDAF-LLVM)

A production-quality **data-access abstraction layer** with a reference implementation in Python (FastAPI) and a standalone, LLVM-backed Rust backend ([theDAF-LLVM](https://github.com/Metis-Avionics/theDAF-LLVM)) exposing a C-compatible ABI for reverse compatibility with Python and other language bindings.

> ⚠️ **Red-team assessment available:** see [BUGS.md](./BUGS.md) and [SECURITY.md](./SECURITY.md).

## Why DAF?

### The Problem

Applications often conflate three distinct concerns:

- **Transport** – How data arrives and leaves
- **Data Access** – How data is retrieved, cached, and manipulated
- **Business Logic** – The computation and validation that governs behavior

This leads to:

- Route handlers bloated with repository logic
- Cache decisions scattered throughout endpoint code
- Inability to reuse data access patterns outside transport
- Testing that requires HTTP clients instead of direct function calls
- Framework coupling preventing code portability

### The Solution

DAF separates these concerns into independent layers:

```
Transport (FastAPI / Axum / FFI)
        ↓
    Adapter
        ↓
    DataAccess Layer
      ↙  ↓  ↘
 Repository  Cache  Algorithm
```

**Key invariant:** The core `DataAccess` layer does not depend on any transport framework. It is pure logic.

## theDAF-LLVM (Standalone Rust Backend)

`theDAF-LLVM` is a separate repository and crate family. It reimplements the same data-access semantics in Rust with:

- **Ownership-safe concurrency** via Tokio, `Arc`, and `Mutex`
- **C-compatible ABI** (`extern "C"`, opaque pointers, `i32` error codes) so Python and other hosts can call into the Rust backend without FFI safety hazards
- **LLVM integration** for compiled query paths and optimized algorithm execution
- **Reverse compatibility** with the Python contract: `QueryResult`, `MutationResult`, and error envelopes preserve the externally observable shape so existing Python adapters can switch backends without breaking

### theDAF-LLVM crate map

| Crate | Responsibility |
|-------|----------------|
| `daf-core` | Traits, errors, contracts (`Repository`, `Cache`, `Algorithm`, `Authorizer`) |
| `daf-application` | `DataAccess` orchestrator with generation tracking, prefix invalidation, re-auth on cache hit |
| `daf-cache` | `MemoryCache` with terminal-only prefix trie, LRU eviction, DFS/BFS/A* traversal |
| `daf-repository` | `MemoryRepository` with CAS (`try_update` / `try_delete`) |
| `daf-algorithms` | `FibonacciDP` demonstrating memoization and stats |
| `daf-runtime` | Tokio runtime configuration |
| `daf-messaging` | Async message processing |
| `daf-http` | Axum router translating HTTP to `DataAccess` |
| `daf-ffi` | C-compatible ABI with opaque pointers and stable error codes |

### Rust invariants enforced by the type system

- `Generation` is an explicit enum (`Missing` / `Valid(u64)`), preventing the Python sentinel-`0` conflation.
- Cache keys are a pure function of canonical JSON + SHA-256 + user context.
- `QueryResult` / `MutationResult` carry typed error envelopes while preserving the external `Option<String>` shape for ABI compatibility.
- Per-resource `tokio::sync::Mutex` lock striping bounds concurrency state.
- No `static mut`, no `unsafe` outside `daf-cache`’s trie, no panics across FFI.

## Python Implementation (Reference)

The Python implementation in this repository remains the semantic reference. It provides:

- FastAPI adapter with rate limiting
- `MemoryRepository` and `MemoryCache`
- `DataAccessFactory` composition
- `FibonacciDP` algorithm

For details, see the sections below.

### Building and testing (Rust)

The Rust backend lives in the [theDAF-LLVM](https://github.com/Metis-Avionics/theDAF-LLVM) repository. From that workspace:

```bash
cargo fmt -- --check
cargo clippy --workspace
cargo test --workspace
```

### Rust integration test coverage

- Authorization × cache isolation (different users get different entries)
- Re-authorization on cache hit (stale grants rejected)
- Prefix invalidation clears all derived projections
- Stale cache entry rejection after mutation (generation comparison)
- Concurrent mutation generation monotonicity
- Cache isolation between different resources
- Authorization prevents mutation side effects (no generation advance on denied mutation)
- Empty `resource_id` rejected before auth
- No authorizer allows all operations
- Query filters return matching data / Null on mismatch
- POST creates unique resource IDs
- Query after successful POST roundtrips
- PUT / DELETE conflict behavior on concurrent updates
- Generation advances on POST / PUT / DELETE

## Architecture

### Core Components

#### 1. **Repository** – Data Source Access

Defines how data is persisted and retrieved. The `Repository` protocol allows any concrete implementation (SQL, NoSQL, file system, memory, etc.).

```python
class Repository(Protocol[T]):
    async def get(self, key: str) -> T | None: ...
    async def save(self, key: str, value: T) -> None: ...
    async def delete(self, key: str) -> None: ...
    async def create(self, value: T) -> str: ...
    async def try_update(self, key: str, expected: T, update: Callable[[T], T]) -> T | None: ...
    async def try_delete(self, key: str, expected: T) -> bool: ...
```

`try_update` and `try_delete` provide compare-and-swap (CAS) semantics: the mutation only succeeds if the stored value still matches the `expected` value observed during authorization. This closes the TOCTOU window between auth and mutation.

#### 2. **Cache** – Result Reuse

Stores frequently accessed data to reduce repository lookups. Cache keys are canonicalized using SHA-256 over JSON-serialized query semantics (resource_id, filters, algorithm, user_id), ensuring no delimiter-collision attacks. The `MemoryCache` implementation includes a terminal-only prefix trie with DFS, BFS, and A* traversal helpers for prefix-key enumeration.

```python
class Cache(Protocol):
    async def get(self, key: str) -> Any | None: ...
    async def set(self, key: str, value: Any) -> None: ...
    async def delete(self, key: str) -> None: ...
    async def delete_prefix(self, prefix: str) -> None: ...
    async def clear(self) -> None: ...
```

#### 3. **Authorizer** – Access Control

Optional pluggable authorization layer. The authorizer receives the operation, resource ID, user, and resource data so it can make ownership decisions without additional repository lookups.

```python
class Authorizer(Protocol):
    async def authorize(
        self,
        operation: str,
        resource_id: str | None,
        user: Any,
        data: Any = None,
    ) -> None: ...
```

Security invariants:
- **Fail-closed:** If resource data is not a dict (or cannot be retrieved), authorization is denied.
- **POST data inspection:** `post()` passes the proposed creation payload to the authorizer, enabling policy enforcement before persistence.
- **Re-authorization on cache hit:** Cached results are re-authorized before return, preventing stale grants from bypassing revoked access.

#### 4. **Algorithm** – Computation

Encapsulates algorithmic behavior (DP, ML inference, etc.) separate from data access.

```python
class Algorithm(Protocol):
    async def execute(self, input_data: Any) -> Any: ...
    async def get_stats(self) -> dict[str, Any]: ...
```

#### 5. **DataAccess** – Orchestration

Composes repository, cache, and algorithm into a cohesive data access layer.

```python
class DataAccess:
    async def query(self, info: QueryInfo) -> QueryResult: ...
    async def post(self, info: PostInfo) -> MutationResult: ...
    async def put(self, info: PutInfo) -> MutationResult: ...
    async def delete(self, info: DeleteInfo) -> MutationResult: ...
```

**Consistency boundary:** DAF cache invalidation is triggered only by mutations through `DataAccess`. Direct writes to the underlying repository bypass cache invalidation. For correctness, all mutations must flow through `DataAccess`, or the caller must manually invalidate affected cache entries.

#### 6. **DataAccessFactory** – Composition

Responsible for constructing a configured `DataAccess` instance with its dependencies.

```python
factory = DataAccessFactory(
    repository=my_repo,
    cache=my_cache,
    algorithms={"fibonacci": FibonacciDP(), "custom": MyAlgorithm()},
)
daf = factory.create()
```

#### 7. **FastAPI Adapter** – HTTP Bridge

Translates HTTP requests into `DataAccess` operations. Endpoint-level rate limiting lives **only here**.

The FastAPI adapter automatically reads `filters` and `algorithm` from query parameters:

```bash
# Query with filters
GET /data/123?filters={"status":"active"}

# Query with algorithm
GET /data/123?algorithm=fibonacci
```

### Query Execution Flow

When you call `daf.query(info)`:

1. **Validation** – Validate `resource_id` and inputs
2. **Cache Lookup** – Check if result is already cached. Cache keys are `query:{resource_id}:{sha256_digest}` of canonical JSON (resource_id, filters, algorithm, user_id). On cache hit, re-authorize with the cached data before returning.
3. **Repository Lookup** (if cache miss) – Fetch from persistent storage (single read)
4. **Authorization** – Authorize with the retrieved data snapshot
5. **Filter Application** – Apply in-memory filters to the retrieved data
6. **Algorithm Execution** (if specified) – Apply computation by algorithm name from registry
7. **Cache Population** – Store result for future hits
8. **Return Typed Result** – `QueryResult` with data, cache status, stats

For cache hits, steps 3-6 are skipped; the cached result is re-authorized and returned directly.

### Authorization Boundary

DataAccess performs authorization **after** reading from the repository on cache miss. The repository is treated as a trusted internal data source; the authorizer enforces **usage** policy, not **access** policy.

This design choice means:
- The repository is read exactly once per cache miss (single-read invariant).
- The authorizer always receives the raw repository data, even on cache hit, to make consistent ownership decisions.
- If your deployment requires authorization-before-read (e.g., for audited data sources or multi-tenant isolation at the storage layer), implement that check at the repository level.

**Resource existence is not concealed.** The default security model maps NotFoundError to 404 and AuthorizationError to 403. This allows callers to infer resource existence. If existence confidentiality is required, implement a masking layer at the adapter level.

### Data Contracts

All I/O uses **Pydantic v2 models** for validation:

- **`QueryInfo`** – Query parameters and optional algorithm name
- **`PostInfo`** – Resource type and creation data
- **`PutInfo`** – Resource ID and update data
- **`DeleteInfo`** – Resource ID
- **`QueryResult`** – Result data, cache hit flag, algorithm stats
- **`MutationResult`** – Success flag, resource ID, updated data

Validation happens at the boundary. Algorithm and repository logic never receives invalid data.

## Installation

### With uv

```bash
uv pip install thedaf
```

### Optional dependencies

For FastAPI integration with rate limiting:

```bash
uv pip install thedaf[fastapi]
```

For development:

```bash
uv pip install thedaf[dev]
```

## Quick Start

### Core Usage (No HTTP)

```python
import asyncio
from daf import DataAccessFactory
from daf.repositories import MemoryRepository
from daf.cache import MemoryCache
from daf.contracts import QueryInfo

# Set up components
repo = MemoryRepository()
cache = MemoryCache()

# Create factory and DataAccess instance
factory = DataAccessFactory(repository=repo, cache=cache)
daf = factory.create()

# Use it
async def main():
    # Save data
    await repo.save("user:1", {"name": "Alice", "email": "alice@example.com"})
    
    # Query with caching
    result = await daf.query(QueryInfo(resource_id="user:1"))
    print(result.data)  # {"name": "Alice", "email": "alice@example.com"}
    print(result.cache_hit)  # False (first access)
    
    # Query again (cache hit)
    result = await daf.query(QueryInfo(resource_id="user:1"))
    print(result.cache_hit)  # True (second access)

asyncio.run(main())
```

### FastAPI Integration

```python
from fastapi import FastAPI
from daf import DataAccessFactory
from daf.repositories import MemoryRepository
from daf.cache import MemoryCache
from daf.adapters.fastapi import DataAccessRouter, limiter

# Set up components
repo = MemoryRepository()
cache = MemoryCache()
factory = DataAccessFactory(repository=repo, cache=cache)
daf = factory.create()

# Create FastAPI app
app = FastAPI()
router_builder = DataAccessRouter(
    daf,
    get_current_user=get_current_user,  # REQUIRED
)
app.include_router(router_builder.get_router())

# Run with: uvicorn main:app --reload
```

Endpoints available:

- `GET /data/{resource_id}` – Query (30 requests/minute)
- `POST /data` – Create (10 requests/minute)
- `PUT /data/{resource_id}` – Update (10 requests/minute)
- `DELETE /data/{resource_id}` – Delete (10 requests/minute)

### With Algorithm (Dynamic Programming)

```python
from daf.algorithms import FibonacciDP

# Create DAF with algorithm registry
repo = MemoryRepository()
cache = MemoryCache()
algo = FibonacciDP()
factory = DataAccessFactory(
    repository=repo,
    cache=cache,
    algorithms={"fibonacci": algo},
)
daf = factory.create()

# Save input
await repo.save("fib_5", 5)

# Query with algorithm
result = await daf.query(
    QueryInfo(resource_id="fib_5", algorithm="fibonacci")
)
print(result.data)  # 5 (fib(5))
print(result.algorithm_stats)
# {"iterations": 5, "cache_hits": 3, "memo_size": 6}
```

## Architecture Principles

### 1. **Separation of Concerns**

- **Core `DataAccess`** is framework-agnostic
- **`FastAPI` adapter** handles HTTP exclusively
- **Repository** abstracts persistence
- **Cache** abstracts caching strategy
- **Algorithm** encapsulates computation

### 2. **Dependency Inversion**

```python
# ❌ DON'T: Depend on concrete implementations
daf = DataAccess(MemoryRepository(), MemoryCache())

# ✅ DO: Depend on abstractions (Protocols)
daf = DataAccess(repo, cache)  # Can be any impl
```

### 3. **Factory Pattern**

```python
# Composition responsibility is isolated
factory = DataAccessFactory(repo, cache, algo)
daf = factory.create()

# Not intermingled with runtime operations
result = await daf.query(info)
```

### 4. **Explicit Memoization**

The `FibonacciDP` algorithm demonstrates explicit memoization, not hidden caching:

```python
async def _compute_fib(self, n: int) -> int:
    if n in self._memo:  # Explicit check
        self._cache_hits += 1  # Track reuse
        return self._memo[n]
    
    # Compute...
    self._memo[n] = result  # Explicit storage
    return result
```

This shows **why** memoization matters:

```
fib(5) without memoization: ~15 function calls
fib(5) with memoization:    6 unique computations
Result: 63% reduction in work
```

## Testing

All components are independently testable:

```bash
# Run all tests
uv run pytest

# Run specific test file
uv run pytest tests/unit/test_contracts.py

# Run with coverage
uv run pytest --cov=src/daf tests/
```

### Test Coverage

- ✅ Pydantic validation
- ✅ Factory construction
- ✅ Query execution flow
- ✅ Cache hits and misses
- ✅ Cache invalidation on mutations
- ✅ Repository substitution (testing with fakes)
- ✅ Algorithm execution and statistics
- ✅ FastAPI endpoints and validation
- ✅ Rate limiting
- ✅ Error translation to HTTP responses
- ✅ DP memoization efficiency verification
- ✅ Direct primitive tests for `Memo`, `ResourceMemo`, `TreeCollector`, `walk_tree`

## Quality Assurance

- **191 tests** passing (17 unit + 25 integration + 8 end-to-end + 141 component/primitive tests)

### Type Checking

```bash
uv run mypy src/
```

Strict mode enabled. No `Any` without documentation.

### Linting

```bash
uv run ruff check .
```

Configured for Python 3.12+:

- E (errors)
- F (Flake8)
- I (isort)
- B (flake8-bugbear)
- UP (pyupgrade)
- SIM (flake8-simplify)

### Formatting

Code follows PEP 8 with 88-character line length.

## Package Building

Build a distribution:

```bash
uv build
```

Produces:

- `dist/fastapi_data_access_factory-0.1.0.tar.gz` (source)
- `dist/fastapi_data_access_factory-0.1.0-py3-none-any.whl` (wheel)

Verify the wheel:

```bash
uv pip install dist/fastapi_data_access_factory-0.1.0-py3-none-any.whl
python -c "from daf import DataAccess, DataAccessFactory; print('✓ Import successful')"
```

## Advanced Usage

### Custom Repository

Implement the `Repository` protocol for your storage backend:

```python
class PostgresRepository:
    async def get(self, key: str) -> dict | None:
        # Query PostgreSQL
        pass
    
    async def save(self, key: str, value: dict) -> None:
        # Insert/update PostgreSQL
        pass
    
    # ... other methods
```

Then use it:

```python
factory = DataAccessFactory(
    repository=PostgresRepository(),
    cache=MemoryCache(),
)
```

**No changes to `DataAccess` required.**

### Custom Algorithm

Implement the `Algorithm` protocol:

```python
class MyMLModel:
    async def execute(self, input_data: Any) -> Any:
        # Run inference
        pass
    
    async def get_stats(self) -> dict[str, Any]:
        return {"inference_time": self.last_time}
```

### Custom Cache Strategy

Replace `MemoryCache` with Redis, Memcached, or application-specific cache:

```python
class RedisCache:
    async def get(self, key: str) -> Any | None:
        # Redis GET
        pass
    
    # ... other methods
```

## Limitations

1. **In-memory repository** – No persistence across restarts. `try_update`/`try_delete` use a coarse lock and identity comparison (`is`) for CAS detection; this is best-effort only. Real transactional backends should implement true atomic CAS.
2. **Bounded cache** — `max_size=0` (default) is unbounded. Set a positive `max_size` for LRU eviction. Generation state shares the same cache namespace as query entries; evicting generation metadata forces a cache miss, which is correct but may increase repository load.
3. **Fibonacci algorithm** – Demonstration only (use `math.fib()` in production)
4. **Single-layer rate limiting** – FastAPI adapter only
5. **Unbounded `_cached_key` cache** — `_cached_key` uses `functools.cache` without eviction. Long-running processes with many unique query combinations may experience unbounded memory growth. See [#23](https://github.com/RAliane-REBORN/theDAF/issues/23).
6. **Test files not mypy-strict clean** — `tests/unit/test_memoize.py`, `tests/unit/test_recursion.py`, and `tests/unit/test_barrels.py` have missing type annotations. See [#22](https://github.com/RAliane-REBORN/theDAF/issues/22).
7. **`graphify_affected.py` dynamic loading** — Uses `importlib.util.spec_from_file_location` for testability, which is fragile across packaging tools. See [#21](https://github.com/RAliane-REBORN/theDAF/issues/21).
8. **`ResourceMemo` type-ignore workaround** — `OrderedDict[str, T]` triggers mypy false positives requiring `# type: ignore[return-value]`. See [#20](https://github.com/RAliane-REBORN/theDAF/issues/20).
9. **`_trie.py` unsound `__init__` reset** — `root.__init__()` is used to reset the trie root; mypy flags this as unsound. See [#19](https://github.com/RAliane-REBORN/theDAF/issues/19).

These are intentional to keep the package focused. Extend as needed for your use case.

## Future Extension Points

- **Redis integration** – Replace `MemoryCache`
- **SQL Alchemy repository** – PostgreSQL, MySQL, etc.
- **Distributed cache** – Multi-service coordination
- **Async queue integration** – Delayed/background operations
- **Observability hooks** – Metrics, tracing, logging
- **GraphQL adapter** – Alternative to FastAPI REST
- **MCP (Model Context Protocol) adapter** – LLM-based queries
- **Batch operations** – Reduce roundtrips

## Error Handling

Core domain errors are defined in `daf.core.errors`:

- `DataAccessError` – Base class
- `NotFoundError` – Resource not found
- `ValidationError` – Invalid input
- `RepositoryError` – Repository operation failed
- `CacheError` – Cache operation failed
- `AlgorithmError` – Algorithm execution failed
- `AuthorizationError` – User not authorized

Core operations raise typed exceptions for expected errors:

- `query()` raises `NotFoundError`, `ValidationError`, `AuthorizationError`
- `post()` raises `ValidationError`, `AuthorizationError`
- `put()` raises `ValidationError`, `NotFoundError`, `AuthorizationError`
- `delete()` raises `ValidationError`, `NotFoundError`, `AuthorizationError`

The FastAPI adapter catches specific exceptions and maps them to HTTP status codes:

- `AuthorizationError` → HTTP 403 Forbidden
- `NotFoundError` → HTTP 404 Not Found
- `ValidationError` → HTTP 422 Unprocessable Entity (Pydantic)
- `DataAccessError` → HTTP 500 Internal Server Error

```python
from daf.core.errors import AuthorizationError, NotFoundError, ValidationError

try:
    result = await daf.query(info)
except NotFoundError:
    log.info("missing resource")
except ValidationError:
    log.warning("bad input")
except AuthorizationError:
    log.warning("auth failure")
```

When using the FastAPI adapter, exceptions are automatically translated to HTTP responses:

```python
from fastapi import FastAPI
from daf.adapters.fastapi import DataAccessRouter

router = DataAccessRouter(daf, get_current_user=get_current_user)
# GET /data/{id} returns 403/404/500 as appropriate
```

## Contributing

This is a foundational architecture demonstration. To extend:

1. Implement custom `Repository`, `Cache`, or `Algorithm`
2. Write tests for your implementation
3. Use `DataAccessFactory` to wire it together
4. No changes to core required

## License

MIT License. See LICENSE for details.

## Principles Summary

| Concern | Responsibility |
|---------|---|
| **HTTP** | FastAPI adapter only |
| **Data Access** | `DataAccess` orchestration |
| **Persistence** | `Repository` abstraction |
| **Result Reuse** | `Cache` abstraction |
| **Computation** | `Algorithm` abstraction |
| **Composition** | `DataAccessFactory` |
| **Authorization** | `Authorizer` protocol (fail-closed, re-auth on cache hit) |
| **Validation** | Pydantic contracts at boundary |
| **CAS Semantics** | `Repository.try_update` / `try_delete` for mutation safety |

**Result:** A reusable, testable, framework-independent data access layer.
Data Access Factory Open SOurce Python contribution
