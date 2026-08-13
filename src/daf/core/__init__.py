"""Core data access abstractions and factory."""

from daf.core.access import DataAccess
from daf.core.errors import (
    AlgorithmError,
    AuthorizationError,
    CacheError,
    DataAccessError,
    NotFoundError,
    RepositoryError,
    ValidationError,
)
from daf.core.factory import DataAccessFactory
from daf.core.protocols import Algorithm, Authorizer, Cache, Repository

__all__ = [
    "DataAccess",
    "DataAccessFactory",
    "Repository",
    "Cache",
    "Algorithm",
    "Authorizer",
    "DataAccessError",
    "NotFoundError",
    "ValidationError",
    "RepositoryError",
    "CacheError",
    "AlgorithmError",
    "AuthorizationError",
]
