"""Example FastAPI application using FastAPI Data Access Factory."""

from fastapi import FastAPI

from daf import DataAccessFactory
from daf.adapters.fastapi import DataAccessRouter, limiter
from daf.cache import MemoryCache
from daf.repositories import MemoryRepository

# Configure components
repository = MemoryRepository()
cache = MemoryCache()

# Create factory and DataAccess instance
factory = DataAccessFactory(
    repository=repository,
    cache=cache,
)
daf = factory.create()

# Create FastAPI app
app = FastAPI(
    title="FastAPI Data Access Factory Example",
    description="A complete example of the DAF architecture",
    version="0.1.0",
)

# Add rate limiter state
app.state.limiter = limiter

# Build and include data access router
router_builder = DataAccessRouter(daf)
app.include_router(router_builder.get_router())


# Root endpoint
@app.get("/")
async def root():
    """Root endpoint with API documentation."""
    return {
        "message": "Welcome to FastAPI Data Access Factory",
        "docs_url": "/docs",
        "endpoints": {
            "GET /data/{resource_id}": "Query a resource",
            "POST /data": "Create a new resource",
            "PUT /data/{resource_id}": "Update a resource",
            "DELETE /data/{resource_id}": "Delete a resource",
        },
        "rate_limits": {
            "GET": "30/minute",
            "POST": "10/minute",
            "PUT": "10/minute",
            "DELETE": "10/minute",
        },
    }


# Health check endpoint
@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "ok"}


# Example: Pre-populate with some data
@app.on_event("startup")
async def startup_event():
    """Populate repository with example data on startup."""
    await repository.save("user:1", {
        "id": "user:1",
        "name": "Alice",
        "email": "alice@example.com",
        "role": "admin",
    })
    await repository.save("user:2", {
        "id": "user:2",
        "name": "Bob",
        "email": "bob@example.com",
        "role": "user",
    })
    print("✓ Example data loaded")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(
        app,
        host="0.0.0.0",  # noqa: S104
        port=8000,
        reload=True,
    )
