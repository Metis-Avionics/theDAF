"""Interaction tests for security and cache invariants."""

import asyncio
import hashlib
import json
from typing import Any

import pytest

from daf.algorithms import FibonacciDP
from daf.cache import MemoryCache
from daf.contracts import DeleteInfo, PostInfo, PutInfo, QueryInfo
from daf.core import DataAccessFactory
from daf.core.errors import AuthorizationError, NotFoundError, ValidationError
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
        self,
        _operation: str,
        resource_id: str | None,
        user: Any,
        data: Any = None,  # noqa: ARG002
    ) -> None:
        if user is None:
            raise AuthorizationError("Unauthenticated")
        if resource_id is None:
            return
        if resource_id not in self._owned_resources:
            return
        if self._owned_resources[resource_id] != user.id:
            raise AuthorizationError(f"Access denied to resource '{resource_id}'")


class _StripOwnerIdAlgorithm:
    """Algorithm that removes owner_id from dict data."""

    async def execute(self, data: Any) -> Any:
        if isinstance(data, dict):
            return {k: v for k, v in data.items() if k != "owner_id"}
        return data

    async def get_stats(self) -> dict[str, Any]:
        return {}


def _expected_cache_key(
    resource_id: str,
    filters: dict[str, Any] | None,
    algorithm: str | None,
    user_id: str,
) -> str:
    payload = {
        "resource_id": resource_id,
        "filters": filters or {},
        "algorithm": algorithm or "",
        "user_id": user_id,
    }
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    namespace = hashlib.sha256(resource_id.encode()).hexdigest()
    return f"query:{namespace}:{hashlib.sha256(canonical.encode()).hexdigest()}"


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
        
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123"), user=FakeUser("user-2")
            )


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
        
        user1_no_filter = _expected_cache_key("123", {}, None, "user-1")
        user1_with_filter = _expected_cache_key(
            "123", {"status": "active"}, None, "user-1"
        )
        user2_no_filter = _expected_cache_key("123", {}, None, "user-2")
        
        assert user1_no_filter in keys
        assert user1_with_filter in keys
        assert user2_no_filter in keys

    @pytest.mark.asyncio
    async def test_invalidation_prefix_is_namespace_isolated(self) -> None:
        """Test that invalidating resource 'a:b' does not invalidate cache
        entries for resource 'a:b:c'."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        user = FakeUser("user-1")
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        await repo.save("a:b", {"name": "Resource A"})
        await repo.save("a:b:c", {"name": "Resource B"})
        
        await daf.query(QueryInfo(resource_id="a:b"), user=user)
        await daf.query(QueryInfo(resource_id="a:b:c"), user=user)
        
        await daf.put(
            PutInfo(resource_id="a:b", data={"name": "Resource A updated"}),
            user=user,
        )
        
        result_b_after = await daf.query(QueryInfo(resource_id="a:b:c"), user=user)
        assert result_b_after.success is True
        assert result_b_after.cache_hit is True
        assert result_b_after.data == {"name": "Resource B"}


class TestErrorTypePreservation:
    """Test that error types are preserved in result envelopes."""

    @pytest.mark.asyncio
    async def test_not_found_error_preserves_type(self) -> None:
        """Test that NotFoundError is raised for missing resources."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(NotFoundError):
            await daf.query(QueryInfo(resource_id="nonexistent"))

    @pytest.mark.asyncio
    async def test_validation_error_preserves_type(self) -> None:
        """Test that ValidationError is raised for invalid post input."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(ValidationError):
            await daf.post(PostInfo(resource_type="", data={"name": "John"}))

    @pytest.mark.asyncio
    async def test_authorization_error_preserves_type(self) -> None:
        """Test that AuthorizationError is raised for unauthorized access."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123"), user=FakeUser("user-2")
            )

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
            
            async def try_update(
                self, _key: str, _expected: Any, update: Any
            ) -> Any:
                return update(_expected)
            
            async def try_delete(self, _key: str, _expected: Any) -> bool:
                return True
        
        repo = FailingRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(RuntimeError):
            await daf.query(QueryInfo(resource_id="123"))


