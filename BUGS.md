# Known Bugs and Structural Defects

> Red-team assessment findings. Issues are ordered by severity.
> Last updated: 2026-08-15

## 🔴 Critical

### 1. ~~Authorization can be silently disabled~~ FIXED

`DataAccessRouter` now requires `get_current_user` at construction time. Missing callback raises `ValueError`. The adapter composes the authorizer from the provided callback rather than mutating `DataAccess._authorizer` after construction.

**Fix:**
- `src/daf/adapters/fastapi.py:48` — `get_current_user` is a required parameter
- `src/daf/adapters/fastapi.py:60` — raises `ValueError` if missing
- `src/daf/adapters/fastapi.py:72` — constructs a new `DataAccess` with the authorizer pre-set

---

### 2. ~~Cache is not authorization-aware~~ FIXED

Cache key now includes canonical representation of all query inputs: `resource_id` + serialized `filters` + `algorithm` + `user_id`. The cache key is built by `DataAccess._cache_key()` and includes the user identity.

**Fix:**
- `src/daf/core/access.py:56` — `_cache_key()` includes `filters`, `algorithm`, and `user_id`
- `src/daf/core/access.py:97` — cache key used in `_execute_query`

---

### 3. ~~Filters are dead data~~ FIXED

`_execute_query()` now applies `info.filters` in-memory against the retrieved resource before returning.

**Fix:**
- `src/daf/core/access.py:64` — `_apply_filters()` method added
- `src/daf/core/access.py:112` — filters applied after repository lookup

---

### 4. ~~Algorithm selection is semantically broken~~ FIXED

`DataAccess` now accepts an `algorithms: dict[str, Algorithm]` registry. `info.algorithm` is used to look up the algorithm by name from the registry. Cache key includes the algorithm name.

**Fix:**
- `src/daf/core/access.py:35` — `algorithms` parameter in `__init__`
- `src/daf/core/access.py:114` — algorithm lookup by name from registry

---

## 🟠 High

### 5. ~~Resource ID generation is race-prone~~ FIXED

`post()` now delegates ID generation to `repository.create(info.data)`, which returns the generated resource ID. `MemoryRepository.create()` uses `uuid.uuid4()`.

**Fix:**
- `src/daf/core/protocols.py:25` — `create(self, value: T) -> str` added to protocol
- `src/daf/core/access.py:139` — `await self._repository.create(info.data)` replaces `list_all()` + manual ID
- `src/daf/repositories/memory.py:37` — `create()` uses UUID

---
### 6. ~~POST cache invalidation is unnecessarily destructive~~ FIXED

`post()` now calls `cache.delete(cache_key)` for the specific resource instead of `cache.clear()`.

**Fix:**
- `src/daf/core/access.py:145` — per-resource `cache.delete(cache_key)` replaces `cache.clear()`

---

### 7. ~~Exceptions are flattened into opaque error strings~~ FIXED

`DataAccess` no longer catches broad `Exception`. Expected errors (`NotFoundError`, `ValidationError`, `AuthorizationError`) are returned as typed `QueryResult`/`MutationResult` with `error_type` preserved. Unexpected errors propagate as exceptions.

**Fix:**
- `src/daf/contracts/query.py` — `error_type: str | None` added to `QueryResult` and `MutationResult`
- `src/daf/core/access.py:69` — `query()` catches only `AuthorizationError`, `NotFoundError`, `ValidationError`
- `src/daf/core/access.py:123` — `post()` catches only `ValidationError`, `AuthorizationError`

---

### 8. ~~Error messages can leak internal information~~ FIXED

`DataAccess` returns sanitized error messages (`"Not found"`, `"Validation error"`, `"Unauthorized"`). Raw `str(error)` is no longer included in external responses. The FastAPI adapter catches `DataAccessError` subclasses and maps to HTTP 500 with `"Internal server error"`.

**Fix:**
- `src/daf/core/access.py:76` — error messages are user-safe strings
- `src/daf/adapters/fastapi.py:144` — `HTTPException(status_code=500, detail="Internal server error")`

