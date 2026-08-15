# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- 3 new `TestMemoryCache` tests: `test_shake_empty_prefix_removes_all_keys`, `test_delete_prefix_empty_removes_all_keys`, `test_trie_prunes_empty_branches_after_delete`
- `graphify_affected.py` raises `RuntimeError` with stderr context on subprocess failure

### Changed

- `DataAccess._resource_namespace` computes SHA-256 inline (no `_namespace_cache` dict)
- `MemoryCache._trie_delete` tracks insertion path and prunes empty branches bottom-up
- `MemoryCache._trie_insert` and `_trie_delete` include root node in key tracking

### Removed

- `DataAccess._namespace_cache` (unbounded dict eliminated as P1 memory-growth risk)
- `TestDataAccessNamespaceCache` (2 tests; cached-namespace behavior no longer exists)
- Duplicate silent `graphify diagnose multigraph` invocation from `scripts/graphify_report.py`

### Fixed

- Empty-prefix semantic regression in `MemoryCache` prefix trie: `shake("")` and `delete_prefix("")` now operate on entire cache (P0)
- Trie never pruned empty branches after key deletion, causing unbounded structural memory growth (P1)
- Unbounded `_namespace_cache` dict in `DataAccess` grows without limit (P1)
- Duplicate `graphify diagnose multigraph` invocation in `graphify_report.py` masks first-call failures (P2)
- `graphify_affected.py` `affected()` swallows subprocess failures, printing "No impacted test files detected" on error (P2)

### Security

- Removed unbounded in-process cache of `resource_id → sha256` mappings (memory-growth / potential DoS vector)
- Subprocess failures in CI tooling now propagate as errors instead of being silently ignored

## [0.2.0] - 2026-08-14

### Added

- GitHub Actions CI workflow (`.github/workflows/ci.yml`)
- GitHub Actions PyPI publish workflow (`.github/workflows/publish.yml`)
- Package renamed to `thedaf` for PyPI distribution
- `twine` verification step in publish script
- `Authorizer` protocol for pluggable access control
- `AuthorizationError` exception for denied access
- Optional `authorizer` parameter on `DataAccess` and `DataAccessFactory`
- Optional `user` parameter on `DataAccess.query`, `post`, `put`, `delete`
- FastAPI adapter closure-based ownership authorizer
- HTTP 403 translation for authorization failures
- HTTP 404 translation for missing resources in adapter
- `tests/integration/test_authorization.py` for IDOR prevention scenarios
- `TestAuthorization` class in `tests/integration/test_fastapi_adapter.py`
- `TestAuthorizerProtocol` class in `tests/unit/test_components.py`
- `pydantic.mypy` plugin configuration for mypy strict compliance
- `py.typed` marker for PEP 561 typed package distribution
- `tests/integration/test_security_invariants.py` for security and cache interaction tests
- Structured logging (`logging.getLogger(__name__)`) to `DataAccess`, `DataAccessRouter`, `MemoryRepository`, and `MemoryCache`
- `DataAccess.get_components()` public method to decouple adapter from private state
- Cache-aware canonical key generation including `filters`, `algorithm`, and `user_id`
- In-memory filter application in `_apply_filters`
- Per-resource cache invalidation in `post()`, `put()`, and `delete()`
- Input validation guards for `resource_id`, `data`, and `resource_type`
- `resource_type` preservation in `MutationResult.data` for POST operations
- GET query parameter support for `filters` (JSON) and `algorithm` in FastAPI adapter
- `Repository.try_update` and `Repository.try_delete` CAS primitives
- `MemoryRepository.try_update` and `try_delete` with coarse lock and identity comparison
- SHA-256 canonical JSON cache keys to prevent delimiter-collision attacks
- Re-authorization on cache hit before returning cached data
- POST authorizer receives proposed creation `data` for pre-persistence policy checks
- Fail-closed authorization: non-dict resource data is denied access
- `conflict` error_type for CAS failures in PUT and DELETE
- Core operations raise typed exceptions instead of returning error envelopes
- Cache keys include `resource_id` prefix for scoped invalidation
- `MemoryRepository.get()` and `MemoryCache.get()` return deep copies for all value types
- `MemoryRepository.save()` and `create()` store deep copies (no caller reference retention)
- `MemoryCache.set()` stores deep copies (no caller reference retention)
- `DeprecationWarning` for `str(user)` fallback when user lacks `.id` attribute
- `Repository` and `Cache` protocols document ownership/value-isolation contract
- `Authorizer` protocol documents `user.id` stability requirement
- FastAPI adapter `_handle_daf_error` helper for consolidated error translation
- Cache entries store `{"raw": ..., "transformed": ...}` to preserve raw data for re-authorization
- `MemoryCache.shake(prefix) -> int` removes all keys under a prefix and returns removal count
- `Cache` protocol exposes `shake()` for proactive stale-branch pruning
- `DataAccess._superedge_invalidate()` atomically collapses query keys, generation key, and generation advancement under per-resource lock
- `put()` and `delete()` replace two-step `delete_prefix + _advance_generation` with single `_superedge_invalidate()` call
- `MemoryCache._delete_prefix_impl()` factors key-scan logic shared by `delete_prefix` and `shake`
- `graphifyy>=0.9.42` runtime dependency for graph extraction and multigraph diagnostics
- GitHub Actions `graphify` CI job running `graphify extract` and `graphify diagnose multigraph --json`
- `graphify-out/` and `graph.json` in `.gitignore`
- `_public(*names)` helper added to all 7 barrel `__init__.py` files for mechanical consistency
- `tests/unit/test_barrels.py` with subset and import-invariant assertions
- `DataAccess._namespace_cache` for cached SHA-256 namespace hashing
- `MemoryCache._TrieNode` prefix trie for O(prefix_len) prefix collection
- `scripts/graphify_report.py` automates graphify extract → diagnose → tree → callflow pipeline
- `scripts/graphify_affected.py` maps changed files to impacted test files using graphify `affected`
- CI graphify job uploads `diagnose.json`, `GRAPH_TREE.html`, and `theDAF-callflow.html` as artifacts

