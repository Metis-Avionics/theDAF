"""FastAPI Data Access Factory (DAF) - A typed data-access abstraction layer."""

from daf.core.access import DataAccess
from daf.core.errors import (
    AuthorizationError,
    DataAccessError,
    NotFoundError,
    RepositoryError,
    ValidationError,
)
from daf.core.factory import DataAccessFactory

__version__ = "0.1.0"
__all__ = [
    "DataAccess",
    "DataAccessFactory",
    "DataAccessError",
    "NotFoundError",
    "ValidationError",
    "RepositoryError",
    "AuthorizationError",
]