---

### 9. ~~FastAPI adapter mutates private implementation detail~~ FIXED

`DataAccessRouter` no longer assigns to `self._daf._authorizer`. Instead, it constructs a new `DataAccess` instance with the authorizer pre-set at construction time.

**Fix:**
- `src/daf/adapters/fastapi.py:72` — `DataAccessRouter.__init__` builds authorizer and constructs new `DataAccess`

---

## 🟡 Medium

### 10. `datetime.utcnow()` deprecated territory FIXED

`QueryResult` and `MutationResult` now use `datetime.now(UTC)` instead of `datetime.utcnow()`.

**Fix:**
- `src/daf/contracts/query.py:98` — `default_factory=lambda: datetime.now(UTC)`

---

### 11. Repository abstraction forces `list_all()` FIXED

`list_all()` has been removed from the `Repository` protocol and replaced with `create(self, value: T) -> str`. The repository owns identity generation.

**Fix:**
- `src/daf/core/protocols.py:25` — `create()` added, `list_all()` removed
- `src/daf/repositories/memory.py:37` — `create()` uses UUID

---

### 12. Query cache now includes query semantics

The cache key is now a canonical representation of all inputs: `resource_id` + serialized `filters` + `algorithm` + `user_id`.

**Fix:**
- `src/daf/core/access.py:56` — `_cache_key()` builds canonical key from all query semantics

---

### 13. POST uses per-resource cache invalidation

`post()` now calls `cache.delete(cache_key)` for the specific resource instead of `cache.clear()`.

**Fix:**
- `src/daf/core/access.py:145` — per-resource invalidation

---

### 14. Authorizer leaks resource existence via enumeration FIXED

`_make_authorizer` no longer calls `repository.get()` and raises `NotFoundError`. Instead, it only checks ownership for existing dict resources. Non-existent resources fall through to `_execute_query()`, which raises `NotFoundError` after the authorization check. This removes the timing side channel that allowed attackers to distinguish missing resources from forbidden ones.

**Fix:**
- `src/daf/adapters/fastapi.py:108` — authorizer skips existence check, only validates ownership for dict data

---

### 15. GET endpoint ignores query parameters FIXED

`_setup_query_route` now reads `filters` and `algorithm` from `Request.query_params`. Filters are parsed as JSON if provided.

**Fix:**
- `src/daf/adapters/fastapi.py:137` — `filters` and `algorithm` extracted from query parameters

---

### 16. `_apply_filters` returns non-dict data when filters are present FIXED

When filters are provided but data is not a dict, `_apply_filters` now returns `{}` to indicate no match, instead of silently returning the raw data.

**Fix:**
- `src/daf/core/access.py:65` — returns `{}` when filters present and data is not a dict

---

### 17. `_cache_key` crashes on non-JSON-serializable filters FIXED

`_cache_key` now wraps `json.dumps` in try/except and raises `ValidationError` with a user-safe message when filters contain non-serializable objects.

**Fix:**
- `src/daf/core/access.py:58` — catches `TypeError`/`ValueError` and raises `ValidationError`

---

### 18. Missing input validation on operations FIXED

`DataAccess` methods now validate inputs:
- `resource_id` must be a non-empty string
- `post()` and `put()` `data` must be a dict
- Raises `ValidationError` with user-safe messages on bad input

**Fix:**
- `src/daf/core/access.py:84` — `query()` validates `resource_id`
- `src/daf/core/access.py:168` — `post()` validates `resource_type` and `data`
- `src/daf/core/access.py:207` — `put()` validates `resource_id` and `data`
- `src/daf/core/access.py:265` — `delete()` validates `resource_id`

---

### 19. `post()` drops `resource_type` FIXED

`MutationResult.data` now includes `resource_type` in the returned data.

**Fix:**
- `src/daf/core/access.py:199` — `data={"id": resource_id, "resource_type": info.resource_type, **info.data}`

