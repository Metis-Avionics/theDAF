#!/usr/bin/env python3
"""Run graphify affected analysis on changed files from a PR/MR.

Usage:
    python scripts/graphify_affected.py [--base BASE_REF] [--depth N]

Reads changed files from git diff against BASE_REF (default: origin/main),
maps them to graphify node IDs, and runs `affected` for each.
Outputs a summary of impacted source and test files.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

GRAPH_JSON = Path("graphify-out/graph.json")


def changed_files(base: str) -> list[str]:
    result = subprocess.run(  # noqa: S603
        ["git", "diff", "--name-only", base, "HEAD"],  # noqa: S607
        capture_output=True, text=True, check=True
    )
    return [f for f in result.stdout.strip().splitlines() if f.endswith(".py")]


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
    result = subprocess.run(  # noqa: S603
        ["uv", "run", "python", "-m", "graphify", "affected", node_id,  # noqa: S607
         "--depth", str(depth), "--graph", str(GRAPH_JSON)],
        capture_output=True, text=True
    )
    return result.stdout


def extract_test_files(affected_output: str) -> set[str]:
    """Extract unique test file paths from affected output."""
    test_files = set()
    for line in affected_output.splitlines():
        m = re.search(r'tests/[^\s\]]+\.py', line)
        if m:
            test_files.add(m.group(0))
    return test_files


def main() -> int:
    parser = argparse.ArgumentParser(description="graphify affected analysis for CI")
    parser.add_argument("--base", default="origin/main", help="Base ref for diff")
    parser.add_argument("--depth", type=int, default=2, help="Traversal depth")
    args = parser.parse_args()

    if not GRAPH_JSON.exists():
        print(
            f"ERROR: {GRAPH_JSON} not found. Run graphify extract first.",
            file=sys.stderr,
        )
        return 1

    files = changed_files(args.base)
    if not files:
        print("No Python files changed.")
        return 0

    print(f"Changed files ({len(files)}):")
    for f in files:
        print(f"  {f}")

    all_test_files: set[str] = set()
    for path in files:
        node_id = file_to_node_id(path)
        if not node_id:
            continue
        output = affected(node_id, args.depth)
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

    return 0


if __name__ == "__main__":
    sys.exit(main())
