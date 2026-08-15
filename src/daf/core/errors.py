"""Core error types for the Data Access Factory."""


class DataAccessError(Exception):
    """Base exception for data access operations."""

    pass


class NotFoundError(DataAccessError):
    """Raised when a requested resource is not found."""

    pass


class ValidationError(DataAccessError):
    """Raised when input validation fails."""

    pass


class RepositoryError(DataAccessError):
    """Raised when a repository operation fails."""

    pass


class CacheError(DataAccessError):
    """Raised when a cache operation fails."""

    pass


class GenerationKeyError(CacheError):
    """Raised when a required generation key is absent from the cache."""

    pass


class AlgorithmError(DataAccessError):
    """Raised when an algorithm execution fails."""

    pass


class AuthorizationError(DataAccessError):
    """Raised when a user is not authorized to access a resource."""

    pass
