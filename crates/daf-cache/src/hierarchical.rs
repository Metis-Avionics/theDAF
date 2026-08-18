use std::fmt;

use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError};

#[derive(Clone)]
pub struct HierarchicalCache {
    l1: Arc<dyn Cache>,
    l2: Arc<dyn Cache>,
    l3: Arc<dyn Cache>,
    l4: Arc<dyn Cache>,
}

impl HierarchicalCache {
    pub fn new(
        l1: Arc<dyn Cache>,
        l2: Arc<dyn Cache>,
        l3: Arc<dyn Cache>,
        l4: Arc<dyn Cache>,
    ) -> Self {
        Self { l1, l2, l3, l4 }
    }

    pub fn l1(&self) -> &Arc<dyn Cache> {
        debug_assert!(
            Arc::strong_count(&self.l1) > 0,
            "l1 cache Arc must be valid"
        );
        &self.l1
    }

    pub fn l2(&self) -> &Arc<dyn Cache> {
        debug_assert!(
            Arc::strong_count(&self.l2) > 0,
            "l2 cache Arc must be valid"
        );
        &self.l2
    }

    pub fn l3(&self) -> &Arc<dyn Cache> {
        debug_assert!(
            Arc::strong_count(&self.l3) > 0,
            "l3 cache Arc must be valid"
        );
        &self.l3
    }

    pub fn l4(&self) -> &Arc<dyn Cache> {
        debug_assert!(
            Arc::strong_count(&self.l4) > 0,
            "l4 cache Arc must be valid"
        );
        &self.l4
    }
}

impl fmt::Debug for HierarchicalCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HierarchicalCache")
            .field("l1", &"Arc<dyn Cache>")
            .field("l2", &"Arc<dyn Cache>")
            .field("l3", &"Arc<dyn Cache>")
            .field("l4", &"Arc<dyn Cache>")
            .finish()
    }
}

#[async_trait]
impl Cache for HierarchicalCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let entry = self.l1.get(key).await?;
        if entry.is_some() {
            return Ok(entry);
        }
        let entry = self.l2.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                origin_tier: e.origin_tier,
            };
            let _ = self.l1.set(key.to_string(), promoted.value.clone()).await;
            return Ok(Some(promoted));
        }
        let entry = self.l3.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                origin_tier: e.origin_tier,
            };
            let _ = self.l1.set(key.to_string(), promoted.value.clone()).await;
            return Ok(Some(promoted));
        }
        let entry = self.l4.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                origin_tier: e.origin_tier,
            };
            let _ = self.l1.set(key.to_string(), promoted.value.clone()).await;
            return Ok(Some(promoted));
        }
        Ok(None)
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.l1.set(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.l1.delete(key).await?;
        self.l2.delete(key).await?;
        self.l3.delete(key).await?;
        self.l4.delete(key).await?;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        self.l1.delete_prefix(prefix).await?;
        self.l2.delete_prefix(prefix).await?;
        self.l3.delete_prefix(prefix).await?;
        self.l4.delete_prefix(prefix).await?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.l1.clear().await?;
        self.l2.clear().await?;
        self.l3.clear().await?;
        self.l4.clear().await?;
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let mut total: usize = 0;
        let r1 = self.l1.shake(prefix).await?;
        total += r1;
        let r2 = self.l2.shake(prefix).await?;
        total += r2;
        let r3 = self.l3.shake(prefix).await?;
        total += r3;
        let r4 = self.l4.shake(prefix).await?;
        total += r4;
        Ok(total)
    }
}
