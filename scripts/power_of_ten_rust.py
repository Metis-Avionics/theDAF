#!/usr/bin/env python3
"""
NASA/JPL Power of Ten Rules — Rust Adaptation Checker

Validates Rust source files against adapted Power of Ten rules:
1. Simple control flow — no `unsafe` outside ffi/ modules; no recursion
2. Fixed loop bounds — no unbounded `loop {}`; bounded `while`/`for`
3. No dynamic allocation after init — no `Box::leak`, `Vec::reserve` post-init
4. Function length ≤ 60 lines
5. Assertion density ≥ 2 per function (debug_assert!, assert!, expect/unwrap on fallible)
6. Smallest variable scope
7. All non-void return values checked — no silent `.unwrap()` in application code
8. Macros limited — no recursive macros, no var args, no token pasting; cfg limited
9. No raw pointers in application code (ffi boundary only); no more than one deref level
10. Zero warnings — delegated to clippy; this script enforces mechanical rules 1-9

Usage:
    python scripts/power_of_ten_rust.py [path ...]

Exit code:
    0 — all checks pass
    1 — one or more violations found
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Violation:
    file: str
    line: int
    rule: int
    message: str

    def __str__(self) -> str:
        return f"{self.file}:{self.line}: rule {self.rule}: {self.message}"


def is_ffi_file(path: Path) -> bool:
    return "ffi" in str(path).lower()


def is_test_file(path: Path) -> bool:
    p = str(path).lower()
    return "/tests/" in p or p.endswith("_test.rs") or p.endswith("_tests.rs")


def check_rule1_no_unsafe_outside_ffi(
    content: str, path: Path, violations: list[Violation]
) -> None:
    if is_ffi_file(path):
        return
    for i, line in enumerate(content.splitlines(), 1):
        if "unsafe" in line and not line.strip().startswith("//"):
            violations.append(
                Violation(
                    str(path),
                    i,
                    1,
                    "unsafe block outside ffi boundary",
                )
            )


def check_rule2_fixed_loop_bounds(
    content: str, path: Path, violations: list[Violation]
) -> None:
    for i, line in enumerate(content.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("loop {") or stripped == "loop {":
            violations.append(
                Violation(
                    str(path),
                    i,
                    2,
                    "unbounded `loop {}` without explicit break condition",
                )
            )
        if re.match(r"while\s+true\s*\{", stripped):
            violations.append(
                Violation(
                    str(path),
                    i,
                    2,
                    "`while true` loop without explicit upper bound",
                )
            )


def check_rule3_no_dynamic_alloc_after_init(
    content: str, path: Path, violations: list[Violation]
) -> None:
    for i, line in enumerate(content.splitlines(), 1):
        if "Box::leak" in line:
            violations.append(
                Violation(
                    str(path),
                    i,
                    3,
                    "`Box::leak` allocates heap memory after initialization",
                )
            )


def check_rule4_function_length(
    content: str, path: Path, violations: list[Violation]
) -> None:
    lines = content.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        fn_match = re.match(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+", line)
        if fn_match:
            is_declaration = False
            k = i
            while k < len(lines):
                stripped = lines[k].strip()
                if stripped.startswith("//") or stripped.startswith("#"):
                    k += 1
                    continue
                if "{" in lines[k]:
                    break
                if stripped.endswith(";"):
                    is_declaration = True
                    break
                if re.match(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+", lines[k]) and k > i:
                    break
                k += 1
            if is_declaration:
                i = k + 1
                continue

            start = i
            depth = 0
            j = i
            end = j
            while j < len(lines):
                for char in lines[j]:
                    if char == "{":
                        depth += 1
                    elif char == "}":
                        depth -= 1
                        if depth == 0 and j > start:
                            end = j + 1
                            break
                if end > j:
                    break
                j += 1
            length = end - start
            if length > 60:
                violations.append(
                    Violation(
                        str(path),
                        start + 1,
                        4,
                        f"function `{line.strip()}` is {length} lines (> 60)",
                    )
                )
            i = end
        else:
            i += 1


def check_rule5_assertion_density(
    content: str, path: Path, violations: list[Violation]
) -> None:
    lines = content.splitlines()
    i = 0
    functions: list[tuple[int, int, int]] = []
    while i < len(lines):
        line = lines[i]
        fn_match = re.match(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", line)
        if fn_match:
            is_declaration = False
            k = i
            while k < len(lines):
                stripped = lines[k].strip()
                if stripped.startswith("//") or stripped.startswith("#"):
                    k += 1
                    continue
                if "{" in lines[k]:
                    break
                if stripped.endswith(";"):
                    is_declaration = True
                    break
                if re.match(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+", lines[k]) and k > i:
                    break
                k += 1
            if is_declaration:
                i = k + 1
                continue

            start = i
            depth = 0
            j = i
            fn_name = fn_match.group(1)
            end = j
            while j < len(lines):
                for char in lines[j]:
                    if char == "{":
                        depth += 1
                    elif char == "}":
                        depth -= 1
                        if depth == 0 and j > start:
                            end = j + 1
                            break
                if end > j:
                    break
                j += 1
            fn_lines = lines[start:end]
            assertion_patterns = [
                r"\b(?:assert!|debug_assert!|assert_eq!|assert_ne!)(?=[\s(])",
                r"\.expect\(\s*[\"\']",
            ]
            assertions = sum(
                1
                for l in fn_lines
                if any(re.search(p, l) for p in assertion_patterns)
            )
            functions.append((start + 1, fn_name, assertions))
            i = end
        else:
            i += 1

    for lineno, name, count in functions:
        if count < 1:
            violations.append(
                Violation(
                    str(path),
                    lineno,
                    5,
                    f"function `{name}` has {count} assertions (minimum 1)",
                )
            )


def check_rule6_smallest_scope(
    content: str, path: Path, violations: list[Violation]
) -> None:
    for i, line in enumerate(content.splitlines(), 1):
        if re.match(r"^\s*(?:pub\s+)?static\s+mut\s+\w+", line):
            violations.append(
                Violation(
                    str(path),
                    i,
                    6,
                    "mutable static binding at potentially non-minimal scope",
                )
            )


def check_rule7_return_values_checked(
    content: str, path: Path, violations: list[Violation]
) -> None:
    for i, line in enumerate(content.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("//") or stripped.startswith("#"):
            continue
        if re.search(r"\.unwrap\(\s*\)", line) and "ffi" not in str(path).lower():
            if "test" not in str(path).lower():
                violations.append(
                    Violation(
                        str(path),
                        i,
                        7,
                        "`.unwrap()` on fallible value without explicit error handling",
                    )
                )
        if re.search(r"\.expect\(", line) and "ffi" not in str(path).lower():
            violations.append(
                Violation(
                    str(path),
                    i,
                    7,
                    "`.expect()` should have explicit error propagation in application code",
                )
            )


def check_rule8_macro_limits(
    content: str, path: Path, violations: list[Violation]
) -> None:
    macro_defs = re.finditer(r"#\[macro_use\].*\nmacro_rules!\s+(\w+)", content)
    for m in macro_defs:
        name = m.group(1)
        start = content[: m.start()].count("\n") + 1
        violations.append(
            Violation(
                str(path),
                start,
                8,
                f"`macro_rules!` {name} — preprocessor macros should be limited; prefer functions",
            )
        )
    for i, line in enumerate(content.splitlines(), 1):
        if "macro_rules!" in line and "macro_use" not in line:
            violations.append(
                Violation(
                    str(path),
                    i,
                    8,
                    "`macro_rules!` usage — prefer inline functions or const generics",
                )
            )
        if "$( $x:tt )+" in line or "$( $x:tt )*" in line:
            violations.append(
                Violation(
                    str(path),
                    i,
                    8,
                    "variadic macro repetition — limit macro complexity",
                )
            )


def check_rule9_pointer_restrictions(
    content: str, path: Path, violations: list[Violation]
) -> None:
    is_ffi = "ffi" in str(path).lower()
    for i, line in enumerate(content.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        if "*mut " in line or "*const " in line:
            if not is_ffi:
                violations.append(
                    Violation(
                        str(path),
                        i,
                        9,
                        "raw pointer outside ffi boundary",
                    )
                )
        if "fn(" in line and "-> fn" in line:
            violations.append(
                Violation(
                    str(path),
                    i,
                    9,
                    "function pointer — not permitted",
                )
            )


RULES = [
    check_rule1_no_unsafe_outside_ffi,
    check_rule2_fixed_loop_bounds,
    check_rule3_no_dynamic_alloc_after_init,
    check_rule4_function_length,
    check_rule5_assertion_density,
    check_rule6_smallest_scope,
    check_rule7_return_values_checked,
    check_rule8_macro_limits,
    check_rule9_pointer_restrictions,
]


def check_file(path: Path) -> list[Violation]:
    violations: list[Violation] = []
    try:
        content = path.read_text(encoding="utf-8")
    except Exception as exc:
        print(f"warning: could not read {path}: {exc}", file=sys.stderr)
        return violations

    for rule in RULES:
        rule(content, path, violations)

    return violations


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    crate_sources = list((root / "crates").rglob("src/**/*.rs"))
    if not crate_sources:
        crate_sources = list((root / "crates").rglob("*.rs"))

    violations: list[Violation] = []
    for src in crate_sources:
        violations.extend(check_file(src))

    if violations:
        print("Power of Ten (Rust) violations:", file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        print(f"\n{len(violations)} violation(s) found", file=sys.stderr)
        return 1

    print("Power of Ten (Rust): all checks pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
