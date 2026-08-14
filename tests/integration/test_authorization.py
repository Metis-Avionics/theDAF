"""Integration tests for authorization and IDOR prevention."""

from typing import Any

import pytest

from daf.cache import MemoryCache
from daf.contracts import DeleteInfo, PostInfo, PutInfo, QueryInfo
from daf.core import DataAccessFactory
from daf.core.errors import AuthorizationError
from daf.repositories import MemoryRepository

SetupResult = tuple[MemoryRepository[dict[str, Any]], MemoryCache, Any]


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


class TestDataAccessAuthorization:
    """Test DataAccess authorization integration."""

    @pytest.fixture
    def setup_daf_with_auth(
        self,
    ) -> SetupResult:
        """Set up a DataAccess instance with authorization."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(
            owned_resources={"123": "user-1", "456": "user-2"}
        )
        factory = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        )
        daf: Any = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_authorized_user_can_query_own_resource(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that an authorized user can query their own resource."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-1")
        )
        assert result.success is True
        assert result.data["name"] == "John"

    @pytest.mark.asyncio
    async def test_user_gets_403_for_other_resource(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test authorization error for another user's resource."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-2")
        )
        assert result.success is False
        assert result.error_type == "authorization"

    @pytest.mark.asyncio
    async def test_unauthenticated_user_gets_403(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that an unauthenticated user gets authorization error."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        result = await daf.query(QueryInfo(resource_id="123"), user=None)
        assert result.success is False
        assert result.error_type == "authorization"

    @pytest.mark.asyncio
    async def test_authorization_on_query_put_delete(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that authorization is checked on query, put, and delete."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        query_result = await daf.query(
            QueryInfo(resource_id="123"), user=FakeUser("user-2")
        )
        assert query_result.success is False
        assert query_result.error_type == "authorization"
        
        put_result = await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=FakeUser("user-2"),
        )
        assert put_result.success is False
        assert put_result.error_type == "authorization"
        
        delete_result = await daf.delete(
            DeleteInfo(resource_id="123"), user=FakeUser("user-2")
        )
        assert delete_result.success is False
        assert delete_result.error_type == "authorization"

    @pytest.mark.asyncio
    async def test_post_skips_authorization(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that post does not check ownership (no resource_id yet)."""
        repo, cache, daf = setup_daf_with_auth
        
        result = await daf.post(
            PostInfo(resource_type="user", data={"name": "John"}),
            user=FakeUser("user-1"),
        )
        assert result.success is True

    @pytest.mark.asyncio
    async def test_not_found_error_precedence(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that NotFoundError takes precedence over AuthorizationError."""
        repo, cache, daf = setup_daf_with_auth
        
        result = await daf.query(
            QueryInfo(resource_id="nonexistent"),
            user=FakeUser("user-1"),
        )
        assert result.success is False
        assert result.error_type == "not_found"

    @pytest.mark.asyncio
    async def test_nonexistent_resource_not_found_for_authenticated_user(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test non-existent resource returns not_found for any authed user."""
        repo, cache, daf = setup_daf_with_auth
        
        result = await daf.query(
            QueryInfo(resource_id="nonexistent"),
            user=FakeUser("user-1"),
        )
        assert result.success is False
        assert result.error_type == "not_found"

    @pytest.mark.asyncio
    async def test_authorized_user_can_put_and_delete(
        self, setup_daf_with_auth: SetupResult
    ) -> None:
        """Test that authorized user can update and delete their resource."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        put_result = await daf.put(
            PutInfo(resource_id="123", data={"name": "Jane"}),
            user=FakeUser("user-1"),
        )
        assert put_result.success is True
        assert put_result.data["name"] == "Jane"
        
        delete_result = await daf.delete(
            DeleteInfo(resource_id="123"),
            user=FakeUser("user-1"),
        )
        assert delete_result.success is True


class TestDataAccessAuthorizationBackwardCompatibility:
    """Test that DataAccess works without authorization (backward compatibility)."""

    @pytest.fixture
    def setup_daf(
        self,
    ) -> SetupResult:
        """Set up a DataAccess instance without authorization."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf: Any = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_query_without_authorizer(
        self, setup_daf: SetupResult
    ) -> None:
        """Test query works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "Test"})
        
        result = await daf.query(QueryInfo(resource_id="123"))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_post_without_authorizer(
        self, setup_daf: SetupResult
    ) -> None:
        """Test post works without authorizer."""
        repo, cache, daf = setup_daf
        
        result = await daf.post(PostInfo(resource_type="user", data={"name": "John"}))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_put_without_authorizer(
        self, setup_daf: SetupResult
    ) -> None:
        """Test put works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "John"})
        
        result = await daf.put(PutInfo(resource_id="123", data={"name": "Jane"}))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_delete_without_authorizer(
        self, setup_daf: SetupResult
    ) -> None:
        """Test delete works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "Test"})
        
        result = await daf.delete(DeleteInfo(resource_id="123"))
        assert result.success is True
