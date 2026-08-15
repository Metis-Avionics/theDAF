"""FastAPI Data Access Factory (DAF) - A typed data-access abstraction layer."""

# daf is a curated public subset of daf.core. When adding a new
# public name, update both this file and daf/core/__init__.py.

from daf._barrel import _public
from daf.core.access import DataAccess  # noqa: F401
from daf.core.errors import (
    AuthorizationError,  # noqa: F401
    DataAccessError,  # noqa: F401
    NotFoundError,  # noqa: F401
    RepositoryError,  # noqa: F401
    ValidationError,  # noqa: F401
)
from daf.core.factory import DataAccessFactory  # noqa: F401

__version__ = "0.1.0"
__all__ = _public(
    "DataAccess",
    "DataAccessFactory",
    "DataAccessError",
    "NotFoundError",
    "ValidationError",
    "RepositoryError",
    "AuthorizationError",
)
