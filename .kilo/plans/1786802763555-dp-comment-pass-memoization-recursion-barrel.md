# DP Comment Pass — Memoization, Recursion, and Barrel Candidates

## Scope

Inspect every module in `src/daf/` (plus `scripts/power_of_ten.py`) for:
1. **Memoization primitive candidates** — places that compute a value from the same input repeatedly and could benefit from a shared memo wrapper.
2. **Recursion primitive candidates** — places that use recursive decomposition and could benefit from a shared recursion helper.
3. **Shared barrel helper candidates** — duplicated `_public()` definitions across `__init__.py` files that could be extracted to a single module.

No source changes are made in this pass. The goal is to annotate every candidate in depth so future implementation passes can work them in cleanly.

---

## 1. Shared Barrel Helper

### Candidate: `_public()` — duplicated across 7 barrel files

**Files affected:**
- `src/daf/__init__.py`
- `src/daf/core/__init__.py`
- `src/daf/cache/__init__.py`
- `src/daf/repositories/__init__.py`
- `src/daf/algorithms/__init__.py`
- `src/daf/contracts/__init__.py`
- `src/daf/adapters/__init__.py`

**Current shape (identical in every file):**
```python
def _public(*names: str) -> list[str]:
    return list(names)

__all__ = _public("Name1", "Name2")
```

**Why it matters:**
- 6 of the 7 definitions are byte-for-byte identical. The function is a trivial identity wrapper, but its duplication means any change to the barrel contract (e.g., adding validation, logging, or deprecation warnings) must be applied 7 times.
- `daf/__init__.py` has an additional invariant: its `__all__` must be a strict subset of `daf.core.__all__` (enforced by `tests/unit/test_barrels.py`). A shared barrel helper could carry that invariant check as a runtime assertion in development mode.

**How to work it in a future pass:**
- Introduce `src/daf/_barrel.py` (or `src/daf/core/_barrel.py`) containing `_public()` plus optional helpers like `_public_subset(core_all, *names)` that asserts the subset invariant.
- Replace every `def _public(...)` in `__init__.py` with `from daf._barrel import _public`.
- Update `test_barrels.py` to also test that no `__init__.py` defines its own `_public` symbol (lint guard).

**Risk:** None. Pure refactor with no behavioral change.

---

## 2. Memoization Primitive Candidates

### Candidate 2a: `FibonacciDP._compute_fib` — explicit dict-based memoization

**File:** `src/daf/algorithms/dynamic_programming.py:41-69`

**Current shape:**
```python
def __init__(self):
    self._memo: dict[int, int] = {}
    self._iterations: int = 0
    self._cache_hits: int = 0

async def _compute_fib(self, n: int) -> int:
    if n in self._memo:           # memo check
        self._cache_hits += 1
        return self._memo[n]
    self._iterations += 1
    if n <= 1:
        result = n
    else:
        fib_n_minus_1 = await self._compute_fib(n - 1)   # recursive decomposition
        fib_n_minus_2 = await self._compute_fib(n - 2)
        result = fib_n_minus_1 + fib_n_minus_2
    self._memo[n] = result         # memo store
    return result
```

**Why it matters:**
- This is the **canonical memoization triple**: check cache → compute → store result. The triple appears explicitly as three separate code locations (check at line 51, compute at lines 56-65, store at line 68). The tracking of `_iterations` and `_cache_hits` is interleaved with the algorithm logic, making it harder to reuse the memoization pattern for other algorithms.
- The `FibonacciDP` class is the **only** algorithm in the registry, but the `Algorithm` protocol is designed for multiple implementations. Every future algorithm (e.g., knapsack, LCS) will re-implement the same memoization triple.

**How to work it in a future pass:**
- Create `src/daf/utils/memoize.py` with a `Memo` class and a `memoize` decorator.
  - `Memo` provides `get(key)`, `set(key, value)`, `has(key)`, `clear()`, and `stats()` (iterations, cache_hits, size).
  - `memoize(fn)` wraps an async function, injecting `Memo` as `self._memo` or passing it explicitly.
- Refactor `FibonacciDP` to delegate memoization to `Memo`, keeping only the recursive decomposition logic in `_compute_fib`.
- The `Algorithm` protocol's `get_stats()` contract maps directly onto `Memo.stats()`, so the primitive becomes the shared stats backend for all algorithms.

**Risk:** Low. The `FibonacciDP` is the only consumer. The `execute()` method resets counters and clears the memo — this lifecycle concern must be preserved in the primitive.

---

### Candidate 2b: `DataAccess._generation_lock` — lazy-init under a global lock

**File:** `src/daf/core/access.py:122-134`

