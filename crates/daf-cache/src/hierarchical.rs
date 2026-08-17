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
        // INV-001: fall through on miss OR error — a broken/missing tier must not
        // abort the read. Promotion writes back to L1; promotion errors are soft
        // (the value is still returned to the caller).
        let tiers: [&Arc<dyn Cache>; 4] = [&self.l1, &self.l2, &self.l3, &self.l4];
        for (i, tier) in tiers.iter().enumerate() {
            match tier.get(key).await {
                Ok(Some(e)) => {
                    if i > 0 {
                        if let Err(promo_err) =
                            self.l1.set(key.to_string(), Arc::clone(&e.value)).await
                        {
                            tracing::warn!(
                                tier = %i,
                                error = %promo_err,
                                "l1 promotion failed; serving value without caching at l1"
                            );
                        }
                    }
                    return Ok(Some(e));
                }
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(tier = %i, error = %e, "tier read error; falling through");
                    continue;
                }
            }
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
        // Best-effort across all tiers (INV-001). Invalidation is advisory; the
        // DataAccess generation check guarantees no stale value is accepted.
        for tier in [&self.l1, &self.l2, &self.l3, &self.l4] {
            if let Err(e) = tier.delete(key).await {
                tracing::warn!(error = %e, "tier delete degraded; continuing");
            }
        }
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        for tier in [&self.l1, &self.l2, &self.l3, &self.l4] {
            if let Err(e) = tier.delete_prefix(prefix).await {
                tracing::warn!(error = %e, "tier delete_prefix degraded; continuing");
            }
        }
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        for tier in [&self.l1, &self.l2, &self.l3, &self.l4] {
            if let Err(e) = tier.clear().await {
                tracing::warn!(error = %e, "tier clear degraded; continuing");
            }
        }
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let mut total: usize = 0;
        for tier in [&self.l1, &self.l2, &self.l3, &self.l4] {
            match tier.shake(prefix).await {
                Ok(n) => total += n,
                Err(e) => tracing::warn!(error = %e, "tier shake degraded; continuing"),
            }
        }
        Ok(total)
    }
}