---

### 20. `DataAccessRouter.__init__` reaches into `daf` private state FIXED

`DataAccessRouter` now uses `daf.get_components()` to extract repository, cache, and algorithms, instead of reading `daf._repository`, `daf._cache`, and `daf._algorithms` directly.

**Fix:**
- `src/daf/core/access.py:52` — `get_components()` method added
- `src/daf/adapters/fastapi.py:71` — uses `daf.get_components()`

---

### 21. `put_endpoint` mutates validated Pydantic model FIXED

`_setup_put_route` now constructs a new `PutInfo` instance instead of mutating the validated model in-place.

**Fix:**
- `src/daf/adapters/fastapi.py:172` — `info = PutInfo(resource_id=resource_id, data=info.data)`

---

### 22. ~~No structured logging~~ FIXED

`logging.getLogger(__name__)` added to `DataAccess`, `DataAccessRouter`, `MemoryRepository`, and `MemoryCache`. Logs at DEBUG for normal flow, WARNING for recoverable errors, and ERROR for failures. Structured logging uses `extra={...}` dict instead of string interpolation.

**Fix:**
- `src/daf/core/access.py` — logs query, mutation, cache, and algorithm events with `extra` dict
- `src/daf/adapters/fastapi.py` — logs route handling and errors with `extra` dict
- `src/daf/repositories/memory.py` — logs get/save/delete/create with `extra` dict
- `src/daf/cache/memory.py` — logs get/set/delete/delete_prefix/has/clear with `extra` dict

---

## Phase 2 Fixes (Invariant Composition)

### 23. ~~Cache invalidation does not cover all derived projections~~ FIXED

`put()` and `delete()` previously deleted only a single exact cache key, leaving stale entries for other filter/algorithm/user combinations. Now they use `cache.delete_prefix(f"query:{id}:")` to invalidate all derived projections atomically.

**Fix:**
- `src/daf/core/protocols.py:41` — `delete_prefix(self, prefix: str)` added to `Cache` protocol
- `src/daf/cache/memory.py:48` — `delete_prefix()` implemented with `str.startswith` scan
- `src/daf/core/access.py:305` — `put()` calls `delete_prefix` instead of single `delete`
- `src/daf/core/access.py:388` — `delete()` calls `delete_prefix` instead of single `delete`

---

### 24. ~~Authorization and mutation reads are not atomic~~ FIXED

`put()` and `delete()` previously performed two separate repository reads: one in the authorizer and another to fetch data for mutation. This created a TOCTOU race condition. Now the resource is read once, passed to the authorizer via `data` parameter, and the same object is used for mutation.

**Fix:**
- `src/daf/core/protocols.py:64` — `Authorizer.authorize()` gained optional `data` parameter
- `src/daf/core/access.py:48` — `_check_authorization()` passes `data` through
- `src/daf/core/access.py:264` — `put()` reads resource first, authorizes with data, then mutates
- `src/daf/core/access.py:376` — `delete()` reads resource first, authorizes with data, then deletes
- `src/daf/adapters/fastapi.py:96` — `_make_authorizer` uses `data` when provided, falls back to repository read for queries
- `tests/integration/test_security_invariants.py:403` — atomic auth+read tests added

---

### 25. ~~Unknown algorithm silently returns raw data~~ FIXED

Querying with an algorithm name not present in the registry silently returned raw repository data with `success=True`. Now `_execute_query()` raises `ValidationError("Unknown algorithm: {name}")`, which flows through the existing `except ValidationError` handler and returns `error_type="validation"`.

**Fix:**
- `src/daf/core/access.py:196` — raises `ValidationError` for unknown algorithm names
- `tests/integration/test_fastapi_adapter.py:214` — updated to expect `error_type="validation"`
- `tests/integration/test_security_invariants.py:515` — new `TestUnknownAlgorithmValidation`

---

## Adversarial Hardening (PR18)

### 26. ~~Trie memory amplification~~ FIXED