**Current shape:**
```python
_generation_locks_lock = asyncio.Lock()

async def _generation_lock(self, resource_id: str) -> asyncio.Lock:
    namespace = self._resource_namespace(resource_id)
    async with self._generation_locks_lock:
        if namespace not in self._generation_locks:   # check
            self._generation_locks[namespace] = asyncio.Lock()  # create & store
        return self._generation_locks[namespace]        # return cached
```

**Why it matters:**
- This is a **memoization pattern disguised as concurrency control**: lazily create a value keyed by `resource_id`, cache it in a dict, and return it on subsequent calls. The memoization concern (check → create → store → return) is embedded inside the concurrency concern (serialize access via a global lock).
- The same pattern appears in `_current_generation` (line 144-150) and `_advance_generation` (line 152-165), where the generation counter itself is memoized in the cache. These three methods form a trio of "lazy state keyed by resource_id" that could share a `ResourceMemo` primitive.

**How to work it in a future pass:**
- A `ResourceMemo[T]` class in `daf.utils.memoize` could encapsulate: key derivation, lazy creation under a global lock, TTL/eviction (for unbounded growth), and stats.
- `_generation_locks` and `_generation_locks_lock` would become an internal detail of `ResourceMemo[asyncio.Lock]`.
- This also opens the door to bounded eviction of lock objects for long-lived `DataAccess` instances that see many unique resource IDs.

**Risk:** Medium. The lock semantics are correctness-critical. Any refactor must preserve the atomicity guarantee: two coroutines requesting the same namespace must receive the same lock object. The primitive must be tested under `pytest-asyncio` with concurrent requests.

---

### Candidate 2c: `DataAccess._cache_key` — deterministic key derivation

**File:** `src/daf/core/access.py:192-209`

**Current shape:**
```python
def _cache_key(self, info: QueryInfo, user: Any) -> str:
    user_id = self._user_id(user)
    payload = {
        "resource_id": info.resource_id,
        "filters": info.filters or {},
        "algorithm": info.algorithm or "",
        "user_id": user_id,
    }
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    namespace = self._resource_namespace(info.resource_id)
    return f"query:{namespace}:{digest}"
```

**Why it matters:**
- `_cache_key` is called on every `query()`, including cache hits. On a cache hit, `_execute_query` computes the key, checks the cache, finds the entry, and returns it. The key derivation is pure (no side effects) and deterministic given the same `(info, user)` pair.
- For a hot resource queried repeatedly with the same parameters, `_cache_key` recomputes the SHA-256 hash on every call, even though the result would be identical. This is memoizable without any correctness concern — the inputs are immutable `QueryInfo` Pydantic models.

**How to work it in a future pass:**
- Memoize `_cache_key` on `(info.resource_id, frozenset(info.filters or {}), info.algorithm, user_id)`. Since `QueryInfo` is a Pydantic model, its fields are stable.
- A `PureMemo` primitive (memoization for side-effect-free functions) would be ideal here. It differs from `Memo` in that it never needs explicit `clear()` — the memo lives as long as the function signature implies the same inputs produce the same output.
- Alternatively, store the computed key inside the `QueryInfo` model (add a `_cache_key: str | None = None` private field and compute it lazily on first access). This moves memoization into the data model rather than the orchestration layer.

**Risk:** Low for a `PureMemo` wrapper. Medium if modifying `QueryInfo` — need to verify Pydantic v2 behavior with private fields and model copying.

---

### Candidate 2d: `MemoryCache` trie — structural memoization of key prefixes

**File:** `src/daf/cache/memory.py:15-20` (`_TrieNode`), used throughout

**Current shape:**
```python
class _TrieNode:
    __slots__ = ("children", "key")
    def __init__(self):
        self.children: dict[str, _TrieNode] = {}
        self.key: str | None = None
```

**Why it matters:**
- The trie is itself a **memoized index** over cache keys. Each `_TrieNode` acts as a memo entry: the path from root to node records the prefix seen so far, and `node.key` records whether a complete cache key terminates at that node. This makes `delete_prefix` and `shake` O(prefix_length + subtree_nodes) instead of O(N).
- The children dict `node.children.setdefault(ch, _TrieNode())` is a per-node memoization of "have we seen this prefix character?". This pattern is repeated in `_trie_insert` (line 203-208), `_trie_delete` (line 210-229), `_trie_collect` (line 231-237), and `_trie_delete_prefix` (line 239-272).
- A `MemoizedTrieNode` primitive could encapsulate `children` access, making the trie a first-class memo structure rather than an ad-hoc dict-of-dicts.

