# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Authorizer` protocol for pluggable access control
- `AuthorizationError` exception for denied access
- Optional `authorizer` parameter on `DataAccess` and `DataAccessFactory`
- Optional `user` parameter on `DataAccess.query`, `post`, `put`, `delete`
- FastAPI adapter closure-based ownership authorizer
- HTTP 403 translation for authorization failures
- HTTP 404 translation for missing resources in adapter
- `tests/integration/test_authorization.py` for IDOR prevention scenarios
- `TestAuthorization` class in `tests/integration/test_fastapi_adapter.py`
- `TestAuthorizerProtocol` class in `tests/unit/test_components.py`

## [0.1.0] - 2026-08-13

### Added

- Initial production-ready release
- Core `DataAccess` orchestration layer with async CRUD operations
- `DataAccessFactory` for dependency composition
- `Repository` protocol with `MemoryRepository` reference implementation
- `Cache` protocol with `MemoryCache` reference implementation
- `Algorithm` protocol with `FibonacciDP` (explicit memoization)
- Pydantic v2 data contracts: `QueryInfo`, `PostInfo`, `PutInfo`, `DeleteInfo`, `QueryResult`, `MutationResult`
- FastAPI adapter (`DataAccessRouter`) with rate limiting
- Domain exception hierarchy: `DataAccessError`, `NotFoundError`, `ValidationError`, `RepositoryError`, `CacheError`, `AlgorithmError`
- 50 passing tests (17 unit + 25 integration + 8 end-to-end)
- Comprehensive type hints with mypy strict mode compliance
- Ruff linting configuration (E, F, I, B, UP, SIM rules)
- MIT License
- Full documentation in README.md
- Build artifacts: wheel + source distribution
- PyPI submission infrastructure (PUBLISH.sh, PYPI_SUBMISSION.md, BUILD_REPORT.txt)

### Architecture

- Zero FastAPI imports in core layer
- Protocol-based dependency injection
- Factory pattern for composition
- Explicit memoization (not functools decorator)
- Pydantic contracts at boundary only
- Rate limiting isolated to FastAPI adapter
- PEP 695 generic syntax (Python 3.12+)
- Power of Ten safety-critical coding rules adapted for Python

### Power of Ten Python Adaptation

- Rule 1: No recursion (AST-checked)
- Rule 2: Loop bounds (AST-checked for unbounded while)
- Rule 3: No dynamic allocation after init (AST-checked + Bandit S rules)
- Rule 4: Function length ≤ 60 lines (AST-checked)
- Rule 5: Validation density ≥ 1 per non-trivial function (AST-checked)
- Rule 6: Smallest variable scope (AST-checked)
- Rule 7: Return values and parameters validated (ARG/RET rules)
- Rule 8: Preprocessor limited (N/A for Python, exec/eval banned)
- Rule 9: No pointer-like operations (AST-checked for ctypes/id)
- Rule 10: Zero warnings (Ruff + mypy + pytest + custom checker)

### Quality Assurance

- mypy strict mode: 0 errors across 17 source files
- Ruff linting: 0 errors
- pytest: 50/50 tests passing
- Power of Ten checks: All pass
- Build verified in clean environment
- Installation verified from wheel

[0.1.0]: https://github.com/RAliane-REBORN/fastapi-data-access-factory/releases/tag/v0.1.0
