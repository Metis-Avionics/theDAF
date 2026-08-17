# PR18 Adversarial Red-Team Review — Hardening Plan

## Context

Adversarial review of PR #18 (`refactor/barrel-overlap-optimizations`) scored 7.8/10 and identified 3 blockers (P1) plus P2 hardening items. This plan addresses all findings while preserving the architectural gains already merged.

Inspection of `graphify-out/graph.json` reveals 741 nodes across 34 unique `source_file` values; 17 source files map to multiple nodes (e.g., `src/daf/adapters/fastapi.py` → 20 nodes). The module-level node ID matches `file_to_node_id(path)`. This affects the canonical-ID design.

## Blockers (must fix before production-hardened stamp)

### B1. Trie memory amplification — refactor to terminal-only keys

**Problem**: `_TrieNode.keys` is maintained at every node along every key's path. For N keys of average length L, this stores O(N × L) redundant string references in addition to `_cache`.

**Fix**: Change trie semantics so only terminal nodes store a single key. Intermediate nodes carry only `children`.

```python
class _TrieNode:
    __slots__ = ("children", "key")

    def __init__(self) -> None:
        self.children: dict[str, _TrieNode] = {}
        self.key: str | None = None
```

Add a private DFS helper:
```python
def _dfs_collect(self, node: _TrieNode | None) -> builtins.set[str]:
    if node is None:
        return builtins.set()
    result = builtins.set()
    if node.key is not None:
        result.add(node.key)
    for child in node.children.values():
        result.update(self._dfs_collect(child))
    return result
```

Update methods:
- `_trie_insert(key)`: walk to terminal node, set `node.key = key`
- `_trie_delete(key)`: walk to terminal node, clear `node.key`, then bottom-up prune any ancestor that has `key is None` and `children == {}`
- `_trie_collect(prefix)`: walk to prefix node, return `self._dfs_collect(node)`
- `_trie_delete_prefix(prefix)`: walk to prefix node, collect keys via `self._dfs_collect(node)`, detach subtree from parent, then bottom-up prune empty ancestors up to root. Return collected keys. Does NOT call `_trie_delete` per key.

**Critical caller contract change**: After `_trie_delete_prefix()` returns, the trie subtree is already detached. Callers `delete_prefix()` and `shake()` must **only** remove returned keys from `_cache` and `_lru`. They must NOT call `_trie_delete(key)` for each returned key (that would re-walk already-removed nodes).

```python
async def delete_prefix(self, prefix: str) -> None:
    keys_to_delete = self._trie_delete_prefix(prefix)
    for key in keys_to_delete:
        del self._cache[key]
        self._lru.pop(key, None)

async def shake(self, prefix: str) -> int:
    keys_to_delete = self._trie_delete_prefix(prefix)
    for key in keys_to_delete:
        del self._cache[key]
        self._lru.pop(key, None)
    return len(keys_to_delete)
```

**Empty-prefix case**: `_trie_delete_prefix("")` calls `self._dfs_collect(self._trie)` to gather all keys, then replaces `self._trie` with a fresh `_TrieNode()`. No ancestor pruning needed because root is replaced entirely.

**Pruning logic for non-empty prefix**: After `del parent.children[ch]`, walk the recorded path in reverse and prune any ancestor that has `key is None` and `children == {}`. Stop at the first non-empty ancestor. Do not delete the root.

```python
# after del parent.children[ch]
for i in range(len(path) - 1, -1, -1):
    ancestor = path[i][0]
    if ancestor.key is None and not ancestor.children:
        if i > 0:
            parent = path[i - 1][0]
            ch = path[i - 1][1]
            del parent.children[ch]
    else:
        break
```

Honest complexity: `O(prefix_length + K)` where K is matching entries. Update docstrings and CHANGELOG accordingly.

### B2. `_canonical_node_id()` must return the graph's module-level ID

**Problem**: Current implementation validates hand-rolled `file_to_node_id(path)` against the graph, but still returns the hand-rolled ID on mismatch. The graph contains multiple nodes per `source_file`; the module-level node is the one whose ID matches `file_to_node_id(path)`.

**Fix**: Query the graph for nodes with `source_file == path`. Prefer the one whose `id == file_to_node_id(path)` (module-level). If found, return `node["id"]`. If no exact match, return the first matching node's `id` (graph-driven). If no nodes match at all, warn and return `file_to_node_id(path)`.

```python
def _canonical_node_id(graph_json: Path, path: str) -> str | None:
    if not graph_json.exists():
        return None
    try:
        data = json.loads(graph_json.read_text())
    except (json.JSONDecodeError, OSError):
        return None
    expected_id = file_to_node_id(path)
    if expected_id is None:
        return None
    matches = [
        node for node in data.get("nodes", [])
        if node.get("source_file") == path
    ]
    if not matches:
        warnings.warn(
            f"graphify graph has no node for '{path}'; "
            f"falling back to hand-rolled node ID '{expected_id}'.",
            stacklevel=2,
        )
        return expected_id
    # Prefer exact module-level match
    for node in matches:
        if node.get("id") == expected_id:
            return expected_id
    # Fallback: first graph node's ID (graph-driven, not hand-rolled)
    return matches[0].get("id")
```

