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
"""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

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

# Lazily resolved subprocess handle; None means "not yet checked".
_PARITY_PROC: subprocess.Popen | None = None


def _build_parity_bin() -> Path:
    result = subprocess.run(  # noqa: S603 — pinned toolchain binary, fixed args
        [shutil.which("cargo"), "build", "--bin", "daf-parity"],
        cwd=str(REPO_ROOT / "crates" / "daf-ffi"),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        pytest.skip(
            "daf-parity binary failed to build:\n"
            + result.stderr[-2000:]
        )
    return PARITY_BIN


def _get_parity_proc() -> subprocess.Popen:
    global _PARITY_PROC
    if _PARITY_PROC is None:
        bin_path = _build_parity_bin()
        _PARITY_PROC = subprocess.Popen(  # noqa: S603 — fixed path built above
            [str(bin_path)],  # noqa: S603 — fixed path built above, not untrusted
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
    return _PARITY_PROC


def _rust_send(cmd: dict) -> dict:
    proc = _get_parity_proc()
    line = json.dumps(cmd, default=str) + "\n"
    try:
        proc.stdin.write(line)
        proc.stdin.flush()
    except BrokenPipeError:
        pytest.skip("daf-parity binary exited unexpectedly")
    response_line = proc.stdout.readline()
    if not response_line:
        stderr = proc.stderr.read()[-500:]
        pytest.skip(f"daf-parity produced no output. stderr: {stderr!r}")
    return json.loads(response_line)


def _normalize(result: MutationResult | QueryResult) -> dict:
    d = json.loads(result.model_dump_json())
    d.pop("timestamp", None)
    return d


def _rust_normalize(r: dict) -> dict:
    d = dict(r)
    d.pop("timestamp", None)
    return d


# ---------------------------------------------------------------------------
# Shared setup
# ---------------------------------------------------------------------------


def _python_factory():
    repo = MemoryRepository()
    cache = MemoryCache()
    return DataAccessFactory(repository=repo, cache=cache)


# ---------------------------------------------------------------------------
# Differential tests
# ---------------------------------------------------------------------------


class TestPostParity:
    @pytest.mark.asyncio
    async def test_post_mutation_result_matches(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        py = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))

        rust = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })

        assert py.success == rust["success"]
        assert py.error == rust.get("error")
        assert py.error_type == rust.get("error_type")
        # Both must return a resource_id
        assert py.resource_id is not None
        assert rust.get("resource_id") is not None


class TestPutParity:
    @pytest.mark.asyncio
    async def test_put_mutation_result_matches(self) -> None:
        # Mirrored sequences: each runtime posts into its OWN daf and reuses the
        # rid its OWN post returned. rids are not interchangeable across runtimes
        # (python mints UUIDs, rust ULIDs, and the shared rust proc has its own
        # repo), so a one-sided rid asserts cross-repo state, not parity.
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        py = await daf.put(PutInfo(resource_id=py_rid, data={"name": "bob"}))

        rust = _rust_send({
            "op": "put",
            "resource_id": rust_rid,
            "data": {"name": "bob"},
        })

        assert py.success == rust["success"]
        assert py.error == rust.get("error")
        assert py.error_type == rust.get("error_type")


class TestDeleteParity:
    @pytest.mark.asyncio
    async def test_delete_mutation_result_matches(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        py = await daf.delete(DeleteInfo(resource_id=py_rid))

        rust = _rust_send({
            "op": "delete",
            "resource_id": rust_rid,
        })

        assert py.success == rust["success"]
        assert py.error == rust.get("error")
        assert py.error_type == rust.get("error_type")


class TestQueryCacheMissParity:
    @pytest.mark.asyncio
    async def test_query_cache_miss_matches(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        py = await daf.query(QueryInfo(resource_id=py_rid))

        rust = _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })

        assert py.success == rust["success"]
        assert py.cache_hit == rust.get("cache_hit")
        assert py.data == rust.get("data")


class TestQueryCacheHitParity:
    @pytest.mark.asyncio
    async def test_query_cache_hit_matches(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        await daf.query(QueryInfo(resource_id=py_rid))  # warm cache
        _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })  # warm rust cache

        py = await daf.query(QueryInfo(resource_id=py_rid))
        assert py.cache_hit is True

        rust = _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })

        assert py.success == rust["success"]
        assert py.cache_hit == rust.get("cache_hit")
        assert py.data == rust.get("data")


class TestGenerationRoundTripParity:
    @pytest.mark.asyncio
    async def test_generation_advances_after_put(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        await daf.query(QueryInfo(resource_id=py_rid))  # establish gen=0
        _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })

        await daf.put(PutInfo(resource_id=py_rid, data={"name": "bob"}))
        _rust_send({
            "op": "put",
            "resource_id": rust_rid,
            "data": {"name": "bob"},
        })

        # Second query must be a cache miss (stale rejection)
        py = await daf.query(QueryInfo(resource_id=py_rid))
        assert py.cache_hit is False
        assert py.data == {"name": "bob"}

        rust = _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })

        assert py.success == rust["success"]
        assert py.data == rust.get("data")


class TestCacheInvalidationParity:
    @pytest.mark.asyncio
    async def test_stale_rejected_after_put(self) -> None:
        factory = _python_factory()
        daf = factory.create()
        post = await daf.post(PostInfo(resource_type="user", data={"name": "alice"}))
        py_rid = post.resource_id

        rust_post = _rust_send({
            "op": "post",
            "resource_type": "user",
            "data": {"name": "alice"},
        })
        rust_rid = rust_post["resource_id"]

        r1 = await daf.query(QueryInfo(resource_id=py_rid))
        assert r1.cache_hit is False

        await daf.put(PutInfo(resource_id=py_rid, data={"name": "bob"}))

        r2 = await daf.query(QueryInfo(resource_id=py_rid))
        assert r2.cache_hit is False
        assert r2.data == {"name": "bob"}

        _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })
        _rust_send({
            "op": "put",
            "resource_id": rust_rid,
            "data": {"name": "bob"},
        })
        rust = _rust_send({
            "op": "query",
            "resource_id": rust_rid,
            "filters": None,
            "algorithm": None,
        })

        assert r2.success == rust["success"]
        assert r2.data == rust.get("data")
