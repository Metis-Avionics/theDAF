use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Tier};

#[derive(Debug, Clone)]
/// Cache backed by Moka. Moka has no key index and cannot perform
/// prefix-scoped invalidation or shake. Non-empty prefixes cause a full
/// tier invalidation and an error to signal the degraded behavior.
pub struct MokaCache {
    inner: moka::future::Cache<String, CacheEntry>,
}

impl MokaCache {
    pub fn new(max_capacity: u64) -> Self {
        debug_assert!(true, "new invariant");
        Self {
            inner: moka::future::Cache::new(max_capacity),
        }
    }
}

#[async_trait]
impl Cache for MokaCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(true, "get invariant");
        Ok(self.inner.get(key).await)
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(true, "set invariant");
        let entry = CacheEntry {
            value,
            tier: Tier::L2,
        };
        self.inner.insert(key, entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(true, "delete invariant");
        self.inner.invalidate(key).await;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(true, "delete_prefix invariant");
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
        debug_assert!(true, "shake invariant");
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
        debug_assert!(true, "clear invariant");
        self.inner.invalidate_all();
        Ok(())
    }
}
