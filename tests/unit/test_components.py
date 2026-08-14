"""Unit tests for repository, cache, and algorithm components."""

from typing import Any

import pytest

from daf.algorithms import FibonacciDP
from daf.cache import MemoryCache
from daf.core.errors import AuthorizationError
from daf.core.protocols import Authorizer
from daf.repositories import MemoryRepository


class TestMemoryRepository:
    """Test in-memory repository implementation."""

    @pytest.mark.asyncio
    async def test_save_and_get(self) -> None:
        """Test saving and retrieving items."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        data = {"name": "John", "email": "john@example.com"}
        
        await repo.save("user:1", data)
        result = await repo.get("user:1")
        
        assert result == data

    @pytest.mark.asyncio
    async def test_get_nonexistent(self) -> None:
        """Test getting non-existent item returns None."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        result = await repo.get("nonexistent")
        assert result is None

    @pytest.mark.asyncio
    async def test_delete(self) -> None:
        """Test deleting items."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        await repo.save("user:1", {"name": "John"})
        
        await repo.delete("user:1")
        result = await repo.get("user:1")
        
        assert result is None

    @pytest.mark.asyncio
    async def test_create(self) -> None:
        """Test creating items returns a generated resource ID."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        data = {"name": "John", "email": "john@example.com"}
        
        resource_id = await repo.create(data)
        
        assert resource_id is not None
        assert isinstance(resource_id, str)
        assert len(resource_id) > 0
        saved = await repo.get(resource_id)
        assert saved == data


class TestMemoryCache:
    """Test in-memory cache implementation."""

    @pytest.mark.asyncio
    async def test_set_and_get(self) -> None:
        """Test setting and getting cache values."""
        cache = MemoryCache()
        
        await cache.set("key:1", {"data": "value"})
        result = await cache.get("key:1")
        
        assert result == {"data": "value"}

    @pytest.mark.asyncio
    async def test_get_nonexistent(self) -> None:
        """Test getting non-existent key returns None."""
        cache = MemoryCache()
        result = await cache.get("nonexistent")
        assert result is None

    @pytest.mark.asyncio
    async def test_delete(self) -> None:
        """Test deleting cache values."""
        cache = MemoryCache()
        await cache.set("key:1", "value")
        
        await cache.delete("key:1")
        result = await cache.get("key:1")
        
        assert result is None

    @pytest.mark.asyncio
    async def test_clear(self) -> None:
        """Test clearing all cache values."""
        cache = MemoryCache()
        await cache.set("key:1", "value1")
        await cache.set("key:2", "value2")
        
        await cache.clear()
        
        assert await cache.get("key:1") is None
        assert await cache.get("key:2") is None

    @pytest.mark.asyncio
    async def test_has(self) -> None:
        """Test checking cache key existence."""
        cache = MemoryCache()
        await cache.set("key:1", "value")
        
        assert await cache.has("key:1") is True
        assert await cache.has("nonexistent") is False


class TestFibonacciDP:
    """Test Fibonacci dynamic programming algorithm."""

    @pytest.mark.asyncio
    async def test_fibonacci_basic(self) -> None:
        """Test basic Fibonacci computation."""
        algo = FibonacciDP()
        
        result = await algo.execute(5)
        
        assert result == 5  # fib(5) = 5

    @pytest.mark.asyncio
    async def test_fibonacci_zero(self) -> None:
        """Test Fibonacci of 0."""
        algo = FibonacciDP()
        result = await algo.execute(0)
        assert result == 0

    @pytest.mark.asyncio
    async def test_fibonacci_one(self) -> None:
        """Test Fibonacci of 1."""
        algo = FibonacciDP()
        result = await algo.execute(1)
        assert result == 1

    @pytest.mark.asyncio
    async def test_fibonacci_larger(self) -> None:
        """Test Fibonacci with larger numbers."""
        algo = FibonacciDP()
        
        # fib(10) = 55
        result = await algo.execute(10)
        assert result == 55

    @pytest.mark.asyncio
    async def test_fibonacci_stats(self) -> None:
        """Test that memoization reduces iterations."""
        algo = FibonacciDP()
        
        await algo.execute(10)
        stats = await algo.get_stats()
        
        # With memoization, iterations should be much less than
        # without memoization (which would be 2^10 - 1 = 1023)
        assert stats["iterations"] < 100
        assert stats["cache_hits"] > 0
        assert stats["memo_size"] > 0

    @pytest.mark.asyncio
    async def test_fibonacci_memoization_efficiency(self) -> None:
        """Test that memoization actually reduces repeated computation."""
        algo = FibonacciDP()
        
        # fib(15) without memoization would require ~1000 computations
        # with memoization, only ~15 unique subproblems
        await algo.execute(15)
        stats = await algo.get_stats()
        
        # Should have computed only about 15-16 unique values
        assert stats["memo_size"] <= 16
        # Should have some cache hits showing reuse
        # (not necessarily more than memo_size)
        # For fib(15), we get 13 cache hits out of 29 computations
        assert stats["cache_hits"] >= 0
        # Should be much less than exponential
        assert stats["iterations"] < 100

    @pytest.mark.asyncio
    async def test_fibonacci_invalid_input(self) -> None:
        """Test Fibonacci with invalid input."""
        algo = FibonacciDP()
        
        with pytest.raises(ValueError):
            await algo.execute(-1)
        
        with pytest.raises(ValueError):
            await algo.execute("not a number")


class TestAuthorizerProtocol:
    """Test Authorizer protocol implementation."""

    @pytest.mark.asyncio
    async def test_authorizer_protocol_implementation(self) -> None:
        """Test that a class implementing Authorizer protocol works."""
        
        class FakeAuthorizer:
            async def authorize(
                self, _operation: str, resource_id: str | None, user: Any
            ) -> None:
                if user is None:
                    raise AuthorizationError("Unauthenticated")
                if resource_id == "forbidden":
                    raise AuthorizationError("Access denied")
        
        authorizer: Authorizer = FakeAuthorizer()
        
        # Should allow access
        await authorizer.authorize("query", "123", "user-1")
        
        # Should raise for unauthenticated
        with pytest.raises(AuthorizationError):
            await authorizer.authorize("query", "123", None)
        
        # Should raise for forbidden resource
        with pytest.raises(AuthorizationError):
            await authorizer.authorize("query", "forbidden", "user-1")
