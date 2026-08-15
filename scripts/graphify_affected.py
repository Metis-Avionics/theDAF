#!/usr/bin/env python3
"""Run graphify affected analysis on changed Python source files from a PR/MR.

Usage:
    python scripts/graphify_affected.py [--base BASE_REF] [--depth N]

Only files ending in ``.py`` are analyzed. Non-Python changes (config,
workflows, docs) are not mapped to test impacts. The CI test job still
runs the full suite; this script only suggests a pytest subset.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import warnings
from pathlib import Path
from typing import Any

GRAPH_JSON = Path("graphify-out/graph.json")


def changed_files(base: str) -> list[str]:
    try:
        subprocess.run(  # noqa: S603
            ["git", "rev-parse", "--verify", f"{base}^{{commit}}"],  # noqa: S607
            capture_output=True, text=True, check=True,
        )
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(
            f"base ref '{base}' not found. "
            "Ensure full clone or fetch the ref."
        ) from exc
    try:
        result = subprocess.run(  # noqa: S603
            ["git", "diff", "--name-only", base, "HEAD"],  # noqa: S607
            capture_output=True, text=True, check=True
        )
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(
            f"git diff failed for base ref '{base}': "
            f"{exc.stderr.strip()}"
        ) from exc
    return [f for f in result.stdout.strip().splitlines() if f.endswith(".py")]


def _canonical_node_id(graph_json: Path, path: str) -> str | None:
    """Return the canonical graphify node ID for a source file path.

    Loads ``graph_json`` and searches for a node whose ``source_file``
    matches ``path``. Prefers the one whose ``id`` equals the hand-rolled
    module-level mapping. If no exact match, returns the first graph
    node's ID (graph-driven, lexicographically sorted). Returns ``None``
    if the graph is malformed or contains no matching node.
    """
    if not graph_json.exists():
        return None
    try:
        raw = graph_json.read_text()
    except OSError:
        return None
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return None
    try:
        _validate_graph_schema(data)
    except RuntimeError:
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
    matches.sort(key=lambda n: n.get("id", ""))
    for node in matches:
        if node.get("id") == expected_id:
            return expected_id
    return matches[0].get("id")


def file_to_node_id(path: str) -> str | None:
    """Map a source file path to its graphify module node ID.

    Examples:
        src/daf/__init__.py         -> src_daf_init
        src/daf/core/access.py      -> src_daf_core_access
        src/daf/cache/memory.py     -> src_daf_cache_memory
    """
    if not path.startswith("src/"):
        return None
    parts = Path(path).parts
    if len(parts) < 3:
        return None
    return "_".join(parts).removesuffix(".py").replace("___init__", "_init")


def affected(node_id: str, depth: int = 2) -> str:
    try:
        result = subprocess.run(  # noqa: S603
            ["uv", "run", "python", "-m", "graphify", "affected", node_id,  # noqa: S607
             "--depth", str(depth), "--graph", str(GRAPH_JSON)],
            capture_output=True, text=True, check=True,
        )
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(
            f"graphify affected failed for node '{node_id}': "
            f"{exc.stderr.strip()}"
        ) from exc
    return result.stdout


def extract_test_files(affected_output: str) -> set[str]:
    """Extract unique test file paths from affected output."""
    test_files = set()
    for line in affected_output.splitlines():
        m = re.search(r'tests/[^\s\]]+\.py', line)
        if m:
            test_files.add(m.group(0))
    return test_files


def _validate_graph_schema(data: dict[str, Any]) -> None:
    if not isinstance(data, dict):
        raise RuntimeError(
            "graphify graph JSON root must be a dict; "
            f"got {type(data).__name__}."
        )
    nodes = data.get("nodes")
    if not isinstance(nodes, list):
        raise RuntimeError(
            "graphify graph JSON missing top-level 'nodes' list."
        )
    seen_ids: set[str] = set()
    for node in nodes:
        if not isinstance(node, dict):
            raise RuntimeError(
                "graphify graph JSON contains a non-dict entry in 'nodes'."
            )
        node_id = node.get("id")
        source_file = node.get("source_file")
        if not isinstance(node_id, str) or not node_id:
            raise RuntimeError(
                "graphify graph JSON node 'id' must be a non-empty string."
            )
        if not isinstance(source_file, str) or not source_file:
            raise RuntimeError(
                "graphify graph JSON node 'source_file' must be a non-empty string."
            )
        if node_id in seen_ids:
            raise RuntimeError(
                f"graphify graph JSON contains duplicate node ID '{node_id}'."
            )
        seen_ids.add(node_id)


def _load_graph(graph_path: Path) -> dict[str, Any] | None:
    if not graph_path.exists():
        print(
            f"ERROR: {graph_path} not found. Run graphify extract first.",
            file=sys.stderr,
        )
        return None
    try:
        data = json.loads(graph_path.read_text())
    except (json.JSONDecodeError, OSError) as exc:
        print(f"ERROR: failed to load {graph_path}: {exc}", file=sys.stderr)
        return None
    try:
        _validate_graph_schema(data)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return None
    return data


def _process_files(files: list[str], depth: int) -> tuple[int, set[str]]:
    print(f"Changed files ({len(files)}):")
    for f in files:
        print(f"  {f}")

    all_test_files: set[str] = set()
    for path in files:
        node_id = _canonical_node_id(GRAPH_JSON, path)
        if not node_id:
            continue
        output = affected(node_id, depth)
        test_files = extract_test_files(output)
        if test_files:
            print(f"\nImpacted tests from {path}:")
            for tf in sorted(test_files):
                print(f"  {tf}")
            all_test_files.update(test_files)

    if all_test_files:
        print("\n=== Suggested pytest subset ===")
        for tf in sorted(all_test_files):
            print(f"  uv run pytest {tf} -q")
    else:
        print("\nNo impacted test files detected.")

    return 0, all_test_files


def main() -> int:
    parser = argparse.ArgumentParser(description="graphify affected analysis for CI")
    parser.add_argument("--base", default="origin/main", help="Base ref for diff")
    parser.add_argument("--depth", type=int, default=2, help="Traversal depth")
    args = parser.parse_args()

    data = _load_graph(GRAPH_JSON)
    if data is None:
        return 1

    try:
        files = changed_files(args.base)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    if not files:
        print("No Python files changed.")
        return 0

    exit_code, _ = _process_files(files, args.depth)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
