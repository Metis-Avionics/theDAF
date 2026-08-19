use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Generation, Tier};
use dashmap::DashMap;

use crate::CachelitoCache;
use crate::MokaCache;

#[derive(Clone, Default)]
pub struct TierStats {
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
}

pub struct CacheManager {
    l1: CachelitoCache,
    l2: MokaCache,
    l3: Option<Arc<dyn Cache>>,
    l4: Option<Arc<dyn Cache>>,
    generations: DashMap<String, Generation>,
    stats: DashMap<Tier, TierStats>,
}

impl CacheManager {
    pub fn new(
        l1: CachelitoCache,
        l2: MokaCache,
        l3: Option<Arc<dyn Cache>>,
        l4: Option<Arc<dyn Cache>>,
    ) -> Self {
        Self {
            l1,
            l2,
            l3,
            l4,
            generations: DashMap::new(),
            stats: DashMap::new(),
        }
    }

    pub fn l1(&self) -> &CachelitoCache {
        &self.l1
    }

    pub fn l2(&self) -> &MokaCache {
        &self.l2
    }

    pub fn l3(&self) -> &Option<Arc<dyn Cache>> {
        &self.l3
    }

    pub fn l4(&self) -> &Option<Arc<dyn Cache>> {
        &self.l4
    }

    fn resource_namespace(key: &str) -> String {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let prefix = "query:";
        let rest = key.strip_prefix(prefix).unwrap_or(key);
        let namespace = rest.split(':').next().unwrap_or(rest);
        namespace.to_string()
    }

    async fn current_generation(&self, key: &str) -> Generation {
        let namespace = Self::resource_namespace(key);
        self.generations
            .get(&namespace)
            .map(|entry| *entry)
            .unwrap_or(Generation::Missing)
    }

    pub async fn current(&self, namespace: &str) -> Generation {
        debug_assert!(
            !namespace.is_empty(),
            "namespace must not be empty for current"
        );
        self.generations
            .get(namespace)
            .map(|entry| *entry)
            .unwrap_or(Generation::Missing)
    }

    pub async fn advance(&self, namespace: &str) -> Generation {
        debug_assert!(
            !namespace.is_empty(),
            "namespace must not be empty for advance"
        );
        let mut entry = self
            .generations
            .entry(namespace.to_string())
            .or_insert(Generation::Missing);
        let next = entry.advance();
        *entry = next;
        next
    }

    fn increment_stats(&self, tier: Tier, hits: u64, misses: u64, errors: u64) {
        let mut entry = self.stats.entry(tier).or_default();
        entry.hits += hits;
        entry.misses += misses;
        entry.errors += errors;
    }
}

#[async_trait]
impl Cache for CacheManager {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let current_gen = self.current_generation(key).await;

        let entry = self.l1.get(key).await;
        match entry {
            Ok(Some(ref e)) if e.generation == current_gen => {
                self.increment_stats(Tier::L1, 1, 0, 0);
                return Ok(Some(CacheEntry {
                    value: Arc::clone(&e.value),
                    origin_tier: Tier::L1,
                    generation: e.generation,
                }));
            }
            Ok(Some(_)) => {
                self.increment_stats(Tier::L1, 0, 1, 0);
            }
            Ok(None) => {
                self.increment_stats(Tier::L1, 0, 1, 0);
            }
            Err(_) => {
                self.increment_stats(Tier::L1, 0, 0, 1);
            }
        }

        let entry = self.l2.get(key).await;
        match entry {
            Ok(Some(ref e)) if e.generation == current_gen => {
                let promoted = CacheEntry {
                    value: Arc::clone(&e.value),
                    origin_tier: Tier::L1,
                    generation: e.generation,
                };
                let _ = self.l1.set(key.to_string(), promoted.clone()).await;
                self.increment_stats(Tier::L2, 1, 0, 0);
                return Ok(Some(promoted));
            }
            Ok(Some(_)) => {
                self.increment_stats(Tier::L2, 0, 1, 0);
            }
            Ok(None) => {
                self.increment_stats(Tier::L2, 0, 1, 0);
            }
            Err(_) => {
                self.increment_stats(Tier::L2, 0, 0, 1);
            }
        }

