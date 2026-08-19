use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError};

#[derive(Debug, Clone)]
pub struct MokaCache {
    inner: moka::future::Cache<String, CacheEntry>,
}

impl MokaCache {
    pub fn new(max_capacity: u64) -> Self {
        debug_assert!(max_capacity > 0, "moka max_capacity must be positive");
        Self {
            inner: moka::future::Cache::builder()
                .max_capacity(max_capacity)
                .support_invalidation_closures()
                .build(),
        }
    }
}

#[async_trait]
impl Cache for MokaCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        Ok(self.inner.get(key).await)
    }

    async fn set(&self, key: String, entry: CacheEntry) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.inner.insert(key, entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.inner.invalidate(key).await;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        let p = prefix.to_string();
        let _ = self
            .inner
            .invalidate_entries_if(move |key, _| key.starts_with(&p));
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let p = prefix.to_string();
        self.inner
            .invalidate_entries_if(move |key, _| key.starts_with(&p))
            .map_err(|e| CacheError::new(format!("moka shake error: {e}")))?;
        Ok(0)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.inner.invalidate_all();
        Ok(())
    }
}
