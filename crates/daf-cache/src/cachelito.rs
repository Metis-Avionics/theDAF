use std::sync::Arc;

use daf_core::{Cache, CacheEntry, CacheError, Tier};
use dashmap::DashMap;

pub struct CachelitoCache {
    inner: Arc<DashMap<String, Arc<dyn std::any::Any + Send + Sync>>>,
}

impl CachelitoCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }
}

impl Default for CachelitoCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Cache for CachelitoCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        Ok(self.inner.get(key).map(|entry| CacheEntry {
            value: entry.value().clone(),
            origin_tier: Tier::L1,
        }))
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.inner.insert(key, value);
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        Ok(0)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        Ok(())
    }
}
