"""Tests for FastAPI adapter."""

import pytest
from fastapi import FastAPI
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
def test_client():
    """Create a test client with FastAPI adapter."""
    # Setup
    repo = MemoryRepository()
    cache = MemoryCache()
    factory = DataAccessFactory(repository=repo, cache=cache)
    daf = factory.create()
    
    # Create FastAPI app
    app = FastAPI()
    router_builder = DataAccessRouter(daf, limiter=Limiter(key_func=get_remote_address))
    app.include_router(router_builder.get_router())
    app.state.limiter = router_builder._limiter
    
    # Return app and repo for test setup
    client = TestClient(app)
    return client, repo


class TestDataAccessEndpoints:
    """Test FastAPI data access endpoints."""

    @pytest.mark.asyncio
    async def test_get_endpoint(self, test_client) -> None:
        """Test GET endpoint."""
        client, repo = test_client
        
        # Populate repository
        await repo.save("123", {"name": "Test Item"})
        
        # Make request
        response = client.get("/data/123")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "Test Item"

    @pytest.mark.asyncio
    async def test_get_not_found(self, test_client) -> None:
        """Test GET for non-existent resource."""
        client, repo = test_client
        
        response = client.get("/data/nonexistent")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is False

    @pytest.mark.asyncio
    async def test_post_endpoint(self, test_client) -> None:
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
    async def test_put_endpoint(self, test_client) -> None:
        """Test PUT endpoint updates resource."""
        client, repo = test_client
        
        # Create initial resource
        await repo.save("123", {"name": "John", "age": 30})
        
        # Update it
        response = client.put(
            "/data/123",
            json={"resource_id": "123", "data": {"name": "Jane"}},
        )
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
        assert data["data"]["name"] == "Jane"

    @pytest.mark.asyncio
    async def test_delete_endpoint(self, test_client) -> None:
        """Test DELETE endpoint removes resource."""
        client, repo = test_client
        
        # Create resource
        await repo.save("123", {"name": "Test"})
        
        # Delete it
        response = client.delete("/data/123")
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True


class TestRateLimiting:
    """Test rate limiting on endpoints."""

    @pytest.mark.asyncio
    async def test_rate_limiting_headers(self, test_client) -> None:
        """Test that rate limiting is applied."""
        # Note: Full rate limit testing requires multiple requests
        # For now, just verify the endpoint works
        client, repo = test_client
        
        response = client.get("/data/123")
        
        # Response should succeed (resource doesn't exist, but no rate limit hit)
        assert response.status_code == 200


class TestEndpointValidation:
    """Test endpoint input validation."""

    @pytest.mark.asyncio
    async def test_post_validation(self, test_client) -> None:
        """Test POST endpoint validates input."""
        client, repo = test_client
        
        # Missing required field
        response = client.post(
            "/data",
            json={"resource_type": "user"},  # Missing 'data'
        )
        
        assert response.status_code == 422  # Validation error

    @pytest.mark.asyncio
    async def test_put_validation(self, test_client) -> None:
        """Test PUT endpoint validates input."""
        client, repo = test_client
        
        # Invalid request body
        response = client.put(
            "/data/123",
            json={"invalid": "data"},
        )
        
        assert response.status_code == 422  # Validation error


class TestErrorTranslation:
    """Test error translation to HTTP responses."""

    @pytest.mark.asyncio
    async def test_query_error_translation(self, test_client) -> None:
        """Test that DataAccess errors are properly translated."""
        client, repo = test_client
        
        # Query non-existent resource
        response = client.get("/data/missing")
        
        assert response.status_code == 200  # HTTP still succeeds
        data = response.json()
        assert data["success"] is False
        assert data["error"] is not None


class TestAuthorization:
    """Test authorization and IDOR prevention in FastAPI adapter."""

    @pytest.fixture
    def test_client_with_auth(self):
        """Create a test client with authorization enabled."""
        repo = MemoryRepository()
        cache = MemoryCache()
        factory = DataAccessFactory(repository=repo, cache=cache)
        daf = factory.create()
        
        def get_current_user(request):
            user_id = request.headers.get("X-User-ID")
            if user_id:
                return FakeUser(user_id)
            return None
        
        test_limiter = Limiter(key_func=get_remote_address)
        app = FastAPI()
        router_builder = DataAccessRouter(
            daf, get_current_user=get_current_user, limiter=test_limiter
        )
        app.include_router(router_builder.get_router())
        app.state.limiter = test_limiter
        
        client = TestClient(app)
        return client, repo

    @pytest.mark.asyncio
    async def test_authorized_user_can_query_own_resource(
        self, test_client_with_auth
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
        self, test_client_with_auth
    ) -> None:
        """Test that user gets 403 when accessing another user's resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.get("/data/123", headers={"X-User-ID": "user-2"})
        
        assert response.status_code == 403

    @pytest.mark.asyncio
    async def test_unauthenticated_user_gets_403(self, test_client_with_auth) -> None:
        """Test that unauthenticated user gets 403."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.get("/data/123")
        
        assert response.status_code == 403

    @pytest.mark.asyncio
    async def test_nonexistent_resource_returns_404(
        self, test_client_with_auth
    ) -> None:
        """Test that non-existent resource returns 404, not 403."""
        client, repo = test_client_with_auth
        
        response = client.get("/data/nonexistent", headers={"X-User-ID": "user-1"})
        
        assert response.status_code == 404

    @pytest.mark.asyncio
    async def test_authorized_user_can_put_own_resource(
        self, test_client_with_auth
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
        self, test_client_with_auth
    ) -> None:
        """Test that authorized user can delete their own resource."""
        client, repo = test_client_with_auth
        await repo.save("123", {"name": "John", "owner_id": "user-1"})
        
        response = client.delete("/data/123", headers={"X-User-ID": "user-1"})
        
        assert response.status_code == 200
        data = response.json()
        assert data["success"] is True
