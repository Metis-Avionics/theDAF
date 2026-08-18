//! Tier-aware cache hierarchy.
//!
//! - `MemoryCache` (L1): in-memory cache with LRU eviction and trie-based prefix operations.
//! - `MokaCache` (L2): degraded tier; non-empty `delete_prefix`/`shake` return errors.
//! - `RedisCache` (L3): **stub** behind `redis` feature flag; all operations return `CacheError::new("redis feature not enabled")`.
//! - `PostgresCache` (L4): **stub** behind `postgres` feature flag; all operations return `CacheError::new("postgres feature not enabled")`.
//!
//! Feature compilation (`--all-features clippy`) proves stub compilation, not backend behavior.

use std::any::Any;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

pub mod hierarchical;
pub mod moka;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "redis")]
pub mod redis;
pub mod trie;

pub use crate::hierarchical::HierarchicalCache;
pub use crate::moka::MokaCache;
#[cfg(feature = "postgres")]
pub use crate::postgres::PostgresCache;
#[cfg(feature = "redis")]
pub use crate::redis::RedisCache;
pub use crate::trie::{
    astar_collect, bfs_collect, dfs_collect, trie_collect, trie_delete, trie_delete_prefix,
    trie_insert, AStarEntry, TrieNode,
};

use async_trait::async_trait;
use dashmap::DashMap;
use daf_core::{CacheEntry, CacheError, Tier};

#[derive(Debug, Clone)]
pub struct MemoryCache {
    inner: Arc<MemoryCacheInner>,
}

#[derive(Debug)]
struct MemoryCacheInner {
    cache: DashMap<String, CacheEntry>,
    lru: Mutex<lru::LruCache<String, ()>>,
    trie: Mutex<TrieNode>,
    max_size: usize,
}

impl MemoryCache {
    pub fn new(max_size: usize) -> Self {
        let lru = if max_size > 0 {
            let nonzero = NonZeroUsize::new(max_size).unwrap_or_else(|| {
                panic!("max_size must be positive");
            });
            lru::LruCache::new(nonzero)
        } else {
            lru::LruCache::unbounded()
        };
        let inner = MemoryCacheInner {
            cache: DashMap::new(),
            lru: Mutex::new(lru),
            trie: Mutex::new(TrieNode::default()),
            max_size,
        };
        debug_assert!(inner.cache.is_empty(), "new MemoryCache must start empty");
        Self { inner: Arc::new(inner) }
    }

    pub async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let entry = self.inner.cache.get(key).map(|r| (*r).clone());
        if entry.is_some() && self.inner.max_size > 0 {
            self.inner.lru.lock().unwrap_or_else(|e| e.into_inner()).promote(key);
        }
        Ok(entry)
    }

    pub async fn set(
        &self,
        key: String,
        value: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let entry = CacheEntry {
            value,
            origin_tier: Tier::L1,
        };
        let was_new = !self.inner.cache.contains_key(&key);
        self.inner.cache.insert(key.clone(), entry);

        if was_new {
            let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
            if self.inner.max_size > 0 && self.inner.cache.len() > self.inner.max_size {
                self.evict_oldest(&mut lru);
            }
            if self.inner.max_size > 0 {
                lru.put(key.clone(), ());
            }
            drop(lru);
            let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
            trie_insert(&mut trie, &key);
        } else {
            let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
            if self.inner.max_size > 0 {
                lru.promote(&key);
            }
        }

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        if self.inner.cache.remove(key).is_some() {
            let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
            if self.inner.max_size > 0 {
                lru.pop(key);
            }
            drop(lru);
            let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
            trie_delete(&mut trie, key);
        }
        Ok(())
    }

    pub async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for delete_prefix");
        let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        let keys = trie_delete_prefix(&mut trie, prefix);
        drop(trie);

        let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
        for key in &keys {
            self.inner.cache.remove(key);
            if self.inner.max_size > 0 {
                lru.pop(key);
            }
        }

        Ok(())
    }

    pub async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        let keys = trie_delete_prefix(&mut trie, prefix);
        drop(trie);

        let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
        for key in &keys {
            self.inner.cache.remove(key);
            if self.inner.max_size > 0 {
                lru.pop(key);
            }
        }

        Ok(keys.len())
    }

    pub async fn clear(&self) -> Result<(), CacheError> {
        let mut lru = self.inner.lru.lock().unwrap_or_else(|e| e.into_inner());
        let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        self.inner.cache.clear();
        *trie = TrieNode::default();
        lru.clear();
        Ok(())
    }

    pub async fn has(&self, key: &str) -> bool {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        self.inner.cache.contains_key(key)
    }

    pub fn _dfs_collect(&self) -> std::collections::HashSet<String> {
        let trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        dfs_collect(Some(&*trie))
    }

    pub fn _bfs_collect(&self) -> std::collections::HashSet<String> {
        let trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        bfs_collect(&trie)
    }

    pub fn _astar_collect(&self, target: &str) -> std::collections::HashSet<String> {
        debug_assert!(!target.is_empty(), "astar target must not be empty");
        let trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        astar_collect(&trie, target)
    }

    pub async fn _trie_collect(&self, prefix: &str) -> std::collections::HashSet<String> {
        debug_assert!(!prefix.is_empty(), "trie prefix must not be empty");
        let trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        trie_collect(&trie, prefix)
    }

    pub fn _trie_delete_prefix(&self, prefix: &str) -> std::collections::HashSet<String> {
        debug_assert!(!prefix.is_empty(), "trie delete prefix must not be empty");
        let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
        trie_delete_prefix(&mut trie, prefix)
    }

    fn evict_oldest(&self, lru: &mut lru::LruCache<String, ()>) {
        debug_assert!(self.inner.max_size > 0, "eviction requires bounded cache");
        if let Some((key, _)) = lru.pop_lru() {
            self.inner.cache.remove(&key);
            let mut trie = self.inner.trie.lock().unwrap_or_else(|e| e.into_inner());
            trie_delete(&mut trie, &key);
        }
    }
}

#[async_trait]
impl daf_core::Cache for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        MemoryCache::get(self, key).await
    }

    async fn set(&self, key: String, value: Arc<dyn Any + Send + Sync>) -> Result<(), CacheError> {
        MemoryCache::set(self, key, value).await
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        MemoryCache::delete(self, key).await
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        MemoryCache::delete_prefix(self, prefix).await
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        MemoryCache::shake(self, prefix).await
    }

    async fn clear(&self) -> Result<(), CacheError> {
        MemoryCache::clear(self).await
    }
}
