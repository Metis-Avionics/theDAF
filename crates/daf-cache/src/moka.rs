use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Tier};

#[derive(Debug, Clone)]
pub struct MokaCache {
    inner: moka::future::Cache<String, CacheEntry>,
}

impl MokaCache {
    pub fn new(max_capacity: u64) -> Self {
        Self {
            inner: moka::future::Cache::new(max_capacity),
        }
    }
}

#[async_trait]
impl Cache for MokaCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        Ok(self.inner.get(key).await)
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        let entry = CacheEntry {
            value,
            tier: Tier::L2,
        };
        self.inner.insert(key, entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.inner.invalidate(key).await;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        if prefix.is_empty() {
            self.inner.invalidate_all();
            Ok(())
        } else {
            Err(CacheError::new(
                "prefix delete not supported by moka backend",
            ))
        }
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        if prefix.is_empty() {
            self.inner.invalidate_all();
            Ok(0)
        } else {
            Err(CacheError::new(
                "prefix shake not supported by moka backend",
            ))
        }
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.inner.invalidate_all();
        Ok(())
    }
}
