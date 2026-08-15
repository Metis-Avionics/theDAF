"""Core data access abstractions and factory."""

from daf._barrel import _public
from daf.core.access import DataAccess  # noqa: F401
from daf.core.errors import (
    AlgorithmError,  # noqa: F401
    AuthorizationError,  # noqa: F401
    CacheError,  # noqa: F401
    DataAccessError,  # noqa: F401
    NotFoundError,  # noqa: F401
    RepositoryError,  # noqa: F401
    ValidationError,  # noqa: F401
)
from daf.core.factory import DataAccessFactory  # noqa: F401
from daf.core.protocols import Algorithm, Authorizer, Cache, Repository  # noqa: F401

__all__ = _public(
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
)
