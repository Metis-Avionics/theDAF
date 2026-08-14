"""Interaction tests for security and cache invariants."""

import asyncio
from typing import Any

import pytest

from daf.algorithms import FibonacciDP
from daf.cache import MemoryCache
from daf.contracts import PostInfo, QueryInfo
from daf.core import DataAccessFactory
from daf.core.errors import AuthorizationError
from daf.repositories import MemoryRepository


class FakeUser:
    """Simple user model for testing."""

    def __init__(self, user_id: str) -> None:
        self.id = user_id


class FakeAuthorizer:
    """Fake authorizer for testing DataAccess authorization."""

    def __init__(self, owned_resources: dict[str, str]) -> None:
        self._owned_resources = owned_resources

    async def authorize(
        self, _operation: str, resource_id: str | None, user: Any
    ) -> None:
        if user is None:
            raise AuthorizationError("Unauthenticated")
        if resource_id is None:
            return
        if resource_id not in self._owned_resources:
            return
        if self._owned_resources[resource_id] != user.id:
            raise AuthorizationError(f"Access denied to resource '{resource_id}'")


class TestAuthorizationCacheIsolation:
    """Test that authorization context is isolated in cache."""

    @pytest.mark.asyncio
    async def test_different_users_get_different_cache_entries(
        self,
    ) -> None:
        """Test same resource queried by different users gets separate caches."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        result_user1 = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result_user1.success is True
        assert result_user1.cache_hit is False
        
        result_user1_again = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result_user1_again.success is True
        assert result_user1_again.cache_hit is True
        
        result_user2 = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-2")
        )
        assert result_user2.success is False
        assert result_user2.error_type == "authorization"
        assert result_user2.cache_hit is False


class TestAlgorithmCacheIsolation:
    """Test that algorithm selection is isolated in cache."""

    @pytest.mark.asyncio
    async def test_different_algorithms_produce_different_cache_keys(self) -> None:
        """Test that different algorithms produce separate cache entries."""
        repo: MemoryRepository[Any] = MemoryRepository()
        cache = MemoryCache()
        algo = FibonacciDP()
        
        await repo.save("fib_5", 5)
        
        factory = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"fibonacci": algo},
        )
        daf = factory.create()
        
        result_algo = await daf.query(
            QueryInfo(resource_id="fib_5", algorithm="fibonacci")
        )
        assert result_algo.success is True
        assert result_algo.data == 5
        assert result_algo.cache_hit is False
        
        result_algo_again = await daf.query(
            QueryInfo(resource_id="fib_5", algorithm="fibonacci")
        )
        assert result_algo_again.success is True
        assert result_algo_again.cache_hit is True
        
        result_no_algo = await daf.query(
            QueryInfo(resource_id="fib_5")
        )
        assert result_no_algo.success is True
        assert result_no_algo.data == 5
        assert result_no_algo.cache_hit is False


class TestFilterCacheIsolation:
    """Test that filters are isolated in cache."""

    @pytest.mark.asyncio
    async def test_different_filters_produce_different_cache_keys(self) -> None:
        """Test that different filters produce separate cache entries."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        await repo.save("123", {"name": "John", "status": "active", "age": 30})
        
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        result_active = await daf.query(
            QueryInfo(resource_id="123", filters={"status": "active"})
        )
        assert result_active.success is True
        assert result_active.data == {"name": "John", "status": "active", "age": 30}
        assert result_active.cache_hit is False
        
        result_active_again = await daf.query(
            QueryInfo(resource_id="123", filters={"status": "active"})
        )
        assert result_active_again.success is True
        assert result_active_again.cache_hit is True
        
        result_deleted = await daf.query(
            QueryInfo(resource_id="123", filters={"status": "deleted"})
        )
        assert result_deleted.success is True
        assert result_deleted.data == {}
        assert result_deleted.cache_hit is False


class TestConcurrentPost:
    """Test concurrent POST operations generate unique IDs."""

    @pytest.mark.asyncio
    async def test_concurrent_posts_generate_unique_ids(self) -> None:
        """Test that concurrent POSTs do not generate duplicate IDs."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        async def create_resource() -> str | None:
            result = await daf.post(
                PostInfo(resource_type="user", data={"name": "Test"})
            )
            assert result.success is True
            return result.resource_id
        
        ids = await asyncio.gather(*[create_resource() for _ in range(10)])
        
        assert len(set(ids)) == 10
        
        for resource_id in ids:
            assert resource_id is not None
            saved = await repo.get(resource_id)
            assert saved is not None
            assert saved["name"] == "Test"


class TestCacheKeyCanonicalization:
    """Test cache key canonicalization."""

    @pytest.mark.asyncio
    async def test_cache_key_includes_all_query_semantics(self) -> None:
        """Test that cache key includes resource_id, filters, algorithm, and user_id."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        await repo.save("123", {"name": "John", "status": "active"})
        
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        await daf.query(QueryInfo(resource_id="123"), user=FakeUser("user-1"))
        await daf.query(
            QueryInfo(resource_id="123", filters={"status": "active"}),
            user=FakeUser("user-1"),
        )
        await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-2")
        )
        
        keys = list(cache._cache.keys())
        assert len(keys) == 3
        assert all(key.startswith("query:123:") for key in keys)
        
        user1_no_filter = "query:123:{}::user-1"
        user1_with_filter = "query:123:{\"status\": \"active\"}::user-1"
        user2_no_filter = "query:123:{}::user-2"
        
        assert user1_no_filter in keys
        assert user1_with_filter in keys
        assert user2_no_filter in keys


