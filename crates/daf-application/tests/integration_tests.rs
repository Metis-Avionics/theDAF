use std::collections::HashMap;
use std::sync::Arc;

use ::hex;
use async_trait::async_trait;
use daf_application::DataAccess;
use daf_application::DataAccessFactory;
use daf_cache::{CachelitoCache, GenerationRegistry, HierarchicalCache};
use daf_core::{
    AuthorizationError, Authorizer, Cache, DeleteInfo, Generation, JsonValue, PostInfo, PutInfo,
    QueryInfo, Repository, ResourceId, Tier, UserId,
};
use daf_repository::MemoryRepository;
use sha2::{Digest, Sha256};

struct FakeAuthorizer {
    owned: HashMap<String, String>,
}

#[async_trait]
impl Authorizer for FakeAuthorizer {
    async fn authorize(
        &self,
        _operation: &str,
        resource_id: Option<&ResourceId>,
        user: Option<&UserId>,
        _data: Option<Arc<dyn std::any::Any + Send + Sync>>,
    ) -> Result<(), AuthorizationError> {
        if user.is_none() {
            return Err(AuthorizationError::new("Unauthenticated"));
        }
        let rid = resource_id.map(|r| r.0.clone()).unwrap_or_default();
        if let Some(owner) = self.owned.get(&rid) {
            let uid = &user.unwrap().0;
            if owner != uid {
                return Err(AuthorizationError::new("Access denied"));
            }
        }
        Ok(())
    }
}

struct DenyAllAuthorizer;

#[async_trait]
impl Authorizer for DenyAllAuthorizer {
    async fn authorize(
        &self,
        _operation: &str,
        _resource_id: Option<&ResourceId>,
        _user: Option<&UserId>,
        _data: Option<Arc<dyn std::any::Any + Send + Sync>>,
    ) -> Result<(), AuthorizationError> {
        Err(AuthorizationError::new("Access denied"))
    }
}

fn make_daf(
    repo: Arc<dyn Repository<JsonValue>>,
    cache: Arc<dyn daf_core::Cache>,
    generation_registry: Arc<GenerationRegistry>,
    authorizer: Option<Arc<dyn Authorizer>>,
) -> DataAccess {
    DataAccess::new(repo, cache, generation_registry, None, authorizer)
}

async fn save(repo: &Arc<MemoryRepository<JsonValue>>, id: &str, data: HashMap<String, JsonValue>) {
    repo.save(
        &ResourceId::new(id),
        JsonValue::Object(data.into_iter().collect()),
    )
    .await
    .unwrap();
}

fn test_cache() -> Arc<dyn Cache> {
    Arc::new(CachelitoCache::new())
}

fn test_generation_registry() -> Arc<GenerationRegistry> {
    Arc::new(GenerationRegistry::new())
}

#[tokio::test]
async fn test_authorization_cache_isolation() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "user-1".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-1")),
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-1")),
        )
        .await
        .unwrap();
    assert!(r2.cache_hit);

    let err = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-2")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_put_advances_generation_and_rejects_stale_entries() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        None,
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    daf.query(
        QueryInfo {
            resource_id: ResourceId::new("123"),
            filters: None,
            algorithm: None,
        },
        None,
    )
    .await
    .unwrap();

    daf.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.success);
    assert!(!r2.cache_hit);
    assert_eq!(
        r2.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Jane".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_stale_cache_entry_rejected_after_mutation() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        None,
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    daf.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.success);
    assert!(!r2.cache_hit);
    assert_eq!(
        r2.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Jane".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_not_found_error_for_missing_resource() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo, cache, gen_reg, None);
    let err = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("nonexistent"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_post_creates_resource() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo, cache, gen_reg, None);
    let result = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.resource_id.is_some());
}

#[tokio::test]
async fn test_concurrent_mutations_generation_monotonic() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let daf1 = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        None,
    );
    let daf2 = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        None,
    );

    let r1 = daf1.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
        },
        None,
    );
    let r2 = daf2.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jack".to_string()))]),
        },
        None,
    );

    let (_res1, _res2) = tokio::join!(r1, r2);

    let gen = gen_reg.current(&ResourceId::new("123")).await;
    assert!(gen.as_u64().unwrap_or(0) >= 1);
}

#[tokio::test]
async fn test_cache_hit_reauthorizes() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "user-1".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-1")),
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-2")),
        )
        .await;
    assert!(r2.is_err());
}

