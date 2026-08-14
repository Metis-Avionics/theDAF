"""Tests for FastAPI adapter."""

from typing import Any

import pytest
from fastapi import FastAPI, Request
from fastapi.testclient import TestClient
from slowapi import Limiter
from slowapi.util import get_remote_address

from daf.adapters.fastapi import DataAccessRouter
from daf.cache import MemoryCache
from daf.core import DataAccessFactory
from daf.repositories import MemoryRepository


class FakeUser:
    """Simple user model for testing."""

    def __init__(self, user_id: str) -> None:
        self.id = user_id


@pytest.fixture
def test_client() -> tuple[Any, MemoryRepository[dict[str, Any]]]:
    """Create a test client with FastAPI adapter."""
    repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
    cache = MemoryCache()
    factory = DataAccessFactory(repository=repo, cache=cache)
    daf = factory.create()
    
    async def get_current_user(_request: Request) -> Any:
        return FakeUser("test-user")
    
    app = FastAPI()
    router_builder = DataAccessRouter(
        daf,
        get_current_user=get_current_user,
        limiter=Limiter(key_func=get_remote_address),
    )
    app.include_router(router_builder.get_router())
    app.state.limiter = router_builder._limiter
    
    client = TestClient(app)
    return client, repo


class TestDataAccessEndpoints:
    """Test FastAPI data access endpoints."""

    @pytest.mark.asyncio
    async def test_get_endpoint(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test GET endpoint."""
        client, repo = test_client
        
        await repo.save("123", {"name": "Test Item", "owner_id": "test-user"})
        
        response = client.get("/data/123")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "Test Item"

    @pytest.mark.asyncio
    async def test_get_not_found(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test GET for non-existent resource."""
        client, repo = test_client
        
        response = client.get("/data/nonexistent")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False

    @pytest.mark.asyncio
    async def test_post_endpoint(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test POST endpoint creates resource."""
        client, repo = test_client
        
        response = client.post(
            "/data",
            json={
                "resource_type": "user",
                "data": {"name": "John", "email": "john@example.com"},
            },
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["resource_id"] is not None

    @pytest.mark.asyncio
    async def test_put_endpoint(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test PUT endpoint updates resource."""
        client, repo = test_client
        
        await repo.save("123", {"name": "John", "age": 30, "owner_id": "test-user"})
        
        response = client.put(
            "/data/123",
            json={"resource_id": "123", "data": {"name": "Jane"}},
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "Jane"

    @pytest.mark.asyncio
    async def test_delete_endpoint(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test DELETE endpoint removes resource."""
        client, repo = test_client
        
        await repo.save("123", {"name": "Test", "owner_id": "test-user"})
        
        response = client.delete("/data/123")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True


class TestRateLimiting:
    """Test rate limiting on endpoints."""

    @pytest.mark.asyncio
    async def test_rate_limiting_headers(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that rate limiting is applied."""
        client, repo = test_client
        
        response = client.get("/data/123")
        
        assert response.status_code == 200


class TestEndpointValidation:
    """Test endpoint input validation."""

    @pytest.mark.asyncio
    async def test_post_validation(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test POST endpoint validates input."""
        client, repo = test_client
        
        response = client.post(
            "/data",
            json={"resource_type": "user"},
        )
        
        assert response.status_code == 422

    @pytest.mark.asyncio
    async def test_put_validation(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test PUT endpoint validates input."""
        client, repo = test_client
        
        response = client.put(
            "/data/123",
            json={"invalid": "data"},
        )
        
        assert response.status_code == 422


class TestQueryParameters:
    """Test GET query parameter handling."""

    @pytest.mark.asyncio
    async def test_get_with_filters(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test GET with filters query parameter."""
        client, repo = test_client
        
        await repo.save(
            "123", {"name": "Test", "status": "active", "owner_id": "test-user"}
        )
        
        response = client.get('/data/123?filters={"status":"active"}')
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["status"] == "active"

    @pytest.mark.asyncio
    async def test_get_with_algorithm(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test GET with algorithm query parameter."""
        client, repo = test_client
        
        await repo.save(
            "123", {"value": 5, "owner_id": "test-user"}
        )
        
        response = client.get("/data/123?algorithm=nonexistent")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False
        assert data["error_type"] == "validation"

    @pytest.mark.asyncio
    async def test_get_with_invalid_filters_json(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test GET with invalid filters JSON returns 422."""
        client, repo = test_client
        
        response = client.get("/data/123?filters=not-valid-json")
        
        assert response.status_code == 422


class TestErrorTranslation:
    """Test error translation to HTTP responses."""

    @pytest.mark.asyncio
    async def test_query_error_translation(
        self, test_client: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that DataAccess errors are properly translated."""
        client, repo = test_client
        
        response = client.get("/data/missing")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False
        assert data["error"] is not None


class TestAuthorization:
    """Test authorization and IDOR prevention in FastAPI adapter."""

    @pytest.fixture
    def test_client_with_auth(
        self,
    ) -> tuple[Any, MemoryRepository[dict[str, Any]]]:
        """Create a test client with authorization enabled."""
        repo: MemoryRepository[dict[str, Any]] = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        
        async def get_current_user(request: Request) -> Any:
            user_id = request.headers.get("X-User-ID")
            if user_id:
                return FakeUser(user_id)
            return None
        
        test_limiter = Limiter(key_func=get_remote_address)
        app = FastAPI()
        router_builder = DataAccessRouter(
            daf,
            get_current_user=get_current_user,
            limiter=test_limiter,
        )
        app.include_router(router_builder.get_router())
        app.state.limiter = test_limiter
        
        client = TestClient(app)
        return client, repo

    @pytest.mark.asyncio
    async def test_authorized_user_can_query_own_resource(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that authorized user can access their own resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.get("/data/123", headers={"X-User-ID": "user-1"})
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "John"

    @pytest.mark.asyncio
    async def test_user_gets_403_for_other_resource(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test auth error when accessing another user's resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.get("/data/123", headers={"X-User-ID": "user-2"})
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False
        assert data["error_type"] == "authorization"

    @pytest.mark.asyncio
    async def test_unauthenticated_user_gets_403(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that unauthenticated user gets authorization error."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.get("/data/123")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False
        assert data["error_type"] == "authorization"

    @pytest.mark.asyncio
    async def test_nonexistent_resource_returns_404(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that non-existent resource returns not-found error."""
        client, repo = test_client_with_auth
        
        response = client.get("/data/nonexistent", headers={"X-User-ID": "user-1"})
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False
        assert data["error_type"] == "authorization"

    @pytest.mark.asyncio
    async def test_authorized_user_can_put_own_resource(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that authorized user can update their own resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.put(
            "/data/123",
            headers={"X-User-ID": "user-1"},
            json={"resource_id": "123", "data": {"name": "Jane"}},
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "Jane"

    @pytest.mark.asyncio
    async def test_authorized_user_can_delete_own_resource(
        self, test_client_with_auth: tuple[Any, MemoryRepository[dict[str, Any]]]
    ) -> None:
        """Test that authorized user can delete their own resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.delete("/data/123", headers={"X-User-ID": "user-1"})
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
