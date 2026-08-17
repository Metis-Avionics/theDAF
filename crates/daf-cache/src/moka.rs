use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Tier};

#[derive(Debug, Clone)]
pub struct MokaCache {
    inner: moka::future::Cache<String, CacheEntry>,
}

impl MokaCache {
    pub fn new(max_capacity: u64) -> Self {
        debug_assert!(max_capacity > 0, "moka max_capacity must be positive");
        Self {
            inner: moka::future::Cache::new(max_capacity),
        }
    }
}

#[async_trait]
impl Cache for MokaCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        Ok(self.inner.get(key).await)
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let entry = CacheEntry {
            value,
            origin_tier: Tier::L2,
        };
        self.inner.insert(key, entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.inner.invalidate(key).await;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for delete_prefix");
        self.inner.invalidate_all();
        if prefix.is_empty() {
            Ok(())
        } else {
            Err(CacheError::new(format!(
                "MokaCache does not support prefix-scoped invalidation; full tier invalidated for prefix '{prefix}'",
            )))
        }
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let count = self.inner.entry_count() as usize;
        self.inner.invalidate_all();
        if prefix.is_empty() {
            Ok(count)
        } else {
            Err(CacheError::new(format!(
                "MokaCache does not support prefix-scoped shake; full tier invalidated for prefix '{prefix}'",
            )))
        }
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.inner.invalidate_all();
        Ok(())
    }
}
