# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in `fastapi-data-access-factory`, please report it responsibly.

### How to Report

- **Email**: security@example.com (replace with actual maintainer email)
- **GitHub**: Open a private security advisory at https://github.com/RAliane-REBORN/fastapi-data-access-factory/security/advisories

### What to Include

1. Description of the vulnerability
2. Steps to reproduce
3. Affected versions
4. Suggested fix (if available)

### Response Timeline

- Acknowledgment: Within 48 hours
- Initial assessment: Within 7 days
- Fix release: Depends on severity
  - Critical: 1-7 days
  - High: 7-30 days
  - Medium: 30-90 days
  - Low: Next scheduled release

## Security Best Practices for Users

### Input Validation

Always validate input at the boundary before passing to `DataAccess`:

```python
from daf.contracts import QueryInfo

# Validate resource_id format
if not re.match(r'^[a-zA-Z0-9_-]+$', resource_id):
    raise ValidationError("Invalid resource_id format")

info = QueryInfo(resource_id=resource_id)
```

### Rate Limiting

Use the built-in rate limiting in the FastAPI adapter:

```python
from daf.adapters.fastapi import limiter

app.state.limiter = limiter
```

Do not disable rate limiting in production.

### Error Handling

Never expose raw exception messages to clients:

```python
try:
    result = await daf.query(info)
except DataAccessError as e:
    logger.error(f"DataAccess error: {e}")
    raise HTTPException(status_code=500, detail="Internal server error")
```

### Secrets Management

Never store secrets in the repository:

```python
# Bad
repo.save("db_password", "super_secret")

# Good
import os
db_password = os.environ["DB_PASSWORD"]
```

### Dependency Scanning

Regularly scan dependencies for vulnerabilities:

```bash
uv pip list --outdated
pip-audit
```

## Known Security Considerations

### In-Memory Components

`MemoryRepository` and `MemoryCache` are reference implementations for development and testing. They do not provide:

- Persistence across restarts
- Access control
- Encryption at rest
- Audit logging

Do not use them in production with sensitive data.

### Rate Limiting

Rate limiting is implemented at the FastAPI adapter layer only. If you expose `DataAccess` directly (without the adapter), you must implement your own rate limiting.

### Algorithm Execution

Custom `Algorithm` implementations execute arbitrary code. Only use trusted algorithms in production.

## Security Updates

Security updates will be released as patch versions (e.g., 0.1.1) and announced via:

- GitHub Security Advisories
- CHANGELOG.md
- GitHub Releases

Subscribe to releases for notifications: https://github.com/RAliane-REBORN/fastapi-data-access-factory/releases