#[tokio::test]
async fn test_unauthorized_user_cannot_query() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "owner".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let err = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("intruder")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_unauthorized_user_cannot_post() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(DenyAllAuthorizer);
    let daf = make_daf(repo, cache, gen_reg, Some(auth));

    let err = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
            },
            Some(&UserId::new("intruder")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_unauthorized_user_cannot_put() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "owner".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let err = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            Some(&UserId::new("intruder")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_unauthorized_user_cannot_delete() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "owner".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let err = daf
        .delete(
            DeleteInfo {
                resource_id: ResourceId::new("123"),
            },
            Some(&UserId::new("intruder")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_empty_resource_id_rejected_before_auth() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::new(),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    let err = daf
        .query(
            QueryInfo {
                resource_id: ResourceId("".to_string()),
                filters: None,
                algorithm: None,
            },
            Some(&UserId::new("user-1")),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_no_authorizer_allows_all_operations() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg, None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await;
    assert!(r.is_ok());

    let r = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            None,
        )
        .await;
    assert!(r.is_ok());

    let r = daf
        .delete(
            DeleteInfo {
                resource_id: ResourceId::new("123"),
            },
            None,
        )
        .await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn test_cache_isolation_between_different_resources() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        None,
    );

    save(
        &repo,
        "res-a",
        HashMap::from([("value".to_string(), JsonValue::String("A".to_string()))]),
    )
    .await;
    save(
        &repo,
        "res-b",
        HashMap::from([("value".to_string(), JsonValue::String("B".to_string()))]),
    )
    .await;

    let r_a = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("res-a"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r_a.success);
    assert_eq!(
        r_a.data,
        Some(JsonValue::Object(
            [("value".to_string(), JsonValue::String("A".to_string()))]
                .into_iter()
                .collect()
        ))
    );

    let r_b = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("res-b"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r_b.success);
    assert_eq!(
        r_b.data,
        Some(JsonValue::Object(
            [("value".to_string(), JsonValue::String("B".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_authorization_prevents_mutation_side_effects() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let auth: Arc<dyn Authorizer> = Arc::new(FakeAuthorizer {
        owned: HashMap::from([("123".to_string(), "owner".to_string())]),
    });
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone() as Arc<dyn daf_core::Cache>,
        gen_reg.clone(),
        Some(auth),
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let gen_before = gen_reg.current(&ResourceId::new("123")).await;

    let err = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            Some(&UserId::new("intruder")),
        )
        .await;
    assert!(err.is_err());

    let gen_after = gen_reg.current(&ResourceId::new("123")).await;
    assert_eq!(gen_before, gen_after);
}

#[tokio::test]
async fn test_post_creates_unique_resource_id() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg, None);

    let r1 = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    let r2 = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();

    assert!(r1.resource_id.is_some());
    assert!(r2.resource_id.is_some());
    assert_ne!(r1.resource_id, r2.resource_id);
}

#[tokio::test]
async fn test_query_after_successful_post_returns_data() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let factory = DataAccessFactory::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);
    let daf = factory.create();

    let post_result = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    let resource_id = post_result.resource_id.unwrap();

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: resource_id.clone(),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: resource_id.clone(),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.cache_hit);
    assert_eq!(
        r2.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("John".to_string()))]
                .into_iter()
                .collect()
        ))
    );

    daf.put(
        PutInfo {
            resource_id: resource_id.clone(),
            data: HashMap::from([("name".to_string(), JsonValue::String("Bob".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let r3 = daf
        .query(
            QueryInfo {
                resource_id,
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(!r3.cache_hit);
    assert_eq!(
        r3.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Bob".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_put_returns_conflict_on_concurrent_update() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg, None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);

    let r2 = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jack".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.success);
    assert_eq!(
        r2.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Jack".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_delete_returns_conflict_on_concurrent_update() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg, None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .delete(
            DeleteInfo {
                resource_id: ResourceId::new("123"),
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await;
    assert!(r2.is_err());
}

#[tokio::test]
async fn test_generation_advances_on_post() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg.clone(), None);

    let post_result = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    let resource_id = post_result.resource_id.unwrap();

    let gen = gen_reg.current(&resource_id).await;
    assert_eq!(gen, Generation::Valid(1));
}

#[tokio::test]
async fn test_generation_advances_on_put() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg.clone(), None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    daf.query(
        QueryInfo {
            resource_id: ResourceId::new("123"),
            filters: None,
            algorithm: None,
        },
        None,
    )
    .await
    .unwrap();

    let gen_before = gen_reg.current(&ResourceId::new("123")).await;

    daf.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let gen_after = gen_reg.current(&ResourceId::new("123")).await;
    assert_eq!(
        gen_after,
        gen_before.advance(),
        "generation must advance after put"
    );
}

#[tokio::test]
async fn test_generation_advances_on_delete() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg.clone(), None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    daf.query(
        QueryInfo {
            resource_id: ResourceId::new("123"),
            filters: None,
            algorithm: None,
        },
        None,
    )
    .await
    .unwrap();

    let gen_before = gen_reg.current(&ResourceId::new("123")).await;

    daf.delete(
        DeleteInfo {
            resource_id: ResourceId::new("123"),
        },
        None,
    )
    .await
    .unwrap();

    let gen_after = gen_reg.current(&ResourceId::new("123")).await;
    assert_eq!(
        gen_after,
        gen_before.advance(),
        "generation must advance after delete"
    );
}

#[tokio::test]
async fn test_query_with_filters_returns_matching_data() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache, gen_reg, None);

    save(
        &repo,
        "123",
        HashMap::from([
            ("name".to_string(), JsonValue::String("John".to_string())),
            (
                "age".to_string(),
                JsonValue::Number(serde_json::Number::from(30)),
            ),
        ]),
    )
    .await;

    let r = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: Some(HashMap::from([(
                    "name".to_string(),
                    JsonValue::String("John".to_string()),
                )])),
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r.success);
    assert_eq!(
        r.data,
        Some(JsonValue::Object(
            [
                ("name".to_string(), JsonValue::String("John".to_string())),
                (
                    "age".to_string(),
                    JsonValue::Number(serde_json::Number::from(30))
                ),
            ]
            .into_iter()
            .collect()
        ))
    );

    let r_filtered = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: Some(HashMap::from([(
                    "name".to_string(),
                    JsonValue::String("Jane".to_string()),
                )])),
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(r_filtered.data, Some(JsonValue::Null));
}

#[tokio::test]
async fn test_data_access_factory_creates_data_access() {
    let repo: Arc<dyn daf_core::Repository<JsonValue>> = Arc::new(MemoryRepository::new());
    let cache: Arc<dyn daf_core::Cache> = Arc::new(CachelitoCache::new());
    let gen_reg = Arc::new(GenerationRegistry::new());
    let factory = DataAccessFactory::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);
    let daf = factory.create();
    let (repo_out, cache_out, gen_out, algs_out) = daf.get_components();
    assert!(Arc::ptr_eq(&repo_out, &repo));
    assert!(Arc::ptr_eq(&cache_out, &cache));
    assert!(Arc::ptr_eq(&gen_out, &gen_reg));
    assert!(algs_out.is_empty());
}

#[tokio::test]
async fn test_post_then_query_returns_fresh_data() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let factory = DataAccessFactory::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);
    let daf = factory.create();

    let post_result = daf
        .post(
            PostInfo {
                resource_type: "user".to_string(),
                data: HashMap::from([("name".to_string(), JsonValue::String("Alice".to_string()))]),
            },
            None,
        )
        .await
        .unwrap();
    let resource_id = post_result.resource_id.unwrap();

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: resource_id.clone(),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: resource_id.clone(),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.cache_hit);

    daf.put(
        PutInfo {
            resource_id: resource_id.clone(),
            data: HashMap::from([("name".to_string(), JsonValue::String("Bob".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let r3 = daf
        .query(
            QueryInfo {
                resource_id,
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(!r3.cache_hit);
    assert_eq!(
        r3.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Bob".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn test_concurrent_queries_share_cache_hit() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf1 = DataAccess::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);
    let daf2 = DataAccess::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf1
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(!r1.cache_hit);

    let r2 = daf2
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.cache_hit);
}

#[tokio::test]
async fn test_generation_missing_initializes_to_zero_on_miss() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let factory = DataAccessFactory::new(repo.clone(), cache.clone(), gen_reg.clone(), None, None);
    let daf = factory.create();

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r.success);
    assert!(!r.cache_hit);

    let gen = gen_reg.current(&ResourceId::new("123")).await;
    assert_eq!(gen, Generation::Missing);
}

#[tokio::test]
async fn test_cache_entry_tier_l1_on_set() {
    let cache = Arc::new(CachelitoCache::new());
    cache.set("k".to_string(), Arc::new("v")).await.unwrap();
    let entry = cache.get("k").await.unwrap();
    assert_eq!(entry.unwrap().origin_tier, Tier::L1);
}

#[tokio::test]
async fn test_cache_entry_tier_from_hierarchical() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let l1 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l2 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l3 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l4 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let hierarchical = Arc::new(HierarchicalCache::new(l1, l2, l3, l4));

    let gen_reg = Arc::new(GenerationRegistry::new());
    let daf = DataAccessFactory::new(repo.clone(), hierarchical, gen_reg, None, None).create();

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r.success);
}

#[tokio::test]
async fn hierarchical_delete_prefix_cachelito_l1_returns_ok() {
    use daf_cache::{CachelitoCache, HierarchicalCache, MokaCache};

    let _repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let l1 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l2 = Arc::new(MokaCache::new(1024)) as Arc<dyn daf_core::Cache>;
    let l3 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l4 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let cache = Arc::new(HierarchicalCache::new(l1, l2, l3, l4));

    let result = cache.delete_prefix("ns:").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn generation_enum_comparison_in_query() {
    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let cache = test_cache();
    let gen_reg = test_generation_registry();
    let daf = make_daf(repo.clone(), cache.clone(), gen_reg.clone(), None);

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    let r1 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r1.success);
    assert!(!r1.cache_hit);

    daf.put(
        PutInfo {
            resource_id: ResourceId::new("123"),
            data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
        },
        None,
    )
    .await
    .unwrap();

    let namespace = hex::encode(Sha256::digest("123"));
    let mut payload = serde_json::Map::new();
    payload.insert("algorithm".to_string(), serde_json::Value::Null);
    payload.insert("filters".to_string(), serde_json::Value::Null);
    payload.insert("resource_id".to_string(), serde_json::json!("123"));
    payload.insert("user_id".to_string(), serde_json::json!("anonymous"));
    let canonical = serde_json::to_string(&payload).unwrap();
    let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
    let cache_key = format!("query:{namespace}:{digest}");

    let stale_value = serde_json::json!({
        "raw": JsonValue::Object([("name".to_string(), JsonValue::String("Stale".to_string()))].into_iter().collect()),
        "transformed": JsonValue::Object([("name".to_string(), JsonValue::String("Stale".to_string()))].into_iter().collect()),
        "generation": 1,
    });
    cache.set(cache_key, Arc::new(stale_value)).await.unwrap();

    let r2 = daf
        .query(
            QueryInfo {
                resource_id: ResourceId::new("123"),
                filters: None,
                algorithm: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(r2.success);
    assert!(!r2.cache_hit);
    assert_eq!(
        r2.data,
        Some(JsonValue::Object(
            [("name".to_string(), JsonValue::String("Jane".to_string()))]
                .into_iter()
                .collect()
        ))
    );
}

#[tokio::test]
async fn put_with_moka_l2_advances_generation_despite_cache_degradation() {
    use daf_cache::{CachelitoCache, HierarchicalCache, MokaCache};

    let repo = Arc::new(MemoryRepository::<JsonValue>::new());
    let l1 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l2 = Arc::new(MokaCache::new(1024)) as Arc<dyn daf_core::Cache>;
    let l3 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let l4 = Arc::new(CachelitoCache::new()) as Arc<dyn daf_core::Cache>;
    let cache = Arc::new(HierarchicalCache::new(l1, l2, l3, l4));
    let gen_reg = Arc::new(GenerationRegistry::new());
    let daf = make_daf(
        repo.clone() as Arc<dyn Repository<JsonValue>>,
        cache.clone(),
        gen_reg,
        None,
    );

    save(
        &repo,
        "123",
        HashMap::from([("name".to_string(), JsonValue::String("John".to_string()))]),
    )
    .await;

    daf.query(
        QueryInfo {
            resource_id: ResourceId::new("123"),
            filters: None,
            algorithm: None,
        },
        None,
    )
    .await
    .unwrap();

    let result = daf
        .put(
            PutInfo {
                resource_id: ResourceId::new("123"),
                data: HashMap::from([("name".to_string(), JsonValue::String("Jane".to_string()))]),
            },
            None,
        )
        .await;

    assert!(result.is_ok());
    let repo_data = repo.get(&ResourceId::new("123")).await.unwrap().unwrap();
    assert_eq!(
        *repo_data,
        JsonValue::Object(
            [("name".to_string(), JsonValue::String("Jane".to_string()))]
                .into_iter()
                .collect()
        )
    );
}