### Changed

- `DataAccessRouter` now requires `get_current_user` at construction time; raises `ValueError` if missing
- `DataAccess.query()` now validates `resource_id` before executing
- `_apply_filters` returns `{}` when filters are present but data is not a dict
- `_cache_key` raises `ValidationError` for non-JSON-serializable filters instead of crashing
- FastAPI adapter authorizer skips existence check to prevent resource enumeration side-channel attacks
- FastAPI PUT endpoint constructs new `PutInfo` instance instead of mutating validated model in-place
- `MutationResult.data` now includes `resource_type` for POST operations
- Cache invalidation uses prefix-based deletion (`delete_prefix`) instead of tracked key map
- PUT and DELETE perform single repository read for auth and mutation (atomic auth+read)
- Cache keys now use `query:{resource_id}:{digest}` format for scoped invalidation
- QUERY performs single repository read on cache miss (auth after read, not before)
- FastAPI adapter maps `AuthorizationError`→403 and `NotFoundError`→404 in all route handlers
- Core `DataAccess` methods raise exceptions for auth/not-found instead of returning error envelopes
- `MemoryRepository.try_update`/`try_delete` use equality comparison for dict values to preserve CAS with deep copies
- `_execute_query` authorizer receives raw repository data on both cache-miss and cache-hit paths
- `post()` no-op `delete_prefix` removed (no prior query cache entries exist for a new resource)
- FastAPI adapter error translation consolidated into single `_handle_daf_error` method
- `MemoryRepository.try_update()` returns an independent deep copy of the updated value
- `MemoryRepository` and `MemoryCache` document deepcopy-able value constraint
- `Repository` and `Cache` protocols document deepcopy-able value constraint
- `Algorithm` protocol documents immutability contract (execute() must not mutate input)
- `DataAccess` module docstring documents write-through-DAF consistency boundary
- `DataAccess.__init__` docstring documents existence-disclosure behavior (404 vs 403)
- README Authorization Boundary documents existence-disclosure and masking layer option
- README Architecture section documents write-through-DAF consistency boundary
- Cache entries extend to `{"raw": ..., "transformed": ..., "generation": N}` for temporal invalidation
- Per-resource generation counters scoped in shared cache via reserved `_daf_gen:{namespace}` keys
- `_generation_lock` helper with lazy per-resource `asyncio.Lock` creation for generation advancement serialization
- Controlled-concurrency tests for stale query interleaving and concurrent mutation generation monotonicity
- Cache key format uses `query:{sha256(resource_id)}:{digest}` to prevent delimiter-collision attacks
- Invalidation prefixes use hashed namespace so `a:b` and `a:b:c` are structurally isolated
- `_execute_cache_miss` deepcopies repository data before algorithm execution to prevent auth-snapshot poisoning
- `post()` advances per-resource generation for newly created resources
- `Cache` protocol now requires `shake()` method (breaking interface change; acceptable pre-1.0)
- Mutation invalidation uses atomic `_superedge_invalidate` instead of two-step `delete_prefix + _advance_generation`
- `_advance_generation` read-modify-write moved inside `_superedge_invalidate` under per-resource lock to prevent lost increments when generation key is absent
- 6 new tests: `shake` unit tests (4) and `_superedge_invalidate` integration tests (2); 127 tests total
- All 7 barrel `__init__.py` files use `_public` helper instead of raw `__all__ = [...]`
- `daf/__init__.py` documents design intent: curated public subset of `daf.core`
- `_resource_namespace` caches SHA-256 hashes for repeated calls
- `MemoryCache` uses prefix trie for O(prefix_len) prefix collection
- 9 new tests: 5 trie tests, 2 namespace-cache tests, 2 barrel-consistency tests; 136 tests total

