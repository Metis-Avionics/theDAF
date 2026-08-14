"""Data access orchestration layer."""

import json
import logging
from typing import Any

from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)
from daf.core.errors import AuthorizationError, NotFoundError, ValidationError
from daf.core.protocols import Algorithm, Authorizer, Cache, Repository

logger = logging.getLogger(__name__)


class DataAccess:
    """Runtime data access orchestration layer.
    
    Composes repository, cache, and algorithm components to execute
    queries and mutations with proper caching and algorithmic processing.
    """

    def __init__(
        self,
        repository: Repository[Any],
        cache: Cache,
        algorithms: dict[str, Algorithm] | None = None,
        authorizer: Authorizer | None = None,
    ) -> None:
        """Initialize DataAccess with dependencies.
        
        Args:
            repository: The underlying data repository.
            cache: The cache layer for result caching.
            algorithms: Optional registry mapping algorithm names to implementations.
            authorizer: Optional authorizer for access control.
        """
        self._repository = repository
        self._cache = cache
        self._algorithms = algorithms or {}
        self._authorizer = authorizer

    async def _check_authorization(
        self,
        operation: str,
        resource_id: str | None,
        user: Any,
        data: Any = None,
    ) -> None:
        """Check authorization for an operation if an authorizer is configured."""
        if self._authorizer is not None:
            await self._authorizer.authorize(operation, resource_id, user, data=data)

    def _user_id(self, user: Any) -> str:
        """Canonicalize user identity for cache keying."""
        if user is None:
            return "anonymous"
        return getattr(user, "id", str(user))

    def get_components(
        self,
    ) -> tuple[Repository[Any], Cache, dict[str, Algorithm] | None]:
        """Get the components used by this DataAccess instance."""
        return self._repository, self._cache, self._algorithms

    def _cache_key(self, info: QueryInfo, user: Any) -> str:
        """Build cache key from full query semantics."""
        user_id = self._user_id(user)
        try:
            filters_json = json.dumps(info.filters or {}, sort_keys=True)
        except (TypeError, ValueError):
            raise ValidationError(
                "Filters contain non-JSON-serializable values"
            ) from None
        algorithm = info.algorithm or ""
        return f"query:{info.resource_id}:{filters_json}:{algorithm}:{user_id}"

    def _apply_filters(self, data: Any, filters: dict[str, Any] | None) -> Any:
        """Apply in-memory filters to retrieved data."""
        if not filters:
            return data
        if not isinstance(data, dict):
            return {}
        for key, value in filters.items():
            if key not in data or data[key] != value:
                return {}
        return data

    async def query(self, info: QueryInfo, user: Any = None) -> QueryResult:
        """Execute a query operation.
        
        Args:
            info: Query information (resource_id, filters, algorithm).
            user: Optional authenticated user context for authorization.
            
        Returns:
            QueryResult with data, cache_hit status, and optional algorithm stats.
        """
        try:
            if not info.resource_id or not isinstance(info.resource_id, str):
                raise ValidationError("resource_id must be a non-empty string")
            logger.debug(
                "query start",
                extra={
                    "resource_id": info.resource_id,
                    "filters": info.filters,
                    "algorithm": info.algorithm,
                    "user": self._user_id(user),
                },
            )
            if self._authorizer is not None:
                await self._check_authorization("query", info.resource_id, user)
            return await self._execute_query(info, user)
        except AuthorizationError:
            logger.warning(
                "query unauthorized",
                extra={
                    "resource_id": info.resource_id,
                    "user": self._user_id(user),
                },
            )
            return QueryResult(
                success=False,
                data=None,
                error="Unauthorized",
                error_type="authorization",
                cache_hit=False,
                algorithm_stats=None,
            )
        except NotFoundError:
            logger.info(
                "query not found",
                extra={
                    "resource_id": info.resource_id,
                    "user": self._user_id(user),
                },
            )
            return QueryResult(
                success=False,
                data=None,
                error="Not found",
                error_type="not_found",
                cache_hit=False,
                algorithm_stats=None,
            )
        except ValidationError:
            logger.warning(
                "query validation error",
                extra={
                    "resource_id": info.resource_id,
                    "user": self._user_id(user),
                },
            )
            return QueryResult(
                success=False,
                data=None,
                error="Validation error",
                error_type="validation",
                cache_hit=False,
                algorithm_stats=None,
            )

    async def _execute_query(self, info: QueryInfo, user: Any) -> QueryResult:
        """Execute the core query logic."""
        logger.debug(
            "execute_query",
            extra={
                "resource_id": info.resource_id,
                "filters": info.filters,
                "algorithm": info.algorithm,
            },
        )
        cache_key = self._cache_key(info, user)

        cached = await self._cache.get(cache_key)
        if cached is not None:
            logger.debug("cache hit", extra={"key": cache_key})
            return QueryResult(
                success=True,
                data=cached,
                error=None,
                error_type=None,
                cache_hit=True,
                algorithm_stats=None,
            )

        data = await self._repository.get(info.resource_id)
        if data is None:
            logger.info("repository miss", extra={"resource_id": info.resource_id})
            raise NotFoundError(
                f"Resource '{info.resource_id}' not found"
            )

        data = self._apply_filters(data, info.filters)

        algorithm_stats = None
        if info.algorithm:
            algorithm = self._algorithms.get(info.algorithm)
            if algorithm is not None:
                logger.debug("running algorithm", extra={"algorithm": info.algorithm})
                algorithm_result = await algorithm.execute(data)
                algorithm_stats = await algorithm.get_stats()
                data = algorithm_result
            else:
                raise ValidationError(f"Unknown algorithm: {info.algorithm}")

        await self._cache.set(cache_key, data)
        logger.debug("cache set", extra={"key": cache_key})

        return QueryResult(
            success=True,
            data=data,
            error=None,
            error_type=None,
            cache_hit=False,
            algorithm_stats=algorithm_stats,
        )

    async def post(self, info: PostInfo, user: Any = None) -> MutationResult:
        """Execute a create/post operation.
        
        Args:
            info: Post information (resource_type, data).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult with created resource details.
        """
        try:
            if not info.resource_type or not isinstance(info.resource_type, str):
                raise ValidationError("resource_type must be a non-empty string")
            if not isinstance(info.data, dict):
                raise ValidationError("Post data must be a dict")
            if self._authorizer is not None:
                await self._check_authorization("post", None, user)
        except ValidationError:
            logger.warning(
                "post validation error",
                extra={"resource_type": info.resource_type},
            )
            return MutationResult(
                success=False,
                resource_id=None,
                data=None,
                error="Validation error",
                error_type="validation",
            )
        except AuthorizationError:
            logger.warning(
                "post unauthorized",
                extra={"user": self._user_id(user)},
            )
            return MutationResult(
                success=False,
                resource_id=None,
                data=None,
                error="Unauthorized",
                error_type="authorization",
            )

        logger.debug(
            "post create",
            extra={"resource_type": info.resource_type},
        )
        resource_id = await self._repository.create(info.data)

        await self._cache.delete_prefix(f"query:{resource_id}:")
        logger.debug(
            "post cache invalidated",
            extra={"resource_id": resource_id},
        )

        return MutationResult(
            success=True,
            resource_id=resource_id,
            data={"id": resource_id, "resource_type": info.resource_type, **info.data},
            error=None,
            error_type=None,
        )

    async def put(self, info: PutInfo, user: Any = None) -> MutationResult:
        """Execute an update/put operation.
        
        Args:
            info: Put information (resource_id, data).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult with updated resource details.
        """
        try:
            if not info.resource_id or not isinstance(info.resource_id, str):
                raise ValidationError("resource_id must be a non-empty string")
            if not isinstance(info.data, dict):
                raise ValidationError("Update data must be a dict")
        except ValidationError:
            logger.warning(
                "put validation error",
                extra={"resource_id": info.resource_id},
            )
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Validation error",
                error_type="validation",
            )

        try:
            logger.debug(
                "put update",
                extra={"resource_id": info.resource_id},
            )
            existing = await self._repository.get(info.resource_id)
            if existing is None:
                raise NotFoundError(
                    f"Resource '{info.resource_id}' not found for update"
                )

            if self._authorizer is not None:
                try:
                    await self._check_authorization(
                        "put", info.resource_id, user, data=existing
                    )
                except AuthorizationError:
                    logger.warning(
                        "put unauthorized",
                        extra={
                            "resource_id": info.resource_id,
                            "user": self._user_id(user),
                        },
                    )
                    return MutationResult(
                        success=False,
                        resource_id=info.resource_id,
                        data=None,
                        error="Unauthorized",
                        error_type="authorization",
                    )

            updated = {**existing, **info.data}
            await self._repository.save(info.resource_id, updated)

            await self._cache.delete_prefix(f"query:{info.resource_id}:")
            logger.debug(
                "put cache invalidated",
                extra={"resource_id": info.resource_id},
            )

            return MutationResult(
                success=True,
                resource_id=info.resource_id,
                data=updated,
                error=None,
                error_type=None,
            )

        except NotFoundError:
            logger.info(
                "put not found",
                extra={"resource_id": info.resource_id},
            )
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Not found",
                error_type="not_found",
            )

    async def delete(self, info: DeleteInfo, user: Any = None) -> MutationResult:
        """Execute a delete operation.
        
        Args:
            info: Delete information (resource_id).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult indicating success or failure.
        """
        try:
            if not info.resource_id or not isinstance(info.resource_id, str):
                raise ValidationError("resource_id must be a non-empty string")
        except ValidationError:
            logger.warning(
                "delete validation error",
                extra={"resource_id": info.resource_id},
            )
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Validation error",
                error_type="validation",
            )

        try:
            logger.debug(
                "delete",
                extra={"resource_id": info.resource_id},
            )
            existing = await self._repository.get(info.resource_id)
            if existing is None:
                raise NotFoundError(
                    f"Resource '{info.resource_id}' not found for deletion"
                )

            if self._authorizer is not None:
                try:
                    await self._check_authorization(
                        "delete", info.resource_id, user, data=existing
                    )
                except AuthorizationError:
                    logger.warning(
                        "delete unauthorized",
                        extra={
                            "resource_id": info.resource_id,
                            "user": self._user_id(user),
                        },
                    )
                    return MutationResult(
                        success=False,
                        resource_id=info.resource_id,
                        data=None,
                        error="Unauthorized",
                        error_type="authorization",
                    )

            await self._repository.delete(info.resource_id)

            await self._cache.delete_prefix(f"query:{info.resource_id}:")
            logger.debug(
                "delete cache invalidated",
                extra={"resource_id": info.resource_id},
            )

            return MutationResult(
                success=True,
                resource_id=info.resource_id,
                data=None,
                error=None,
                error_type=None,
            )

        except NotFoundError:
            logger.info(
                "delete not found",
                extra={"resource_id": info.resource_id},
            )
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Not found",
                error_type="not_found",
            )
