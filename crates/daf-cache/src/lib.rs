use std::any::Any;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod hierarchical;
pub mod moka;
pub mod postgres;
pub mod redis;
pub mod trie;

pub use crate::hierarchical::HierarchicalCache;
pub use crate::moka::MokaCache;
pub use crate::postgres::PostgresCache;
pub use crate::redis::RedisCache;
pub use crate::trie::{
    astar_collect, bfs_collect, dfs_collect, trie_collect, trie_delete, trie_delete_prefix,
    trie_insert, AStarEntry, TrieNode,
};

use async_trait::async_trait;
use daf_core::{CacheEntry, CacheError, Tier};

#[derive(Debug, Clone)]
pub struct MemoryCache {
    inner: Arc<RwLock<MemoryCacheInner>>,
}

#[derive(Debug)]
struct MemoryCacheInner {
    cache: HashMap<String, CacheEntry>,
    lru: lru::LruCache<String, ()>,
    trie: TrieNode,
    max_size: usize,
}

impl MemoryCache {
    pub fn new(max_size: usize) -> Self {
        let lru = if max_size > 0 {
            lru::LruCache::new(NonZeroUsize::new(max_size).unwrap())
        } else {
            lru::LruCache::unbounded()
        };
        Self {
            inner: Arc::new(RwLock::new(MemoryCacheInner {
                cache: HashMap::new(),
                lru,
                trie: TrieNode::default(),
                max_size,
            })),
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        let mut inner = self.inner.write().await;
        if let Some(entry) = inner.cache.get(key).cloned() {
            if inner.max_size > 0 {
                inner.lru.promote(key);
            }
            return Ok(Some(entry));
        }
        Ok(None)
    }

    pub async fn set(
        &self,
        key: String,
        value: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().await;
        let entry = CacheEntry {
            value,
            tier: Tier::L1,
        };
        if inner.cache.contains_key(&key) {
            if inner.max_size > 0 {
                inner.lru.promote(&key);
            }
        } else {
            if inner.max_size > 0 && inner.cache.len() >= inner.max_size {
                self.evict_oldest(&mut inner);
            }
            if inner.max_size > 0 {
                inner.lru.put(key.clone(), ());
            }
        }
        inner.cache.insert(key.clone(), entry);
        trie_insert(&mut inner.trie, &key);
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut inner = self.inner.write().await;
        if inner.cache.contains_key(key) {
            trie_delete(&mut inner.trie, key);
            inner.cache.remove(key);
            if inner.max_size > 0 {
                inner.lru.pop(key);
            }
        }
        Ok(())
    }

    pub async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        let mut inner = self.inner.write().await;
        let keys = trie_delete_prefix(&mut inner.trie, prefix);
        for key in keys {
            inner.cache.remove(&key);
            if inner.max_size > 0 {
                inner.lru.pop(&key);
            }
        }
        Ok(())
    }

    pub async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        let mut inner = self.inner.write().await;
        let keys = trie_delete_prefix(&mut inner.trie, prefix);
        for key in &keys {
            inner.cache.remove(key);
            if inner.max_size > 0 {
                inner.lru.pop(key);
            }
        }
        Ok(keys.len())
    }

    pub async fn clear(&self) -> Result<(), CacheError> {
        let mut inner = self.inner.write().await;
        inner.cache.clear();
        inner.trie = TrieNode::default();
        inner.lru.clear();
        Ok(())
    }

    pub async fn has(&self, key: &str) -> bool {
        let inner = self.inner.read().await;
        inner.cache.contains_key(key)
    }

    pub fn _dfs_collect(&self) -> std::collections::HashSet<String> {
        let inner = self.inner.blocking_read();
        dfs_collect(Some(&inner.trie))
    }

    pub fn _bfs_collect(&self) -> std::collections::HashSet<String> {
        let inner = self.inner.blocking_read();
        bfs_collect(&inner.trie)
    }

    pub fn _astar_collect(&self, target: &str) -> std::collections::HashSet<String> {
        let inner = self.inner.blocking_read();
        astar_collect(&inner.trie, target)
    }

    pub async fn _trie_collect(&self, prefix: &str) -> std::collections::HashSet<String> {
        let inner = self.inner.read().await;
        trie_collect(&inner.trie, prefix)
    }

    pub fn _trie_delete_prefix(&self, prefix: &str) -> std::collections::HashSet<String> {
        let mut inner = self.inner.blocking_write();
        trie_delete_prefix(&mut inner.trie, prefix)
    }

    fn evict_oldest(&self, inner: &mut MemoryCacheInner) {
        if let Some((key, _)) = inner.lru.pop_lru() {
            trie_delete(&mut inner.trie, &key);
            inner.cache.remove(&key);
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
