use std::collections::HashSet;
use std::sync::Arc;

use daf_cache::{trie::*, CachelitoCache};
use daf_core::{Cache, CacheEntry, Generation, Tier};

fn build_trie(keys: &[&str]) -> TrieNode {
    let mut root = TrieNode::default();
    for &k in keys {
        trie_insert(&mut root, k);
    }
    root
}

#[test]
fn trie_insert_and_collect() {
    let mut root = TrieNode::default();
    trie_insert(&mut root, "abc");
    trie_insert(&mut root, "abd");
    trie_insert(&mut root, "bcd");
    let keys = trie_collect(&root, "");
    assert_eq!(
        keys,
        HashSet::from(["abc".to_string(), "abd".to_string(), "bcd".to_string()])
    );
}

#[test]
fn trie_collect_prefix() {
    let root = build_trie(&["alpha", "alb", "beta", "b", "gamma"]);
    assert_eq!(
        trie_collect(&root, "al"),
        HashSet::from(["alpha".to_string(), "alb".to_string()])
    );
    assert_eq!(
        trie_collect(&root, "b"),
        HashSet::from(["beta".to_string(), "b".to_string()])
    );
    assert_eq!(trie_collect(&root, "z"), HashSet::new());
}

#[test]
fn trie_delete_removes_key() {
    let mut root = build_trie(&["abc", "abd", "bcd"]);
    trie_delete(&mut root, "abd");
    assert_eq!(
        trie_collect(&root, ""),
        HashSet::from(["abc".to_string(), "bcd".to_string()])
    );
    assert_eq!(
        trie_collect(&root, "ab"),
        HashSet::from(["abc".to_string()])
    );
}

#[test]
fn trie_delete_nonexistent_is_noop() {
    let mut root = build_trie(&["abc"]);
    trie_delete(&mut root, "xyz");
    assert_eq!(trie_collect(&root, ""), HashSet::from(["abc".to_string()]));
}

#[test]
fn trie_delete_prefix_removes_subtree() {
    let mut root = build_trie(&["ns:a:1", "ns:a:2", "ns:b:1", "other:x"]);
    let removed = trie_delete_prefix(&mut root, "ns:a:");
    assert_eq!(
        removed,
        HashSet::from(["ns:a:1".to_string(), "ns:a:2".to_string()])
    );
    assert_eq!(
        trie_collect(&root, ""),
        HashSet::from(["ns:b:1".to_string(), "other:x".to_string()])
    );
}

#[test]
fn trie_delete_prefix_empty_clears_all() {
    let mut root = build_trie(&["a", "b", "c"]);
    let removed = trie_delete_prefix(&mut root, "");
    assert_eq!(
        removed,
        HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
    );
    assert!(trie_collect(&root, "").is_empty());
}

#[test]
fn trie_delete_prefix_nonexistent_returns_empty() {
    let mut root = build_trie(&["abc"]);
    let removed = trie_delete_prefix(&mut root, "z");
    assert!(removed.is_empty());
    assert_eq!(trie_collect(&root, ""), HashSet::from(["abc".to_string()]));
}

#[test]
fn dfs_and_bfs_collect_same_set() {
    let root = build_trie(&["a", "ab", "abc", "b", "bc"]);
    assert_eq!(dfs_collect(Some(&root)), bfs_collect(&root));
}

#[test]
fn dfs_collect_none_returns_empty() {
    assert!(dfs_collect(None).is_empty());
}

#[test]
fn bfs_collect_empty_node() {
    let root = TrieNode::default();
    assert!(bfs_collect(&root).is_empty());
}

#[test]
fn astar_collect_longest_common_prefix() {
    let root = build_trie(&["xabc", "abc", "xabd", "ab"]);
    assert_eq!(
        astar_collect(&root, "abc"),
        HashSet::from(["abc".to_string()])
    );
    assert_eq!(
        astar_collect(&root, "ab"),
        HashSet::from(["ab".to_string(), "abc".to_string()])
    );
    assert_eq!(
        astar_collect(&root, "xab"),
        HashSet::from(["xabc".to_string(), "xabd".to_string()])
    );
    assert_eq!(astar_collect(&root, "z"), HashSet::new());
}