class TestInputValidation:
    """Test input validation guards."""

    @pytest.mark.asyncio
    async def test_empty_resource_id_returns_validation_error(self) -> None:
        """Test that empty resource_id raises validation error."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        with pytest.raises(ValidationError):
            await daf.query(QueryInfo(resource_id=""))

    @pytest.mark.asyncio
    async def test_nonexistent_resource_returns_not_found_for_authenticated_user(
        self,
    ) -> None:
        """Test that non-existent resource raises NotFoundError for authenticated
        user."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        with pytest.raises(NotFoundError):
            await daf.query(
                QueryInfo(resource_id="nonexistent"),
                user=FakeUser("user-1"),
            )


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
        
        with pytest.raises(ValidationError):
            await daf.query(
                QueryInfo(resource_id="123", filters={"created": datetime.now()})
            )


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


class TestPrefixCacheInvalidation:
    """Test that mutations invalidate all cached projections for a resource."""

    @pytest.mark.asyncio
    async def test_put_invalidates_all_filtered_projections(self) -> None:
        """Test that PUT invalidates cached entries for all filter variants."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save(
            "123",
            {"name": "John", "status": "active", "owner_id": "user-1"},
        )
        
        user = FakeUser("user-1")
        await daf.query(QueryInfo(resource_id="123"), user=user)
        await daf.query(
            QueryInfo(resource_id="123", filters={"status": "active"}), user=user
        )
        
        keys_before = set(cache._cache.keys())
        assert len(keys_before) == 2
        
        await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=user,
        )
        
        assert not any(key in cache._cache for key in keys_before)

    @pytest.mark.asyncio
    async def test_delete_invalidates_all_algorithm_projections(self) -> None:
        """Test that DELETE invalidates cached entries for all algorithm variants."""
        from daf.algorithms import FibonacciDP
        
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
        
        await daf.query(QueryInfo(resource_id="fib_5"))
        await daf.query(QueryInfo(resource_id="fib_5", algorithm="fibonacci"))
        
        keys_before = set(cache._cache.keys())
        assert len(keys_before) == 2
        
        await daf.delete(DeleteInfo(resource_id="fib_5"))
        
        assert not any(key in cache._cache for key in keys_before)


class TestAtomicAuthAndRead:
    """Test that authorization and mutation reads are atomic."""

    @pytest.mark.asyncio
    async def test_put_uses_single_repository_read(self) -> None:
        """Test that PUT reads the repository only once."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        read_count = 0
        original_get = repo.get
        
        async def counting_get(key: str) -> Any:
            nonlocal read_count
            read_count += 1
            return await original_get(key)
        
        repo.get = counting_get  # type: ignore
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=FakeUser("user-1"),
        )
        assert result.success is True
        assert read_count == 1

    @pytest.mark.asyncio
    async def test_delete_uses_single_repository_read(self) -> None:
        """Test that DELETE reads the repository only once."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        read_count = 0
        original_get = repo.get
        
        async def counting_get(key: str) -> Any:
            nonlocal read_count
            read_count += 1
            return await original_get(key)
        
        repo.get = counting_get  # type: ignore
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.delete(
            DeleteInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result.success is True
        assert read_count == 1

    @pytest.mark.asyncio
    async def test_query_uses_single_repository_read(self) -> None:
        """Test that QUERY reads the repository only once on cache miss."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        read_count = 0
        original_get = repo.get
        
        async def counting_get(key: str) -> Any:
            nonlocal read_count
            read_count += 1
            return await original_get(key)
        
        repo.get = counting_get  # type: ignore
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result.success is True
        assert read_count == 1