**How to work it in a future pass:**
- Extract `_TrieNode` into a `Trie` class in `daf.utils.memoize` or `daf.utils.trie` with methods `insert`, `delete`, `collect_prefix`, `delete_prefix`.
- The `children.setdefault` pattern becomes a `get_or_create(child_key)` method on the node — a memoization primitive at the node level.
- This also makes the trie independently testable without the full `MemoryCache` machinery.

**Risk:** Low. The trie is internal (`_TrieNode` is private). The main risk is ensuring the prefix-delete cleanup logic (pruning empty child nodes at lines 223-229 and 263-271) is preserved.

---

## 3. Recursion Primitive Candidates

### Candidate 3a: `MemoryCache._dfs_collect` — classic tree DFS

**File:** `src/daf/cache/memory.py:149-157`

**Current shape:**
```python
def _dfs_collect(self, node: _TrieNode | None) -> builtins.set[str]:
    if node is None:
        return builtins.set()
    result = builtins.set()
    if node.key is not None:
        result.add(node.key)
    for child in node.children.values():
        result.update(self._dfs_collect(child))   # recursive self-call
    return result
```

**Why it matters:**
- This is a textbook recursive tree walk: null guard → accumulate current → recurse on children → merge → return. The accumulation pattern (`result = set(); result.add(...); result.update(recursion)`) is generic.
- It is used in three places: `_trie_collect` (line 237), `_trie_delete_prefix` (line 248, 261), and implicitly by `_evict_oldest` (line 146). All three callers need the same "collect all terminal keys in a subtree" operation.
- `_dfs_collect` has a sibling `_bfs_collect` (line 159-169) that does the same logical operation (collect all terminal keys) but iteratively. The two functions share the same purpose but different traversal strategies — a sign that traversal strategy should be a parameter, not separate functions.

**How to work it in a future pass:**
- Create `src/daf/utils/recursion.py` with a `TreeCollector` class parameterized by traversal strategy (`"dfs"`, `"bfs"`, `"astar"`).
  - The collector receives a node, a `key_extractor(node) -> str | None`, and a `children_extractor(node) -> Iterable`.
  - It returns `set[str]` of all keys found.
- `_dfs_collect`, `_bfs_collect`, and `_astar_collect` become thin wrappers around `TreeCollector` with different strategy arguments.
- The `_astar_collect` (line 171-201) currently has custom heap logic. Under the primitive, it becomes a `"best_match"` strategy that uses the priority queue internally — the caller interface remains identical.

**Risk:** Low for the collector primitive. Medium for integrating `_astar_collect` — the heap tuple structure `(neg_match, counter, node, depth, match_len)` is specific to the best-match algorithm and may not fit the generic collector interface cleanly. The primitive should allow custom per-node processing callbacks.

---

### Candidate 3b: `FibonacciDP._compute_fib` — recursive decomposition with memoization

**File:** `src/daf/algorithms/dynamic_programming.py:41-69`

**Current shape:**
```python
async def _compute_fib(self, n: int) -> int:
    if n in self._memo: return self._memo[n]
    if n <= 1: result = n
    else:
        fib_n_minus_1 = await self._compute_fib(n - 1)
        fib_n_minus_2 = await self._compute_fib(n - 2)
        result = fib_n_minus_1 + fib_n_minus_2
    self._memo[n] = result
    return result
```

**Why it matters:**
- This is **recursive decomposition**: the problem `fib(n)` is broken into subproblems `fib(n-1)` and `fib(n-2)`, solved recursively, and combined. The recursion structure (`n → n-1, n-2`) is algorithm-specific, but the memoization wrapper around it is generic.
- A recursion primitive should separate the **decomposition strategy** (what are the subproblems?) from the **memoization policy** (how do we cache subproblem results?). Currently they are intertwined in a single method.
- The `await` in recursive calls is important: each subproblem may be I/O-bound in a real algorithm. The primitive must preserve async semantics.

**How to work it in a future pass:**
- Create `src/daf/utils/recursion.py` with a `RecursionEngine` that accepts:
  - `base_cases: dict[int, Callable[[], Awaitable[T]]` — handles `n <= 1`
  - `decompose(n: int) -> list[int]` — returns subproblem indices (e.g., `[n-1, n-2]`)
  - `combine(results: list[T]) -> T` — combines subproblem results (e.g., `sum`)
- The engine handles memoization, async dispatch, and stats collection. `FibonacciDP._compute_fib` becomes a configuration of the engine rather than a hand-written recursive method.

**Risk:** Medium. The `FibonacciDP` class also tracks `_iterations` and `_cache_hits` for demo/observability purposes. The primitive must expose these stats through the `Algorithm.get_stats()` interface. Also, unbounded recursion depth for large `n` could hit Python's recursion limit (default ~1000) — the primitive should either enforce a max depth or document the limitation.

