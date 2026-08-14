# PR #17 Concurrency Hardening

## Context

PR #17 at `093ba67` passes 119 tests and all quality gates. A red-team
assessment scored it 8.8/10 and identified two remaining correctness concerns
in the cache-consistency model:

1. **`_advance_generation` is non-atomic**: read-modify-write race between
   concurrent mutations can lose a generation increment.
2. **Query/mutation interleaving is untested**: a cache-miss query that completes
   after a mutation can write a stale entry; no test currently exercises this
   interleaving under controlled scheduling.

Both issues are **Medium** severity. The architecture already mitigates
user-visible stale-data serving via generation comparison and `delete_prefix`,
but the interleaving behavior is unvalidated.

---

## Goal

Hardened concurrency semantics with:
- Controlled-concurrency tests proving the interleaving design is correct
- Clear documentation of the concurrency model (what is atomic, what is best-effort)
- Optional in-process serialization for generation advancement

---

## Design Decision: What is the authoritative invalidation mechanism?

### Current state

Two mechanisms work together:

| Mechanism | Scope | Atomicity |
|-----------|-------|-----------|
| `delete_prefix(query:{namespace}:)` | Removes all cache entries for a resource | Atomic within `MemoryCache` (single dict sweep) |
| `_advance_generation` | Increments per-resource generation counter | NOT atomic — read-modify-write race |

On cache hit, DAF checks `cached["generation"] == current_gen`. A stale entry
with an old generation is rejected and triggers a cache miss.

### Key insight

`delete_prefix` is the **authoritative** invalidation mechanism. Generation is a
**fast-path optimization** that avoids re-authorizing and re-running algorithms
when the cache is warm.

Even if two concurrent mutations both read `gen=7` and both write `gen=8`:
- Both mutations call `delete_prefix`, which clears all cache entries for the resource
- Any stale query that later writes `gen=7` creates an entry that the next read will reject
- The "lost" increment (7→8 instead of 7→9) does not cause user-visible stale data

### Decision

**Do NOT attempt cross-process atomic generation** without a `Cache` protocol
change (out of scope). Instead:

1. Document the concurrency model explicitly in `access.py`
2. Add in-process per-resource `asyncio.Lock` to serialize `_advance_generation`
   within the same process (handles the multi-instance same-process case)
3. Add controlled-concurrency tests that prove the interleaving semantics

---

## Implementation Plan

### Step 1: Add per-resource generation lock

In `DataAccess.__init__`, add a `dict[str, asyncio.Lock]` for generation locks.

```python
import asyncio

class DataAccess:
    def __init__(self, ...):
        ...
        self._generation_locks: dict[str, asyncio.Lock] = {}
        self._generation_locks_lock = asyncio.Lock()
```

Add a helper:

```python
async def _generation_lock(self, resource_id: str) -> asyncio.Lock:
    namespace = self._resource_namespace(resource_id)
    async with self._generation_locks_lock:
        if namespace not in self._generation_locks:
            self._generation_locks[namespace] = asyncio.Lock()
        return self._generation_locks[namespace]
```

Update `_current_generation` and `_advance_generation` to acquire the lock:

```python
async def _current_generation(self, resource_id: str) -> int:
    lock = await self._generation_lock(resource_id)
    async with lock:
        namespace = self._resource_namespace(resource_id)
        value = await self._cache.get(f"_daf_gen:{namespace}")
        return value if isinstance(value, int) else 0

async def _advance_generation(self, resource_id: str) -> None:
    lock = await self._generation_lock(resource_id)
    async with lock:
        namespace = self._resource_namespace(resource_id)
        current = await self._cache.get(f"_daf_gen:{namespace}")
        if not isinstance(current, int):
            current = 0
        await self._cache.set(f"_daf_gen:{namespace}", current + 1)
```

**Rationale**: This serializes generation advancement within the same process,
eliminating the read-modify-write race for the common case of multiple
`DataAccess` instances sharing a cache in the same event loop. Cross-process
atomicity would require cache-level CAS (out of scope).

