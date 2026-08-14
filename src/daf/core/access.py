"""Data access orchestration layer."""

import hashlib
import json
import logging
import warnings
from typing import Any

from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)
from daf.core.errors import NotFoundError, ValidationError
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
        user_id = getattr(user, "id", None)
        if user_id is None:
            warnings.warn(
                "user object has no .id attribute; falling back to str(user). "
                "This is deprecated and will be removed in a future version. "
                "Pass a user object with a stable .id attribute.",
                DeprecationWarning,
                stacklevel=2,
            )
            return str(user)
        return str(user_id)

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
        return f"query:{info.resource_id}:{digest}"

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
            
        Raises:
            AuthorizationError: If the user is not authorized.
            NotFoundError: If the resource is not found.
            ValidationError: If input validation fails.
        """
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
        return await self._execute_query(info, user)

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

        if self._authorizer is not None:
            await self._check_authorization("query", info.resource_id, user, data=data)

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
            
        Raises:
            AuthorizationError: If the user is not authorized.
            ValidationError: If input validation fails.
        """
        if not info.resource_type or not isinstance(info.resource_type, str):
            raise ValidationError("resource_type must be a non-empty string")
        if not isinstance(info.data, dict):
            raise ValidationError("Post data must be a dict")
        if self._authorizer is not None:
            await self._check_authorization("post", None, user, data=info.data)

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

        await self._cache.delete_prefix(f"query:{info.resource_id}:")
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
            
        Raises:
            AuthorizationError: If the user is not authorized.
            NotFoundError: If the resource is not found.
            ValidationError: If input validation fails.
        """
        if not info.resource_id or not isinstance(info.resource_id, str):
            raise ValidationError("resource_id must be a non-empty string")
        if not isinstance(info.data, dict):
            raise ValidationError("Update data must be a dict")
        return await self._execute_put(info, user)

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

    async def delete(self, info: DeleteInfo, user: Any = None) -> MutationResult:
        """Execute a delete operation.
        
        Args:
            info: Delete information (resource_id).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult indicating success or failure.
            
        Raises:
            AuthorizationError: If the user is not authorized.
            NotFoundError: If the resource is not found.
            ValidationError: If input validation fails.
        """
        if not info.resource_id or not isinstance(info.resource_id, str):
            raise ValidationError("resource_id must be a non-empty string")
        return await self._execute_delete(info, user)
