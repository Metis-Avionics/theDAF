"""Trie data structure for prefix-keyed lookup.

Standalone implementation with no daf imports.  Used by MemoryCache
for O(prefix_length + subtree_nodes) prefix operations.
"""

from __future__ import annotations

import builtins


class _TrieNode:
    __slots__ = ("children", "key")
    children: dict[str, _TrieNode]
    key: str | None

    def clear(self) -> None:
        self.children.clear()
        self.key = None

    def __init__(self) -> None:
        self.children = {}
        self.key = None


def _trie_insert(root: _TrieNode, key: str) -> None:
    node = root
    for ch in key:
        node.children.setdefault(ch, _TrieNode())
        node = node.children[ch]
    node.key = key


def _trie_delete(root: _TrieNode, key: str) -> None:
    path: builtins.list[tuple[_TrieNode, str]] = []
    node: _TrieNode | None = root
    for ch in key:
        if node is None:
            return
        path.append((node, ch))
        node = node.children.get(ch)
        if node is None:
            return
    if node is None or node.key != key:
        return
    node.key = None
    for i in range(len(path) - 1, -1, -1):
        parent, ch = path[i]
        child = parent.children.get(ch)
        if child is not None and child.key is None and not child.children:
            del parent.children[ch]
        else:
            break


def _trie_collect(root: _TrieNode, prefix: str) -> builtins.set[str]:
    node: _TrieNode | None = root
    for ch in prefix:
        node = node.children.get(ch) if node is not None else None
        if node is None:
            return builtins.set()
    return _dfs_collect(node)


def _dfs_collect(node: _TrieNode | None) -> builtins.set[str]:
    if node is None:
        return builtins.set()
    result = builtins.set()
    if node.key is not None:
        result.add(node.key)
    for child in node.children.values():
        result.update(_dfs_collect(child))
    return result


def _trie_delete_prefix(root: _TrieNode, prefix: str) -> builtins.set[str]:
    if prefix == "":
        keys = _dfs_collect(root)
        root.clear()
        return keys
    path: builtins.list[tuple[_TrieNode, str]] = []
    node: _TrieNode | None = root
    for ch in prefix:
        if node is None:
            return builtins.set()
        path.append((node, ch))
        node = node.children.get(ch)
        if node is None:
            return builtins.set()
    parent, ch = path[-1]
    keys = _dfs_collect(node)
    del parent.children[ch]
    for i in range(len(path) - 1, -1, -1):
        ancestor = path[i][0]
        if ancestor.key is None and not ancestor.children:
            if i > 0:
                parent = path[i - 1][0]
                child_ch = path[i - 1][1]
                del parent.children[child_ch]
            else:
                break
        else:
            break
    return keys
