use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Generation, Tier};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};

const DEFAULT_TTL_SECONDS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedCacheEntry {
    origin_tier: Tier,
    generation: Generation,
    value: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct RedisCache {
    client: Client,
}

impl RedisCache {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    async fn get_connection(&self) -> Result<ConnectionManager, CacheError> {
        self.client
            .get_connection_manager()
            .await
            .map_err(|e| CacheError::new(format!("redis connection error: {e}")))
    }

    fn serialize_entry(&self, entry: &CacheEntry) -> Result<String, CacheError> {
        let value = entry
            .value
            .downcast_ref::<serde_json::Value>()
            .ok_or_else(|| {
                CacheError::new("cache value is not a JSON value; cannot serialize to redis")
            })?;
        let serialized = SerializedCacheEntry {
            origin_tier: entry.origin_tier,
            generation: entry.generation,
            value: value.clone(),
        };
        serde_json::to_string(&serialized)
            .map_err(|e| CacheError::new(format!("failed to serialize cache entry: {e}")))
    }

    fn deserialize_entry(&self, raw: &str) -> Result<CacheEntry, CacheError> {
        let serialized: SerializedCacheEntry = serde_json::from_str(raw)
            .map_err(|e| CacheError::new(format!("failed to deserialize cache entry: {e}")))?;
        Ok(CacheEntry {
            value: Arc::new(serialized.value) as Arc<dyn std::any::Any + Send + Sync>,
            origin_tier: serialized.origin_tier,
            generation: serialized.generation,
        })
    }

    async fn collect_scan_match(
        conn: &mut ConnectionManager,
        pattern: &str,
    ) -> Result<Vec<String>, CacheError> {
        let mut iter = conn
            .scan_match::<_, String>(pattern)
            .await
            .map_err(|e| CacheError::new(format!("redis scan error: {e}")))?;
        let mut keys = Vec::new();
        while let Some(key) = iter.next_item().await {
            keys.push(key);
        }
        Ok(keys)
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let mut conn = self.get_connection().await?;
        let raw: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| CacheError::new(format!("redis get error: {e}")))?;
        match raw {
            Some(s) => Ok(Some(self.deserialize_entry(&s)?)),
            None => Ok(None),
        }
    }

    async fn set(&self, key: String, entry: CacheEntry) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let mut conn = self.get_connection().await?;
        let raw = self.serialize_entry(&entry)?;
        let ttl = DEFAULT_TTL_SECONDS;
        conn.set_ex::<_, _, ()>(key, raw, ttl)
            .await
            .map_err(|e| CacheError::new(format!("redis set error: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let mut conn = self.get_connection().await?;
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| CacheError::new(format!("redis delete error: {e}")))?;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for delete_prefix");
        let mut conn = self.get_connection().await?;
        let pattern = format!("{prefix}*");
        let keys = Self::collect_scan_match(&mut conn, &pattern).await?;
        if !keys.is_empty() {
            conn.del::<_, ()>(&keys)
                .await
                .map_err(|e| CacheError::new(format!("redis delete error: {e}")))?;
        }
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let mut conn = self.get_connection().await?;
        let pattern = format!("{prefix}*");
        let keys = Self::collect_scan_match(&mut conn, &pattern).await?;
        let count = keys.len();
        if !keys.is_empty() {
            conn.del::<_, ()>(&keys)
                .await
                .map_err(|e| CacheError::new(format!("redis delete error: {e}")))?;
        }
        Ok(count)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let mut conn = self.get_connection().await?;
        let keys = Self::collect_scan_match(&mut conn, "*").await?;
        if !keys.is_empty() {
            conn.del::<_, ()>(&keys)
                .await
                .map_err(|e| CacheError::new(format!("redis clear error: {e}")))?;
        }
        Ok(())
    }
}
