#!/usr/bin/env python3
"""Power of Ten – Python Safety-Critical Coding Rules Checker.

Adapted from NASA/JPL's "Power of Ten" rules for developing safety-critical code.
This script enforces rules that cannot be checked by Ruff alone using AST analysis.

Usage:
    python scripts/power_of_ten.py [src_path]

Exit codes:
    0 - All checks passed
    1 - One or more violations found
"""

from __future__ import annotations

import ast
import sys
from collections import deque
from collections.abc import Iterator
from pathlib import Path


class PowerOfTenChecker(ast.NodeVisitor):
    """AST visitor that checks Python code against Power of Ten rules."""

    def __init__(self, filepath: Path) -> None:
        """Initialize the checker."""
        self.filepath = filepath
        self.violations: list[str] = []
        self.function_assertions: dict[str, int] = {}
        self.function_lines: dict[str, int] = {}
        self.function_nodes: dict[str, ast.FunctionDef | ast.AsyncFunctionDef] = {}
        self.current_function: str | None = None

    def _report(self, node: ast.AST, message: str) -> None:
        """Report a violation with file location."""
        lineno = getattr(node, "lineno", "?")
        self.violations.append(f"{self.filepath}:{lineno}: {message}")

    def _walk_no_nested_functions(self, node: ast.AST) -> Iterator[ast.AST]:
        """Walk AST nodes, excluding nested function/class definitions."""
        todo = deque(ast.iter_child_nodes(node))
        while todo:
            current = todo.pop()
            yield current
            if isinstance(
                current, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)
            ):
                continue
            todo.extend(ast.iter_child_nodes(current))

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Process a function definition."""
        self._check_function(node)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Process an async function definition."""
        self._check_function(node)
        self.generic_visit(node)

    def _is_delegator(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> bool:
        """Check if a function is a simple delegator (single return/expression)."""
        body = node.body
        if len(body) == 1:
            stmt = body[0]
            if isinstance(stmt, ast.Return):
                return True
            if isinstance(stmt, ast.Expr):
                return True
        return False

    def _check_function(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> None:
        """Check a function node for violations."""
        func_name = node.name
        self.current_function = func_name
        self.function_nodes[func_name] = node

        # Rule 4: Function length <= 60 lines
        end_lineno = getattr(node, "end_lineno", node.lineno)
        func_lines = end_lineno - node.lineno + 1
        self.function_lines[func_name] = func_lines
        if func_lines > 60:
            self._report(
                node,
                f"Rule 4: Function '{func_name}' is {func_lines} lines (max 60)",
            )

        # Count assertions for Rule 5 (excluding nested functions)
        assertion_count = self._count_assertions(node)
        self.function_assertions[func_name] = assertion_count

        # Rule 5: Validation density >= 1 per function for non-trivial functions
        is_init = func_name == "__init__"
        is_delegator = self._is_delegator(node)
        if not is_init and not is_delegator and func_lines > 35 and assertion_count < 1:
            self._report(
                node,
                f"Rule 5: Function '{func_name}' has {assertion_count} "
                f"validation checks (expected at least 1 for functions > 35 lines)",
            )

        # Rule 1: Check for recursion
        self._check_recursion(node, func_name)

        # Rule 3: Check for dynamic allocation after init
        self._check_dynamic_allocation(node, func_name)

        # Rule 2: Check loop bounds
        self._check_loop_bounds(node)

        # Rule 9: Check for pointer-like operations
        self._check_pointers(node)

        # Rule 6: Check variable scope
        self._check_variable_scope(node)

    def _count_assertions(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> int:
        """Count assertion and validation statements in a function body."""
        count = 0
        for child in self._walk_no_nested_functions(node):
            if isinstance(child, ast.Assert):
                count += 1
            if isinstance(child, ast.Raise) and child.exc is not None:
                count += 1
        return count

    def _check_recursion(
        self, node: ast.FunctionDef | ast.AsyncFunctionDef, func_name: str
    ) -> None:
        """Rule 1: Check for direct or indirect recursion."""
        called_names: set[str] = set()
        for child in self._walk_no_nested_functions(node):
            if isinstance(child, ast.Call):
                if isinstance(child.func, ast.Name):
                    called_names.add(child.func.id)
                if (
                    isinstance(child.func, ast.Attribute)
                    and isinstance(child.func.value, ast.Name)
                ):
                    called_names.add(child.func.value.id)

        if func_name in called_names:
            self._report(
                node,
                f"Rule 1: Function '{func_name}' calls itself (recursion)",
            )

    def _check_dynamic_allocation(
        self, node: ast.FunctionDef | ast.AsyncFunctionDef, func_name: str
    ) -> None:
        """Rule 3: Check for dynamic allocation after initialization."""
        is_init = func_name == "__init__"
        is_class_method = False

        # Check if we're inside a class method
        parent = node
        while hasattr(parent, "parent"):
            parent = getattr(parent, "parent", parent)
            if isinstance(parent, ast.ClassDef):
                is_class_method = True
                break

        if is_init or not is_class_method:
            for child in self._walk_no_nested_functions(node):
                if isinstance(child, ast.Call):
                    func = child.func
                    if (
                        isinstance(func, ast.Name)
                        and func.id in ("exec", "eval", "compile", "__import__")
                        and not is_init
                    ):
                            self._report(
                                child,
                                f"Rule 3: Dynamic allocation '{func.id}'() "
                                f"used outside __init__",
                            )

    def _check_loop_bounds(self, node: ast.AST) -> None:
        """Rule 2: Check that loops have fixed upper bounds."""
        for child in self._walk_no_nested_functions(node):
            if isinstance(child, ast.While):
                test = child.test
                if isinstance(test, ast.Constant) and test.value is True:
                    self._report(
                        child,
                        "Rule 2: Unbounded while True loop (requires explicit break)",
                    )
                elif isinstance(test, ast.Constant) and test.value == 1:
                    self._report(
                        child,
                        "Rule 2: Unbounded while 1 loop (requires explicit break)",
                    )

    def _check_pointers(self, node: ast.AST) -> None:
        """Rule 9: Check for pointer-like operations."""
        for child in self._walk_no_nested_functions(node):
            if isinstance(child, ast.Import):
                for alias in child.names:
                    if alias.name == "ctypes":
                        self._report(
                            child, "Rule 9: ctypes import (pointer-like operations)"
                        )
                if (
                    isinstance(child, ast.ImportFrom)
                    and child.module == "ctypes"
                ):
                    self._report(
                        child, "Rule 9: ctypes import (pointer-like operations)"
                    )

            if (
                isinstance(child, ast.Call)
                and isinstance(child.func, ast.Name)
                and child.func.id == "id"
                and len(child.args) == 1
                and isinstance(child.args[0], ast.Name)
            ):
                        self._report(
                            child,
                            "Rule 9: id() used (potential pointer simulation)",
                        )

    def _check_variable_scope(self, node: ast.AST) -> None:
        """Rule 6: Check that variables are declared at smallest possible scope."""
        assignments: list[tuple[int, str]] = []
        usages: dict[str, list[int]] = {}

        for child in self._walk_no_nested_functions(node):
            if isinstance(child, ast.Assign):
                for target in child.targets:
                    if isinstance(target, ast.Name):
                        assignments.append((child.lineno, target.id))
                        usages.setdefault(target.id, []).append(child.lineno)
            elif isinstance(child, ast.Name) and isinstance(child.ctx, ast.Load):
                usages.setdefault(child.id, []).append(child.lineno)

        for assign_line, var_name in assignments:
            if var_name in usages:
                other_usages = [
                    line_num
                    for line_num in usages[var_name]
                    if line_num != assign_line
                ]
                if other_usages:
                    first_usage = min(other_usages)
                    if first_usage - assign_line > 20:
                        self._report(
                            node,
                            f"Rule 6: Variable '{var_name}' declared at line "
                            f"{assign_line} but first used at line {first_usage} "
                            f"(consider smaller scope)",
                        )

    def check_file(self) -> list[str]:
        """Check a file and return list of violations."""
        source = self.filepath.read_text(encoding="utf-8")
        try:
            tree = ast.parse(source)
        except SyntaxError as e:
            return [f"{self.filepath}:{e.lineno}: Syntax error: {e.msg}"]

        self._add_parents(tree)
        self.visit(tree)

        # Rule 5: Check validation density (average across all functions)
        non_trivial = [lines for lines in self.function_lines.values() if lines > 35]
        if self.function_lines and non_trivial:
            total_assertions = sum(self.function_assertions.values())
            _ = sum(self.function_lines.values())
            avg_assertions = total_assertions / len(self.function_lines)
            if avg_assertions < 0.2:
                self.violations.append(
                    f"{self.filepath}: Rule 5: Average validation density is "
                    f"{avg_assertions:.1f} per function (expected >= 0.2)"
                )

        return self.violations

    def _add_parents(self, node: ast.AST, parent: ast.AST | None = None) -> None:
        """Add parent references to AST nodes."""
        for child in ast.iter_child_nodes(node):
            child.parent = parent  # type: ignore[attr-defined]
            self._add_parents(child, node)


def find_python_files(root: Path) -> list[Path]:
    """Find all Python files in a directory."""
    return list(root.rglob("*.py"))


def main() -> int:
    """Main entry point."""
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("src")
    files = find_python_files(root)

    all_violations: list[str] = []
    for filepath in files:
        if "__pycache__" in str(filepath) or ".venv" in str(filepath):
            continue
        checker = PowerOfTenChecker(filepath)
        violations = checker.check_file()
        all_violations.extend(violations)

    if all_violations:
        print("Power of Ten violations found:")
        for violation in all_violations:
            print(f"  {violation}")
        return 1

    print("Power of Ten checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
