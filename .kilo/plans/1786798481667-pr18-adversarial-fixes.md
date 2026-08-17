# PR18 Adversarial Red-Team Fixes — Implementation Plan

## Context

Adversarial review of PR18 found the P0/P1 trie and CI fixes are correct, but identified 7 follow-up findings. The overall score improved from 8.9 → ~9.0/10, but the reviewer flagged **REQUEST CHANGES, narrowly** on resource accounting, invalidation performance, state-duplication invariants, and CI observability.

## Findings and Actions

### 1. 🟠 P1: Trie memory amplification (new risk from optimization)

**Problem:** Every key is stored in root + every node along its path. A 100-byte key of length 100 produces ~100 set memberships. 100k keys × 100 bytes ≈ 10M set memberships before allocator overhead. The trie is a second large index over the same unbounded domain.

**Decision:** Make `MemoryCache` optionally bounded with LRU eviction. Keep `max_size=0` as the default (unbounded, backward-compatible). Document that unbounded mode is for development/testing only.

**Implementation:**
- `MemoryCache.__init__(self, max_size: int = 0)` — `0` means unbounded
- Add `collections.OrderedDict` for LRU tracking when `max_size > 0`
- On `set()`: if bounded and at capacity, evict oldest entry (delete from `_cache` and `_trie`)
- Document memory characteristics in `MemoryCache` docstring
- Update `Cache` protocol docstring to mention the optional bounded behavior

### 2. 🟠 P1: `delete_prefix()` is not actually O(prefix_length)

**Problem:** After collecting N keys in O(prefix_length + N), each key is individually deleted via `_trie_delete(key)` which walks the full key path. Total: O(prefix_length + N × avg_key_length). For broad prefixes (e.g., `resource_namespace:` with 100k keys), this is slower than the original linear scan.

**Decision:** Add `_trie_delete_prefix(prefix)` that detaches a subtree in O(prefix_length), returning all terminal keys for bulk `_cache` cleanup.

**Implementation:**
- `_trie_delete_prefix(self, prefix: str) -> builtins.set[str]`:
  1. Walk to prefix node
  2. Collect all terminal keys from the subtree (DFS/BFS)
  3. Remove the subtree from its parent
  4. Return collected keys
- `delete_prefix()` and `shake()` call `_trie_delete_prefix()` instead of looping `_trie_delete()`
- Keep `_trie_delete()` for single-key deletion (used by `delete()`)

### 3. 🟠 P1: Cache/trie synchronization invariant untested

**Problem:** `_cache` and `_trie` are two mutable sources of truth. No test verifies they stay synchronized after arbitrary mutation sequences.

**Decision:** Add a randomized adversarial invariant test.

**Implementation:**
- In `TestMemoryCache`, add `test_cache_trie_invariant_under_random_mutations()`
- Use a fixed-seed random sequence of: `set`, `overwrite`, `delete`, `delete_prefix`, `shake`, `clear`
- After each operation, assert `set(cache._cache.keys()) == cache._trie.keys`
- Also assert `len(cache._cache) == len(cache._trie.keys)` for non-root nodes via a helper that walks the trie

### 4. 🟡 P2: `graphify_report.py` lacks `check=True` and stderr propagation

**Problem:** After removing the duplicate silent invocation, the remaining `subprocess.run()` lacks `check=True`. A failed Graphify subprocess becomes a `JSONDecodeError` rather than a clean error with stderr and exit status.

**Decision:** Add `check=True` and wrap in try/except that prints stderr and returns a non-zero exit code.

**Implementation:**
```python
try:
    result = subprocess.run([...], capture_output=True, text=True, check=True)
except subprocess.CalledProcessError as exc:
    print(f"FAIL: graphify diagnose multigraph failed (exit {exc.returncode})", file=sys.stderr)
    print(exc.stderr, file=sys.stderr)
    return 1
```

### 5. 🟡 P2: `graphify_affected.py` node-ID mapping is fragile

**Problem:** `file_to_node_id()` hand-rolls the filesystem→Graphify node-ID transformation. If Graphify changes its naming convention, the script silently produces wrong node IDs.

**Decision:** Query the graph JSON for the canonical node ID matching the source path, falling back to the hand-rolled mapping only if the graph lookup fails.

**Implementation:**
- Add `_canonical_node_id(graph_json: Path, path: str) -> str | None`:
  1. Load `graph_json`
  2. Search nodes for one whose `path` or `source` matches the input path
  3. Return its `id` if found
  4. Fall back to `file_to_node_id(path)` if not found (with a warning)
- Update `main()` to use `_canonical_node_id()` first

### 6. 🟡 P2: `--base` handling assumes base SHA exists locally

**Problem:** `git diff --name-only BASE HEAD` fails if the base SHA isn't available in a shallow clone or fork PR checkout.

**Decision:** Check if the base ref is resolvable before diffing; if not, fetch it or fail with a clear message.

**Implementation:**
- In `changed_files()`, run `git rev-parse --verify BASE^{commit}` first
- If that fails, print: `ERROR: base ref '{base}' not found. Ensure full clone or fetch the ref.`
- Return `[]` with exit code 1

### 7. 🟡 P2: Barrel test naming mismatch

**Problem:** `test_daf_is_strict_subset_of_core` uses `issubset()`, which permits equality. The test name says "strict subset" but the assertion allows equality.

**Decision:** Rename the test to `test_daf_is_subset_of_core` to match the actual assertion.

## Validation

```bash
uv run ruff check src/ tests/ scripts/
uv run mypy src/
uv run pytest tests/ -q
```

All existing tests must continue to pass. New tests:
- `test_cache_trie_invariant_under_random_mutations`
- `test_memory_cache_bounded_eviction` (if bounded mode is added)

## Out of Scope

- Trie node cleanup beyond empty-branch pruning (per cache optimization plan)
- `_superedge_invalidate` overlapping-key idempotency safeguard (deliberate)
- Graphify graph-schema changes (external dependency)
