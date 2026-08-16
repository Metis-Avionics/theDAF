use std::collections::HashMap;

use daf_core::{
    AlgorithmStats, DeleteInfo, Generation, PostInfo, PutInfo, QueryInfo, QueryResult, ResourceId,
    UserId,
};
use serde_json::json;

#[test]
fn resource_id_round_trip() {
    let id = ResourceId::new("res-123");
    assert_eq!(id.to_string(), "res-123");
}

#[test]
fn user_id_round_trip() {
    let uid = UserId::new("user-1");
    assert_eq!(uid.to_string(), "user-1");
}

#[test]
fn query_info_serde_round_trip() {
    let mut filters = HashMap::new();
    filters.insert("status".to_string(), json!("active"));
    let info = QueryInfo {
        resource_id: ResourceId::new("123"),
        filters: Some(filters),
        algorithm: Some("fib".to_string()),
    };
    let json = serde_json::to_value(&info).unwrap();
    let decoded: QueryInfo = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.resource_id.0, "123");
    assert_eq!(decoded.algorithm.as_deref(), Some("fib"));
}

#[test]
fn query_info_empty_defaults() {
    let info = QueryInfo {
        resource_id: ResourceId::new("123"),
        filters: None,
        algorithm: None,
    };
    let json = serde_json::to_value(&info).unwrap();
    let decoded: QueryInfo = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.resource_id.0, "123");
    assert!(decoded.filters.is_none());
    assert!(decoded.algorithm.is_none());
}

#[test]
fn post_info_serde_round_trip() {
    let mut data = HashMap::new();
    data.insert("name".to_string(), json!("John"));
    let info = PostInfo {
        resource_type: "user".to_string(),
        data,
    };
    let json = serde_json::to_value(&info).unwrap();
    let decoded: PostInfo = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.resource_type, "user");
}

#[test]
fn put_info_serde_round_trip() {
    let mut data = HashMap::new();
    data.insert("name".to_string(), json!("Jane"));
    let info = PutInfo {
        resource_id: ResourceId::new("123"),
        data,
    };
    let json = serde_json::to_value(&info).unwrap();
    let decoded: PutInfo = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.resource_id.0, "123");
}

#[test]
fn delete_info_serde_round_trip() {
    let info = DeleteInfo {
        resource_id: ResourceId::new("123"),
    };
    let json = serde_json::to_value(&info).unwrap();
    let decoded: DeleteInfo = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.resource_id.0, "123");
}

#[test]
fn query_result_contains_expected_fields() {
    let result = QueryResult {
        success: true,
        data: Some(json!("hello")),
        error: None,
        error_type: None,
        cache_hit: false,
        algorithm_stats: None,
        timestamp: chrono::Utc::now(),
    };
    assert!(result.success);
    assert!(!result.cache_hit);
}

#[test]
fn mutation_result_contains_expected_fields() {
    let result = daf_core::MutationResult {
        success: true,
        resource_id: Some(ResourceId::new("123")),
        data: Some(json!({"id": "123"})),
        error: None,
        error_type: None,
        timestamp: chrono::Utc::now(),
    };
    assert!(result.success);
    assert_eq!(result.resource_id.unwrap().0, "123");
}

#[test]
fn query_result_serde_round_trip() {
    let result = QueryResult {
        success: true,
        data: Some(json!("hello")),
        error: None,
        error_type: None,
        cache_hit: true,
        algorithm_stats: None,
        timestamp: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&result).unwrap();
    let decoded: QueryResult = serde_json::from_value(json).unwrap();
    assert!(decoded.success);
    assert!(decoded.cache_hit);
}

#[test]
fn mutation_result_serde_round_trip() {
    let result = daf_core::MutationResult {
        success: true,
        resource_id: Some(ResourceId::new("123")),
        data: Some(json!({"name": "John"})),
        error: None,
        error_type: None,
        timestamp: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&result).unwrap();
    let decoded: daf_core::MutationResult = serde_json::from_value(json).unwrap();
    assert!(decoded.success);
    assert_eq!(decoded.resource_id.unwrap().0, "123");
}

#[test]
fn algorithm_stats_defaults_and_accessors() {
    let stats = AlgorithmStats::new(10, 2, 5);
    assert_eq!(stats.iterations, 10);
    assert_eq!(stats.cache_hits, 2);
    assert_eq!(stats.memo_size, 5);
}

#[test]
fn algorithm_stats_serde_round_trip() {
    let stats = AlgorithmStats::new(10, 2, 5);
    let json = serde_json::to_value(&stats).unwrap();
    let decoded: AlgorithmStats = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.iterations, 10);
    assert_eq!(decoded.cache_hits, 2);
    assert_eq!(decoded.memo_size, 5);
}

#[test]
fn generation_advancement() {
    assert_eq!(Generation::Missing.advance(), Generation::Valid(1));
    assert_eq!(Generation::Valid(1).advance(), Generation::Valid(2));
    assert_eq!(Generation::Valid(0).advance(), Generation::Valid(1));
}

#[test]
fn generation_as_u64() {
    assert_eq!(Generation::Missing.as_u64(), None);
    assert_eq!(Generation::Valid(7).as_u64(), Some(7));
}

#[test]
fn generation_missing_serde_round_trip() {
    let gen = Generation::Missing;
    let json = serde_json::to_value(gen).unwrap();
    let decoded: Generation = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, Generation::Missing);
}

#[test]
fn generation_valid_serde_round_trip() {
    let gen = Generation::Valid(42);
    let json = serde_json::to_value(gen).unwrap();
    let decoded: Generation = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, Generation::Valid(42));
}
