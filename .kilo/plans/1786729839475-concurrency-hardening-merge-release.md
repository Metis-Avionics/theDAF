# Remainder of Work: Concurrency Hardening, PR Merge, and v0.2.0 Release

## Current State

- Branch: `fix/r7-r12-red-team-composition-fixes` (5 commits ahead of main)
- PR #17: open, mergeable, CI green
- Uncommitted: `src/daf/core/access.py` contains concurrency hardening implementation (Steps 1-2 of plan `1786729023035-concurrency-hardening.md`)
- Tests: 119/119 passing, mypy strict clean, ruff clean, Power of Ten clean
- `pyproject.toml` version still `0.1.0`; `CHANGELOG.md` already has `[0.2.0]` section

## Steps

### 1. Commit concurrency hardening changes

File: `src/daf/core/access.py`

Changes:
- `import asyncio`
- `self._generation_locks: dict[str, asyncio.Lock] = {}`
- `self._generation_locks_lock = asyncio.Lock()`
- `_generation_lock()` helper
- Updated `_current_generation()` to acquire per-resource lock
- Updated `_advance_generation()` to acquire per-resource lock
- Concurrency model docstring in `DataAccess` class

Command:
```bash
git add src/daf/core/access.py
git commit -m "fix: add per-resource asyncio locks for generation advancement"
```

### 2. Bump version to 0.2.0

File: `pyproject.toml`

Change `version = "0.1.0"` to `version = "0.2.0"`.

Commit:
```bash
git add pyproject.toml
git commit -m "chore: bump version to 0.2.0"
```

### 3. Push branch

```bash
git push origin fix/r7-r12-red-team-composition-fixes
```

This updates PR #17 with the new commits. Verify CI passes on GitHub.

### 4. Merge PR #17

Merge PR #17 into `main` via GitHub UI or CLI:
```bash
gh pr merge 17 --squash --delete_branch
```

### 5. Tag v0.2.0 and push tag

```bash
git checkout main
git pull origin main
git tag v0.2.0
git push origin v0.2.0
```

Pushing the `v*` tag triggers `.github/workflows/publish.yml` which builds and publishes to PyPI.

### 6. Verify publication

- Check GitHub Actions run for the publish workflow completes successfully
- Verify package appears at https://pypi.org/p/thedaf/0.2.0/

## Validation Gates

Before merging PR #17, confirm:
- `pytest tests/ -q` → 119/119 passing
- `mypy src/ --strict` → 0 errors
- `ruff check src/ tests/` → 0 errors
- `python scripts/power_of_ten.py src/` → 0 violations

## Notes

- Do not commit `.kilo/plans/*.md` files; they are planning artifacts, not source code.
- The concurrency hardening plan's original target was 121 tests. Current count is 119/119 with the two key concurrency scenarios (`test_stale_cache_write_after_mutation_is_rejected`, `test_concurrent_mutations_generation_is_monotonic`) already covered. No additional tests are required.
- Deferred items (R15, R17, R18) remain out of scope for this release.