class TestMutableValueIsolation:
    """Test that repository and cache return independent copies."""

    @pytest.mark.asyncio
    async def test_repository_returns_independent_copy(self) -> None:
        """Test that mutating returned dict does not affect repository state."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        original = {"name": "John", "owner_id": "user-1"}
        await repo.save("123", original)
        
        fetched = await repo.get("123")
        assert fetched is not None
        fetched_copy: dict[str, Any] = fetched
        fetched_copy["name"] = "Jane"
        
        stored = await repo.get("123")
        assert stored is not None
        assert stored["name"] == "John"

        list_original = [1, 2, {"nested": "value"}]
        await repo.save("list:1", list_original)
        
        list_fetched = await repo.get("list:1")
        assert list_fetched is not None
        list_fetched.append(3)
        list_fetched[2]["nested"] = "mutated"
        
        stored_list = await repo.get("list:1")
        assert stored_list == [1, 2, {"nested": "value"}]

    @pytest.mark.asyncio
    async def test_try_update_returns_independent_copy(self) -> None:
        """Test that try_update returns a deep copy; mutating it does not
        affect stored state."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        await repo.save("123", {"name": "John", "owner_id": "user-1"})

        current = await repo.get("123")
        assert current is not None

        returned = await repo.try_update(
            "123",
            expected=current,
            update=lambda e: {**e, "name": "Jane"},
        )
        assert returned is not None
        returned["name"] = "Mutated"

        stored = await repo.get("123")
        assert stored is not None
        assert stored["name"] == "Jane"

    @pytest.mark.asyncio
    async def test_cache_returns_independent_copy(self) -> None:
        """Test that mutating cached dict does not affect cache state."""
        cache = MemoryCache()
        original = {"name": "John", "owner_id": "user-1"}
        await cache.set("query:123:abc", original)
        
        fetched = await cache.get("query:123:abc")
        assert fetched is not None
        fetched_copy: dict[str, Any] = fetched
        fetched_copy["name"] = "Jane"
        
        cached_again = await cache.get("query:123:abc")
        assert cached_again is not None
        assert cached_again["name"] == "John"

        list_original = [1, 2, {"nested": "value"}]
        await cache.set("list:1", list_original)
        
        list_fetched = await cache.get("list:1")
        assert list_fetched is not None
        list_fetched.append(3)
        list_fetched[2]["nested"] = "mutated"
        
        cached_list_again = await cache.get("list:1")
        assert cached_list_again == [1, 2, {"nested": "value"}]


class TestUnknownAlgorithmValidation:
    """Test that unknown algorithms return validation errors."""

    @pytest.mark.asyncio
    async def test_unknown_algorithm_returns_validation_error(self) -> None:
        """Test that querying with unknown algorithm raises validation error."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()
        
        await repo.save("123", {"name": "Test"})
        
        with pytest.raises(ValidationError):
            await daf.query(
                QueryInfo(resource_id="123", algorithm="nonexistent")
            )


class TestPostAuthorization:
    """Test POST authorization with data inspection."""

    class RejectAdminPosts:
        """Authorizer that rejects POST data containing 'admin' key."""

        async def authorize(
            self,
            operation: str,
            _resource_id: str | None,
            _user: Any,
            data: Any = None,
        ) -> None:
            if (
                operation == "post"
                and data is not None
                and isinstance(data, dict)
                and "admin" in data
            ):
                raise AuthorizationError("Cannot create admin users via POST")

    @pytest.mark.asyncio
    async def test_post_authorizer_rejects_creation(self) -> None:
        """Test that authorizer can reject POST creation based on data content."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = TestPostAuthorization.RejectAdminPosts()
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        with pytest.raises(AuthorizationError):
            await daf.post(
                PostInfo(resource_type="user", data={"name": "Admin", "admin": True}),
                user=FakeUser("user-1"),
            )

    @pytest.mark.asyncio
    async def test_post_authorizer_receives_data(self) -> None:
        """Test that POST authorize() is called with the proposed data."""
        received_calls: list[Any] = []

        class SpyAuthorizer:
            async def authorize(
                self,
                operation: str,
                resource_id: str | None,
                user: Any,
                data: Any = None,
            ) -> None:
                received_calls.append((operation, resource_id, user, data))

        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = SpyAuthorizer()
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        post_data = {"name": "John"}
        await daf.post(
            PostInfo(resource_type="user", data=post_data),
            user=FakeUser("user-1"),
        )
        
        assert len(received_calls) == 1
        op, rid, usr, data = received_calls[0]
        assert op == "post"
        assert rid is None
        assert data == post_data


