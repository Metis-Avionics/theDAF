use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Generation, Tier};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedCacheEntry {
    origin_tier: Tier,
    generation: Generation,
    value: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct PostgresCache {
    pool: PgPool,
}

impl PostgresCache {
    pub async fn new(database_url: &str) -> Result<Self, CacheError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| CacheError::new(format!("postgres connection error: {e}")))?;
        let cache = Self { pool };
        cache.ensure_table_exists().await?;
        Ok(cache)
    }

    async fn ensure_table_exists(&self) -> Result<(), CacheError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cache (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                generation BIGINT NOT NULL DEFAULT 0,
                origin_tier TEXT NOT NULL DEFAULT 'L1'
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CacheError::new(format!("postgres create table error: {e}")))?;
        Ok(())
    }

    fn serialize_entry(&self, entry: &CacheEntry) -> Result<String, CacheError> {
        let value = entry
            .value
            .downcast_ref::<serde_json::Value>()
            .ok_or_else(|| {
                CacheError::new("cache value is not a JSON value; cannot serialize to postgres")
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
}

#[async_trait]
impl Cache for PostgresCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let row = sqlx::query("SELECT value FROM cache WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres select error: {e}")))?;
        match row {
            Some(r) => {
                let raw: String = r.get("value");
                Ok(Some(self.deserialize_entry(&raw)?))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: String, entry: CacheEntry) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let raw = self.serialize_entry(&entry)?;
        let generation = match entry.generation {
            Generation::Valid(g) => g,
            Generation::Missing => 0,
        };
        let origin_tier = format!("{:?}", entry.origin_tier);
        sqlx::query(
            r#"
            INSERT INTO cache (key, value, generation, origin_tier)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE SET value = $2, generation = $3, origin_tier = $4
            "#,
        )
        .bind(key)
        .bind(raw)
        .bind(generation as i64)
        .bind(origin_tier)
        .execute(&self.pool)
        .await
        .map_err(|e| CacheError::new(format!("postgres upsert error: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        sqlx::query("DELETE FROM cache WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres delete error: {e}")))?;
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        let pattern = format!("{prefix}%");
        sqlx::query("DELETE FROM cache WHERE key LIKE $1")
            .bind(pattern)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres delete_prefix error: {e}")))?;
        Ok(())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let pattern = format!("{prefix}%");
        let result = sqlx::query("DELETE FROM cache WHERE key LIKE $1")
            .bind(pattern)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres shake error: {e}")))?;
        Ok(result.rows_affected() as usize)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        sqlx::query("TRUNCATE cache")
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres truncate error: {e}")))?;
        Ok(())
    }
}
