//! Tier-aware cache hierarchy.
//!
//! - `CachelitoCache` (L1): async concurrent cache via Cachelito primitive.
//! - `MokaCache` (L2): degraded tier; non-empty `delete_prefix`/`shake` return errors.
//! - `RedisCache` (L3): **stub** behind `redis` feature flag; all operations return `CacheError::new("redis feature not enabled")`.
//! - `PostgresCache` (L4): **stub** behind `postgres` feature flag; all operations return `CacheError::new("postgres feature not enabled")`.
//!
//! Feature compilation (`--all-features clippy`) proves stub compilation, not backend behavior.

pub mod cache_manager;
pub mod cachelito;
pub mod moka;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "redis")]
pub mod redis;
pub mod trie;

pub use crate::cache_manager::CacheManager;
pub use crate::cachelito::CachelitoCache;
pub use crate::moka::MokaCache;
#[cfg(feature = "postgres")]
pub use crate::postgres::PostgresCache;
#[cfg(feature = "redis")]
pub use crate::redis::RedisCache;
pub use crate::trie::{
    astar_collect, bfs_collect, dfs_collect, trie_collect, trie_delete, trie_delete_prefix,
    trie_insert, AStarEntry, TrieNode,
};