class TestCacheHitReauthorization:
    """Test that cache hits re-authorize before returning data."""

    @pytest.mark.asyncio
    async def test_cache_hit_reauthorizes(self) -> None:
        """Test that cache hit re-authorizes and rejects when access is revoked."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        class RevocableAuthorizer:
            def __init__(self) -> None:
                self._allowed = True
            
            async def authorize(
                self,
                _operation: str,
                _resource_id: str | None,
                user: Any,
                data: Any = None,
            ) -> None:
                if not self._allowed:
                    raise AuthorizationError("Access revoked")
                if data is not None and isinstance(data, dict):
                    owner_id = data.get("owner_id")
                    if owner_id != user.id:
                        raise AuthorizationError("Access denied")
        
        authorizer = RevocableAuthorizer()
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result1 = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result1.success is True
        assert result1.cache_hit is False
        
        authorizer._allowed = False
        
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123"), user=FakeUser("user-1")
            )


class TestCacheHitAuthorizationRawData:
    """Test that the authorizer receives raw repository data on cache hit."""

    @pytest.mark.asyncio
    async def test_authorizer_receives_raw_data_on_cache_hit(self) -> None:
        """Test that authorization on cache hit uses raw data, not cached result."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        received_data: list[Any] = []
        
        class SpyAuthorizer:
            async def authorize(
                self,
                _operation: str,
                _resource_id: str | None,
                _user: Any,
                data: Any = None,
            ) -> None:
                received_data.append(data)
        
        authorizer = SpyAuthorizer()
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result1 = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result1.success is True
        assert result1.cache_hit is False
        assert len(received_data) == 1
        assert received_data[0] == {"name": "John", "owner_id": "user-1"}
        
        result2 = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result2.success is True
        assert result2.cache_hit is True
        assert len(received_data) == 2
        assert received_data[1] == {"name": "John", "owner_id": "user-1"}

    @pytest.mark.asyncio
    async def test_cache_hit_authorization_sees_raw_owner_id(
        self,
    ) -> None:
        """Test that algorithm-stripped owner_id is still visible to
        authorizer on cache hit."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        algo = _StripOwnerIdAlgorithm()
        factory = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"strip_owner": algo},
            authorizer=authorizer,
        )
        daf = factory.create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result1 = await daf.query(
            QueryInfo(resource_id="123", algorithm="strip_owner"),
            user=FakeUser("user-1"),
        )
        assert result1.success is True
        assert result1.cache_hit is False
        assert "owner_id" not in result1.data
        
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123", algorithm="strip_owner"),
                user=FakeUser("user-2"),
            )


class TestConcurrentModificationDetection:
    """Test CAS detection of concurrent modifications."""

    @pytest.mark.asyncio
    async def test_put_detects_concurrent_modification(self) -> None:
        """Test that PUT fails when concurrent modification is detected."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        async def failing_try_update(_key: str, expected: Any, update: Any) -> Any:  # noqa: ARG001
            return None
        
        repo.try_update = failing_try_update  # type: ignore
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=FakeUser("user-1"),
        )
        assert result.success is False
        assert result.error_type == "conflict"

    @pytest.mark.asyncio
    async def test_delete_detects_concurrent_modification(self) -> None:
        """Test that DELETE fails when concurrent modification is detected."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        
        async def failing_try_delete(_key: str, expected: Any) -> bool:  # noqa: ARG001
            return False
        
        repo.try_delete = failing_try_delete  # type: ignore
        
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.delete(
            DeleteInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result.success is False
        assert result.error_type == "conflict"


class TestStaleCacheResurrection:
    """Test that stale cache entries are never served after a mutation."""

    @pytest.mark.asyncio
    async def test_stale_cache_not_resurrected_after_mutation(self) -> None:
        """Test that a stale cache entry with an old generation is treated
        as a miss after a mutation increments the generation counter."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})
        daf = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()

        await repo.save(
            "123",
            {"name": "John", "status": "active", "owner_id": "user-1"},
        )
        user = FakeUser("user-1")

        result1 = await daf.query(QueryInfo(resource_id="123"), user=user)
        assert result1.success is True
        assert result1.cache_hit is False
        assert result1.data == {
            "name": "John",
            "status": "active",
            "owner_id": "user-1",
        }

        await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=user,
        )

        cache_key = _expected_cache_key("123", {}, None, "user-1")
        stale_entry = {
            "raw": {"name": "John", "status": "active", "owner_id": "user-1"},
            "transformed": {"name": "John", "status": "active"},
            "generation": 0,
        }
        cache._cache[cache_key] = stale_entry

        result2 = await daf.query(QueryInfo(resource_id="123"), user=user)
        assert result2.success is True
        assert result2.cache_hit is False
        assert result2.data == {
            "name": "Jane",
            "status": "active",
            "owner_id": "user-1",
        }


