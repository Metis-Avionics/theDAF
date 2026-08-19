use async_trait::async_trait;
use cachelito::{AsyncGlobalCache, CacheStats, EvictionPolicy};
use daf_core::{Cache, CacheEntry, CacheError, Tier};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;

pub struct CachelitoCache {
    inner: AsyncGlobalCache<'static, CacheEntry>,
    capacity: usize,
}

impl CachelitoCache {
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let map: &'static DashMap<String, (CacheEntry, u64, u64)> =
            Box::leak(Box::new(DashMap::new()));
        let order: &'static Mutex<VecDeque<String>> =
            Box::leak(Box::new(Mutex::new(VecDeque::new())));
        let stats: &'static CacheStats = Box::leak(Box::new(CacheStats::new()));
        Self {
            inner: AsyncGlobalCache::new(
                map,
                order,
                Some(capacity),
                None,
                EvictionPolicy::LRU,
                None,
                None,
                None,
                stats,
            ),
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for CachelitoCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cache for CachelitoCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        let entry = self.inner.get(key);
        Ok(entry.map(|e| CacheEntry {
            value: e.value,
            origin_tier: Tier::L1,
            generation: e.generation,
        }))
    }

    async fn set(&self, key: String, entry: CacheEntry) -> Result<(), CacheError> {
        self.inner.insert(&key, entry);
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn delete_prefix(&self, _prefix: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn shake(&self, _prefix: &str) -> Result<usize, CacheError> {
        Ok(0)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        Ok(())
    }
}
