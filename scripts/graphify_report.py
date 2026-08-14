#!/usr/bin/env python3
"""Generate graphify architecture report: extract, diagnose, tree, callflow."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

GRAPH_DIR = Path("graphify-out")
GRAPH_JSON = GRAPH_DIR / "graph.json"
DIAGNOSE_JSON = GRAPH_DIR / "diagnose.json"
THRESHOLD = 30


def run(cmd: list[str], check: bool = True) -> None:
    print(f"+ {' '.join(cmd)}")
    subprocess.run(cmd, check=check)  # noqa: S603


def main() -> int:
    GRAPH_DIR.mkdir(exist_ok=True)

    print("=== graphify extract ===")
    run(
        [
            "uv", "run", "python", "-m", "graphify",
            "extract", ".", "--code-only", "--no-cluster",
        ]
    )

    print("\n=== graphify diagnose multigraph ===")
    run(["uv", "run", "python", "-m", "graphify", "diagnose", "multigraph",
         "--graph", str(GRAPH_JSON), "--json"], check=False)
    # CLI writes to stdout; capture via shell redirect instead
    result = subprocess.run(  # noqa: S603
        ["uv", "run", "python", "-m", "graphify", "diagnose", "multigraph",  # noqa: S607
         "--graph", str(GRAPH_JSON), "--json"],
        capture_output=True, text=True
    )
    DIAGNOSE_JSON.write_text(result.stdout)

    data = json.loads(result.stdout)
    summary = data["summary"]
    collapsed = summary["directed_same_endpoint_collapsed_edges"]
    print(f"directed_same_endpoint_collapsed_edges={collapsed}, "
          f"threshold={THRESHOLD}")
    if collapsed > THRESHOLD:
        print(
            f"FAIL: collapse {collapsed} exceeds threshold {THRESHOLD}",
            file=sys.stderr,
        )
        return 1
    print("PASS: multigraph diagnostics within threshold")

    print("\n=== graphify tree ===")
    run(["uv", "run", "python", "-m", "graphify", "tree", "--graph", str(GRAPH_JSON)])

    print("\n=== graphify export callflow-html ===")
    run(["uv", "run", "python", "-m", "graphify", "export", "callflow-html",
         "--graph", str(GRAPH_JSON), "--out", str(GRAPH_DIR)])

    print(f"\nArtifacts in {GRAPH_DIR}/:")
    for p in sorted(GRAPH_DIR.iterdir()):
        if p.is_file():
            print(f"  {p.name} ({p.stat().st_size:,} bytes)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
