"""Request and response contract models."""

from datetime import UTC, datetime
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class QueryInfo(BaseModel):
    """Query operation input contract."""

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "resource_id": "123",
                "filters": {"type": "active"},
                "algorithm": "fibonacci",
            }
        }
    )

    resource_id: str = Field(..., description="The ID of the resource to query")
    filters: dict[str, Any] | None = Field(
        None, description="Optional filters to apply"
    )
    algorithm: str | None = Field(
        None, description="Optional algorithm to apply during query"
    )


class PostInfo(BaseModel):
    """Create/insert operation input contract."""

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "resource_type": "user",
                "data": {"name": "John", "email": "john@example.com"},
            }
        }
    )

    resource_type: str = Field(..., description="Type of resource to create")
    data: dict[str, Any] = Field(..., description="Data for the new resource")


class PutInfo(BaseModel):
    """Update operation input contract."""

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "resource_id": "123",
                "data": {"name": "Jane", "email": "jane@example.com"},
            }
        }
    )

    resource_id: str = Field(..., description="The ID of the resource to update")
    data: dict[str, Any] = Field(..., description="Updated data")


class DeleteInfo(BaseModel):
    """Delete operation input contract."""

    model_config = ConfigDict(
        json_schema_extra={"example": {"resource_id": "123"}}
    )

    resource_id: str = Field(..., description="The ID of the resource to delete")


class QueryResult(BaseModel):
    """Query operation result contract."""

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "success": True,
                "data": {"id": "123", "name": "Sample"},
                "error": None,
                "error_type": None,
                "cache_hit": False,
                "algorithm_stats": None,
                "timestamp": "2025-01-01T00:00:00+00:00",
            }
        }
    )

    success: bool = Field(..., description="Whether the operation succeeded")
    data: Any = Field(None, description="The query result data")
    error: str | None = Field(
        None, description="User-safe error message if operation failed"
    )
    error_type: str | None = Field(
        None, description="Machine-readable error classification"
    )
    cache_hit: bool = Field(False, description="Whether the result came from cache")
    algorithm_stats: dict[str, Any] | None = Field(
        None, description="Algorithm execution statistics"
    )
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))


class MutationResult(BaseModel):
    """Mutation operation result contract."""

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "success": True,
                "resource_id": "123",
                "data": {"id": "123", "name": "Sample"},
                "error": None,
                "error_type": None,
                "timestamp": "2025-01-01T00:00:00+00:00",
            }
        }
    )

    success: bool = Field(..., description="Whether the operation succeeded")
    resource_id: str | None = Field(None, description="The resource ID involved")
    data: Any = Field(None, description="The result data")
    error: str | None = Field(
        None, description="User-safe error message if operation failed"
    )
    error_type: str | None = Field(
        None, description="Machine-readable error classification"
    )
    timestamp: datetime = Field(default_factory=lambda: datetime.now(UTC))
