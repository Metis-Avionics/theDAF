"""FastAPI integration adapter.

This module bridges the gap between HTTP requests and the core
DataAccess layer. It translates HTTP requests into DataAccess
operations and applies endpoint-level rate limiting.

Note: FastAPI is optional. Core DataAccess does not depend on this.
"""

import json
import logging
from collections.abc import Awaitable, Callable
from typing import Any, TypeVar

from fastapi import APIRouter, HTTPException, Request
from pydantic import ValidationError
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
from daf.core.errors import AuthorizationError, DataAccessError
from daf.core.protocols import Authorizer, Repository

logger = logging.getLogger(__name__)

F = TypeVar("F", bound=Callable[..., Any])

# Configure rate limiter
limiter = Limiter(key_func=get_remote_address)


class DataAccessRouter:
    """FastAPI router builder for DataAccess operations.
    
    Provides endpoint construction with rate limiting and proper
    error translation to HTTP responses.
    
    Security invariant: `get_current_user` is required so externally
    exposed routes cannot accidentally operate without an authorization
    policy.
    """

    def __init__(
        self,
        daf: DataAccess,
        get_current_user: Callable[[Request], Awaitable[Any]],
        limiter: Limiter | None = None,
    ) -> None:
        """Initialize router with DataAccess instance.
        
        Args:
            daf: The DataAccess instance to use for operations.
            get_current_user: Required async callable that extracts the current
                user from a FastAPI Request. Authorization is enforced on all
                mutating and query operations.
            limiter: Optional rate limiter instance. Defaults to the module-level
                limiter if not provided.
        
        Raises:
            ValueError: If `get_current_user` is not provided.
        """
        if get_current_user is None:
            raise ValueError("get_current_user is required for DataAccessRouter")
        
        self._get_current_user = get_current_user
        self._limiter = limiter
        
        repository, cache, algorithms = daf.get_components()
        authorizer = self._make_authorizer(repository)
        
        self._daf = DataAccess(
            repository=repository,
            cache=cache,
            algorithms=algorithms,
            authorizer=authorizer,
        )
        self._router = APIRouter(prefix="/data", tags=["data"])
        self._setup_routes()

    def _limit(self, rate: str) -> Callable[[F], F]:
        """Apply rate limit if a limiter is configured."""
        if self._limiter is not None:
            return self._limiter.limit(rate)
        def noop(func: F) -> F:
            return func
        return noop

    def _make_authorizer(self, repository: Repository[Any]) -> Authorizer:
        """Create a closure-based authorizer that checks resource ownership.
        
        Args:
            repository: The repository to use for resource lookups.
            
        Returns:
            An Authorizer that validates the current user owns the requested resource.
        """
        class _Authorizer:
            async def authorize(
                self,
                _operation: str,
                resource_id: str | None,
                user: Any,
                data: Any = None,
            ) -> None:
                if user is None:
                    raise AuthorizationError("Unauthenticated")
                if resource_id is None:
                    return
                if data is None:
                    data = await repository.get(resource_id)
                if data is not None and isinstance(data, dict):
                    owner_id = data.get("owner_id")
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
            filters = None
            algorithm = None
            if "filters" in request.query_params:
                try:
                    parsed = json.loads(request.query_params["filters"])
                    if isinstance(parsed, dict):
                        filters = parsed
                except (json.JSONDecodeError, ValueError):
                    raise HTTPException(
                        status_code=422, detail="Invalid filters JSON"
                    ) from None
            if "algorithm" in request.query_params:
                algorithm = request.query_params["algorithm"]

            try:
                info = QueryInfo(
                    resource_id=resource_id,
                    filters=filters,
                    algorithm=algorithm,
                )
            except ValidationError:
                raise HTTPException(
                    status_code=422, detail="Invalid query parameters"
                ) from None

            current_user = await self._get_current_user(request)
            try:
                logger.debug(
                    "query endpoint",
                    extra={"resource_id": resource_id},
                )
                return await self._daf.query(info, user=current_user)
            except DataAccessError:
                logger.error(
                    "query endpoint error",
                    extra={"resource_id": resource_id},
                )
                raise HTTPException(
                    status_code=500, detail="Internal server error"
                ) from None

    def _setup_post_route(self) -> None:
        @self._router.post("", response_model=MutationResult)
        @self._limit("10/minute")
        async def post_endpoint(
            request: Request, info: PostInfo
        ) -> MutationResult:
            """Create a new resource."""
            current_user = await self._get_current_user(request)
            try:
                logger.debug("post endpoint")
                return await self._daf.post(info, user=current_user)
            except DataAccessError:
                logger.error("post endpoint error")
                raise HTTPException(
                    status_code=500, detail="Internal server error"
                ) from None

    def _setup_put_route(self) -> None:
        @self._router.put("/{resource_id}", response_model=MutationResult)
        @self._limit("10/minute")
        async def put_endpoint(
            request: Request, resource_id: str, info: PutInfo
        ) -> MutationResult:
            """Update a resource."""
            info = PutInfo(resource_id=resource_id, data=info.data)
            current_user = await self._get_current_user(request)
            try:
                logger.debug(
                    "put endpoint",
                    extra={"resource_id": resource_id},
                )
                return await self._daf.put(info, user=current_user)
            except DataAccessError:
                logger.error(
                    "put endpoint error",
                    extra={"resource_id": resource_id},
                )
                raise HTTPException(
                    status_code=500, detail="Internal server error"
                ) from None

    def _setup_delete_route(self) -> None:
        @self._router.delete("/{resource_id}", response_model=MutationResult)
        @self._limit("10/minute")
        async def delete_endpoint(
            request: Request, resource_id: str
        ) -> MutationResult:
            """Delete a resource."""
            info = DeleteInfo(resource_id=resource_id)
            current_user = await self._get_current_user(request)
            try:
                logger.debug(
                    "delete endpoint",
                    extra={"resource_id": resource_id},
                )
                return await self._daf.delete(info, user=current_user)
            except DataAccessError:
                logger.error(
                    "delete endpoint error",
                    extra={"resource_id": resource_id},
                )
                raise HTTPException(
                    status_code=500, detail="Internal server error"
                ) from None

    def get_router(self) -> APIRouter:
        """Get the configured router for inclusion in FastAPI app.
        
        Returns:
            The APIRouter instance with all data access routes.
        """
        return self._router