#[test]
fn astar_collect_no_mismatch_descendant_penalty() {
    let root = build_trie(&["xabc", "abc", "xabd"]);
    assert_eq!(
        astar_collect(&root, "abc"),
        HashSet::from(["abc".to_string()])
    );
}

#[tokio::test]
async fn cachelito_cache_set_get_round_trip_with_tier() {
    let cache = CachelitoCache::new();
    let value = Arc::new(42_i64) as Arc<dyn std::any::Any + Send + Sync>;
    cache
        .set(
            "key:1".to_string(),
            CacheEntry {
                value,
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    let entry = cache.get("key:1").await.unwrap();
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.origin_tier, Tier::L1);
    assert_eq!(*entry.value.downcast_ref::<i64>().unwrap(), 42);
}

#[tokio::test]
async fn cachelito_cache_delete_prefix_is_noop() {
    let cache = CachelitoCache::new();
    cache
        .set(
            "ns:a:1".to_string(),
            CacheEntry {
                value: Arc::new("v1"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "ns:a:2".to_string(),
            CacheEntry {
                value: Arc::new("v2"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "ns:b:1".to_string(),
            CacheEntry {
                value: Arc::new("v3"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    cache.delete_prefix("ns:a:").await.unwrap();

    assert!(cache.get("ns:a:1").await.unwrap().is_some());
    assert!(cache.get("ns:a:2").await.unwrap().is_some());
    assert!(cache.get("ns:b:1").await.unwrap().is_some());
}

#[tokio::test]
async fn cachelito_cache_shake_returns_zero() {
    let cache = CachelitoCache::new();
    cache
        .set(
            "ns:a:1".to_string(),
            CacheEntry {
                value: Arc::new("v1"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "ns:a:2".to_string(),
            CacheEntry {
                value: Arc::new("v2"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "other:x".to_string(),
            CacheEntry {
                value: Arc::new("v3"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    let removed = cache.shake("ns:a:").await.unwrap();
    assert_eq!(removed, 0);
    assert!(cache.get("ns:a:1").await.unwrap().is_some());
    assert!(cache.get("other:x").await.unwrap().is_some());
}

#[tokio::test]
async fn moka_delete_prefix_non_empty_returns_ok_and_clears_matching() {
    use daf_cache::MokaCache;
    let cache = MokaCache::new(1024);
    cache
        .set(
            "ns:a:1".to_string(),
            CacheEntry {
                value: Arc::new("v1"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "ns:a:2".to_string(),
            CacheEntry {
                value: Arc::new("v2"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "other:x".to_string(),
            CacheEntry {
                value: Arc::new("v3"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    let result = cache.delete_prefix("ns:").await;
    assert!(result.is_ok());
    assert!(cache.get("ns:a:1").await.unwrap().is_none());
    assert!(cache.get("ns:a:2").await.unwrap().is_none());
    assert!(cache.get("other:x").await.unwrap().is_some());
}

#[tokio::test]
async fn moka_shake_non_empty_returns_ok_and_clears_matching() {
    use daf_cache::MokaCache;
    let cache = MokaCache::new(1024);
    cache
        .set(
            "ns:a:1".to_string(),
            CacheEntry {
                value: Arc::new("v1"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "ns:a:2".to_string(),
            CacheEntry {
                value: Arc::new("v2"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();
    cache
        .set(
            "other:x".to_string(),
            CacheEntry {
                value: Arc::new("v3"),
                origin_tier: Tier::L1,
                generation: Generation::Missing,
            },
        )
        .await
        .unwrap();

    let result = cache.shake("ns:").await;
    assert!(result.is_ok());
    assert!(cache.get("ns:a:1").await.unwrap().is_none());
    assert!(cache.get("ns:a:2").await.unwrap().is_none());
    assert!(cache.get("other:x").await.unwrap().is_some());
}
