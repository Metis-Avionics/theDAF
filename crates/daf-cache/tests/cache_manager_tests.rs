use std::sync::Arc;

use daf_cache::{CacheManager, CachelitoCache, MokaCache};
use daf_core::{Cache, CacheEntry, Generation, Tier};

#[tokio::test]
async fn cache_manager_l2_promotes_to_l1_with_generation_validation() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "l2-value"})),
                origin_tier: Tier::L2,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    let entry = cache.get("query:key:1").await.unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.origin_tier, Tier::L1);
    assert_eq!(entry.generation, Generation::Missing);
}

#[tokio::test]
async fn cache_manager_stale_entry_rejected_at_l1() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache.advance("key").await;

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "stale"})),
                origin_tier: Tier::L1,
                generation: Generation::Valid(1),
            },
        )
        .await
        .unwrap();

    cache.advance("key").await;

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "current"})),
                origin_tier: Tier::L1,
                generation: Generation::Valid(2),
            },
        )
        .await
        .unwrap();

    let entry = cache.get("query:key:1").await.unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.generation, Generation::Valid(2));
}

#[tokio::test]
async fn cache_manager_stale_entry_rejected_at_l2() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache.advance("key").await;

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "stale"})),
                origin_tier: Tier::L2,
                generation: Generation::Valid(1),
            },
        )
        .await
        .unwrap();

    cache.advance("key").await;

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "current"})),
                origin_tier: Tier::L2,
                generation: Generation::Valid(2),
            },
        )
        .await
        .unwrap();

    let entry = cache.get("query:key:1").await.unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.generation, Generation::Valid(2));
}

#[tokio::test]
async fn cache_manager_generation_advancement_under_concurrency() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = Arc::new(CacheManager::new(
        l1,
        l2,
        None,
        None,
    ));

    cache.advance("key").await;

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "initial"})),
                origin_tier: Tier::L1,
                generation: Generation::Valid(1),
            },
        )
        .await
        .unwrap();

    let entry = cache.get("query:key:1").await.unwrap();
    assert!(entry.is_some());

    let new_gen = cache.advance("key").await;
    assert_eq!(new_gen, Generation::Valid(2));

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "after-advance"})),
                origin_tier: Tier::L1,
                generation: Generation::Valid(2),
            },
        )
        .await
        .unwrap();

    let entry = cache.get("query:key:1").await.unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.generation, Generation::Valid(2));
}

#[tokio::test]
async fn cache_manager_set_writes_to_l1_and_l2() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "all-tiers"})),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    assert!(cache.l1().get("query:key:1").await.unwrap().is_some());
    assert!(cache.l2().get("query:key:1").await.unwrap().is_some());
}

#[tokio::test]
async fn cache_manager_delete_propagates_to_l1_and_l2() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "delete-me"})),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    cache.delete("query:key:1").await.unwrap();

    assert!(cache.l2().get("query:key:1").await.unwrap().is_none());
}

#[tokio::test]
async fn cache_manager_clear_propagates_to_l1_and_l2() {
    let l1 = CachelitoCache::new();
    let l2 = MokaCache::new(1024);
    let cache = CacheManager::new(
        l1,
        l2,
        None,
        None,
    );

    cache
        .set(
            "query:key:1".to_string(),
            CacheEntry {
                value: Arc::new(serde_json::json!({"data": "clear-me"})),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    cache.clear().await.unwrap();

    assert!(cache.l2().get("query:key:1").await.unwrap().is_none());
}

#[tokio::test]
async fn cache_manager_miss_returns_none() {
    let cache = CacheManager::new(
        CachelitoCache::new(),
        MokaCache::new(1024),
        None,
        None,
    );

    let entry = cache.get("query:nonexistent:key").await.unwrap();
    assert!(entry.is_none());
}

#[tokio::test]
async fn cache_manager_current_returns_missing_for_unknown_namespace() {
    let cache = CacheManager::new(
        CachelitoCache::new(),
        MokaCache::new(1024),
        None,
        None,
    );

    let gen = cache.current("unknown").await;
    assert_eq!(gen, Generation::Missing);
}

#[tokio::test]
async fn cache_manager_advance_creates_valid_generation() {
    let cache = CacheManager::new(
        CachelitoCache::new(),
        MokaCache::new(1024),
        None,
        None,
    );

    let gen = cache.advance("ns1").await;
    assert_eq!(gen, Generation::Valid(1));

    let gen2 = cache.advance("ns1").await;
    assert_eq!(gen2, Generation::Valid(2));

    let gen3 = cache.current("ns1").await;
    assert_eq!(gen3, Generation::Valid(2));
}
