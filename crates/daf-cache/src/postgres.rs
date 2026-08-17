use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError};

#[derive(Debug, Clone)]
pub struct PostgresCache {
    _pool: (),
}

impl PostgresCache {
    pub fn new(_pool: ()) -> Self {
        Self { _pool: () }
    }
}

#[async_trait]
impl Cache for PostgresCache {
    async fn get(&self, _key: &str) -> Result<Option<CacheEntry>, CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }

    async fn set(
        &self,
        _key: String,
        _value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }

    async fn delete(&self, _key: &str) -> Result<(), CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }

    async fn delete_prefix(&self, _prefix: &str) -> Result<(), CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }

    async fn shake(&self, _prefix: &str) -> Result<usize, CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }

    async fn clear(&self) -> Result<(), CacheError> {
        Err(CacheError::new("postgres feature not enabled"))
    }
}
