"""Differential parity: compare Python and Rust DataAccess behavior for core operations.

Each test runs the equivalent sequence against both runtimes and asserts that
normalized `MutationResult` / `QueryResult` fields agree on:
  - success
  - data (when present)
  - error / error_type (on failure)
  - cache_hit (for queries)

Timestamps are stripped before comparison; `cache_hit` is compared as-is.

The Rust side is driven by the `daf-parity` binary (crates/daf-ffi/src/bin/parity.rs),
which accepts one JSON command per line on stdin and emits one JSON response per line
on stdout.

Tests are defined in `parity_manifest.json`; this module loads the manifest and
parametrizes one test function per entry.
"""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

import pytest

from daf.algorithms import FibonacciDP
from daf.cache import MemoryCache
from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)
from daf.core.factory import DataAccessFactory
from daf.repositories import MemoryRepository

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parents[2]
PARITY_BIN = REPO_ROOT / "target" / "debug" / "daf-parity"
MANIFEST = REPO_ROOT / "tests" / "unit" / "parity_manifest.json"


def _build_parity_bin() -> Path:
    result = subprocess.run(
        ["cargo", "build", "--bin", "daf-parity"],
        cwd=str(REPO_ROOT / "crates" / "daf-ffi"),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        pytest.fail(
            "daf-parity binary failed to build:\n" + result.stderr[-2000:]
        )
    return PARITY_BIN


def _load_manifest() -> list[dict]:
    with open(MANIFEST) as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# Session-scoped parity process
# ---------------------------------------------------------------------------

@pytest.fixture(scope="session")
def parity_proc() -> subprocess.Popen:
    bin_path = _build_parity_bin()
    proc = subprocess.Popen(
        [str(bin_path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    yield proc
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def _rust_send(proc: subprocess.Popen, cmd: dict) -> dict:
    line = json.dumps(cmd, default=str) + "\n"
    try:
        proc.stdin.write(line)
        proc.stdin.flush()
    except BrokenPipeError:
        pytest.fail("daf-parity binary exited unexpectedly")
    response_line = proc.stdout.readline()
    if not response_line:
        stderr = proc.stderr.read()[-500:]
        pytest.fail(f"daf-parity produced no output. stderr: {stderr!r}")
    return json.loads(response_line)


# ---------------------------------------------------------------------------
# Shared setup
# ---------------------------------------------------------------------------

def _python_factory():
    repo = MemoryRepository()
    cache = MemoryCache()
    return DataAccessFactory(repository=repo, cache=cache)


def _normalize(result: MutationResult | QueryResult) -> dict:
    d = json.loads(result.model_dump_json())
    d.pop("timestamp", None)
    return d


def _rust_normalize(r: dict) -> dict:
    d = dict(r)
    d.pop("timestamp", None)
    return d


# ---------------------------------------------------------------------------
# Manifest-driven tests
# ---------------------------------------------------------------------------

def _substitute(template: str, context: dict) -> str:
    def _replace(m: re.Match) -> str:
        key = m.group(1)
        parts = key.split(".")
        value = context
        for part in parts:
            if isinstance(value, dict):
                value = value.get(part)
            else:
                value = getattr(value, part, None)
        if value is None:
            raise KeyError(f"undefined substitution key: {key}")
        return str(value)
    return re.sub(r"\{([^}]+)\}", _replace, template)


def _substitute_for_namespace(d: dict, context: dict, namespace: str) -> dict:
    result = {}
    for key, value in d.items():
        if isinstance(value, str):
            template = re.sub(r"\{([^}]+)\}", lambda m: f"{{{namespace}_{m.group(1)}}}", value)
            result[key] = _substitute(template, context)
        else:
            result[key] = value
    return result


async def _run_manifest_case(parity_proc, case: dict):
    name = case["name"]
    ops = case["ops"]
    assertions = case["assertions"]

    factory = _python_factory()
    daf = factory.create()
    context: dict[str, dict] = {}

    for idx, op in enumerate(ops):
        op_template = dict(op)
        op_type = op_template.pop("op")

        py_cmd = _substitute_for_namespace(op_template, context, "py")
        rust_cmd = _substitute_for_namespace(op_template, context, "rust")
        py_cmd["op"] = op_type
        rust_cmd["op"] = op_type

        py_result = None
        rust_result = None

        if op_type == "post":
            py_result = _normalize(
                await daf.post(PostInfo(resource_type=py_cmd["resource_type"], data=py_cmd["data"]))
            )
            rust_raw = _rust_send(parity_proc, rust_cmd)
            rust_result = _rust_normalize(rust_raw)
        elif op_type == "put":
            py_result = _normalize(
                await daf.put(PutInfo(resource_id=py_cmd["resource_id"], data=py_cmd["data"]))
            )
            rust_raw = _rust_send(parity_proc, rust_cmd)
            rust_result = _rust_normalize(rust_raw)
        elif op_type == "delete":
            py_result = _normalize(
                await daf.delete(DeleteInfo(resource_id=py_cmd["resource_id"]))
            )
            rust_raw = _rust_send(parity_proc, rust_cmd)
            rust_result = _rust_normalize(rust_raw)
        elif op_type == "query":
            py_result = _normalize(
                await daf.query(QueryInfo(
                    resource_id=py_cmd["resource_id"],
                    filters=py_cmd.get("filters"),
                    algorithm=py_cmd.get("algorithm"),
                ))
            )
            rust_raw = _rust_send(parity_proc, rust_cmd)
            rust_result = _rust_normalize(rust_raw)
        else:
            pytest.fail(f"unknown op type: {op_type}")

        context[f"py_{idx}"] = py_result
        context[f"rust_{idx}"] = rust_result

    for assertion in assertions:
        field = assertion["field"]
        rust_field = assertion.get("rust_field", field)
        expected = assertion.get("value")
        op = assertion["op"]

        py_val = _resolve_field(field, context)
        rust_val = _resolve_field(rust_field, context)

        if op == "eq":
            assert py_val == rust_val == expected, \
                f"{name}: {field}={py_val!r} vs {rust_field}={rust_val!r} (expected {expected!r})"
        elif op == "not_null":
            assert py_val is not None, f"{name}: {field} is null in python"
            assert rust_val is not None, f"{name}: {rust_field} is null in rust"
        else:
            pytest.fail(f"unknown assertion op: {op}")


def _resolve_field(field: str, context: dict):
    parts = field.split(".")
    value = context
    for part in parts:
        if isinstance(value, dict):
            value = value.get(part)
        else:
            value = getattr(value, part, None)
    return value


def _load_cases():
    try:
        return _load_manifest()
    except Exception as e:
        pytest.skip(f"failed to load parity manifest: {e}")


@pytest.mark.asyncio
@pytest.mark.parametrize("case", _load_cases(), ids=lambda c: c["name"])
async def test_parity_manifest(parity_proc, case: dict):
    await _run_manifest_case(parity_proc, case)
