use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use daf_application::DataAccessFactory;
use daf_cache::{CachelitoCache, GenerationRegistry};
use daf_core::{
    Algorithm, AlgorithmError, AlgorithmStats, Authorizer, Cache, JsonValue, PostInfo, Repository,
    ResourceId, UserId,
};
use daf_repository::MemoryRepository;

struct NoopAuthorizer;

#[async_trait]
impl Authorizer for NoopAuthorizer {
    async fn authorize(
        &self,
        _operation: &str,
        _resource_id: Option<&ResourceId>,
        _user: Option<&UserId>,
        _data: Option<Arc<dyn std::any::Any + Send + Sync>>,
    ) -> Result<(), daf_core::AuthorizationError> {
        Ok(())
    }
}

struct CountingAlgorithm;

#[async_trait]
impl Algorithm for CountingAlgorithm {
    async fn execute(
        &self,
        _input: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<Arc<dyn std::any::Any + Send + Sync>, AlgorithmError> {
        Ok(Arc::new(1_i64))
    }

    async fn get_stats(&self) -> Result<AlgorithmStats, AlgorithmError> {
        Ok(AlgorithmStats::new(1, 0, 1))
    }
}

#[tokio::test]
async fn test_factory_stores_dependencies() {
    let repo: Arc<dyn Repository<JsonValue>> = Arc::new(MemoryRepository::new());
    let cache: Arc<dyn Cache> = Arc::new(CachelitoCache::new());
    let gen_reg = Arc::new(GenerationRegistry::new());
    let mut algorithms: HashMap<String, Arc<dyn Algorithm>> = HashMap::new();
    algorithms.insert("counting".to_string(), Arc::new(CountingAlgorithm));
    let authorizer: Arc<dyn Authorizer> = Arc::new(NoopAuthorizer);

    let factory = DataAccessFactory::new(
        repo.clone(),
        cache.clone(),
        gen_reg.clone(),
        Some(algorithms.clone()),
        Some(authorizer.clone()),
    );
    let daf = factory.create();

    let (repo_out, cache_out, gen_out, algs_out) = daf.get_components();
    assert!(Arc::ptr_eq(&repo_out, &repo));
    assert!(Arc::ptr_eq(&cache_out, &cache));
    assert!(Arc::ptr_eq(&gen_out, &gen_reg));
    assert_eq!(algs_out.len(), 1);
    assert!(algs_out.contains_key("counting"));
}

#[tokio::test]
async fn test_factory_create_returns_usable_data_access() {
    let repo: Arc<dyn Repository<JsonValue>> = Arc::new(MemoryRepository::new());
    let cache: Arc<dyn Cache> = Arc::new(CachelitoCache::new());
    let gen_reg = Arc::new(GenerationRegistry::new());

    let factory = DataAccessFactory::new(repo, cache, gen_reg, None, None);
    let daf = factory.create();

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