### Step 2: Document the concurrency model

Add a `### Concurrency` section to the `DataAccess` class docstring:

```python
"""
Concurrency model:

- `delete_prefix` is the authoritative invalidation mechanism and is atomic
  within the MemoryCache implementation.
- Generation counters are per-resource and stored in the shared cache.
- Within a single process, generation advancement is serialized via
  per-resource asyncio locks.
- Across processes, generation advancement is best-effort (read-modify-write).
  Concurrent mutations may observe a temporarily stale generation value, but
  stale cache entries are always rejected by generation comparison on the next
  read. The system never serves stale data to callers.
- For distributed cache backends, atomic generation advancement requires
  cache-level CAS or compare-and-set primitives (out of scope for the current
  Cache protocol).
"""
```

### Step 3: Add controlled-concurrency tests

In `tests/integration/test_security_invariants.py`, add two new test classes:

#### Test A: Stale query does not resurrect after mutation

```python
class TestStaleQueryAfterMutation:
    """Test that a query completing after a mutation does not serve stale data."""

    @pytest.mark.asyncio
    async def test_stale_cache_write_after_mutation_is_rejected(self) -> None:
        """
        Simulate: query misses → mutation advances gen and clears cache →
        stale query writes old entry → next query sees generation mismatch
        and returns fresh data.
        """
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()

        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        user = FakeUser("user-1")

        # Step 1: cache miss (gen=0)
        result1 = await daf.query(QueryInfo(resource_id="123"), user=user)
        assert result1.success is True
        assert result1.cache_hit is False
        assert result1.data["name"] == "John"

        # Step 2: mutation advances gen to 1, clears cache
        await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=user,
        )

        # Step 3: inject stale entry as if a late-arriving query wrote it
        cache_key = _expected_cache_key("123", {}, None, "user-1")
        cache._cache[cache_key] = {
            "raw": {"name": "John", "owner_id": "user-1"},
            "transformed": {"name": "John", "owner_id": "user-1"},
            "generation": 0,
        }

        # Step 4: next query must reject stale entry and return fresh data
        result2 = await daf.query(QueryInfo(resource_id="123"), user=user)
        assert result2.success is True
        assert result2.cache_hit is False
        assert result2.data["name"] == "Jane"
```

#### Test B: Concurrent mutations do not corrupt generation

```python
class TestConcurrentMutationGeneration:
    """Test generation advancement under concurrent mutations."""

    @pytest.mark.asyncio
    async def test_concurrent_mutations_generation_is_monotonic(self) -> None:
        """
        Two concurrent mutations on the same resource both advance generation.
        Final generation must be at least initial + 2 (no lost increments
        within the same process due to per-resource lock serialization).
        """
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()

        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        user = FakeUser("user-1")

        # Prime the cache so generation key exists
        await daf.query(QueryInfo(resource_id="123"), user=user)

        async def mutate(name: str) -> None:
            await daf.put(
                PutInfo(resource_id="123", data={"name": name}),
                user=user,
            )

        await asyncio.gather(
            mutate("Jane"),
            mutate("Jack"),
        )

        # Both mutations must have advanced generation at least once each
        namespace = hashlib.sha256("123".encode()).hexdigest()
        final_gen = await cache.get(f"_daf_gen:{namespace}")
        assert final_gen is not None and isinstance(final_gen, int)
        assert final_gen >= 2
```

### Step 4: Verify test count

Target: **121 passing** (119 existing + 2 new).

---

## Validation

- `pytest`: 121/121 passing
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- `python scripts/power_of_ten.py src`: 0 violations

---

## Out of Scope

- Cross-process atomic generation (requires cache-level CAS protocol change)
- Persistent/distributed cache backends
- Performance benchmarking of deepcopy overhead
- `_user_id()` `str(user)` fallback removal (R5 compatibility debt)
- POST authorization policy change (product decision, not a core defect)
