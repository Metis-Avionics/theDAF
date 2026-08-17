# Cache Optimization Plan

## Goal

Reduce redundant work in the hot query/mutation path:
1. Eliminate repeated `sha256` computation for the same `resource_id` in `DataAccess`.
2. Replace the O(N) linear scan in `MemoryCache.delete_prefix` / `shake` with a prefix trie, making both O(len(prefix) + m) where m is the number of matched keys.

## Current State

- `_resource_namespace()` is called 2–3× per query and 2× per mutation, each time recomputing `sha256(resource_id.encode()).hexdigest()`.
- `_delete_prefix_impl` does `[key for key in self._cache if key.startswith(prefix)]` — a full dict sweep on every call.
- `_superedge_invalidate` calls `delete_prefix` twice (query keys + gen key) plus a redundant `shake` on the same gen-key prefix.

## Optimization A — Namespace cache in `DataAccess`

**File:** `src/daf/core/access.py`

1. Add `self._namespace_cache: dict[str, str] = {}` in `DataAccess.__init__`.
2. Change `_resource_namespace` to:
   ```python
   def _resource_namespace(self, resource_id: str) -> str:
       if resource_id not in self._namespace_cache:
           self._namespace_cache[resource_id] = hashlib.sha256(
               resource_id.encode()
           ).hexdigest()
       return self._namespace_cache[resource_id]
   ```
3. No invalidation needed: `resource_id → namespace` is a pure function; the mapping is stable for the lifetime of a `DataAccess` instance.

**Effect:** `_cache_key`, `_current_generation`, `_advance_generation`, `_superedge_invalidate` all benefit. A single query drops from 2 SHA-256 hashes to 0 (both hit the cache after the first call).

## Optimization B — Prefix trie in `MemoryCache`

**File:** `src/daf/cache/memory.py`

1. Add a `_TrieNode` class at module level:
   ```python
   class _TrieNode:
       __slots__ = ("children", "keys")
       def __init__(self) -> None:
           self.children: dict[str, _TrieNode] = {}
           self.keys: set[str] = set()
   ```

2. Add `self._trie = _TrieNode()` in `MemoryCache.__init__`.

3. Add three private methods:
   ```python
   def _trie_insert(self, key: str) -> None:
       node = self._trie
       for ch in key:
           node.children.setdefault(ch, _TrieNode())
           node = node.children[ch]
           node.keys.add(key)

   def _trie_delete(self, key: str) -> None:
       node = self._trie
       for ch in key:
           node = node.children.get(ch)
           if node is None:
               return
           node.keys.discard(key)

   def _trie_collect(self, prefix: str) -> set[str]:
       node = self._trie
       for ch in prefix:
           node = node.children.get(ch)
           if node is None:
               return set()
       return set(node.keys)
   ```

4. Update existing methods to maintain the trie:
   - `set`: call `self._trie_insert(key)` after storing.
   - `delete`: call `self._trie_delete(key)` before removing from dict.
   - `clear`: replace `self._trie` with a fresh `_TrieNode()`.
   - `delete_prefix`: replace `_delete_prefix_impl` body with `return list(self._trie_collect(prefix))`.
   - `shake`: same — calls `_delete_prefix_impl` which now uses the trie.

5. `_delete_prefix_impl` stays as the shared private method name; its body changes to use `_trie_collect`.

**Invariant:** Every key in `self._cache` is registered in the trie; every key removed from `self._cache` is removed from the trie. The trie is a strict index, not a copy.

**Edge cases handled:**
- `set` with an existing key: `_trie_insert` is idempotent (set add).
- `delete` on a non-existent key: `_trie_delete` traverses and discards from empty sets — no error.
- `clear`: fresh root discards all accumulated trie nodes.
- `shake` after `delete_prefix` on the same prefix: `_trie_collect` returns empty set (keys already removed) — returns 0. Correct no-op.

**Not addressed (out of scope):** Trie node cleanup on delete (empty child nodes are left in place). Acceptable for bounded in-memory caches; the total trie node count is bounded by the total character count of all keys ever inserted, which is O(cache_size × avg_key_length).

## Deduplication in `_superedge_invalidate`

**File:** `src/daf/core/access.py`

The current code calls `delete_prefix(f"query:{ns}:")`, then `delete(f"_daf_gen:{ns}")`, then `shake(f"_daf_gen:{ns}")`. The `shake` call after `delete` on the same exact key is a no-op. With the trie, it's a fast no-op (single trie traversal, empty result). No code change needed here — the redundancy is harmless and the plan's "optionally call shake" intent is preserved for future use with different prefixes.

If a future optimization wants to collapse the two prefix scans into one, that can be done by calling `shake` once with a combined prefix or by collecting both key sets from the trie in a single traversal. Out of scope for this pass.

## Validation

1. **Unit tests (existing must pass):**
   - `tests/unit/test_components.py` — 23 tests including 4 `shake` tests.

2. **New unit tests to add:**
   - `test_namespace_cache_returns_same_namespace_for_same_resource_id`
   - `test_namespace_cache_returns_different_namespaces_for_different_ids`
   - `test_trie_delete_prefix_matches_all_keys`
   - `test_trie_delete_prefix_matches_subset`
   - `test_trie_delete_removes_key_from_index`
   - `test_trie_clear_resets_index`
   - `test_trie_set_existing_key_is_idempotent`

3. **Integration tests (existing must pass):**
   - `tests/integration/test_data_access.py` — 16 tests.
   - `tests/integration/test_security_invariants.py` — 30 tests.

4. **Validation gates:**
   - `pytest tests/ -q` — all pass.
   - `ruff check src/ tests/` — clean.
   - `mypy src/ --strict` — clean.
   - `python scripts/power_of_ten.py src/` — clean.
   - `uv run python -m graphify extract . --code-only --no-cluster` — succeeds.
   - `uv run python -m graphify diagnose multigraph --graph graphify-out/graph.json --json` — `directed_same_endpoint_collapsed_edges` ≤ 30.

## Risks

- **Trie memory overhead:** Each key contributes O(len(key)) trie nodes. For the current ~100-key test caches this is negligible; for production caches with millions of keys, the overhead should be measured. The trie is an index, not a copy — values stay in `self._cache` only.
- **`_namespace_cache` unbounded growth:** Same risk profile as `_generation_locks` — both are per-resource dicts that grow with unique resource_ids. Acceptable for bounded in-memory usage. If needed, an LRU eviction policy can be added later.
- **Thread safety:** Neither change introduces new thread-safety concerns. `MemoryCache` was already not thread-safe (no locks), and the trie is mutated under the same call paths as `self._cache`.
- **Protocol compatibility:** No protocol changes. `Cache` protocol is unchanged.