### Fixed

- Core operations return typed exceptions instead of error envelopes (R1)
- QUERY performs two reads on cache miss; cache hits miss data-aware auth (R2)
- Cache invalidation does not cover all derived projections (R3)
- Cache key format is opaque SHA-256 hash without resource_id scope (R3)
- `_cache_key_map` is local to each DataAccess instance, breaking shared caches (R3)
- `MemoryRepository.get()` and `MemoryCache.get()` return mutable direct references (R4)
- `_user_id()` falls back to `str(user)` with no stability or uniqueness guarantee (R5)
- POST authorization policy is implicit; authorizer receives no data for creation decisions (R6)
- Resource enumeration via authorizer existence check (R1)
- GET endpoint hardcoded `filters=None, algorithm=None` (R2)
- `_apply_filters` returning non-dict data when filters present (R3)
- `_cache_key` crash on non-JSON-serializable filters (R4)
- Missing input validation on query/post/put/delete (R5)
- `post()` dropping `resource_type` from result (R6)
- `DataAccessRouter` reaching into `DataAccess` private state (R7)
- PUT endpoint mutating validated Pydantic model (R8)
- No structured logging in core components (R9)
- POST ownership bypass due to `None` resource_id short-circuit (R11)
- TOCTOU race between authorization and mutation in PUT/DELETE (R12)
- Cache-key collision via delimiter injection (R13)
- Stale authorization grants on cache hits (R14)
- Query pre-authorization without data causes redundant repository reads (R15)
- Cache hit re-authorization without data on cache misses (R16)
- Non-dict mutable values (list, set, custom objects) escape deep-copy isolation (R17)
- `save()`/`create()` retain caller object references in MemoryRepository (R17)
- `set()` retains caller object reference in MemoryCache (R17)
- Authorizer receives transformed/post-algorithm data on cache hit instead of raw repository data (R18)
- No-op `delete_prefix` in `post()` after creating new resource (R19)
- FastAPI route handlers duplicate error translation logic (R20)
- `try_update()` returns direct reference to new value, allowing caller mutation of stored state (R21)
- Existence-disclosure behavior (403 vs 404) undocumented as intentional security model (R22)
- Direct repository writes bypass DAF cache invalidation (R23)
- Stale cache resurrection after mutation via concurrent query repopulation race (R24)
- Deepcopy-able value constraint undocumented for Repository/Cache/Memory implementations (R25)
- Algorithm immutability contract undocumented; in-place mutation can corrupt raw auth data (R26)
- Generation counter local to DataAccess instance; stale cache resurrection across instances sharing cache (R19b)
- Cache invalidation prefix collides when resource_id contains `:` delimiter (R3b)
- Algorithm input and raw_data share reference; in-place mutation poisons authorization snapshot (R21b)
- Global generation counter advances on any mutation, invalidating unrelated resource caches (R19c)
- `_advance_generation` read-modify-write is non-atomic; concurrent mutations can lose increments (R22)
- Query/mutation interleaving untested; stale query can repopulate cache after mutation (R23)
- Empty-prefix semantic regression in `MemoryCache` prefix trie: `shake("")` and `delete_prefix("")` return empty set instead of all keys (P0)
- Trie never prunes empty branches after key deletion, causing unbounded structural memory growth (P1)
- Unbounded `_namespace_cache` dict in `DataAccess` grows without limit (P1)
- Duplicate silent `graphify diagnose multigraph` invocation in `graphify_report.py` masks failures (P2)
- `graphify_affected.py` `affected()` swallows subprocess failures, printing "No impacted test files detected" on error (P2)

### Security

