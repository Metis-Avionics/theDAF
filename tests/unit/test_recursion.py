"""Direct unit tests for daf.utils._recursion primitives."""

import pytest

from daf.utils._recursion import TreeCollector, collect_tree, walk_tree


def _make_tree() -> dict:
    return {
        "key": "root",
        "children": [
            {"key": "a", "children": []},
            {
                "key": "b",
                "children": [
                    {"key": "b1", "children": []},
                    {"key": "b2", "children": []},
                ],
            },
        ],
    }


def _key_fn(node: dict) -> str | None:
    return node["key"]


def _children_fn(node: dict) -> list[dict]:
    return node["children"]


class TestTreeCollector:
    """Tests for the TreeCollector traversal strategies."""

    def test_dfs_collects_all_keys(self) -> None:
        tree = _make_tree()
        collector = TreeCollector(
            key_extractor=_key_fn,
            children_extractor=_children_fn,
        )
        result = collector.collect(tree)
        assert result == {"root", "a", "b", "b1", "b2"}

    def test_bfs_same_set_as_dfs(self) -> None:
        tree = _make_tree()
        dfs_collector = TreeCollector(
            key_extractor=_key_fn, children_extractor=_children_fn, strategy="dfs"
        )
        bfs_collector = TreeCollector(
            key_extractor=_key_fn, children_extractor=_children_fn, strategy="bfs"
        )
        assert dfs_collector.collect(tree) == bfs_collector.collect(tree)

    def test_none_root_returns_empty(self) -> None:
        for strategy in ("dfs", "bfs"):
            collector = TreeCollector(
                key_extractor=_key_fn,
                children_extractor=_children_fn,
                strategy=strategy,
            )
            assert collector.collect(None) == set()

    def test_invalid_strategy_raises(self) -> None:
        collector = TreeCollector(
            key_extractor=_key_fn, children_extractor=_children_fn, strategy="invalid"
        )
        with pytest.raises(ValueError, match="Unknown strategy"):
            collector.collect(_make_tree())

    def test_collect_tree_convenience(self) -> None:
        tree = _make_tree()
        result = collect_tree(tree, key_fn=_key_fn, children_fn=_children_fn)
        assert result == {"root", "a", "b", "b1", "b2"}


class TestWalkTree:
    """Tests for the walk_tree recursive walker."""

    def test_callback_called_pre_order(self) -> None:
        tree = _make_tree()
        visited: list[str] = []

        def callback(node: dict) -> None:
            visited.append(node["key"])

        walk_tree(tree, children_fn=_children_fn, callback=callback)
        assert visited == ["root", "a", "b", "b1", "b2"]

    def test_walk_tree_empty_children(self) -> None:
        node = {"key": "leaf", "children": []}
        visited: list[str] = []

        def callback(node: dict) -> None:
            visited.append(node["key"])

        walk_tree(node, children_fn=_children_fn, callback=callback)
        assert visited == ["leaf"]

    def test_walk_tree_nested(self) -> None:
        tree = _make_tree()
        visited: list[str] = []

        def callback(node: dict) -> None:
            visited.append(node["key"])

        walk_tree(tree, children_fn=_children_fn, callback=callback)
        assert set(visited) == {"root", "a", "b", "b1", "b2"}