---

### Candidate 3c: `PowerOfTenChecker._add_parents` — recursive AST injection

**File:** `scripts/power_of_ten.py:281+` (exact line depends on file length)

**Why it matters:**
- This is a recursive AST walk that injects parent pointers into a tree. It is a **structural recursion** pattern: visit each node, modify it, recurse on children.
- While this is in a script (not the library), it shows that the recursion primitive would also benefit internal tooling. The same `TreeCollector`/`TreeWalker` could serve both library code and dev scripts.

**How to work it in a future pass:**
- If a `TreeWalker` primitive is created for `MemoryCache`, `scripts/power_of_ten.py` can import and use it for its AST traversal. This is a cross-cutting benefit.

**Risk:** Low. Scripts are not part of the library's public API.

---

## 4. Cross-Cutting Observations

### 4a. Async semantics everywhere

Every recursive and memoization candidate in this codebase is async. Any primitive must:
- Accept `async def` callables
- Use `await` for recursive calls
- Provide `async` versions of memo lookup/set operations (or make them synchronous since dict access is inherently sync — but the primitive API should be async-consistent)

### 4b. Stats collection is cross-cutting

`FibonacciDP` tracks `_iterations` and `_cache_hits`. `MemoryCache` has no explicit stats but could track trie node count, prefix operation counts, etc. A shared `StatsMixin` or `StatsCollector` primitive would let all memoization and recursion implementations report standardized metrics.

### 4c. The barrel helper is the highest-leverage, lowest-risk change

Extracting `_public()` to a shared module:
- Eliminates 6 duplicate definitions
- Creates a single point for barrel contract evolution
- Has zero runtime overhead and zero behavioral change
- Is a prerequisite for future barrel enhancements (validation, deprecation warnings)

It should be the first change in any implementation pass.

---

## 5. Implementation Order (Recommended)

| Order | Pass | Candidate | Files to touch | Risk | Leverage |
|-------|------|-----------|----------------|------|----------|
| 1 | Barrel helper | Shared `_public()` | All 7 `__init__.py` + new `_barrel.py` | Low | High (dedup + contract enforcement) |
| 2 | Memo primitive | `Memo` class + `memoize` decorator | `utils/memoize.py`, `algorithms/dynamic_programming.py` | Low | Medium (enables all future algorithm implementations) |
| 3 | Memoize `_cache_key` | `PureMemo` or inline memo | `core/access.py` | Low | Low (micro-optimization, but sets pattern) |
| 4 | Recursion primitive | `TreeCollector` / `RecursionEngine` | `utils/recursion.py`, `cache/memory.py` | Medium | Medium (unifies DFS/BFS/A* collection) |
| 5 | Memoize generation locks | `ResourceMemo` | `core/access.py` | Medium | Low (concurrency correctness-critical) |

---

## 6. Encapsulation Run

This pass makes the module structure, visibility boundaries, and import graph
explicit so that future implementation passes have a concrete target to work
toward.

### 6a. Barrel helper module location

**Decision:** `daf/_barrel.py` at the package root.

**Rationale:**
- Package infrastructure, not domain logic — belongs at the root, not inside
  `core/` or `utils/`.
- Importable by all 7 `__init__.py` files without creating a new package
  dependency or circular import risk.
- `daf/_barrel.py` has zero imports from subpackages, so it is always safe to
  import first.

**After encapsulation, every barrel becomes:**
```python
# e.g. daf/cache/__init__.py
from daf._barrel import _public

from daf.cache.memory import MemoryCache  # noqa: F401

__all__ = _public("MemoryCache",)
```

### 6b. New `daf/utils/` package for cross-cutting primitives

**Decision:** Create `daf/utils/` as an internal package (underscore-prefixed
modules) for memoization and recursion primitives shared across `core/`,
`cache/`, and `algorithms/`.

**Module map:**

```
src/daf/utils/
  __init__.py       ← barrel, imports _public from daf._barrel
  _memoize.py       ← Memo, PureMemo, ResourceMemo (internal)
  _recursion.py     ← TreeCollector, RecursionEngine (internal)
```

**Visibility:**
- `_memoize.py` and `_recursion.py` are internal (underscore prefix).
- `daf/utils/__init__.py` starts with an empty `__all__`. No primitive is
  exposed publicly in this pass; that decision is deferred to a future pass
  when a consumer outside the library needs it.
- Internal import style: `from daf.utils._memoize import Memo`.

**Rationale:**
- `Memo` is needed by `algorithms/dynamic_programming.py` and potentially by
  `core/access.py` (for `ResourceMemo`).