- Core raises AuthorizationError/NotFoundError; FastAPI maps to 403/404
- Removed timing side channel in authorizer that allowed distinguishing missing vs forbidden resources
- Added input validation to prevent malformed requests from reaching repository layer
- Added structured logging for audit trail and debugging
- Fail-closed authorization rejects non-dict resources instead of silently granting access
- POST creation payloads are inspectable by authorizer before persistence
- Cached query results are re-authorized on every hit to prevent revoked-access bypass
- CAS mutations prevent lost-update races between auth and persistence
- Repository and cache return independent copies to prevent mutation of internal state
- user.id contract documented; str(user) fallback emits DeprecationWarning
- Prefix-based cache invalidation prevents stale entries across filter/algorithm projections
- Authorizer always receives raw repository data, ensuring consistent ownership decisions regardless of cache state
- `try_update()` returns independent deep copy, formalizing mutation-return ownership boundary
- Existence-disclosure behavior documented as intentional security model property
- Write-through-DAF consistency boundary documented; direct repository writes bypass invalidation
- Generation counter prevents stale cache resurrection after mutations
- Per-resource generation scoped in shared cache prevents cross-instance stale resurrection
- Algorithm immutability contract documented; algorithms must not mutate their input snapshot
- Cache key namespace hashing prevents delimiter-collision attacks on invalidation prefixes
- Deep copy of repository data before algorithm execution prevents auth-snapshot poisoning
- Per-resource asyncio locks serialize generation advancement within a single process, eliminating RMW races
- Concurrency model documented: delete_prefix is authoritative invalidation, generation is best-effort fast-path
- Controlled-concurrency tests prove stale query interleaving is rejected and concurrent mutations are monotonic
- Superedge collapse reads generation under lock before deletion, preventing lost increments and ensuring atomic invalidation
- AST tree shaking enables proactive stale-branch pruning via `shake()` on cache backends that support prefix traversal
- graphifyy multigraph diagnostics in CI detect same-endpoint edge-collapse risk in dependency graph
- CI enforces `directed_same_endpoint_collapsed_edges` threshold at 30 (first-run baseline 26); diagnose output written to `graphify-out/diagnose.json`

## [0.1.0] - 2026-08-13

### Added

- Initial production-ready release
- Core `DataAccess` orchestration layer with async CRUD operations
- `DataAccessFactory` for dependency composition
- `Repository` protocol with `MemoryRepository` reference implementation
- `Cache` protocol with `MemoryCache` reference implementation
- `Algorithm` protocol with `FibonacciDP` (explicit memoization)
- Pydantic v2 data contracts: `QueryInfo`, `PostInfo`, `PutInfo`, `DeleteInfo`, `QueryResult`, `MutationResult`
- FastAPI adapter (`DataAccessRouter`) with rate limiting
- Domain exception hierarchy: `DataAccessError`, `NotFoundError`, `ValidationError`, `RepositoryError`, `CacheError`, `AlgorithmError`
- 50 passing tests (17 unit + 25 integration + 8 end-to-end)
- Comprehensive type hints with mypy strict mode compliance
- Ruff linting configuration (E, F, I, B, UP, SIM rules)
- MIT License
- Full documentation in README.md
- Build artifacts: wheel + source distribution
- PyPI submission infrastructure (PUBLISH.sh, PYPI_SUBMISSION.md, BUILD_REPORT.txt)

### Architecture

- Zero FastAPI imports in core layer
- Protocol-based dependency injection
- Factory pattern for composition
- Explicit memoization (not functools decorator)
- Pydantic contracts at boundary only
- Rate limiting isolated to FastAPI adapter
- PEP 695 generic syntax (Python 3.12+)
- Power of Ten safety-critical coding rules adapted for Python

### Power of Ten Python Adaptation

- Rule 1: No recursion (AST-checked)
- Rule 2: Loop bounds (AST-checked for unbounded while)
- Rule 3: No dynamic allocation after init (AST-checked + Bandit S rules)
- Rule 4: Function length ≤ 60 lines (AST-checked)
- Rule 5: Validation density ≥ 1 per non-trivial function (AST-checked)
- Rule 6: Smallest variable scope (AST-checked)
- Rule 7: Return values and parameters validated (ARG/RET rules)
- Rule 8: Preprocessor limited (N/A for Python, exec/eval banned)
- Rule 9: No pointer-like operations (AST-checked for ctypes/id)
- Rule 10: Zero warnings (Ruff + mypy + pytest + custom checker)

### Quality Assurance

- mypy strict mode: 0 errors across 17 source files
- Ruff linting: 0 errors
- pytest: 50/50 tests passing
- Power of Ten checks: All pass
- Build verified in clean environment
- Installation verified from wheel

[0.1.0]: https://github.com/RAliane-REBORN/theDAF/releases/tag/v0.1.0