class TestStaleCacheResurrectionAcrossInstances:
    """Test that stale cache entries are not served across DataAccess instances."""

    @pytest.mark.asyncio
    async def test_stale_cache_not_resurrected_across_data_access_instances(
        self,
    ) -> None:
        """Test that instance A's stale cache miss is rejected after instance B
        mutates the resource and advances the generation counter."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(owned_resources={"123": "user-1"})

        instance_a = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()
        instance_b = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        ).create()

        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        user = FakeUser("user-1")

        result1 = await instance_a.query(QueryInfo(resource_id="123"), user=user)
        assert result1.success is True
        assert result1.cache_hit is False

        await instance_b.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=user,
        )

        result2 = await instance_a.query(QueryInfo(resource_id="123"), user=user)
        assert result2.success is True
        assert result2.cache_hit is False
        assert result2.data == {
            "name": "Jane",
            "owner_id": "user-1",
        }


class TestResourceScopedGenerationIsolation:
    """Test that generation counters are scoped per resource."""

    @pytest.mark.asyncio
    async def test_resource_scoped_generation_does_not_invalidate_unrelated(
        self,
    ) -> None:
        """Test that mutating resource '456' does not invalidate cache entries
        for resource '123'."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()

        await repo.save("123", {"name": "Resource 123"})
        await repo.save("456", {"name": "Resource 456"})

        await daf.query(QueryInfo(resource_id="123"))
        assert (await daf.query(QueryInfo(resource_id="123"))).cache_hit is True

        await daf.put(
            PutInfo(resource_id="456", data={"name": "Resource 456 updated"})
        )

        result = await daf.query(QueryInfo(resource_id="123"))
        assert result.success is True
        assert result.cache_hit is True
        assert result.data == {"name": "Resource 123"}


class TestAlgorithmMutationDoesNotPoisonRawData:
    """Test that algorithm in-place mutation does not poison cached raw data."""

    @pytest.mark.asyncio
    async def test_algorithm_mutation_does_not_poison_raw_data(self) -> None:
        """Test that the 'raw' field stored in the cache is an independent deep
        copy and is not modified by an algorithm's in-place mutation."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()

        original_data = {"name": "John", "status": "active", "owner_id": "user-1"}

        class MutatingAlgorithm:
            async def execute(self, data: Any) -> Any:
                if isinstance(data, dict):
                    data["name"] = "Mutated"
                return data

            async def get_stats(self) -> dict[str, Any]:
                return {}

        daf = DataAccessFactory(
            repository=repo,
            cache=cache,
            algorithms={"mutating": MutatingAlgorithm()},
        ).create()

        await repo.save("123", original_data)

        result = await daf.query(
            QueryInfo(resource_id="123", algorithm="mutating")
        )
        assert result.success is True
        assert result.data == {
            "name": "Mutated",
            "status": "active",
            "owner_id": "user-1",
        }

        cache_key = _expected_cache_key("123", {}, "mutating", "anonymous")
        cached_entry = cache._cache.get(cache_key)
        assert cached_entry is not None
        assert cached_entry["raw"] == original_data
        assert cached_entry["raw"]["name"] == "John"


class TestCacheEntryStoresGeneration:
    """Test that cache entries carry a generation field for invalidation."""

    @pytest.mark.asyncio
    async def test_cache_entry_contains_generation_field(self) -> None:
        """Test that a cache entry stored on miss includes the current
        resource-scoped generation counter."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        daf = DataAccessFactory(repository=repo, cache=cache).create()

        await repo.save("123", {"name": "John"})

        result = await daf.query(QueryInfo(resource_id="123"))
        assert result.success is True
        assert result.cache_hit is False

        cache_key = _expected_cache_key("123", {}, None, "anonymous")
        entry = cache._cache.get(cache_key)
        assert entry is not None
        assert "generation" in entry
        assert entry["generation"] == 0

        await daf.put(PutInfo(resource_id="123", data={"name": "Jane"}))

        entry_after = cache._cache.get(cache_key)
        assert entry_after is None
