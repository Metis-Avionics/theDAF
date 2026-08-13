"""Data access orchestration layer."""

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


class DataAccess:
    """Runtime data access orchestration layer.
    
    Composes repository, cache, and algorithm components to execute
    queries and mutations with proper caching and algorithmic processing.
    """

    def __init__(
        self,
        repository: Repository[Any],
        cache: Cache,
        algorithm: Algorithm | None = None,
        authorizer: Authorizer | None = None,
    ) -> None:
        """Initialize DataAccess with dependencies.
        
        Args:
            repository: The underlying data repository.
            cache: The cache layer for result caching.
            algorithm: Optional algorithm for computational operations.
            authorizer: Optional authorizer for access control.
        """
        self._repository = repository
        self._cache = cache
        self._algorithm = algorithm
        self._authorizer = authorizer

    async def _check_authorization(
        self, operation: str, resource_id: str | None, user: Any
    ) -> None:
        """Check authorization for an operation if an authorizer is configured.
        
        Args:
            operation: The operation being performed.
            resource_id: The resource being accessed, or None.
            user: The authenticated user context.
            
        Raises:
            AuthorizationError: If access is denied.
            NotFoundError: If the resource does not exist.
        """
        if self._authorizer is not None:
            await self._authorizer.authorize(operation, resource_id, user)

    async def query(self, info: QueryInfo, user: Any = None) -> QueryResult:
        """Execute a query operation.
        
        Follows the orchestration:
        1. Check authorization if authorizer is configured
        2. Check cache for the resource
        3. If cache miss, fetch from repository
        4. Apply algorithm if specified
        5. Populate cache with result
        6. Return typed result
        
        Args:
            info: Query information (resource_id, filters, algorithm).
            user: Optional authenticated user context for authorization.
            
        Returns:
            QueryResult with data, cache_hit status, and optional algorithm stats.
        """
        if self._authorizer is not None:
            await self._check_authorization("query", info.resource_id, user)
        try:
            return await self._execute_query(info)
        except NotFoundError as e:
            return self._not_found_result(str(e))
        except Exception as e:
            return self._error_result("Query", e)

    async def _execute_query(self, info: QueryInfo) -> QueryResult:
        """Execute the core query logic."""
        cache_key = f"query:{info.resource_id}"

        # Step 1: Cache lookup
        cached = await self._cache.get(cache_key)
        if cached is not None:
            return QueryResult(
                success=True,
                data=cached,
                error=None,
                cache_hit=True,
                algorithm_stats=None,
            )

        # Step 2: Repository lookup on cache miss
        data = await self._repository.get(info.resource_id)
        if data is None:
            raise NotFoundError(
                f"Resource '{info.resource_id}' not found"
            )

        # Step 3: Apply algorithm if specified
        algorithm_stats = None
        if info.algorithm and self._algorithm:
            algorithm_result = await self._algorithm.execute(data)
            algorithm_stats = await self._algorithm.get_stats()
            data = algorithm_result

        # Step 4: Cache population
        await self._cache.set(cache_key, data)

        # Step 5: Return typed result
        return QueryResult(
            success=True,
            data=data,
            error=None,
            cache_hit=False,
            algorithm_stats=algorithm_stats,
        )

    def _not_found_result(self, error: str) -> QueryResult:
        """Create a not-found query result."""
        return QueryResult(
            success=False,
            data=None,
            error=error,
            cache_hit=False,
            algorithm_stats=None,
        )

    def _error_result(self, operation: str, error: Exception) -> QueryResult:
        """Create an error query result."""
        return QueryResult(
            success=False,
            data=None,
            error=f"{operation} failed: {str(error)}",
            cache_hit=False,
            algorithm_stats=None,
        )

    async def post(self, info: PostInfo, user: Any = None) -> MutationResult:
        """Execute a create/post operation.
        
        Args:
            info: Post information (resource_type, data).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult with created resource details.
        """
        if not info.resource_type:
            raise ValidationError("Resource type must not be empty")
        if not info.data:
            raise ValidationError("Post data must not be empty")
        if self._authorizer is not None:
            await self._check_authorization("post", None, user)
        try:
            # Generate a simple ID for the new resource
            existing = await self._repository.list_all()
            resource_id = str(len(existing) + 1)

            # Save to repository
            await self._repository.save(resource_id, info.data)

            # Invalidate relevant caches
            await self._cache.clear()

            return MutationResult(
                success=True,
                resource_id=resource_id,
                data={"id": resource_id, **info.data},
                error=None,
            )

        except Exception as e:
            return MutationResult(
                success=False,
                resource_id=None,
                data=None,
                error=f"Post failed: {str(e)}",
            )

    async def put(self, info: PutInfo, user: Any = None) -> MutationResult:
        """Execute an update/put operation.
        
        Args:
            info: Put information (resource_id, data).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult with updated resource details.
        """
        if self._authorizer is not None:
            await self._check_authorization("put", info.resource_id, user)
        try:
            # Verify resource exists
            existing = await self._repository.get(info.resource_id)
            if existing is None:
                raise NotFoundError(
                    f"Resource '{info.resource_id}' not found for update"
                )

            # Update in repository
            updated = {**existing, **info.data}
            await self._repository.save(info.resource_id, updated)

            # Invalidate cache for this resource
            await self._cache.delete(f"query:{info.resource_id}")

            return MutationResult(
                success=True,
                resource_id=info.resource_id,
                data=updated,
                error=None,
            )

        except NotFoundError as e:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error=str(e),
            )
        except Exception as e:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error=f"Put failed: {str(e)}",
            )

    async def delete(self, info: DeleteInfo, user: Any = None) -> MutationResult:
        """Execute a delete operation.
        
        Args:
            info: Delete information (resource_id).
            user: Optional authenticated user context for authorization.
            
        Returns:
            MutationResult indicating success or failure.
        """
        if self._authorizer is not None:
            await self._check_authorization("delete", info.resource_id, user)
        try:
            # Verify resource exists
            existing = await self._repository.get(info.resource_id)
            if existing is None:
                raise NotFoundError(
                    f"Resource '{info.resource_id}' not found for deletion"
                )

            # Delete from repository
            await self._repository.delete(info.resource_id)

            # Invalidate cache for this resource
            await self._cache.delete(f"query:{info.resource_id}")

            return MutationResult(
                success=True,
                resource_id=info.resource_id,
                data=None,
                error=None,
            )

        except NotFoundError as e:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error=str(e),
            )
        except Exception as e:
            return MutationResult(
                success=False,
                resource_id=info.resource_id,
                data=None,
                error=f"Delete failed: {str(e)}",
            )
