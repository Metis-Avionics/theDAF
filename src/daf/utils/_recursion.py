"""Recursion and tree-walking primitives.

Provides:
- TreeCollector: generic tree collector parameterised by traversal strategy.
- collect_tree: convenience function for DFS collection.
- walk_tree: generic recursive tree walker with a per-node callback.
"""

from __future__ import annotations

import builtins
import logging
from collections import deque
from collections.abc import Callable, Iterable
from typing import Any, TypeVar

logger = logging.getLogger(__name__)

T = TypeVar("T")


class TreeCollector:
    """Collect terminal keys from a tree using a configurable traversal strategy.

    Parameters
    ----------
    key_extractor:
        Called with each node; returns the node's key (str) or ``None``
        if the node is not terminal.
    children_extractor:
        Called with each node; returns an iterable of child nodes.
    strategy:
        One of ``"dfs"`` or ``"bfs"``.

    The collector does **not** modify the tree.
    """

    def __init__(
        self,
        key_extractor: Callable[[Any], str | None],
        children_extractor: Callable[[Any], Iterable[Any]],
        strategy: str = "dfs",
    ) -> None:
        self._key_extractor = key_extractor
        self._children_extractor = children_extractor
        self._strategy = strategy

    def collect(self, root: Any) -> builtins.set[str]:
        if root is None:
            return builtins.set()
        if self._strategy == "dfs":
            return self._dfs(root)
        if self._strategy == "bfs":
            return self._bfs(root)
        raise ValueError(f"Unknown strategy: {self._strategy!r}")

    def _dfs(self, node: Any) -> builtins.set[str]:
        result = builtins.set()
        key = self._key_extractor(node)
        if key is not None:
            result.add(key)
        for child in self._children_extractor(node):
            result.update(self._dfs(child))
        return result

    def _bfs(self, root: Any) -> builtins.set[str]:
        result: builtins.set[str] = builtins.set()
        queue: deque[Any] = deque([root])
        while queue:
            current = queue.popleft()
            key = self._key_extractor(current)
            if key is not None:
                result.add(key)
            queue.extend(self._children_extractor(current))
        return result


def collect_tree(
    root: Any,
    key_fn: Callable[[Any], str | None],
    children_fn: Callable[[Any], Iterable[Any]],
    strategy: str = "dfs",
) -> builtins.set[str]:
    """Convenience function: build a TreeCollector and collect keys from *root*."""
    collector = TreeCollector(
        key_extractor=key_fn,
        children_extractor=children_fn,
        strategy=strategy,
    )
    return collector.collect(root)


def walk_tree(
    node: Any,
    children_fn: Callable[[Any], Iterable[Any]],
    callback: Callable[[Any], None],
) -> None:
    """Recursively walk *node* and all descendants, calling *callback* on each.

    This is a synchronous, side-effect-only walker: *callback* is called
    for every node in pre-order and its return value is discarded.
    Suitable for AST walks in scripts such as ``power_of_ten.py``.
    """
    callback(node)
    for child in children_fn(node):
        walk_tree(child, children_fn, callback)
