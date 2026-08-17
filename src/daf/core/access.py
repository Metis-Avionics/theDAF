"""Data access orchestration layer.

Write-through-DAF consistency model: all mutations must flow through
DataAccess to trigger cache invalidation. Direct writes to the underlying
repository bypass invalidation entirely and may leave stale cache entries.
"""

import asyncio
import copy
import hashlib
import json
import logging
import warnings
from functools import cache
from typing import Any

from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)
from daf.core.errors import GenerationKeyError, NotFoundError, ValidationError
from daf.core.protocols import Algorithm, Authorizer, Cache, Repository
from daf.utils._memoize import ResourceMemo

logger = logging.getLogger(__name__)


@cache
def _cached_key(
    resource_id: str,
    filters: tuple[tuple[str, Any], ...],
    algorithm: str,
    user_id: str,
    resource_namespace: str,
) -> str:
    payload = {
        "resource_id": resource_id,
        "filters": dict(filters),
        "algorithm": algorithm,
        "user_id": user_id,
    }
    try:
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    except (TypeError, ValueError):
        raise ValidationError(
            "Filters contain non-JSON-serializable values"
        ) from None
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    return f"query:{resource_namespace}:{digest}"


class DataAccess:
    """Runtime data access orchestration layer.
    
    Composes repository, cache, and algorithm components to execute
    queries and mutations with proper caching and algorithmic processing.

    Security model: authorization is evaluated after data retrieval on
    cache miss. The authorizer receives the raw repository data and
    decides whether the caller may use it. The repository is treated as
    a trusted internal data source; authorization enforces usage policy,
    not access policy. This preserves the single-read invariant: the
    repository is read exactly once per cache miss.

    This security model does not provide resource-existence confidentiality.
    Callers may distinguish nonexistent resources (NotFoundError / HTTP 404)
    from existing resources for which they lack authorization
    (AuthorizationError / HTTP 403).

    Concurrency model:

    - `delete_prefix` is the authoritative invalidation mechanism and is
      atomic within the MemoryCache implementation (single dict sweep).
    - Generation counters are per-resource and stored in the shared cache.
    - Within a single process, generation advancement is serialized via
      per-resource asyncio locks, eliminating read-modify-write races for
      the common case of multiple DataAccess instances sharing a cache in
      the same event loop.
    - Across processes, generation advancement is best-effort
      (read-modify-write). Concurrent mutations may observe a temporarily
      stale generation value, but stale cache entries are always rejected
      by generation comparison on the next read.

      Cache-correctness invariant: a query result is served from cache
      only if its stored generation equals the current generation for
      that resource. The generation key is correctness metadata and
      must not be silently defaulted; missing generation state forces
      a cache miss and repository read.
    - For distributed cache backends, atomic generation advancement
      requires cache-level CAS or compare-and-set primitives (out of
      scope for the current Cache protocol).
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
        self._generation_locks_memo = ResourceMemo(
            key_fn=lambda resource_id: self._resource_namespace(resource_id),
            factory=lambda _: asyncio.Lock(),
            max_size=256,
        )

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

    async def _generation_lock(self, resource_id: str) -> asyncio.Lock:
        """Return the per-resource asyncio lock for generation advancement.

        Locks are lazily created and cached by ResourceMemo.
        Access is serialised via the ResourceMemo's internal lock so
        two coroutines requesting the same namespace receive the same
        lock object.
        """
        return await self._generation_locks_memo.get(resource_id)

    def _resource_namespace(self, resource_id: str) -> str:
        """Return a fixed-width namespace for a resource_id.

        Uses SHA-256 to produce a collision-resistant, structurally
        unambiguous prefix regardless of resource_id contents.
        """
        return hashlib.sha256(resource_id.encode()).hexdigest()

    async def _current_generation(self, resource_id: str) -> int:
        """Read the current generation counter for a resource from the cache."""
        lock = await self._generation_lock(resource_id)
        async with lock:
            namespace = self._resource_namespace(resource_id)
            value = await self._cache.get(f"_daf_gen:{namespace}")
            if not isinstance(value, int):
                raise GenerationKeyError(
                    f"Generation key '_daf_gen:{namespace}' is absent or not an int"
                )
            return value

    async def _advance_generation(self, resource_id: str) -> None:
        """Increment the generation counter for a resource in the cache.

        The read-modify-write is performed atomically under the per-resource
        lock, so concurrent mutations within the same process cannot lose
        an increment.
        """
        lock = await self._generation_lock(resource_id)
        async with lock:
            namespace = self._resource_namespace(resource_id)
            current = await self._cache.get(f"_daf_gen:{namespace}")
            if current is not None and not isinstance(current, int):
                raise GenerationKeyError(
                    f"Generation key '_daf_gen:{namespace}' is not an int"
                )
            await self._cache.set(f"_daf_gen:{namespace}", (current or 0) + 1)

    async def _superedge_invalidate(self, resource_id: str) -> None:
        """Atomically invalidate all cached projections for a resource.

        Performs the "superedge collapse": deletes all query cache entries
        and the generation counter for the resource, then advances the
        generation counter so subsequent reads rebuild from the repository.

        The generation counter is read under the per-resource lock before
        any deletion so that concurrent mutations do not lose increments
        when the generation key is absent.

        Optionally calls ``shake`` on the cache to prune any orphaned
        sub-branches that prefix deletion may have missed.
        """
        namespace = self._resource_namespace(resource_id)
        lock = await self._generation_lock(resource_id)
        async with lock:
            current = await self._cache.get(f"_daf_gen:{namespace}")
            if current is not None and not isinstance(current, int):
                raise GenerationKeyError(
                    f"Generation key '_daf_gen:{namespace}' is not an int"
                )
            await self._cache.delete_prefix(f"query:{namespace}:")
            await self._cache.shake(f"_daf_gen:{namespace}")
            await self._cache.set(f"_daf_gen:{namespace}", (current or 0) + 1)

    def _cache_key(self, info: QueryInfo, user: Any) -> str:
        """Build cache key from full query semantics."""
        user_id = self._user_id(user)
        return _cached_key(
            resource_id=info.resource_id,
            filters=tuple(sorted((info.filters or {}).items())),
            algorithm=info.algorithm or "",
            user_id=user_id,
            resource_namespace=self._resource_namespace(info.resource_id),
        )

    @staticmethod
    def _apply_filters(data: Any, filters: dict[str, Any] | None) -> Any:
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
        """Execute the core query logic.

        On cache miss, the raw repository value is retrieved first, then
        authorization is checked against that value. This preserves the
        single-read invariant. The cache stores both the raw data (for
        re-authorization on cache hit) and the transformed result (for
        returning to the caller).
        """
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
            try:
                current_gen = await self._current_generation(info.resource_id)
            except GenerationKeyError:
                return await self._execute_cache_miss(cache_key, info, user)
            if cached.get("generation") == current_gen:
                return await self._handle_cache_hit(
                    cache_key, info.resource_id, user, cached
                )
        return await self._execute_cache_miss(cache_key, info, user)

    async def _handle_cache_hit(
        self,
        cache_key: str,
        resource_id: str,
        user: Any,
        cached: dict[str, Any],
    ) -> QueryResult:
        """Authorize and return a cached query result."""
        raw_data = cached["raw"]
        if self._authorizer is not None:
            await self._check_authorization("query", resource_id, user, data=raw_data)
        logger.debug("cache hit", extra={"key": cache_key})
        return QueryResult(
            success=True,
            data=cached["transformed"],
            error=None,
            error_type=None,
            cache_hit=True,
            algorithm_stats=None,
        )

    async def _execute_cache_miss(
        self, cache_key: str, info: QueryInfo, user: Any
    ) -> QueryResult:
        """Execute a full query on cache miss and populate the cache."""
        try:
            current_generation = await self._current_generation(info.resource_id)
        except GenerationKeyError:
            current_generation = 0
            namespace = self._resource_namespace(info.resource_id)
            await self._cache.set(f"_daf_gen:{namespace}", 0)
        data = await self._repository.get(info.resource_id)
        if data is None:
            logger.info("repository miss", extra={"resource_id": info.resource_id})
            raise NotFoundError(f"Resource '{info.resource_id}' not found")
        raw_data = copy.deepcopy(data)
        if self._authorizer is not None:
            await self._check_authorization(
                "query", info.resource_id, user, data=data
            )
        data = self._apply_filters(data, info.filters)
        algorithm_stats = None
        if info.algorithm:
            data, algorithm_stats = await self._run_algorithm(data, info.algorithm)
        await self._cache.set(
            cache_key,
            {"raw": raw_data, "transformed": data, "generation": current_generation},
        )
        logger.debug("cache set", extra={"key": cache_key})
        return QueryResult(
            success=True,
            data=data,
            error=None,
            error_type=None,
            cache_hit=False,
            algorithm_stats=algorithm_stats,
        )

    async def _run_algorithm(
        self, data: Any, algorithm_name: str
    ) -> tuple[Any, dict[str, Any] | None]:
        """Execute an algorithm by name and return (result, stats)."""
        algorithm = self._algorithms.get(algorithm_name)
        if algorithm is None:
            raise ValidationError(f"Unknown algorithm: {algorithm_name}")
        logger.debug("running algorithm", extra={"algorithm": algorithm_name})
        result = await algorithm.execute(data)
        stats = await algorithm.get_stats()
        return result, stats

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

        await self._advance_generation(resource_id)

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

        await self._superedge_invalidate(info.resource_id)
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

        await self._superedge_invalidate(info.resource_id)
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