`_TrieNode` previously stored a `keys: set[str]` at every node along every key's path. For N keys of average length L, this stored O(N × L) redundant string references in addition to `_cache`. Fixed by storing keys only at terminal nodes; intermediate nodes carry only `children`.

**Fix:**
- `src/daf/cache/memory.py:14` — `_TrieNode` now has `children` and `key` (singular, `str | None`)
- `src/daf/cache/memory.py:147` — `_dfs_collect` DFS helper collects terminal keys
- `src/daf/cache/memory.py:157` — `_trie_insert` sets `node.key = key` at terminal node only
- `src/daf/cache/memory.py:164` — `_trie_delete` clears `node.key` and prunes empty ancestors
- `src/daf/cache/memory.py:193` — `_trie_delete_prefix` detaches subtree and returns terminal keys

---

### 27. ~~Canonical node ID ignores graph~~ FIXED

`_canonical_node_id()` previously validated `file_to_node_id(path)` against the graph but still returned the hand-rolled ID on mismatch. Fixed to return the first graph node's ID when the graph contains a matching `source_file`.

**Fix:**
- `scripts/graphify_affected.py:62` — collect all nodes matching `source_file == path`
- `scripts/graphify_affected.py:72` — prefer exact module-level match, else return first graph node's ID
- `scripts/graphify_affected.py:66` — warn and fall back to hand-rolled only when no graph match exists

---

### 28. ~~Missing base SHA produces false-green CI~~ FIXED

`changed_files()` returned `[]` when the base ref was missing, causing `main()` to print "No Python files changed" and exit 0. Fixed to raise `RuntimeError` on missing base SHA.

**Fix:**
- `scripts/graphify_affected.py:26` — `changed_files()` raises `RuntimeError` on missing base
- `scripts/graphify_affected.py:168` — `main()` catches `RuntimeError` and returns 1

---

### 29. ~~Graph JSON schema not validated~~ FIXED

`main()` did not validate graph JSON structure; malformed output caused misleading "no impacted test files detected" behavior. Fixed with `_validate_graph_schema()`.

**Fix:**
- `scripts/graphify_affected.py:119` — `_validate_graph_schema()` validates `nodes` list and node fields
- `scripts/graphify_affected.py:146` — `main()` validates schema before processing

---

### 30. `httpx` deprecation warning in tests FIXED

Test suite emits `StarletteDeprecationWarning: Using httpx with starlette.testclient is deprecated; install httpx2 instead`. Fixed by upgrading `httpx>=0.27` to `httpx2>=0.27` in dev and optional-dependencies.

**Fix:**
- `pyproject.toml:49` — `httpx2>=0.27`
- `pyproject.toml:73` — `httpx2>=0.27`

---

### 31. `_astar_collect` LCP score does not reset on mismatch FIXED

With keys `["xabc", "abc"]` and target `"abc"`, the path `x→a→b→c` scored `match_len=3` because after the root mismatch (`x` ≠ `a`), the algorithm incremented when child `a` happened to match `target[0]`. The LCP of `"xabc"` with `"abc"` is 0, not 3. Fixed by tracking depth in each heap entry and only extending `match_len` when `match_len == depth` (no mismatch yet).

**Fix:**
- `src/daf/cache/memory.py:170` — heap tuple expanded to `(priority, tiebreaker, node, depth, match_len)`
- `src/daf/cache/memory.py:186` — child only extends match if `match_len == depth` and `ch == target[match_len]`

---

### 32. `_canonical_node_id` non-deterministic selection FIXED

When multiple nodes share `source_file` and none matches hand-rolled ID, `matches[0]` depends on graphify output ordering. Fixed by sorting matching nodes by `id` before selecting the first.

**Fix:**
- `scripts/graphify_affected.py:62` — `matches.sort(key=lambda n: n.get("id", ""))`

---

### 33. `graphify_affected.py` schema validation gaps FIXED

