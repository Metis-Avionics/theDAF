#![cfg(feature = "sled")]
//! The `Cache`-trait battery for the L4 disk tier (`SledCache`).
//!
//! Unlike the L3 Postgres battery's live legs, these run for real with no
//! external service: every test opens its own ephemeral sled store
//! (`temporary(true)`), so the battery exercises the actual backing storage
//! — set/get/overwrite/delete/delete_prefix/shake/clear against real pages —
//! and pins the tier's position on the ladder. The prefix semantics are
//! pinned LITERAL (sled's `scan_prefix` never treats `%`/`_` as wildcards),
//! matching the tier contract the L3 Postgres tier upholds with `ESCAPE '\'`.

use std::sync::Arc;

use daf_cache::SledCache;
use daf_core::{Cache, Tier};

fn ephemeral_cache() -> SledCache {
    let db = match sled::Config::new().temporary(true).open() {
        Ok(db) => db,
        Err(e) => panic!("ephemeral sled store must open: {e}"),
    };
    SledCache::new(db)
}

fn stored_bytes(entry: daf_core::CacheEntry) -> Vec<u8> {
    *entry
        .value
        .downcast::<Vec<u8>>()
        .expect("the battery stores Vec<u8>")
}

#[test]
fn tier_pin_l4_orders_between_l3_and_l5() {
    let cache = ephemeral_cache();
    assert_eq!(cache.tier(), Tier::L4);
    assert!(
        (Tier::L3 as u8) < (Tier::L4 as u8) && (Tier::L4 as u8) < (Tier::L5 as u8),
        "L4 orders strictly between L3 and L5"
    );
}

#[tokio::test]
async fn set_get_roundtrip_carries_the_l4_origin_marker() {
    let cache = ephemeral_cache();
    Cache::set(&cache, "s4-a".to_string(), Arc::new(b"payload-a".to_vec()))
        .await
        .unwrap();
    let entry = Cache::get(&cache, "s4-a")
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(entry.origin_tier, Tier::L4);
    assert_eq!(stored_bytes(entry), b"payload-a".to_vec());
}

#[tokio::test]
async fn get_miss_is_none() {
    let cache = ephemeral_cache();
    assert!(Cache::get(&cache, "s4-missing").await.unwrap().is_none());
}

#[tokio::test]
async fn overwrite_by_key_last_write_wins() {
    let cache = ephemeral_cache();
    Cache::set(&cache, "s4-o".to_string(), Arc::new(b"first".to_vec()))
        .await
        .unwrap();
    Cache::set(&cache, "s4-o".to_string(), Arc::new(b"second".to_vec()))
        .await
        .unwrap();
    let entry = Cache::get(&cache, "s4-o")
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(stored_bytes(entry), b"second".to_vec());
}

#[tokio::test]
async fn set_rejects_mismatched_values() {
    let cache = ephemeral_cache();
    let result = Cache::set(&cache, "s4-bad".to_string(), Arc::new(String::from("nope"))).await;
    match result {
        Err(e) => assert!(
            e.0.contains("serialize before set"),
            "the value contract names the remedy: {e}"
        ),
        Ok(()) => panic!("a non-byte payload must be rejected"),
    }
}

#[tokio::test]
async fn delete_removes_exactly_the_key_and_absent_delete_is_ok() {
    let cache = ephemeral_cache();
    Cache::set(&cache, "s4-d".to_string(), Arc::new(b"payload".to_vec()))
        .await
        .unwrap();
    Cache::delete(&cache, "s4-d").await.unwrap();
    assert!(Cache::get(&cache, "s4-d").await.unwrap().is_none());
    Cache::delete(&cache, "s4-d-absent").await.unwrap();
}

#[tokio::test]
async fn delete_prefix_is_literal_and_counts() {
    let cache = ephemeral_cache();
    for key in ["s4-p%one", "s4-p_one", "s4-ptwo", "outside"] {
        Cache::set(&cache, key.to_string(), Arc::new(b"payload".to_vec()))
            .await
            .unwrap();
    }
    // The caller's '%' is a literal byte here, not a wildcard: only
    // "s4-p%one" is in the "s4-p%" class.
    let n = Cache::delete_prefix(&cache, "s4-p%").await.unwrap();
    assert_eq!(n, 1);
    assert!(Cache::get(&cache, "s4-p_one").await.unwrap().is_some());
    assert!(Cache::get(&cache, "s4-ptwo").await.unwrap().is_some());
    assert!(Cache::get(&cache, "outside").await.unwrap().is_some());

    // A plain prefix counts every matching key and leaves the rest.
    let n = Cache::delete_prefix(&cache, "s4-p").await.unwrap();
    assert_eq!(n, 2);
    assert!(Cache::get(&cache, "outside").await.unwrap().is_some());
}

#[tokio::test]
async fn shake_counts_and_empties_the_prefix_class() {
    let cache = ephemeral_cache();
    for key in ["s4-s-one", "s4-s-two", "s4-s-three", "s4-keep"] {
        Cache::set(&cache, key.to_string(), Arc::new(b"payload".to_vec()))
            .await
            .unwrap();
    }
    let n = Cache::shake(&cache, "s4-s").await.unwrap();
    assert_eq!(n, 3);
    assert!(Cache::get(&cache, "s4-s-one").await.unwrap().is_none());
    assert!(Cache::get(&cache, "s4-keep").await.unwrap().is_some());
}

#[tokio::test]
async fn clear_empties_everything() {
    let cache = ephemeral_cache();
    for key in ["s4-c-one", "s4-c-two"] {
        Cache::set(&cache, key.to_string(), Arc::new(b"payload".to_vec()))
            .await
            .unwrap();
    }
    Cache::clear(&cache).await.unwrap();
    assert!(Cache::get(&cache, "s4-c-one").await.unwrap().is_none());
    assert!(Cache::get(&cache, "s4-c-two").await.unwrap().is_none());
}
