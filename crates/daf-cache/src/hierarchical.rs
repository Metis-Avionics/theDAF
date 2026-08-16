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
        &self.l1
    }

    pub fn l2(&self) -> &Arc<dyn Cache> {
        &self.l2
    }

    pub fn l3(&self) -> &Arc<dyn Cache> {
        &self.l3
    }

    pub fn l4(&self) -> &Arc<dyn Cache> {
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
        let entry = self.l1.get(key).await?;
        if entry.is_some() {
            return Ok(entry);
        }
        let entry = self.l2.get(key).await?;
        if entry.is_some() {
            return Ok(entry);
        }
        let entry = self.l3.get(key).await?;
        if entry.is_some() {
            return Ok(entry);
        }
        self.l4.get(key).await
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        self.l1.set(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.l1.delete(key).await?;
        self.l2.delete(key).await?;
        self.l3.delete(key).await?;
        self.l4.delete(key).await
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        self.l1.delete_prefix(prefix).await?;
        self.l2.delete_prefix(prefix).await?;
        self.l3.delete_prefix(prefix).await?;
        self.l4.delete_prefix(prefix).await
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.l1.clear().await?;
        self.l2.clear().await?;
        self.l3.clear().await?;
        self.l4.clear().await
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        let r1 = self.l1.shake(prefix).await?;
        let r2 = self.l2.shake(prefix).await?;
        let r3 = self.l3.shake(prefix).await?;
        let r4 = self.l4.shake(prefix).await?;
        Ok(r1 + r2 + r3 + r4)
    }
}
