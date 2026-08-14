# FastAPI Data Access Factory (DAF)

A production-quality Python package that implements a reusable **data-access abstraction layer** with an optional **FastAPI integration**. The architecture cleanly separates data access logic from HTTP transport, enabling the core abstraction to be used in any context (HTTP, MCP, workers, tests, etc.).

> ⚠️ **Red-team assessment available:** see [BUGS.md](./BUGS.md) and [SECURITY.md](./SECURITY.md).

## Why DAF?

### The Problem

FastAPI applications often conflate three distinct concerns:

- **HTTP Transport** – How data arrives and leaves
- **Data Access** – How data is retrieved, cached, and manipulated
- **Business Logic** – The computation and validation that governs behavior

This leads to:

- Route handlers bloated with repository logic
- Cache decisions scattered throughout endpoint code
- Inability to reuse data access patterns outside HTTP
- Testing that requires HTTP clients instead of direct function calls
- Framework coupling preventing code portability

### The Solution

DAF separates these concerns into independent layers:

```
HTTP Transport (FastAPI)
        ↓
    FastAPI Adapter
        ↓
    DataAccess Layer
      ↙  ↓  ↘
Repository  Cache  Algorithm
```

**Key invariant:** The core `DataAccess` layer does not depend on FastAPI, HTTP requests, or any web framework. It's pure Python.

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
```

#### 2. **Cache** – Result Reuse

Stores frequently accessed data to reduce repository lookups.

```python
class Cache(Protocol):
    async def get(self, key: str) -> Any | None: ...
    async def set(self, key: str, value: Any) -> None: ...
    async def delete(self, key: str) -> None: ...
    async def clear(self) -> None: ...
```

#### 3. **Algorithm** – Computation

Encapsulates algorithmic behavior (DP, ML inference, etc.) separate from data access.

```python
class Algorithm(Protocol):
    async def execute(self, input_data: Any) -> Any: ...
    async def get_stats(self) -> dict[str, Any]: ...
```

#### 4. **DataAccess** – Orchestration

Composes repository, cache, and algorithm into a cohesive data access layer.

```python
class DataAccess:
    async def query(self, info: QueryInfo) -> QueryResult: ...
    async def post(self, info: PostInfo) -> MutationResult: ...
    async def put(self, info: PutInfo) -> MutationResult: ...
    async def delete(self, info: DeleteInfo) -> MutationResult: ...
```

#### 5. **DataAccessFactory** – Composition

Responsible for constructing a configured `DataAccess` instance with its dependencies.

```python
factory = DataAccessFactory(
    repository=my_repo,
    cache=my_cache,
    algorithms={"fibonacci": FibonacciDP(), "custom": MyAlgorithm()},
)
daf = factory.create()
```

#### 6. **FastAPI Adapter** – HTTP Bridge

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

1. **Authorization** – Check access if authorizer is configured
2. **Cache Lookup** – Check if result is already cached (key includes resource_id, filters, algorithm, and user)
3. **Repository Lookup** (if cache miss) – Fetch from persistent storage
4. **Filter Application** – Apply in-memory filters to the retrieved data
5. **Algorithm Execution** (if specified) – Apply computation by algorithm name from registry
6. **Cache Population** – Store result for future hits
7. **Return Typed Result** – `QueryResult` with data, cache status, stats, and error classification

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

## Quality Assurance

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

1. **In-memory repository** – No persistence across restarts
2. **Basic cache** – No TTL or eviction policy
3. **Fibonacci algorithm** – Demonstration only (use `math.fib()` in production)
4. **Single-layer rate limiting** – FastAPI adapter only

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

Expected errors are returned as typed result envelopes with `error_type` preserved:

- `QueryResult.error_type` – `"not_found"`, `"validation"`, `"authorization"`
- `MutationResult.error_type` – `"not_found"`, `"validation"`, `"authorization"`

Unexpected errors propagate as exceptions. The FastAPI adapter catches `DataAccessError` subclasses and maps them to HTTP 500 with a generic message.

```python
result = await daf.query(info)
if not result.success:
    if result.error_type == "authorization":
        raise HTTPException(status_code=403)
    elif result.error_type == "not_found":
        raise HTTPException(status_code=404)
    else:
        raise HTTPException(status_code=400, detail=result.error)
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
| **Validation** | Pydantic contracts at boundary |

**Result:** A reusable, testable, framework-independent data access layer.
Data Access Factory Open SOurce Python contribution
