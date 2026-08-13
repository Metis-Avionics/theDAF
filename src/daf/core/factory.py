"""Factory for constructing DataAccess instances."""

from typing import Any

from daf.core.access import DataAccess
from daf.core.protocols import Algorithm, Authorizer, Cache, Repository


class DataAccessFactory:
    """Factory for constructing configured DataAccess instances.
    
    Accepts dependencies through construction and provides a create()
    operation to instantiate DataAccess with those dependencies.
    
    This separates composition concerns from runtime operation concerns.
    """

    def __init__(
        self,
        repository: Repository[Any],
        cache: Cache,
        algorithm: Algorithm | None = None,
        authorizer: Authorizer | None = None,
    ) -> None:
        """Initialize the factory with dependencies.
        
        Args:
            repository: The repository implementation to use.
            cache: The cache implementation to use.
            algorithm: Optional algorithm implementation.
            authorizer: Optional authorizer for access control.
        """
        self._repository = repository
        self._cache = cache
        self._algorithm = algorithm
        self._authorizer = authorizer

    def create(self) -> DataAccess:
        """Create and return a configured DataAccess instance.
        
        Returns:
            A new DataAccess instance with the factory's configured
            dependencies.
        """
        return DataAccess(
            repository=self._repository,
            cache=self._cache,
            algorithm=self._algorithm,
            authorizer=self._authorizer,
        )
