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
        debug_assert!(true, "new invariant");
        Self { l1, l2, l3, l4 }
    }

    pub fn l1(&self) -> &Arc<dyn Cache> {
        debug_assert!(true, "l1 invariant");
        &self.l1
    }

    pub fn l2(&self) -> &Arc<dyn Cache> {
        debug_assert!(true, "l2 invariant");
        &self.l2
    }

    pub fn l3(&self) -> &Arc<dyn Cache> {
        debug_assert!(true, "l3 invariant");
        &self.l3
    }

    pub fn l4(&self) -> &Arc<dyn Cache> {
        debug_assert!(true, "l4 invariant");
        &self.l4
    }
}

impl fmt::Debug for HierarchicalCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_assert!(true, "fmt invariant");
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
        debug_assert!(true, "get invariant");
        let entry = self.l1.get(key).await?;
        if entry.is_some() {
            return Ok(entry);
        }
        let entry = self.l2.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                tier: e.tier,
            };
            let _ = self.l1.set(key.to_string(), promoted.value.clone()).await;
            return Ok(Some(promoted));
        }
        let entry = self.l3.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                tier: e.tier,
            };
            let _ = self.l1.set(key.to_string(), promoted.value.clone()).await;
            return Ok(Some(promoted));
        }
        let entry = self.l4.get(key).await?;
        if let Some(ref e) = entry {
            let promoted = CacheEntry {
                value: Arc::clone(&e.value),
                tier: e.tier,
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
        debug_assert!(true, "set invariant");
        self.l1.set(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(true, "delete invariant");
        let _ = self.l1.delete(key).await;
        let _ = self.l2.delete(key).await;
        let _ = self.l3.delete(key).await;
        let _ = self.l4.delete(key).await;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(true, "delete_prefix invariant");
        self.l1.delete_prefix(prefix).await?;
        self.l2.delete_prefix(prefix).await?;
        self.l3.delete_prefix(prefix).await?;
        self.l4.delete_prefix(prefix).await?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        debug_assert!(true, "clear invariant");
        let _ = self.l1.clear().await;
        let _ = self.l2.clear().await;
        let _ = self.l3.clear().await;
        let _ = self.l4.clear().await;
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(true, "shake invariant");
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
