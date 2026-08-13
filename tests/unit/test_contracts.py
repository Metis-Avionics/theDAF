"""Unit tests for Pydantic contracts."""

import pytest

from daf.contracts.query import (
    DeleteInfo,
    MutationResult,
    PostInfo,
    PutInfo,
    QueryInfo,
    QueryResult,
)


class TestQueryInfo:
    """Test QueryInfo model."""

    def test_basic_query_info(self) -> None:
        """Test basic QueryInfo creation."""
        info = QueryInfo(resource_id="123")
        assert info.resource_id == "123"
        assert info.filters is None
        assert info.algorithm is None

    def test_query_info_with_filters(self) -> None:
        """Test QueryInfo with filters."""
        info = QueryInfo(
            resource_id="123",
            filters={"type": "active"},
        )
        assert info.resource_id == "123"
        assert info.filters == {"type": "active"}

    def test_query_info_with_algorithm(self) -> None:
        """Test QueryInfo with algorithm."""
        info = QueryInfo(
            resource_id="123",
            algorithm="fibonacci",
        )
        assert info.algorithm == "fibonacci"

    def test_query_info_validation(self) -> None:
        """Test QueryInfo validation fails without resource_id."""
        with pytest.raises(ValueError):
            QueryInfo()  # type: ignore


class TestMutationInfos:
    """Test mutation info models."""

    def test_post_info(self) -> None:
        """Test PostInfo creation."""
        info = PostInfo(
            resource_type="user",
            data={"name": "John", "email": "john@example.com"},
        )
        assert info.resource_type == "user"
        assert info.data["name"] == "John"

    def test_put_info(self) -> None:
        """Test PutInfo creation."""
        info = PutInfo(
            resource_id="123",
            data={"name": "Jane"},
        )
        assert info.resource_id == "123"
        assert info.data["name"] == "Jane"

    def test_delete_info(self) -> None:
        """Test DeleteInfo creation."""
        info = DeleteInfo(resource_id="123")
        assert info.resource_id == "123"


class TestQueryResult:
    """Test QueryResult model."""

    def test_successful_query_result(self) -> None:
        """Test successful QueryResult."""
        result = QueryResult(
            success=True,
            data={"id": "123", "name": "Sample"},
            cache_hit=False,
        )
        assert result.success is True
        assert result.data["id"] == "123"
        assert result.error is None

    def test_failed_query_result(self) -> None:
        """Test failed QueryResult."""
        result = QueryResult(
            success=False,
            error="Not found",
        )
        assert result.success is False
        assert result.error == "Not found"
        assert result.data is None

    def test_cached_query_result(self) -> None:
        """Test QueryResult with cache hit."""
        result = QueryResult(
            success=True,
            data={"id": "123"},
            cache_hit=True,
        )
        assert result.cache_hit is True

    def test_query_result_with_algorithm_stats(self) -> None:
        """Test QueryResult with algorithm statistics."""
        stats = {"iterations": 10, "cache_hits": 5}
        result = QueryResult(
            success=True,
            data=42,
            algorithm_stats=stats,
        )
        assert result.algorithm_stats == stats


class TestMutationResult:
    """Test MutationResult model."""

    def test_successful_mutation_result(self) -> None:
        """Test successful MutationResult."""
        result = MutationResult(
            success=True,
            resource_id="123",
            data={"id": "123", "name": "Sample"},
        )
        assert result.success is True
        assert result.resource_id == "123"

    def test_failed_mutation_result(self) -> None:
        """Test failed MutationResult."""
        result = MutationResult(
            success=False,
            resource_id="123",
            error="Update failed",
        )
        assert result.success is False
        assert result.error == "Update failed"