Validation checked `isinstance(nodes, list)` and key presence, but not types, non-emptiness, or uniqueness. Fixed with type checks, non-empty string checks, and a second-pass uniqueness check.

**Fix:**
- `scripts/graphify_affected.py:120` — `_validate_graph_schema` validates types, non-emptiness, and uniqueness

---

### 34. `git diff` subprocess failure asymmetry FIXED

`git rev-parse` failure raises `RuntimeError`, but `git diff` failure raised raw `CalledProcessError`. Fixed by wrapping `git diff` in try/except and raising `RuntimeError` with stderr context.

**Fix:**
- `scripts/graphify_affected.py:37` — `git diff` wrapped in try/except

---

### 35. CI graphify job duplicates graph extraction FIXED

CI ran `graphify extract` then `graphify_report.py`, which runs `graphify extract` again. Fixed by removing the redundant explicit extraction step.

**Fix:**
- `.github/workflows/ci.yml:55` — removed `uv run python -m graphify extract . --code-only --no-cluster`
- `.github/workflows/ci.yml:52` — added `fetch-depth: 0` to checkout step

---

### 36. `_generation_locks` unbounded dict FIXED

`_generation_locks` was an unbounded `dict[str, asyncio.Lock]` with the same cardinality risk as the removed `_namespace_cache`. Replaced with a fixed-size lock-striping array (N=16) using hash-mod indexing on the resource namespace.

**Fix:**
- `src/daf/core/access.py:112` — `_generation_locks` replaced with `_generation_locks_memo = ResourceMemo(key_fn=..., factory=...)`
- `src/daf/utils/_memoize.py` — `ResourceMemo` provides bounded lazy-init memoization

---

### 37. Bounded LRU can evict `_daf_gen:*` metadata FIXED

When `max_size > 0`, LRU eviction could remove `_daf_gen:<namespace>` while leaving the query cache entry. The old `_current_generation` silently returned 0 for missing keys, serving potentially stale data. Fixed by raising `GenerationKeyError` when the generation key is absent; callers treat this as a cache miss.

**Fix:**
- `src/daf/core/errors.py:34` — `GenerationKeyError(CacheError)` added
- `src/daf/core/access.py:168` — `_current_generation` raises `GenerationKeyError` on missing/non-int value
- `src/daf/core/access.py:288` — `_execute_query` catches `GenerationKeyError` → cache miss
- `src/daf/core/access.py:330` — `_execute_cache_miss` catches `GenerationKeyError` → gen=0, writes generation key

---

### 38. Multi-index invariant not documented FIXED

`MemoryCache.set`/`delete`/`delete_prefix`/`shake`/`clear` must not `await` between updates to `_cache`, `_trie`, and `_lru`. Added explicit atomicity note to `MemoryCache` class docstring.

**Fix:**
- `src/daf/cache/memory.py:31` — class docstring documents no-await-between-indexes invariant

---

### 39. BFS uses O(n²) list.pop(0) FIXED

`TreeCollector._bfs` used `list.pop(0)` on a growing queue, yielding O(n²) worst-case traversal. Replaced with `collections.deque` and `popleft()` for O(1) per operation.

**Fix:**
- `src/daf/utils/_recursion.py:14` — `from collections import deque`
- `src/daf/utils/_recursion.py:72` — `queue: deque = deque([root])` + `popleft()`

---

### 40. A* / BFS architectural homeless FIXED

A* and BFS collectors accumulated algorithmic capability without a production consumer, expanding the trusted surface unjustifiably. Marked both as experimental in docstrings.

**Fix:**
- `src/daf/cache/memory.py:160` — `_bfs_collect` docstring notes "**Experimental** — no production consumer yet."
- `src/daf/cache/memory.py:170` — `_astar_collect` docstring notes "**Experimental** — no production consumer yet."

---

### 41. `_canonical_node_id` does not validate graph schema FIXED

`_canonical_node_id` loaded graph JSON without schema validation; malformed graphs fell through to `file_to_node_id` with a plausible but unverified ID. Fixed to call `_validate_graph_schema` and return `None` on validation failure.