This ensures the graph's ID is returned whenever the graph contains a matching node, even if the ID differs from `file_to_node_id(path)`.

### B3. `changed_files()` must hard-fail on missing base SHA

**Problem**: Missing base ref returns `[]`, which `main()` interprets as "No Python files changed" and exits 0. Green CI from invalid baseline.

**Fix**: Raise `RuntimeError` from `changed_files()` on missing base. `main()` catches it, prints the error, and returns 1.

```python
def changed_files(base: str) -> list[str]:
    try:
        subprocess.run(
            ["git", "rev-parse", "--verify", f"{base}^{{commit}}"],
            capture_output=True, text=True, check=True,
        )
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(
            f"base ref '{base}' not found. "
            "Ensure full clone or fetch the ref."
        ) from exc
    result = subprocess.run(
        ["git", "diff", "--name-only", base, "HEAD"],
        capture_output=True, text=True, check=True
    )
    return [f for f in result.stdout.strip().splitlines() if f.endswith(".py")]
```

```python
    try:
        files = changed_files(args.base)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
```

## P2 Hardening (implement alongside blockers)

### P2-1. Reject negative `max_size`

```python
if max_size < 0:
    raise ValueError("max_size must be non-negative (0 = unbounded)")
```

### P2-2. Prefix reference-model test

Add a test that compares `cache._trie_collect(prefix)` against brute-force `{k for k in cache._cache if k.startswith(prefix)}` for random prefixes after random mutations. This directly attacks the trie algorithm rather than merely proving both structures lose the same keys.

### P2-3. LRU adversarial edge-case tests

Add tests for:
- `max_size=1` (single entry, overwrite evicts oldest)
- Empty-string key with bounded cache
- `shake("")` with bounded cache
- Delete after LRU promotion (key should be removable)
- Prefix deletion after LRU promotion (prefix matches should be removed from LRU)
- Repeated `get()` does not change relative order of other keys (move_to_end is correct)

### P2-4. Graphify schema validation

In `main()`, after confirming `GRAPH_JSON.exists()` and loading the JSON, validate:
- `data` is a dict with top-level `nodes` as a list
- Each node in `nodes` is a dict with `source_file` and `id` fields

Fail fast with `RuntimeError` if schema is missing. This prevents misleading "no impacted tests" behavior from malformed graph output.

### P2-5. Graphify canonical-ID tests

Add unit tests for `_canonical_node_id()`:
- Graph has node with `source_file == path` and `id == file_to_node_id(path)` → returns that graph ID
- Graph has node with matching `source_file` but different `id` → returns first graph node's ID (not hand-rolled)
- No matching node → warns and returns hand-rolled ID
- Malformed JSON → returns None
- Missing `nodes` key → returns None
- Missing `graph.json` → returns None

### P2-6. Update complexity/docstring claims

Change "O(prefix_length)" to "O(prefix_length + K)" where K is the number of matching entries, in:
- `MemoryCache` class docstring
- `_trie_delete_prefix` docstring
- `CHANGELOG.md`

## Test plan

All existing tests must continue to pass. New tests:
- `test_memory_cache_rejects_negative_max_size`
- `test_memory_cache_max_size_one`
- `test_memory_cache_lru_delete_after_promotion`
- `test_memory_cache_lru_prefix_delete_after_promotion`
- `test_memory_cache_shake_empty_prefix_bounded`
- `test_memory_cache_empty_key_bounded`
- `test_trie_collect_matches_bruteforce_prefix` (reference model)
- `test_canonical_node_id_uses_graph_module_level_id`
- `test_canonical_node_id_returns_graph_id_when_differs`
- `test_canonical_node_id_warns_when_no_match`
- `test_canonical_node_id_malformed_json`
- `test_canonical_node_id_missing_nodes_key`
- `test_canonical_node_id_missing_graph_file`
- `test_changed_files_raises_on_missing_base`
- `test_main_exits_one_on_missing_base`
- `test_graphify_schema_validation_missing_nodes`
- `test_graphify_schema_validation_missing_id_field`

Validation:
```bash
uv run ruff check src/ tests/ scripts/
uv run mypy src/
uv run pytest tests/ -q
```

## Files to modify

| File | Changes |
|------|---------|
| `src/daf/cache/memory.py` | B1: terminal-only trie + DFS helper; P2-1: negative max_size validation; P2-6: complexity docstrings |
| `tests/unit/test_components.py` | B1: update invariant test to use `_trie_collect("")`; P2-2, P2-3: new LRU/trie tests |
| `tests/unit/test_graphify.py` | B2, P2-4, P2-5: canonical ID, schema, changed_files tests |
| `scripts/graphify_affected.py` | B2: canonical lookup preferring graph ID; B3: hard-fail on missing base; P2-4: schema validation |
| `CHANGELOG.md` | Update complexity claims, add new findings |
| `HANDOVER.md` | Update test count, uncommitted work |
| `SESSION.md` | Add new session entry |
