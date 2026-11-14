#![cfg(feature = "postgres")]
//! The `Cache`-trait battery for the L3 Postgres tier.
//!
//! Two legs, both honest:
//! - **Server-free legs** — construction inlets, tier ordering, URL-parse
//!   rejection, the value contract (checked before any I/O), and transport
//!   failures surfacing as `CacheError` (never silent collapse, never
//!   `Ok(None)` ghosts) against a guaranteed-unreachable endpoint.
//! - **Live legs** — the full trait cycle against a real PostgreSQL, gated
//!   on `DAF_CACHE_PG_URL` (skipped with a recorded line when unset). CI or
//!   a local container sets the url; the battery stays green either way.

use std::sync::Arc;
use std::time::Duration;

use daf_cache::PostgresCache;
use daf_core::{Cache, Tier};

fn live_url() -> Option<String> {
    std::env::var("DAF_CACHE_PG_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
}

/// A lazy pool against a guaranteed-unreachable endpoint with a tiny acquire
/// timeout: construction succeeds, the first I/O surfaces `Err` quickly.
fn unreachable_cache() -> PostgresCache {
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(250))
        .connect_lazy("postgres://127.0.0.1:1/daf_cache_probe")
    {
        Ok(pool) => pool,
        Err(e) => panic!("probe url must parse: {e}"),
    };
    PostgresCache::with_pool(pool)
}

#[test]
fn tier_pin_l3_orders_between_l2_and_l4() {
    let cache = unreachable_cache();
    assert_eq!(cache.tier(), Tier::L3);
    assert!(
        (Tier::L2 as u8) < (Tier::L3 as u8) && (Tier::L3 as u8) < (Tier::L4 as u8),
        "L3 orders strictly between L2 and L4"
    );
}

#[test]
fn bad_url_is_a_parse_error_not_a_construction() {
    let result = PostgresCache::connect_lazy("http://127.0.0.1:1/not-a-postgres");
    assert!(result.is_err(), "a non-postgres url must be rejected");
}

#[test]
fn connect_lazy_never_touches_the_network() {
    // A parseable but unreachable url constructs eagerly and cheaply; no
    // connection is attempted until the first trait call.
    let cache = match PostgresCache::connect_lazy("postgres://127.0.0.1:1/unreachable-tier") {
        Ok(cache) => cache,
        Err(e) => panic!("lazy inlet must construct from a parseable url: {e}"),
    };
    assert_eq!(cache.tier(), Tier::L3);
}

#[tokio::test]
async fn get_surfaces_transport_failures_as_cache_error() {
    let cache = unreachable_cache();
    let result = Cache::get(&cache, "probe-key").await;
    match result {
        Err(_) => {}
        Ok(v) => panic!("an unreachable tier must error, never ghost Ok: {v:?}"),
    }
}

#[tokio::test]
async fn delete_surfaces_transport_failures_as_cache_error() {
    let cache = unreachable_cache();
    let result = Cache::delete(&cache, "probe-key").await;
    assert!(result.is_err(), "an unreachable tier must error on delete");
}

#[tokio::test]
async fn set_rejects_mismatched_values_before_any_io() {
    let cache = unreachable_cache();
    let result = Cache::set(
        &cache,
        "probe-key".to_string(),
        Arc::new(String::from("nope")),
    )
    .await;
    match result {
        Err(e) => assert!(
            e.0.contains("serialize before set"),
            "the value contract names the remedy: {e}"
        ),
        Ok(()) => panic!("a non-byte payload must be rejected"),
    }
}

#[tokio::test]
async fn set_reports_transport_failures_not_silent_success() {
    // The byte payload passes the value contract and proceeds to I/O, where
    // the unreachable endpoint surfaces the transport error — set must NOT
    // report Ok on a failed write.
    let cache = unreachable_cache();
    let result = Cache::set(&cache, "probe-key".to_string(), Arc::new(Vec::<u8>::new())).await;
    assert!(result.is_err(), "failed transport must surface as Err");
}