**Fix:**
- `scripts/graphify_affected.py:65` — `_validate_graph_schema(data)` called after `json.loads`
- `scripts/graphify_affected.py:67` — `RuntimeError` from validation caught, returns `None`

---

### 42. `graphify_affected.py` scope narrower than advertised FIXED

Module docstring implied "changed files" without specifying the `.py`-only filter. Updated docstring to state scope explicitly and note that CI still runs the full suite.

**Fix:**
- `scripts/graphify_affected.py:2` — docstring documents `.py`-only scope and CI full-suite guarantee

---

### 43. `TreeCollector` `astar` strategy is broken FIXED

`TreeCollector._astar` cannot correctly implement LCP matching because its API (`key_extractor(node) -> str | None`, `children_extractor(node) -> Iterable`) does not expose path characters. The condition `self._key_extractor(child) is not None` checks whether a child is terminal, not whether its character matches the target. `MemoryCache._astar_collect` already exists with the correct implementation. No code uses `TreeCollector(strategy="astar")`. Removed `astar` strategy and `_astar` method.

**Fix:**
- `src/daf/utils/_recursion.py` — removed `_astar` method and `heapq` import
- `src/daf/utils/_recursion.py` — `collect()` only dispatches to `dfs` and `bfs`

---

### 44. Dead code in `_memoize.py` FIXED

`memoize` decorator and `PureMemo`/`_make_key` were new code with zero consumers, expanding the attack surface and confusing readers. Removed entirely.

**Fix:**
- `src/daf/utils/_memoize.py` — removed `memoize`, `PureMemo`, `_make_key`
- `src/daf/utils/_memoize.py` — removed unused `hashlib`, `Awaitable`/`Callable` imports

---

### 45. Unused `heapq` import in `_trie.py` FIXED

`heapq` was imported but never used in the trie implementation.

**Fix:**
- `src/daf/cache/_trie.py` — removed unused `heapq` import

---

## Missing Test Dimensions

The existing tests validate component behavior well, but the following interaction dimensions are now covered:

- ✅ Authorization × cache (different users get different cache entries)
- ✅ Algorithm × cache (different algorithms produce different cache keys)
- ✅ Filters × cache (different filters produce different cache keys)
- ✅ Concurrency × POST (ID collision)
- ✅ Cache × non-dict data (filters on non-dict return empty)
- ✅ Cache × non-serializable filters (returns validation error)
- ✅ Auth × non-existent resource (returns not_found for authenticated users)
- ✅ Input validation (empty resource_id, malformed data)
- ✅ GET query parameters (filters and algorithm passed via query string)
- ✅ PUT mutation of request model (new instance instead of in-place mutation)
- ✅ Repository failure × cache
- ✅ Cache failure × mutation
- ✅ Concurrent PUT × query
- ✅ Concurrent DELETE × query
- ✅ Multiple users × same resource
- ✅ Multiple tenants × same resource ID
- ✅ Prefix cache invalidation (PUT/DELETE invalidate all projections)
- ✅ Atomic auth+read (single repository read for mutation operations)
- ✅ Unknown algorithm validation (returns validation error)
- ✅ Stale query interleaving after mutation (stale entry rejected by generation comparison)
- ✅ Concurrent mutation generation monotonicity (per-resource lock serialization)

---

## Core Finding

The repository has a clean conceptual decomposition (Repository / Cache / Algorithm / Authorizer → DataAccess → FastAPI adapter), but the state-space represented by the interfaces is larger than the state-space represented by the cache and orchestration invariants.

Inputs include `resource_id`, `filters`, `algorithm`, and `user`, but the cache only models `resource_id`. Failures include `NotFound`, `Validation`, `Repository`, `Cache`, `Algorithm`, `Authorization`, and unexpected, but query orchestration largely models `success / error string`. Operations are concurrent and async, but the repository contract does not model atomicity or transactions.

The system is modular structurally, but not yet invariant-complete operationally.