- `TreeCollector` is needed by `cache/memory.py` and potentially by
  `scripts/power_of_ten.py`.
- A shared `utils` namespace prevents either `algorithms/` or `cache/` from
  owning a primitive that the other also needs.

### 6c. Trie extraction

**Decision:** Extract `_TrieNode` and all trie operations from
`cache/memory.py` into `cache/_trie.py`.

**Module map:**

```
src/daf/cache/
  __init__.py       ← barrel, exports only MemoryCache
  _trie.py          ← TrieNode + trie ops (insert, delete, collect, delete_prefix)
  memory.py         ← MemoryCache (imports from _trie)
```

**Visibility:**
- `_trie.py` is internal. It is not re-exported from `cache/__init__.py`.
- `MemoryCache` remains the sole public symbol of `daf.cache`.

**Rationale:**
- `_TrieNode` and its operations (`_trie_insert`, `_trie_delete`,
  `_trie_collect`, `_trie_delete_prefix`) form a cohesive data-structure unit
  that is independent of the LRU/cache policy in `MemoryCache`.
- Separating them makes each file single-responsibility and independently
  testable.

### 6d. Import structure after encapsulation

```
daf/_barrel.py
  └─ no subpackage imports

daf/__init__.py
  ├─ from daf._barrel import _public
  ├─ from daf.core.access import DataAccess
  ├─ from daf.core.errors import ...
  └─ from daf.core.factory import DataAccessFactory

daf/core/__init__.py
  ├─ from daf._barrel import _public
  ├─ from daf.core.access import DataAccess
  ├─ from daf.core.errors import ...
  ├─ from daf.core.factory import DataAccessFactory
  └─ from daf.core.protocols import ...

daf/cache/__init__.py
  ├─ from daf._barrel import _public
  └─ from daf.cache.memory import MemoryCache

daf/cache/_trie.py
  └─ no daf imports (standalone data structure)

daf/cache/memory.py
  ├─ from daf.cache._trie import _TrieNode, _trie_insert, _trie_delete, ...
  └─ (rest of MemoryCache)

daf/algorithms/__init__.py
  ├─ from daf._barrel import _public
  └─ from daf.algorithms.dynamic_programming import FibonacciDP

daf/algorithms/dynamic_programming.py
  └─ from daf.utils._memoize import Memo

daf/utils/__init__.py
  ├─ from daf._barrel import _public
  └─ __all__ = _public()   ← empty; no public primitives yet

daf/utils/_memoize.py
  └─ no daf imports (standalone)

daf/utils/_recursion.py
  └─ no daf imports (standalone)
```

### 6e. Barrel subset invariant

**Current invariant:** `daf.__all__` ⊆ `daf.core.__all__`.

**After encapsulation:** The invariant still holds. `daf.utils` is not added to
either `daf.__all__` or `daf.core.__all__`. The invariant only concerns the
two top-level barrels.

**New test for `test_barrels.py`:** Verify that no `__init__.py` in the package
defines its own `_public` symbol (all must import from `daf._barrel`).

---

## 7. Implementation Order (Recommended)

| Order | Pass | Candidate | Files to touch | Risk | Leverage |
|-------|------|-----------|----------------|------|----------|
| 1 | Barrel helper encapsulation | Shared `_public()` in `daf/_barrel.py` | All 7 `__init__.py` + new `_barrel.py` | Low | High (dedup + contract enforcement) |
| 2 | Trie extraction | `cache/_trie.py` | New `_trie.py`, refactor `memory.py` | Low | Medium (separates data structure from cache policy) |
| 3 | Memo primitive | `Memo` class in `daf/utils/_memoize.py` | New `_memoize.py`, refactor `dynamic_programming.py` | Low | Medium (enables all future algorithm implementations) |
| 4 | Recursion primitive | `TreeCollector` in `daf/utils/_recursion.py` | New `_recursion.py`, refactor `memory.py` | Medium | Medium (unifies DFS/BFS/A* collection) |
| 5 | Memoize `_cache_key` | `PureMemo` or inline memo | `core/access.py` | Low | Low (micro-optimization, but sets pattern) |
| 6 | Memoize generation locks | `ResourceMemo` | `core/access.py` | Medium | Low (concurrency correctness-critical) |

---

## 8. Out of Scope for This Pass

- Actual implementation of any primitive
- Changes to the `Algorithm` protocol signature
- Changes to `QueryInfo` Pydantic model
- Performance benchmarking of memoization impact
- Thread-safety analysis for cross-process generation counters
- Exposing `daf.utils` primitives in the public API (`daf.__all__`)
