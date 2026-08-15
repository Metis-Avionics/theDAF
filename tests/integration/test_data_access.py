"""Integration tests for DataAccess and DataAccessFactory."""

import copy
from typing import Any

import pytest

from daf.algorithms import FibonacciDP
from daf.cache import MemoryCache
from daf.contracts import DeleteInfo, PostInfo, PutInfo, QueryInfo
from daf.core import DataAccessFactory
from daf.core.errors import NotFoundError
from daf.repositories import MemoryRepository


class TestDataAccessFactory:
    """Test DataAccessFactory construction and behavior."""

    @pytest.mark.asyncio
    async def test_factory_creation(self) -> None:
        """Test that factory creates DataAccess instances."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        
        daf = factory.create()
        
        assert daf is not None

    @pytest.mark.asyncio
    async def test_factory_with_algorithm(self) -> None:
        """Test factory with algorithm."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        algo = FibonacciDP()
        factory = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"fibonacci": algo},
        )
        
        daf = factory.create()
        assert daf is not None


class TestDataAccessQuery:
    """Test DataAccess query operations."""

    @pytest.fixture
    def setup_daf(
        self,
    ) -> tuple[MemoryRepository[Any], MemoryCache, Any]:
        """Set up a DataAccess instance with test data."""
        repo: MemoryRepository[Any] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_query_cache_miss_then_hit(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test query: cache miss, repository hit, then cache hit."""
        repo, cache, daf = setup_daf
        
        # Populate repository
        test_data = {"id": "123", "name": "Test"}
        await repo.save("123", test_data)
        
        # First query: cache miss
        result1 = await daf.query(QueryInfo(resource_id="123"))
        assert result1.success is True
        assert result1.cache_hit is False
        assert result1.data == test_data
        
        # Second query: cache hit
        result2 = await daf.query(QueryInfo(resource_id="123"))
        assert result2.success is True
        assert result2.cache_hit is True
        assert result2.data == test_data

    @pytest.mark.asyncio
    async def test_query_not_found(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test query for non-existent resource."""
        repo, cache, daf = setup_daf
        
        with pytest.raises(NotFoundError):
            await daf.query(QueryInfo(resource_id="nonexistent"))

    @pytest.mark.asyncio
    async def test_query_with_algorithm(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test query with algorithm execution."""
        repo, cache, daf_orig = setup_daf
        
        algo = FibonacciDP()
        factory = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"fibonacci": algo},
        )
        daf = factory.create()
        
        await repo.save("fib_input", 10)
        
        result = await daf.query(
            QueryInfo(resource_id="fib_input", algorithm="fibonacci")
        )
        
        assert result.success is True
        assert result.data == 55
        assert result.algorithm_stats is not None
        assert "iterations" in result.algorithm_stats


class TestDataAccessMutations:
    """Test DataAccess mutation operations."""

    @pytest.fixture
    def setup_daf(
        self,
    ) -> tuple[MemoryRepository[Any], MemoryCache, Any]:
        """Set up a DataAccess instance."""
        repo: MemoryRepository[Any] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_post_create_resource(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test POST creates a new resource."""
        repo, cache, daf = setup_daf
        
        result = await daf.post(
            PostInfo(
                resource_type="user",
                data={"name": "John", "email": "john@example.com"},
            )
        )
        
        assert result.success is True
        assert result.resource_id is not None
        assert result.data["name"] == "John"

    @pytest.mark.asyncio
    async def test_put_update_resource(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test PUT updates an existing resource."""
        repo, cache, daf = setup_daf
        
        # Create initial resource
        await repo.save("123", {"name": "John", "age": 30})
        
        # Update it
        result = await daf.put(
            PutInfo(
                resource_id="123",
                data={"name": "Jane"},
            )
        )
        
        assert result.success is True
        assert result.data["name"] == "Jane"
        assert result.data["age"] == 30  # Original field preserved

    @pytest.mark.asyncio
    async def test_put_not_found(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test PUT on non-existent resource fails."""
        repo, cache, daf = setup_daf
        
        with pytest.raises(NotFoundError):
            await daf.put(
                PutInfo(resource_id="nonexistent", data={"name": "Jane"})
            )

    @pytest.mark.asyncio
    async def test_delete_resource(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test DELETE removes a resource."""
        repo, cache, daf = setup_daf
        
        # Create resource
        await repo.save("123", {"name": "John"})
        
        # Delete it
        result = await daf.delete(DeleteInfo(resource_id="123"))
        
        assert result.success is True
        
        # Verify it's gone
        remaining = await repo.get("123")
        assert remaining is None

    @pytest.mark.asyncio
    async def test_delete_not_found(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test DELETE on non-existent resource fails."""
        repo, cache, daf = setup_daf
        
        with pytest.raises(NotFoundError):
            await daf.delete(DeleteInfo(resource_id="nonexistent"))

    @pytest.mark.asyncio
    async def test_mutations_invalidate_cache(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test that mutations invalidate relevant cache entries."""
        repo, cache, daf = setup_daf
        
        await repo.save("123", {"name": "John"})
        await daf.query(QueryInfo(resource_id="123"))
        
        keys_before = {k for k in cache._cache if k.startswith("query:")}
        assert len(keys_before) == 1
        
        await daf.put(PutInfo(resource_id="123", data={"name": "Jane"}))
        
        assert not any(key in cache._cache for key in keys_before)


class TestSuperedgeInvalidate:
    """Test _superedge_invalidate behavior."""

    @pytest.fixture
    def setup_daf(
        self,
    ) -> tuple[MemoryRepository[Any], MemoryCache, Any]:
        repo: MemoryRepository[Any] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_superedge_invalidate_advances_generation_and_clears_prefix(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test that _superedge_invalidate clears query keys and advances generation."""
        repo, cache, daf = setup_daf

        await repo.save("res-1", {"name": "Alice"})
        await daf.query(QueryInfo(resource_id="res-1"))

        gen_before = await daf._current_generation("res-1")
        assert gen_before == 0

        keys_before = {k for k in cache._cache if k.startswith("query:")}
        assert len(keys_before) == 1

        await daf._superedge_invalidate("res-1")

        assert not any(
            key in cache._cache for key in keys_before
        ), "query keys should be removed after superedge invalidate"

        gen_after = await daf._current_generation("res-1")
        assert gen_after == gen_before + 1

    @pytest.mark.asyncio
    async def test_concurrent_mutations_with_superedge_do_not_lose_invalidations(
        self, setup_daf: tuple[MemoryRepository[Any], MemoryCache, Any]
    ) -> None:
        """Test concurrent _superedge_invalidate calls advance
        generation monotonically."""
        import asyncio

        repo, cache, daf = setup_daf
        await repo.save("res-concurrent", {"name": "Alice"})

        async def do_invalidate() -> int:
            await daf._superedge_invalidate("res-concurrent")
            return await daf._current_generation("res-concurrent")

        results = await asyncio.gather(*[do_invalidate() for _ in range(5)])

        final_gen = await daf._current_generation("res-concurrent")
        assert final_gen == 5, (
            f"Expected generation 5 after 5 invalidations, got {final_gen}"
        )
        for i, g in enumerate(results):
            assert g == i + 1, (
                f"Invalidation {i} returned generation {g}, expected {i + 1}"
            )


class TestDataAccessSubstitution:
    """Test repository substitution for testing."""

    @pytest.mark.asyncio
    async def test_fake_repository_substitution(self) -> None:
        """Test that a fake repository can be substituted."""
        
        class FakeRepository:
            async def get(self, key: str) -> Any:
                if key == "test":
                    return {"fake": True}
                return None
            
            async def save(self, key: str, value: Any) -> None:
                pass
            
            async def delete(self, key: str) -> None:
                pass
            
            async def create(self, _value: Any) -> str:
                return "generated-id"
            
            async def try_update(
                self, _key: str, _expected: Any, update: Any
            ) -> Any:
                return update(_expected)
            
            async def try_delete(self, _key: str, _expected: Any) -> bool:
                return True
        
        fake_repo = FakeRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=fake_repo, cache=cache)
        daf = factory.create()
        
        result = await daf.query(QueryInfo(resource_id="test"))
        
        assert result.success is True
        assert result.data["fake"] is True


class TestAlgorithmImmutabilityContract:
    """Test that algorithms must not mutate their input."""

    @pytest.mark.asyncio
    async def test_algorithm_must_not_mutate_input(self) -> None:
        """Test that the raw data passed to the authorizer is unaffected
        even if an algorithm attempts in-place mutation."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()

        original_data = {"name": "John", "status": "active"}

        class MutatingAlgorithm:
            async def execute(self, data: Any) -> Any:
                if isinstance(data, dict):
                    data["name"] = "Mutated"
                return data

            async def get_stats(self) -> dict[str, Any]:
                return {}

        authorizer_calls: list[Any] = []

        class SpyAuthorizer:
            async def authorize(
                self,
                _operation: str,
                _resource_id: str | None,
                _user: Any,
                data: Any = None,
            ) -> None:
                authorizer_calls.append(
                    copy.deepcopy(data) if isinstance(data, dict) else data
                )

        factory = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"mutating": MutatingAlgorithm()},
            authorizer=SpyAuthorizer(),
        )
        daf = factory.create()

        await repo.save("123", original_data)

        result = await daf.query(
            QueryInfo(resource_id="123", algorithm="mutating")
        )
        assert result.success is True
        assert result.data == {"name": "Mutated", "status": "active"}

        assert len(authorizer_calls) == 1
        assert authorizer_calls[0] == original_data
