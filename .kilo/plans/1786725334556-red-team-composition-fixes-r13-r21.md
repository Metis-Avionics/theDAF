# Red Team Composition Fixes: R13-R21

## Context

PR #17 at `2027904` (fix: R7-R12 red-team composition fixes) passes local CI (112 tests, mypy --strict, ruff, power-of-ten). This plan addresses 9 new findings from a fresh red-team adversarial pass against the post-R7-R12 codebase.

The previous pass (R7-R12) fixed component and interaction invariants. This pass focuses on ownership semantics, security-model clarity, and temporal cache correctness.

All 112 tests currently pass locally.

---

## Design Decisions (locked)

| Finding | Decision |
|---------|----------|
| R13 | `try_update()` and `try_delete()` return deep copies, formalizing the ownership boundary. |
| R14 | Document existence-disclosure behavior (403 vs 404) as an intentional security-model property. |
| R15 | Deferred: authorization-policy versioning requires persistent/distributed cache design; not applicable to in-memory-only `MemoryCache`. |
| R16 | Document the "write-through-DAF" consistency model explicitly. |
| R17 | Deferred: `UserIdentity` protocol is already listed as out-of-scope in R7-R12 plan; keep deferred. |
| R18 | Deferred: default POST policy is a product decision, not a correctness bug; keep permissive default but document it. |
| R19 | Add a `generation` counter to `DataAccess`; mutations increment it on success, cache entries carry the generation captured at cache-miss start, and cache hits reject entries whose generation no longer matches the current `DataAccess` generation. Stale entries may still be written but are never served. |
| R20 | Document that `Repository`/`Cache` values must be deepcopy-able; constrain `MemoryRepository`/`MemoryCache` docstrings. |
| R21 | Document algorithm immutability contract; algorithms must not mutate their input. |

---

## R13: Mutation-return ownership boundary (Medium severity)

**Files:** `src/daf/repositories/memory.py`, `src/daf/core/protocols.py`

**Root cause:** `MemoryRepository.try_update()` returns the new value directly from the `update()` callable without copying it. A caller that mutates the returned value mutates the stored object.

**Fix:**
1. In `MemoryRepository.try_update()`: return `copy.deepcopy(new_value)` instead of `new_value`
2. In `MemoryRepository.try_delete()`: no return value to copy (returns `bool`), no change needed
3. Update `Repository` protocol docstring to clarify that `try_update` returns an owned snapshot

**Test changes:**
- Add `test_try_update_returns_independent_copy` to `TestMemoryRepository`

---

## R14: Existence disclosure (Medium severity)

**Files:** `src/daf/core/access.py`, `README.md`

**Root cause:** The auth-after-read model means an unauthenticated caller can distinguish nonexistent resources (404) from forbidden resources (403). This is an intentional design decision but is not documented as a security contract.

**Fix:**
1. In `DataAccess.__init__()` docstring, add:
   > This security model does not provide resource-existence confidentiality. Callers may distinguish nonexistent resources (NotFoundError / HTTP 404) from existing resources for which they lack authorization (AuthorizationError / HTTP 403).
2. In `README.md` Authorization Boundary section, add:
   > **Resource existence is not concealed.** The default security model maps NotFoundError to 404 and AuthorizationError to 403. This allows callers to infer resource existence. If existence confidentiality is required, implement a masking layer at the adapter level.

**Test changes:** None.

---

## R15: Authorization-policy versioning (Low-Medium severity)

**Decision:** Deferred. In-memory `MemoryCache` has no persistence, so stale-policy window is bounded by process lifetime. Revisit when adding a persistent/distributed cache backend. Document in `CHANGELOG.md` as known limitation.

---

## R16: External writes bypass invalidation (Medium severity)

**Files:** `README.md`, `src/daf/core/access.py`

**Root cause:** Cache invalidation is triggered only by mutations flowing through `DataAccess`. Direct repository writes bypass invalidation entirely.

**Fix:**
1. In `README.md` Architecture section, add:
   > **Consistency boundary:** DAF cache invalidation is triggered only by mutations through `DataAccess`. Direct writes to the underlying repository bypass cache invalidation. For correctness, all mutations must flow through `DataAccess`, or the caller must manually invalidate affected cache entries.
2. In `src/daf/core/access.py` module docstring, add a note about the write-through-DAF consistency model.

**Test changes:** None.

---

## R19: Stale cache resurrection race (Medium-High severity)

**Files:** `src/daf/core/access.py`, `tests/integration/test_security_invariants.py`

**Root cause:** A concurrent query can repopulate stale data after a mutation invalidates the cache:

```
T1 query: repository.get() → A
T2 put:   repository.update(A → B); cache.delete_prefix()
T1:       cache.set(A)   ← stale data resurrected
```

**Fix:**
1. Add a `generation` counter to `DataAccess`:
   - `self._generation: int = 0`
   - On cache miss, capture `current_generation = self._generation` at the START of `_execute_cache_miss` (before any I/O)
   - Cache entries store `{"raw": ..., "transformed": ..., "generation": current_generation}`
   - On cache hit, verify `cached["generation"] == self._generation`; if mismatch, treat as miss
   - On SUCCESSFUL mutation only (`post` always succeeds; `put` when result is not None; `delete` when deleted is True), increment `self._generation` after invalidation
2. This ensures a query that started before a mutation cannot have its cached result served after the mutation, even if `cache.set()` happens after `cache.delete_prefix()`.

**Test changes:**
- Add `test_stale_cache_not_resurrected_after_mutation`:
  1. Query resource (cache miss, populates cache with gen=0)
  2. Mutate resource (increments gen to 1, invalidates cache)
  3. Manually inject a stale cache entry with gen=0 directly into `cache._cache`
  4. Query again and verify cache miss (stale entry rejected by generation check)

---

## R20: Deepcopy/value-domain constraint (Low severity)

**Files:** `src/daf/core/protocols.py`, `src/daf/repositories/memory.py`, `src/daf/cache/memory.py`

**Root cause:** `copy.deepcopy()` is applied to all values. Not all Python objects are deepcopy-able (e.g., open file handles, locks, some C extensions).

**Fix:**
1. In `Repository` and `Cache` protocol docstrings, add:
   > Implementations store and return values that are deepcopy-able. The reference implementation uses `copy.deepcopy()` at all ownership boundaries.
2. In `MemoryRepository` and `MemoryCache` class docstrings, add:
   > Values must support `copy.deepcopy()`. Non-deepcopy-able values (e.g., open file handles, locks) are not supported.

**Test changes:** None.

---

## R21: Algorithm mutation/failure contract (Low-Medium severity)

**Files:** `src/daf/core/protocols.py`, `src/daf/core/access.py`

**Root cause:** The `Algorithm` protocol does not specify whether `execute()` may mutate its input. If an algorithm mutates `data` in-place and then throws, the raw data passed to the authorizer may already be corrupted.

**Fix:**
1. In `Algorithm` protocol docstring, add:
   > `execute()` must not mutate its input. It receives a snapshot and returns a new value. In-place mutation breaks the authorization-raw-data invariant.
2. In `_execute_query` / `_execute_cache_miss`, add a comment:
   > The algorithm receives a snapshot; if it mutates its input, the raw data passed to the authorizer is unaffected because `raw_data` was captured before transformation.

**Test changes:**
- Add `test_algorithm_must_not_mutate_input` to `tests/integration/test_data_access.py` using a spy algorithm that attempts in-place mutation and verifying the raw data is unchanged.

---

## Out of Scope (explicit)

- R15: Authorization-policy versioning (deferred to persistent cache design)
- R17: `UserIdentity` protocol replacing `Any` user parameter
- R18: Default POST authorization policy change (product decision)

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/daf/repositories/memory.py` | R13: `try_update()` returns deep copy; docstring update |
| `src/daf/core/protocols.py` | R13: `Repository` docstring; R20: deepcopy-able value constraint; R21: algorithm immutability contract |
| `src/daf/core/access.py` | R14: existence-disclosure doc; R16: write-through-DAF consistency note; R19: generation counter on `DataAccess` and cache entries |
| `src/daf/adapters/fastapi.py` | R14: existence-disclosure in adapter docs (if applicable) |
| `README.md` | R14: existence-disclosure section; R16: write-through-DAF consistency note |
| `tests/integration/test_security_invariants.py` | R13: `test_try_update_returns_independent_copy`; R19: `test_stale_cache_not_resurrected_after_mutation` |
| `tests/integration/test_data_access.py` | R21: `test_algorithm_must_not_mutate_input` |

---

## Validation

- pytest: ≥112 passing (target: 115–118 with new tests)
- `mypy --strict`: 0 errors
- `ruff check`: 0 errors
- `python scripts/power_of_ten.py src`: 0 violations

---

## Risks

| Risk | Mitigation |
|------|-----------|
| Generation counter requires `DataAccess` to be stateful across queries | Generation is an `int` incremented only on mutation; negligible overhead |
| `try_update()` deepcopy adds overhead on mutation path | Mutation path is already CAS + lock; one additional deepcopy is acceptable |
| Algorithm immutability contract is unenforceable at runtime | Documented as protocol requirement; tests verify with a spy algorithm |
