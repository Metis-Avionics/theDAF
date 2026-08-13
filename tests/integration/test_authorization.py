"""Integration tests for authorization and IDOR prevention."""

from typing import Any

import pytest

from daf.cache import MemoryCache
from daf.contracts import DeleteInfo, PostInfo, PutInfo, QueryInfo
from daf.core import DataAccessFactory
from daf.core.errors import AuthorizationError, NotFoundError
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
            raise NotFoundError(f"Resource '{resource_id}' not found")
        if self._owned_resources[resource_id] != user.id:
            raise AuthorizationError(f"Access denied to resource '{resource_id}'")


class TestDataAccessAuthorization:
    """Test DataAccess authorization integration."""

    @pytest.fixture
    def setup_daf_with_auth(self):
        """Set up a DataAccess instance with authorization."""
        repo = MemoryRepository()
        cache = MemoryCache()
        authorizer = FakeAuthorizer(
            owned_resources={"123": "user-1", "456": "user-2"}
        )
        factory = DataAccessFactory(
            repository=repo, cache=cache, authorizer=authorizer
        )
        daf = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_authorized_user_can_query_own_resource(
        self, setup_daf_with_auth
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
        self, setup_daf_with_auth
    ) -> None:
        """Test that a user gets 403 when accessing another user's resource."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123"), user=FakeUser("user-2")
            )

    @pytest.mark.asyncio
    async def test_unauthenticated_user_gets_403(self, setup_daf_with_auth) -> None:
        """Test that an unauthenticated user gets 403."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        with pytest.raises(AuthorizationError):
            await daf.query(QueryInfo(resource_id="123"), user=None)

    @pytest.mark.asyncio
    async def test_authorization_on_query_put_delete(
        self, setup_daf_with_auth
    ) -> None:
        """Test that authorization is checked on query, put, and delete."""
        repo, cache, daf = setup_daf_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        # query should be authorized
        with pytest.raises(AuthorizationError):
            await daf.query(
                QueryInfo(resource_id="123"), user=FakeUser("user-2")
            )
        
        # put should be authorized
        with pytest.raises(AuthorizationError):
            await daf.put(
                PutInfo(resource_id="123", data={"name": "Jane"}),
                user=FakeUser("user-2"),
            )
        
        # delete should be authorized
        with pytest.raises(AuthorizationError):
            await daf.delete(
                DeleteInfo(resource_id="123"), user=FakeUser("user-2")
            )

    @pytest.mark.asyncio
    async def test_post_skips_authorization(self, setup_daf_with_auth) -> None:
        """Test that post does not check ownership (no resource_id yet)."""
        repo, cache, daf = setup_daf_with_auth
        
        result = await daf.post(
            PostInfo(resource_type="user", data={"name": "John"}),
            user=FakeUser("user-1"),
        )
        assert result.success is True

    @pytest.mark.asyncio
    async def test_not_found_error_precedence(
        self, setup_daf_with_auth
    ) -> None:
        """Test that NotFoundError takes precedence over AuthorizationError."""
        repo, cache, daf = setup_daf_with_auth
        
        with pytest.raises(NotFoundError):
            await daf.query(
                QueryInfo(resource_id="nonexistent"),
                user=FakeUser("user-1"),
            )

    @pytest.mark.asyncio
    async def test_authorized_user_can_put_and_delete(
        self, setup_daf_with_auth
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
    def setup_daf(self):
        """Set up a DataAccess instance without authorization."""
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        return repo, cache, daf

    @pytest.mark.asyncio
    async def test_query_without_authorizer(self, setup_daf) -> None:
        """Test query works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "Test"})
        
        result = await daf.query(QueryInfo(resource_id="123"))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_post_without_authorizer(self, setup_daf) -> None:
        """Test post works without authorizer."""
        repo, cache, daf = setup_daf
        
        result = await daf.post(PostInfo(resource_type="user", data={"name": "John"}))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_put_without_authorizer(self, setup_daf) -> None:
        """Test put works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "John"})
        
        result = await daf.put(PutInfo(resource_id="123", data={"name": "Jane"}))
        assert result.success is True

    @pytest.mark.asyncio
    async def test_delete_without_authorizer(self, setup_daf) -> None:
        """Test delete works without authorizer."""
        repo, cache, daf = setup_daf
        await repo.save("123", {"name": "Test"})
        
        result = await daf.delete(DeleteInfo(resource_id="123"))
        assert result.success is True
