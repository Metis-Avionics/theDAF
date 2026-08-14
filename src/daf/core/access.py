"""Data access orchestration layer."""

import hashlib
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
        self._cache_key_map: dict[str, set[str]] = {}

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
        payload = {
            "resource_id": info.resource_id,
            "filters": info.filters or {},
            "algorithm": info.algorithm or "",
            "user_id": user_id,
        }
        try:
            canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
        except (TypeError, ValueError):
            raise ValidationError(
                "Filters contain non-JSON-serializable values"
            ) from None
        digest = hashlib.sha256(canonical.encode()).hexdigest()
        return f"query:{digest}"

    async def _invalidate_cache_for_resource(self, resource_id: str) -> None:
        """Invalidate all cached query results for a resource."""
        keys = self._cache_key_map.pop(resource_id, set())
        for key in keys:
            await self._cache.delete(key)
        logger.debug(
            "cache invalidated",
            extra={"resource_id": resource_id, "keys": len(keys)},
        )

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

    def _query_auth_error(self, info: QueryInfo, user: Any) -> QueryResult:
        """Build unauthorized query result."""
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

    def _query_not_found_error(self, info: QueryInfo, user: Any) -> QueryResult:
        """Build not-found query result."""
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

    def _query_validation_error(self, info: QueryInfo, user: Any) -> QueryResult:
        """Build validation error query result."""
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
            return self._query_auth_error(info, user)
        except NotFoundError:
            return self._query_not_found_error(info, user)
        except ValidationError:
            return self._query_validation_error(info, user)

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
        self._cache_key_map.setdefault(info.resource_id, set()).add(cache_key)

        cached = await self._cache.get(cache_key)
        if cached is not None:
            if self._authorizer is not None:
                await self._check_authorization(
                    "query", info.resource_id, user, data=cached
                )
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

    def _post_validation_error(self, info: PostInfo) -> MutationResult:
        """Build validation error post result."""
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

    def _post_auth_error(self, user: Any) -> MutationResult:
        """Build unauthorized post result."""
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
                await self._check_authorization("post", None, user, data=info.data)
        except ValidationError:
            return self._post_validation_error(info)
        except AuthorizationError:
            return self._post_auth_error(user)

        logger.debug(
            "post create",
            extra={"resource_type": info.resource_type},
        )
        resource_id = await self._repository.create(info.data)

        await self._invalidate_cache_for_resource(resource_id)
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

    def _mutation_validation_error(
        self, resource_id: str | None, operation: str
    ) -> MutationResult:
        """Build validation error mutation result."""
        logger.warning(
            f"{operation} validation error",
            extra={"resource_id": resource_id},
        )
        return MutationResult(
            success=False,
            resource_id=resource_id,
            data=None,
            error="Validation error",
            error_type="validation",
        )

    def _mutation_not_found_error(
        self, resource_id: str | None, operation: str
    ) -> MutationResult:
        """Build not-found mutation result."""
        logger.info(
            f"{operation} not found",
            extra={"resource_id": resource_id},
        )
        return MutationResult(
            success=False,
            resource_id=resource_id,
            data=None,
            error="Not found",
            error_type="not_found",
        )

    def _mutation_auth_error(
        self, resource_id: str | None, user: Any, operation: str
    ) -> MutationResult:
        """Build unauthorized mutation result."""
        logger.warning(
            f"{operation} unauthorized",
            extra={
                "resource_id": resource_id,
                "user": self._user_id(user),
            },
        )
        return MutationResult(
            success=False,
            resource_id=resource_id,
            data=None,
            error="Unauthorized",
            error_type="authorization",
        )

    async def _execute_put(self, info: PutInfo, user: Any) -> MutationResult:
        """Execute the core put mutation logic."""
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
            await self._check_authorization(
                "put", info.resource_id, user, data=existing
            )

        result = await self._repository.try_update(
            info.resource_id,
            expected=existing,
            update=lambda e: {**e, **info.data},
        )
        if result is None:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Conflict",
                error_type="conflict",
            )

        await self._invalidate_cache_for_resource(info.resource_id)
        logger.debug(
            "put cache invalidated",
            extra={"resource_id": info.resource_id},
        )

        return MutationResult(
            success=True,
            resource_id=info.resource_id,
            data=result,
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
            return self._mutation_validation_error(info.resource_id, "put")

        try:
            return await self._execute_put(info, user)
        except NotFoundError:
            return self._mutation_not_found_error(info.resource_id, "put")
        except AuthorizationError:
            return self._mutation_auth_error(info.resource_id, user, "put")

    async def _execute_delete(self, info: DeleteInfo, user: Any) -> MutationResult:
        """Execute the core delete mutation logic."""
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
            await self._check_authorization(
                "delete", info.resource_id, user, data=existing
            )

        deleted = await self._repository.try_delete(
            info.resource_id, expected=existing
        )
        if not deleted:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error="Conflict",
                error_type="conflict",
            )

        await self._invalidate_cache_for_resource(info.resource_id)
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
            return self._mutation_validation_error(info.resource_id, "delete")

        try:
            return await self._execute_delete(info, user)
        except NotFoundError:
            return self._mutation_not_found_error(info.resource_id, "delete")
        except AuthorizationError:
            return self._mutation_auth_error(info.resource_id, user, "delete")
