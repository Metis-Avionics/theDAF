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
        if prefix.is_empty() {
            self.inner.invalidate_all();
            return Ok(());
        }
        // Non-empty prefix is unsupported. Do NOT invalidate_all first (that would
        // over-invalidate unrelated keys, CON-006). Leave the tier intact and let
        // HierarchicalCache treat this tier as best-effort.
        Err(CacheError::new(format!(
            "MokaCache does not support prefix-scoped invalidation; tier left intact for prefix '{prefix}'"
        )))
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        if prefix.is_empty() {
            let count = self.inner.entry_count() as usize;
            self.inner.invalidate_all();
            return Ok(count);
        }
        Err(CacheError::new(format!(
            "MokaCache does not support prefix-scoped shake; tier left intact for prefix '{prefix}'"
        )))
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.inner.invalidate_all();
        Ok(())
    }
}
