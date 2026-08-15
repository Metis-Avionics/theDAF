"""Unit tests for graphify_affected.py helpers."""

from __future__ import annotations

import importlib.util
import json
import sys
import warnings
from pathlib import Path
from typing import Any

import pytest

_SCRIPTS_DIR = Path(__file__).resolve().parent.parent.parent / "scripts"
_SPEC = importlib.util.spec_from_file_location(
    "graphify_affected",
    _SCRIPTS_DIR / "graphify_affected.py",
)
_GA = importlib.util.module_from_spec(_SPEC)
sys.modules["graphify_affected"] = _GA
_SPEC.loader.exec_module(_GA)

changed_files = _GA.changed_files
_canonical_node_id = _GA._canonical_node_id
_validate_graph_schema = _GA._validate_graph_schema
GRAPH_JSON = _GA.GRAPH_JSON


class TestCanonicalNodeId:
    def test_uses_graph_module_level_id(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text(
            json.dumps({
                "nodes": [
                    {
                        "id": "src_daf_cache_memory",
                        "source_file": "src/daf/cache/memory.py",
                    },
                ]
            })
        )
        result = _canonical_node_id(g, "src/daf/cache/memory.py")
        assert result == "src_daf_cache_memory"

    def test_returns_lexicographically_first_when_no_exact_match(
        self, tmp_path: Path,
    ) -> None:
        g = tmp_path / "graph.json"
        g.write_text(
            json.dumps({
                "nodes": [
                    {
                        "id": "z_last",
                        "source_file": "src/daf/cache/memory.py",
                    },
                    {
                        "id": "a_first",
                        "source_file": "src/daf/cache/memory.py",
                    },
                ]
            })
        )
        result = _canonical_node_id(g, "src/daf/cache/memory.py")
        assert result == "a_first"

    def test_returns_graph_id_when_differs(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text(
            json.dumps({
                "nodes": [
                    {
                        "id": "graph_override_id",
                        "source_file": "src/daf/cache/memory.py",
                    },
                ]
            })
        )
        result = _canonical_node_id(g, "src/daf/cache/memory.py")
        assert result == "graph_override_id"

    def test_warns_when_no_match(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text(json.dumps({"nodes": []}))
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            result = _canonical_node_id(g, "src/daf/unknown.py")
        assert len(caught) == 1
        assert "no node for" in str(caught[0].message)
        assert result == "src_daf_unknown"

    def test_malformed_json(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text("not json")
        assert _canonical_node_id(g, "src/daf/x.py") is None

    def test_missing_nodes_key(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text(json.dumps({}))
        assert _canonical_node_id(g, "src/daf/x.py") is None

    def test_missing_graph_file(self, tmp_path: Path) -> None:
        g = tmp_path / "nonexistent.json"
        assert _canonical_node_id(g, "src/daf/x.py") is None

    def test_malformed_graph_schema_returns_none(self, tmp_path: Path) -> None:
        g = tmp_path / "graph.json"
        g.write_text(json.dumps({"nodes": [{"id": 123, "source_file": "x.py"}]}))
        assert _canonical_node_id(g, "src/daf/x.py") is None


class TestChangedFiles:
    def test_raises_on_missing_base(self, monkeypatch: pytest.MonkeyPatch) -> None:
        def _fake_run(*_args: Any, **_kwargs: Any) -> None:
            import subprocess
            raise subprocess.CalledProcessError(128, _args[0], stderr="not found")

        monkeypatch.setattr(_GA.subprocess, "run", _fake_run)
        with pytest.raises(RuntimeError, match="base ref 'origin/ghost' not found"):
            changed_files("origin/ghost")

    def test_raises_on_git_diff_failure(self, monkeypatch: pytest.MonkeyPatch) -> None:
        import subprocess

        def _fake_run(cmd, *_, **__):
            if cmd[0] == "git" and cmd[1] == "diff":
                raise subprocess.CalledProcessError(128, cmd, stderr="diff failed")
            return subprocess.CompletedProcess(cmd, 0, stdout="", stderr="")

        monkeypatch.setattr(_GA.subprocess, "run", _fake_run)
        with pytest.raises(RuntimeError, match="git diff failed"):
            changed_files("origin/main")


class TestGraphifySchemaValidation:
    def test_missing_nodes(self) -> None:
        with pytest.raises(RuntimeError, match="missing top-level 'nodes' list"):
            _validate_graph_schema({"nodes": "not-a-list"})

    def test_missing_id_field(self) -> None:
        with pytest.raises(RuntimeError, match="'id' must be a non-empty string"):
            _validate_graph_schema({
                "nodes": [{"source_file": "x.py"}]
            })

    def test_node_id_must_be_non_empty_string(self) -> None:
        with pytest.raises(RuntimeError, match="'id' must be a non-empty string"):
            _validate_graph_schema({"nodes": [{"id": "", "source_file": "x.py"}]})

    def test_node_id_must_be_string_type(self) -> None:
        with pytest.raises(RuntimeError, match="'id' must be a non-empty string"):
            _validate_graph_schema({"nodes": [{"id": 123, "source_file": "x.py"}]})

    def test_source_file_must_be_non_empty_string(self) -> None:
        with pytest.raises(
            RuntimeError, match="'source_file' must be a non-empty string"
        ):
            _validate_graph_schema({"nodes": [{"id": "x", "source_file": ""}]})

    def test_duplicate_node_ids(self) -> None:
        with pytest.raises(RuntimeError, match="duplicate node ID"):
            _validate_graph_schema({
                "nodes": [
                    {"id": "same", "source_file": "x.py"},
                    {"id": "same", "source_file": "y.py"},
                ]
            })

    def test_non_dict_root_raises(self) -> None:
        with pytest.raises(RuntimeError, match="root must be a dict"):
            _validate_graph_schema(["not", "a", "dict"])

    def test_non_dict_node_entry(self) -> None:
        with pytest.raises(RuntimeError, match="non-dict entry in 'nodes'"):
            _validate_graph_schema({"nodes": ["bad"]})


class TestMainExitsOneOnMissingBase:
    def test_main_exits_one(
        self,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path: Path,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        graph = tmp_path / "graph.json"
        graph.write_text(json.dumps({"nodes": []}))
        monkeypatch.setattr(_GA, "GRAPH_JSON", graph)

        def _fake_run(*_args: Any, **_kwargs: Any) -> None:
            import subprocess
            raise subprocess.CalledProcessError(128, _args[0], stderr="not found")

        monkeypatch.setattr(_GA.subprocess, "run", _fake_run)
        monkeypatch.setattr(
            _GA.sys, "argv", ["graphify_affected.py", "--base", "origin/ghost"]
        )
        exit_code = _GA.main()
        assert exit_code == 1
        captured = capsys.readouterr()
        assert "base ref 'origin/ghost' not found" in captured.err