class TestErrorTypePreservation:
    """Test that error types are preserved in result envelopes."""

    @pytest.mark.asyncio
    async def test_not_found_error_preserves_type(self) -> None:
        """Test that NotFoundError is preserved in QueryResult."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        result = await daf.query(QueryInfo(resource_id="nonexistent"))
        
        assert result.success is False
        assert result.error_type == "not_found"
        assert result.error == "Not found"

    @pytest.mark.asyncio
    async def test_validation_error_preserves_type(self) -> None:
        """Test that ValidationError is preserved in MutationResult."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        result = await daf.post(PostInfo(resource_type="", data={"name": "John"}))
        
        assert result.success is False
        assert result.error_type == "validation"
        assert result.error == "Validation error"

    @pytest.mark.asyncio
    async def test_authorization_error_preserves_type(self) -> None:
        """Test that AuthorizationError is preserved in QueryResult."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-2")
        )
        
        assert result.success is False
        assert result.error_type == "authorization"
        assert result.error == "Unauthorized"

    @pytest.mark.asyncio
    async def test_unexpected_error_propagates(self) -> None:
        """Test that unexpected errors propagate as exceptions."""
        
        class FailingRepository:
            async def get(self, _key: str) -> Any:
                raise RuntimeError("Unexpected repository failure")
            
            async def save(self, _key: str, _value: Any) -> None:
                pass
            
            async def delete(self, _key: str) -> None:
                pass
            
            async def create(self, _value: Any) -> str:
                return "id"
        
        repo = FailingRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(RuntimeError):
            await daf.query(QueryInfo(resource_id="123"))


class TestInputValidation:
    """Test input validation guards."""

    @pytest.mark.asyncio
    async def test_empty_resource_id_returns_validation_error(self) -> None:
        """Test that empty resource_id returns validation error."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        result = await daf.query(QueryInfo(resource_id=""))
        assert result.success is False
        assert result.error_type == "validation"

    @pytest.mark.asyncio
    async def test_nonexistent_resource_returns_not_found_for_authenticated_user(
        self,
    ) -> None:
        """Test that non-existent resource returns not_found for authenticated user."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        result = await daf.query(
            QueryInfo(resource_id="nonexistent"),
            user=FakeUser("user-1"),
        )
        assert result.success is False
        assert result.error_type == "not_found"


class TestFilterEdgeCases:
    """Test filter edge cases."""

    @pytest.mark.asyncio
    async def test_filters_with_non_dict_data_returns_empty(self) -> None:
        """Test that filters on non-dict data return empty dict."""
        repo: MemoryRepository[Any] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        await repo.save("123", 42)
        
        result = await daf.query(
            QueryInfo(resource_id="123", filters={"status": "active"})
        )
        assert result.success is True
        assert result.data == {}

    @pytest.mark.asyncio
    async def test_non_serializable_filters_returns_validation_error(
        self,
    ) -> None:
        """Test that non-JSON-serializable filters return validation error."""
        from datetime import datetime
        
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        await repo.save("123", {"name": "Test"})
        
        result = await daf.query(
            QueryInfo(resource_id="123", filters={"created": datetime.now()})
        )
        assert result.success is False
        assert result.error_type == "validation"


class TestFastAPIAdapterRequiresGetCurrentUser:
    """Test that FastAPI adapter requires get_current_user."""

    def test_missing_get_current_user_raises(self) -> None:
        """Test that DataAccessRouter raises ValueError without get_current_user."""
        from daf.adapters.fastapi import DataAccessRouter
        
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(ValueError, match="get_current_user is required"):
            DataAccessRouter(daf, get_current_user=None)  # type: ignore

    @pytest.mark.asyncio
    async def test_post_without_get_current_user_raises(self) -> None:
        """Test that POST without get_current_user raises at router construction."""
        from daf.adapters.fastapi import DataAccessRouter
        
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(ValueError, match="get_current_user is required"):
            DataAccessRouter(daf, get_current_user=None)  # type: ignore
