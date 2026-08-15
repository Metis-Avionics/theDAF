"""Direct unit tests for daf.utils._memoize primitives."""

import asyncio

import pytest

from daf.utils._memoize import Memo, ResourceMemo


class TestMemo:
    """Tests for the Memo key→value cache."""

    def test_has_miss(self) -> None:
        memo = Memo()
        assert memo.has("missing") is False

    def test_get_raises_on_miss(self) -> None:
        memo = Memo()
        with pytest.raises(KeyError):
            memo.get("missing")
        assert memo.stats()["iterations"] == 1
        assert memo.stats()["cache_hits"] == 0

    def test_get_returns_on_hit(self) -> None:
        memo = Memo()
        memo.set("k", "v")
        assert memo.get("k") == "v"
        assert memo.stats()["cache_hits"] == 1
        assert memo.stats()["iterations"] == 0

    def test_set_then_get(self) -> None:
        memo = Memo()
        memo.set("key", 42)
        assert memo.get("key") == 42

    def test_clear_resets(self) -> None:
        memo = Memo()
        memo.set("k", "v")
        memo.get("k")
        memo.clear()
        assert memo.has("k") is False
        assert memo.stats() == {"iterations": 0, "cache_hits": 0, "memo_size": 0}

    def test_stats_shape(self) -> None:
        memo = Memo()
        memo.set("a", 1)
        memo.get("a")
        with pytest.raises(KeyError):
            memo.get("b")
        stats = memo.stats()
        assert set(stats.keys()) == {"iterations", "cache_hits", "memo_size"}
        assert stats["memo_size"] == 1
        assert stats["cache_hits"] == 1
        assert stats["iterations"] == 1


class TestResourceMemo:
    """Tests for the ResourceMemo lazy-init resource cache."""

    @pytest.mark.asyncio
    async def test_lazy_init_same_object(self) -> None:
        seen: list = []

        def factory(key_arg: str) -> str:
            seen.append(key_arg)
            return f"obj-{key_arg}"

        memo = ResourceMemo(key_fn=str, factory=factory)
        result_a = await memo.get("x")
        result_b = await memo.get("x")
        assert result_a is result_b
        assert len(seen) == 1

    @pytest.mark.asyncio
    async def test_different_keys_different_objects(self) -> None:
        counter = 0

        def factory(key_arg: str) -> str:
            nonlocal counter
            counter += 1
            return f"obj-{key_arg}-{counter}"

        memo = ResourceMemo(key_fn=str, factory=factory)
        r1 = await memo.get("a")
        r2 = await memo.get("b")
        assert r1 != r2
        assert r1 == "obj-a-1"
        assert r2 == "obj-b-2"

    @pytest.mark.asyncio
    async def test_concurrent_same_lock(self) -> None:
        creation_order: list[int] = []

        def factory(key_arg: int) -> int:
            creation_order.append(key_arg)
            return key_arg * 10

        memo = ResourceMemo(key_fn=int, factory=factory)
        key_arg = 7
        results = await asyncio.gather(
            memo.get(key_arg),
            memo.get(key_arg),
            memo.get(key_arg),
        )
        assert all(r == 70 for r in results)
        assert creation_order == [7]

    @pytest.mark.asyncio
    async def test_clear_empties_store(self) -> None:
        call_count = 0

        def factory(key_arg: str) -> str:
            nonlocal call_count
            call_count += 1
            return f"new-{key_arg}"

        memo = ResourceMemo(key_fn=str, factory=factory)
        await memo.get("k")
        assert call_count == 1
        await memo.clear()
        await memo.get("k")
        assert call_count == 2
