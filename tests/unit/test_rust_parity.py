"""Parity tests between Python and Rust implementations."""

import copy

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


class TestContractRoundTrip:
    """Serialise/deserialise every daf.contracts.query model and assert field preservation."""

    def test_query_info_round_trip(self) -> None:
        info = QueryInfo(
            resource_id="123",
            filters={"status": "active"},
            algorithm="fib",
        )
        dumped = info.model_dump()
        loaded = QueryInfo(**dumped)
        assert loaded.resource_id == "123"
        assert loaded.filters == {"status": "active"}
        assert loaded.algorithm == "fib"

    def test_query_info_empty_defaults(self) -> None:
        info = QueryInfo(resource_id="123")
        dumped = info.model_dump()
        loaded = QueryInfo(**dumped)
        assert loaded.resource_id == "123"
        assert loaded.filters is None
        assert loaded.algorithm is None

    def test_post_info_round_trip(self) -> None:
        info = PostInfo(
            resource_type="user",
            data={"name": "John"},
        )
        dumped = info.model_dump()
        loaded = PostInfo(**dumped)
        assert loaded.resource_type == "user"
        assert loaded.data == {"name": "John"}

    def test_put_info_round_trip(self) -> None:
        info = PutInfo(
            resource_id="123",
            data={"name": "Jane"},
        )
        dumped = info.model_dump()
        loaded = PutInfo(**dumped)
        assert loaded.resource_id == "123"
        assert loaded.data == {"name": "Jane"}

    def test_delete_info_round_trip(self) -> None:
        info = DeleteInfo(resource_id="123")
        dumped = info.model_dump()
        loaded = DeleteInfo(**dumped)
        assert loaded.resource_id == "123"

    def test_query_result_round_trip(self) -> None:
        result = QueryResult(
            success=True,
            data={"id": "123"},
            cache_hit=True,
        )
        dumped = result.model_dump()
        loaded = QueryResult(**dumped)
        assert loaded.success is True
        assert loaded.cache_hit is True

    def test_mutation_result_round_trip(self) -> None:
        result = MutationResult(
            success=True,
            resource_id="123",
            data={"id": "123"},
        )
        dumped = result.model_dump()
        loaded = MutationResult(**dumped)
        assert loaded.success is True
        assert loaded.resource_id == "123"


class TestTrieTraversal:
    """Mirror Rust traversal_tests.rs using Python MemoryCache trie primitives."""

    @pytest.mark.asyncio
    async def test_trie_collect_matches_bruteforce_prefix(self) -> None:
        cache = MemoryCache()
        keys = ["alpha", "alb", "beta", "b", "gamma", "ga:1"]
        for k in keys:
            await cache.set(k, k)
        for prefix in ["", "a", "b", "g", "ga", "al", "be", "z"]:
            trie_result = cache._trie_collect(prefix)
            brute = {k for k in keys if k.startswith(prefix)}
            assert trie_result == brute, (
                f"prefix={prefix!r}: trie={trie_result}, brute={brute}"
            )

    @pytest.mark.asyncio
    async def test_trie_delete_prefix_removes_subtree(self) -> None:
        cache = MemoryCache()
        await cache.set("ns:a:1", "v1")
        await cache.set("ns:a:2", "v2")
        await cache.set("ns:b:1", "v3")
        await cache.set("other:x", "v4")

        removed = cache._trie_delete_prefix("ns:a:")
        assert removed == {"ns:a:1", "ns:a:2"}
        assert cache._trie_collect("") == {"ns:b:1", "other:x"}

    @pytest.mark.asyncio
    async def test_trie_delete_prefix_empty_clears_all(self) -> None:
        cache = MemoryCache()
        await cache.set("a", "v1")
        await cache.set("b", "v2")
        removed = cache._trie_delete_prefix("")
        assert removed == {"a", "b"}
        assert cache._trie_collect("") == set()

    @pytest.mark.asyncio
    async def test_trie_collect_after_mutations_matches_bruteforce(self) -> None:
        cache = MemoryCache()
        await cache.set("abc", "v1")
        await cache.set("abd", "v2")
        await cache.delete("abc")
        assert cache._trie_collect("") == {"abd"}


class TestFibonacciParity:
    """Mirror Rust fibonacci_tests.rs."""

    @pytest.mark.asyncio
    async def test_fib_zero(self) -> None:
        algo = FibonacciDP()
        result = await algo.execute(0)
        assert result == 0

    @pytest.mark.asyncio
    async def test_fib_one(self) -> None:
        algo = FibonacciDP()
        result = await algo.execute(1)
        assert result == 1

    @pytest.mark.asyncio
    async def test_fib_ten(self) -> None:
        algo = FibonacciDP()
        result = await algo.execute(10)
        assert result == 55

    @pytest.mark.asyncio
    async def test_get_stats_shape(self) -> None:
        algo = FibonacciDP()
        await algo.execute(10)
        stats = await algo.get_stats()
        assert set(stats.keys()) == {"iterations", "cache_hits", "memo_size"}
        assert stats["memo_size"] > 0


class TestGenerationAdvancement:
    """Verify post/put/delete advance generation counter in cache."""

    @pytest.mark.asyncio
    async def test_post_advances_generation(self) -> None:
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()

        post_result = await daf.post(
            PostInfo(resource_type="item", data={"name": "alice"})
        )
        resource_id = post_result.resource_id

        result = await daf.query(QueryInfo(resource_id=resource_id))
        assert result.success is True
        assert result.cache_hit is False

        result2 = await daf.query(QueryInfo(resource_id=resource_id))
        assert result2.cache_hit is True

    @pytest.mark.asyncio
    async def test_put_advances_generation(self) -> None:
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()

        await repo.save("123", {"name": "alice"})

        await daf.query(QueryInfo(resource_id="123"))

        await daf.put(
            PutInfo(resource_id="123", data={"name": "bob"})
        )

        result = await daf.query(QueryInfo(resource_id="123"))
        assert result.cache_hit is False
        assert result.data == {"name": "bob"}

    @pytest.mark.asyncio
    async def test_delete_advances_generation(self) -> None:
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()

        await repo.save("123", {"name": "alice"})

        await daf.query(QueryInfo(resource_id="123"))

        await daf.delete(DeleteInfo(resource_id="123"))

        with pytest.raises(Exception):
            await daf.query(QueryInfo(resource_id="123"))


class TestCacheInvalidation:
    """Verify delete_prefix clears query projections after mutation."""

    @pytest.mark.asyncio
    async def test_prefix_invalidation_clears_all_projections(self) -> None:
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()

        await repo.save("123", {"name": "alice"})

        await daf.query(QueryInfo(resource_id="123"))

        keys_before = cache._trie_collect("query:")
        assert len(keys_before) == 1

        await daf.put(
            PutInfo(resource_id="123", data={"name": "bob"})
        )

        keys_after = cache._trie_collect("query:")
        assert len(keys_after) == 0

    @pytest.mark.asyncio
    async def test_stale_cache_entry_rejected_after_mutation(self) -> None:
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()

        await repo.save("123", {"name": "alice"})

        r1 = await daf.query(QueryInfo(resource_id="123"))
        assert r1.success is True
        assert r1.cache_hit is False

        await daf.put(
            PutInfo(resource_id="123", data={"name": "bob"})
        )

        r2 = await daf.query(QueryInfo(resource_id="123"))
        assert r2.success is True
        assert r2.cache_hit is False
        assert r2.data == {"name": "bob"}
