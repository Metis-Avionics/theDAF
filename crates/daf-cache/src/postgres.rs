//! L3 — the Postgres-backed remote shared cache tier.
//!
//! Session/row storage: every entry is a `(key TEXT PRIMARY KEY, value BYTEA)`
//! row in a dedicated cache table, so the tier behaves exactly like the other
//! `Vec<u8>`-carrying remotes (`RedisCache`) behind the [`Cache`] trait:
//! `set` requires pre-serialized bytes (mismatched payloads are rejected
//! before any I/O), `get` returns the bytes with `origin_tier = Tier::L3`,
//! and `delete_prefix`/`shake` count rows against a **literal** prefix
//! (`%`, `_`, `\` are escaped — the prefix is never treated as a wildcard).
//!
//! Three construction inlets, each with an explicit schema contract:
//! [`PostgresCache::connect`] (eager — connects and bootstraps the table),
//! [`PostgresCache::connect_lazy`] (deferred — no connection at construction),
//! and [`PostgresCache::with_pool`] (the caller's pool, unmutated). The lazy
//! and shared-pool inlets do not run DDL; callers bootstrap once via
//! [`PostgresCache::ensure_schema`].

use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{Cache, CacheEntry, CacheError, Tier};
use sqlx::postgres::PgPool;

/// The idempotent bootstrap DDL for the tier's session/row storage.
const SCHEMA_DDL: &str = "CREATE TABLE IF NOT EXISTS daf_cache_entries (\
    key TEXT PRIMARY KEY, \
    value BYTEA NOT NULL\
)";

/// The upsert carried by [`Cache::set`]: insert-or-overwrite by primary key.
const UPSERT_SQL: &str = "INSERT INTO daf_cache_entries (key, value)\
    VALUES ($1, $2)\
    ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value";

/// The literal-prefix delete carried by [`Cache::delete_prefix`]: the bound
/// pattern is escaped by [`escape_like_prefix`], so `%`/`_` inside the
/// caller's prefix match themselves, never wildcards.
const DELETE_PREFIX_SQL: &str = "DELETE FROM daf_cache_entries WHERE key LIKE $1 ESCAPE '\\'";

#[derive(Debug, Clone)]
pub struct PostgresCache {
    pool: PgPool,
}

impl PostgresCache {
    /// Eager inlet: connect to `url`, then bootstrap the row store
    /// (`ensure_schema`). The returned tier is immediately usable.
    pub async fn connect(url: &str) -> Result<Self, CacheError> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| CacheError::new(format!("postgres connect error: {}", e)))?;
        let cache = Self::with_pool(pool);
        cache.ensure_schema().await?;
        Ok(cache)
    }

    /// Deferred inlet: never attempts a connection at construction (a
    /// malformed URL is still rejected at parse time). Callers bootstrap via
    /// [`PostgresCache::ensure_schema`]; transport failures surface through
    /// the trait methods as [`CacheError`], never silent collapse.
    pub fn connect_lazy(url: &str) -> Result<Self, CacheError> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy(url)
            .map_err(|e| CacheError::new(format!("postgres url parse error: {}", e)))?;
        Ok(Self::with_pool(pool))
    }

    /// Shared-pool inlet: wrap a caller-owned pool without mutating its
    /// configuration (or its schema). Bootstrap remains the caller's call.
    pub fn with_pool(pool: PgPool) -> Self {
        debug_assert!(
            !pool.is_closed(),
            "shared pool must be open when handed to the tier"
        );
        Self { pool }
    }

    /// Idempotent bootstrap of the `(key, value)` row store. Safe to run on
    /// every startup; runs once per inlet at most in [`PostgresCache::connect`].
    pub async fn ensure_schema(&self) -> Result<(), CacheError> {
        sqlx::query(SCHEMA_DDL)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres ensure_schema error: {}", e)))?;
        Ok(())
    }
}

/// Escape the LIKE metacharacters of a prefix so the bound pattern matches
/// the prefix **literally** (`%`, `_`, `\` match themselves; the trailing
/// `%` is the only wildcard). Unit-tested below against every class.
fn escape_like_prefix(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 2);
    for c in prefix.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    // Invariant: the built pattern is always "<escaped input>%", so the
    // prefix class can never silently become an exact-match or a bare
    // wildcard.
    debug_assert!(
        out.ends_with('%'),
        "the trailing wildcard is always appended"
    );
    out
}

#[async_trait]
impl Cache for PostgresCache {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let bytes: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT value FROM daf_cache_entries WHERE key = $1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| CacheError::new(format!("postgres get error: {}", e)))?;
        Ok(bytes.map(|b| CacheEntry {
            value: Arc::new(b),
            origin_tier: Tier::L3,
        }))
    }

    async fn set(
        &self,
        key: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        // The value contract is checked before any I/O: a mismatched payload
        // is rejected even when the pool is unreachable.
        let bytes = Arc::unwrap_or_clone(value.downcast::<Vec<u8>>().map_err(|_| {
            CacheError::new("PostgresCache requires Vec<u8> values; serialize before set")
        })?);
        sqlx::query(UPSERT_SQL)
            .bind(&key)
            .bind(&bytes)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres set error: {}", e)))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        debug_assert!(!key.is_empty(), "cache key must not be empty");
        let result = sqlx::query("DELETE FROM daf_cache_entries WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres delete error: {}", e)))?;
        debug_assert!(
            result.rows_affected() <= 1,
            "key is the table's primary key: at most one row is deleted"
        );
        Ok(())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64, CacheError> {
        debug_assert!(
            !prefix.is_empty(),
            "prefix must not be empty for delete_prefix"
        );
        let pattern = escape_like_prefix(prefix);
        let result = sqlx::query(DELETE_PREFIX_SQL)
            .bind(&pattern)
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres delete_prefix error: {}", e)))?;
        Ok(result.rows_affected())
    }

    async fn shake(&self, prefix: &str) -> Result<usize, CacheError> {
        debug_assert!(!prefix.is_empty(), "prefix must not be empty for shake");
        let n = self.delete_prefix(prefix).await? as usize;
        debug_assert!(n <= u64::MAX as usize, "count fits the wire counter domain");
        Ok(n)
    }

    async fn clear(&self) -> Result<(), CacheError> {
        sqlx::query("DELETE FROM daf_cache_entries")
            .execute(&self.pool)
            .await
            .map_err(|e| CacheError::new(format!("postgres clear error: {}", e)))?;
        Ok(())
    }

    fn tier(&self) -> Tier {
        debug_assert!(
            (Tier::L2 as u8) < (Tier::L3 as u8) && (Tier::L3 as u8) < (Tier::L4 as u8),
            "L3 orders strictly between L2 and L4"
        );
        Tier::L3
    }
}

#[cfg(test)]
mod tests {
    use super::escape_like_prefix;

    #[test]
    fn plain_prefix_gains_only_the_wildcard() {
        assert_eq!(escape_like_prefix("alpha"), "alpha%");
    }

    #[test]
    fn every_metacharacter_is_escaped_literally() {
        assert_eq!(escape_like_prefix("a%b_c"), "a\\%b\\_c%");
        assert_eq!(escape_like_prefix("a\\b"), "a\\\\b%");
        assert_eq!(escape_like_prefix("\\%_"), "\\\\\\%\\_%");
    }

    #[test]
    fn unicode_prefixes_survive_escaping() {
        assert_eq!(escape_like_prefix("ключ"), "ключ%");
        assert_eq!(escape_like_prefix("日_%本"), "日\\_\\%本%");
    }
}
