"""FastAPI integration adapter.

This module bridges the gap between HTTP requests and the core
DataAccess layer. It translates HTTP requests into DataAccess
operations and applies endpoint-level rate limiting.

Note: FastAPI is optional. Core DataAccess does not depend on this.
"""

from collections.abc import Callable
from typing import Any, TypeVar

from fastapi import APIRouter, HTTPException, Request
from slowapi import Limiter
from slowapi.util import get_remote_address

from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)
from daf.core.access import DataAccess
from daf.core.errors import AuthorizationError, NotFoundError
from daf.core.protocols import Authorizer

F = TypeVar("F", bound=Callable[..., Any])

# Configure rate limiter
limiter = Limiter(key_func=get_remote_address)


class DataAccessRouter:
    """FastAPI router builder for DataAccess operations.
    
    Provides endpoint construction with rate limiting and proper
    error translation to HTTP responses.
    """

    def __init__(
        self,
        daf: DataAccess,
        get_current_user: Callable[[Request], Any] | None = None,
        limiter: Limiter | None = None,
    ) -> None:
        """Initialize router with DataAccess instance.
        
        Args:
            daf: The DataAccess instance to use for operations.
            get_current_user: Optional callable that extracts the current
                user from a FastAPI Request. When provided, authorization
                is enforced on all mutating and query operations.
            limiter: Optional rate limiter instance. Defaults to the module-level
                limiter if not provided.
        """
        self._daf = daf
        self._get_current_user = get_current_user
        self._limiter = limiter
        if get_current_user is not None:
            self._daf._authorizer = self._make_authorizer()
        self._router = APIRouter(prefix="/data", tags=["data"])
        self._setup_routes()

    def _limit(self, rate: str) -> Callable[[F], F]:
        """Apply rate limit if a limiter is configured."""
        if self._limiter is not None:
            return self._limiter.limit(rate)
        def noop(func: F) -> F:
            return func
        return noop

    def _make_authorizer(self) -> Authorizer:
        """Create a closure-based authorizer that checks resource ownership.
        
        Returns:
            An Authorizer that validates the current user owns the requested resource.
        """
        repository = self._daf._repository

        class _Authorizer:
            async def authorize(
                self, _operation: str, resource_id: str | None, user: Any
            ) -> None:
                if user is None:
                    raise AuthorizationError("Unauthenticated")
                if resource_id is None:
                    return
                data = await repository.get(resource_id)
                if data is None:
                    raise NotFoundError(
                        f"Resource '{resource_id}' not found"
                    )
                owner_id = (
                    data.get("owner_id") if isinstance(data, dict) else None
                )
                if owner_id != user.id:
                    raise AuthorizationError(
                        f"Access denied to resource '{resource_id}'"
                    )

        return _Authorizer()

    def _setup_routes(self) -> None:
        """Set up all data access routes."""
        self._setup_query_route()
        self._setup_post_route()
        self._setup_put_route()
        self._setup_delete_route()

    def _setup_query_route(self) -> None:
        @self._router.get("/{resource_id}", response_model=QueryResult)
        @self._limit("30/minute")
        async def query_endpoint(
            request: Request, resource_id: str
        ) -> QueryResult:
            """Query a resource."""
            info = QueryInfo(
                resource_id=resource_id,
                filters=None,
                algorithm=None,
            )
            current_user = (
                self._get_current_user(request)
                if self._get_current_user
                else None
            )
            try:
                return await self._daf.query(info, user=current_user)
            except AuthorizationError as e:
                raise HTTPException(
                    status_code=403, detail=str(e)
                ) from None
            except NotFoundError as e:
                raise HTTPException(
                    status_code=404, detail=str(e)
                ) from None

    def _setup_post_route(self) -> None:
        @self._router.post("", response_model=MutationResult)
        @self._limit("10/minute")
        async def post_endpoint(
            request: Request, info: PostInfo
        ) -> MutationResult:
            """Create a new resource."""
            current_user = (
                self._get_current_user(request)
                if self._get_current_user
                else None
            )
            try:
                return await self._daf.post(info, user=current_user)
            except AuthorizationError as e:
                raise HTTPException(
                    status_code=403, detail=str(e)
                ) from None

    def _setup_put_route(self) -> None:
        @self._router.put("/{resource_id}", response_model=MutationResult)
        @self._limit("10/minute")
        async def put_endpoint(
            request: Request, resource_id: str, info: PutInfo
        ) -> MutationResult:
            """Update a resource."""
            info.resource_id = resource_id
            current_user = (
                self._get_current_user(request)
                if self._get_current_user
                else None
            )
            try:
                return await self._daf.put(info, user=current_user)
            except AuthorizationError as e:
                raise HTTPException(
                    status_code=403, detail=str(e)
                ) from None
            except NotFoundError as e:
                raise HTTPException(
                    status_code=404, detail=str(e)
                ) from None

    def _setup_delete_route(self) -> None:
        @self._router.delete("/{resource_id}", response_model=MutationResult)
        @self._limit("10/minute")
        async def delete_endpoint(
            request: Request, resource_id: str
        ) -> MutationResult:
            """Delete a resource."""
            info = DeleteInfo(resource_id=resource_id)
            current_user = (
                self._get_current_user(request)
                if self._get_current_user
                else None
            )
            try:
                return await self._daf.delete(info, user=current_user)
            except AuthorizationError as e:
                raise HTTPException(
                    status_code=403, detail=str(e)
                ) from None
            except NotFoundError as e:
                raise HTTPException(
                    status_code=404, detail=str(e)
                ) from None

    def get_router(self) -> APIRouter:
        """Get the configured router for inclusion in FastAPI app.
        
        Returns:
            The APIRouter instance with all data access routes.
        """
        return self._router