        if let Some(ref l3) = self.l3 {
            let entry = l3.get(key).await;
            match entry {
                Ok(Some(ref e)) if e.generation == current_gen => {
                    let promoted = CacheEntry {
                        value: Arc::clone(&e.value),
                        origin_tier: Tier::L1,
                        generation: e.generation,
                    };
                    let _ = l3.set(key.to_string(), promoted.clone()).await;
                    let _ = self.l2.set(key.to_string(), promoted.clone()).await;
                    let _ = self.l1.set(key.to_string(), promoted.clone()).await;
                    self.increment_stats(Tier::L3, 1, 0, 0);
                    return Ok(Some(promoted));
                }
                Ok(Some(_)) => {
                    self.increment_stats(Tier::L3, 0, 1, 0);
                }
                Ok(None) => {
                    self.increment_stats(Tier::L3, 0, 1, 0);
                }
                Err(_) => {
                    self.increment_stats(Tier::L3, 0, 0, 1);
                }
            }
        }

        if let Some(ref l4) = self.l4 {
            let entry = l4.get(key).await?;
            match entry {
                Some(ref e) if e.generation == current_gen => {
                    let promoted = CacheEntry {
                        value: Arc::clone(&e.value),
                        origin_tier: Tier::L1,
                        generation: e.generation,
                    };
                    let _ = l4.set(key.to_string(), promoted.clone()).await;
                    if let Some(ref l3) = self.l3 {
                        let _ = l3.set(key.to_string(), promoted.clone()).await;
                    }
                    let _ = self.l2.set(key.to_string(), promoted.clone()).await;
                    let _ = self.l1.set(key.to_string(), promoted.clone()).await;
                    self.increment_stats(Tier::L4, 1, 0, 0);
                    return Ok(Some(promoted));
                }
                Some(_) => {
                    self.increment_stats(Tier::L4, 0, 1, 0);
                }
                None => {
                    self.increment_stats(Tier::L4, 0, 1, 0);
                }
            }
        }

        Ok(None)
    }

    async fn set(&self, key: String, entry: CacheEntry) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        if let Some(ref l4) = self.l4 {
            l4.set(key.clone(), entry.clone()).await?;
        }
        if let Some(ref l3) = self.l3 {
            let _ = l3.set(key.clone(), entry.clone()).await;
        }
        let _ = self.l2.set(key.clone(), entry.clone()).await;
        let _ = self.l1.set(key, entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let _ = self.l1.delete(key).await;
        let _ = self.l2.delete(key).await;
        if let Some(ref l3) = self.l3 {
            let _ = l3.delete(key).await;
        }
        if let Some(ref l4) = self.l4 {
            l4.delete(key).await?;
        }
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for delete_prefix");
        let _ = self.l1.delete_prefix(prefix).await;
        let _ = self.l2.delete_prefix(prefix).await;
        if let Some(ref l3) = self.l3 {
            let _ = l3.delete_prefix(prefix).await;
        }
        if let Some(ref l4) = self.l4 {
            l4.delete_prefix(prefix).await?;
        }
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let l1_count = self.l1.shake(prefix).await.unwrap_or(0);
        let l2_count = self.l2.shake(prefix).await.unwrap_or(0);
        let l3_count = if let Some(ref l3) = self.l3 {
            l3.shake(prefix).await.unwrap_or(0)
        } else {
            0
        };
        let l4_count = if let Some(ref l4) = self.l4 {
            l4.shake(prefix).await?
        } else {
            0
        };
        Ok(l1_count + l2_count + l3_count + l4_count)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let _ = self.l1.clear().await;
        let _ = self.l2.clear().await;
        if let Some(ref l3) = self.l3 {
            let _ = l3.clear().await;
        }
        if let Some(ref l4) = self.l4 {
            l4.clear().await?;
        }
        Ok(())
    }
}