#[tokio::test]
async fn live_trait_battery_full_cycle() {
    let Some(url) = live_url() else {
        eprintln!("skip: DAF_CACHE_PG_URL unset; the live legs run against a real PostgreSQL");
        return;
    };
    let cache = match PostgresCache::connect(&url).await {
        Ok(cache) => cache,
        Err(e) => panic!("live leg needs a reachable url ({url}): {e}"),
    };

    // Self-cleaning start: the battery owns its key space regardless of any
    // prior aborted run.
    Cache::clear(&cache).await.unwrap();

    // get-miss on a fresh key space.
    assert!(Cache::get(&cache, "daf-l3-t-a").await.unwrap().is_none());

    // set → get with the L3 origin marker.
    Cache::set(&cache, "daf-l3-t-a".to_string(), Arc::new(b"a1".to_vec()))
        .await
        .unwrap();
    let entry = Cache::get(&cache, "daf-l3-t-a")
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(entry.origin_tier, Tier::L3);
    let bytes: Vec<u8> = *entry
        .value
        .downcast::<Vec<u8>>()
        .expect("the battery stores Vec<u8>");
    assert_eq!(bytes, b"a1".to_vec());

    // Overwrite by primary key.
    Cache::set(&cache, "daf-l3-t-a".to_string(), Arc::new(b"a2".to_vec()))
        .await
        .unwrap();
    let entry = Cache::get(&cache, "daf-l3-t-a")
        .await
        .unwrap()
        .expect("row present");
    let bytes: Vec<u8> = *entry.value.downcast::<Vec<u8>>().expect("Vec<u8> stored");
    assert_eq!(bytes, b"a2".to_vec());

    // Value contract: mismatched payloads are rejected pre-I/O.
    assert!(
        Cache::set(
            &cache,
            "daf-l3-t-a".to_string(),
            Arc::new(String::from("x"))
        )
        .await
        .is_err(),
        "non-byte payload rejected"
    );

    // delete removes exactly the key; deleting an absent key is Ok.
    Cache::delete(&cache, "daf-l3-t-a").await.unwrap();
    assert!(Cache::get(&cache, "daf-l3-t-a").await.unwrap().is_none());
    Cache::delete(&cache, "daf-l3-t-absent").await.unwrap();

    // delete_prefix is LITERAL: the caller's % and _ match themselves.
    for key in ["daf-l3-pref%one", "daf-l3-pref_one", "daf-l3-preftwo"] {
        Cache::set(&cache, key.to_string(), Arc::new(b"payload".to_vec()))
            .await
            .unwrap();
    }
    let n = Cache::delete_prefix(&cache, "daf-l3-pref%").await.unwrap();
    assert_eq!(n, 1, "the caller's '%' is literal: only pref%one goes");
    assert!(
        Cache::get(&cache, "daf-l3-pref_one")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Cache::get(&cache, "daf-l3-preftwo")
            .await
            .unwrap()
            .is_some()
    );

    // shake counts and empties the prefix class.
    let n = Cache::shake(&cache, "daf-l3-pref").await.unwrap();
    assert_eq!(n, 2, "pref_one + preftwo");
    assert!(
        Cache::get(&cache, "daf-l3-pref_one")
            .await
            .unwrap()
            .is_none()
    );

    // clear empties everything visible.
    Cache::clear(&cache).await.unwrap();
    assert!(
        Cache::get(&cache, "daf-l3-preftwo")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn live_schema_bootstrap_is_idempotent() {
    let Some(url) = live_url() else {
        eprintln!("skip: DAF_CACHE_PG_URL unset; the live legs run against a real PostgreSQL");
        return;
    };
    let cache = match PostgresCache::connect_lazy(&url) {
        Ok(cache) => cache,
        Err(e) => panic!("live leg needs a parseable url: {e}"),
    };
    cache.ensure_schema().await.unwrap();
    cache.ensure_schema().await.unwrap(); // second call must be a no-op
}
