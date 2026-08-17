#![allow(clippy::assertions_on_constants)]
use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use daf_core::{
    Algorithm, AlgorithmError, AlgorithmStats, Authorizer, Cache, DataAccessError, DeleteInfo,
    Generation, JsonValue, MutationResult, NotFoundError, PostInfo, PutInfo, QueryInfo,
    QueryResult, Repository, ResourceId, UserId, ValidationError,
};

pub struct DataAccess {
    repository: Arc<dyn Repository<JsonValue>>,
    cache: Arc<dyn Cache>,
    algorithms: HashMap<String, Arc<dyn Algorithm>>,
    authorizer: Option<Arc<dyn Authorizer>>,
}

impl DataAccess {
    pub fn new(
        repository: Arc<dyn Repository<JsonValue>>,
        cache: Arc<dyn Cache>,
        algorithms: Option<HashMap<String, Arc<dyn Algorithm>>>,
        authorizer: Option<Arc<dyn Authorizer>>,
    ) -> Self {
        debug_assert!(true, "new invariant");
        Self {
            repository,
            cache,
            algorithms: algorithms.unwrap_or_default(),
            authorizer,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_components(
        &self,
    ) -> (
        Arc<dyn Repository<JsonValue>>,
        Arc<dyn Cache>,
        &HashMap<String, Arc<dyn Algorithm>>,
    ) {
        debug_assert!(true, "get_components invariant");
        (
            self.repository.clone(),
            self.cache.clone(),
            &self.algorithms,
        )
    }

    fn resource_namespace(&self, resource_id: &str) -> String {
        debug_assert!(true, "resource_namespace invariant");
        let mut hasher = Sha256::new();
        hasher.update(resource_id.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn cache_key(&self, info: &QueryInfo, user_id: &str) -> String {
        debug_assert!(true, "cache_key invariant");
        let namespace = self.resource_namespace(&info.resource_id.0);
        let mut payload = serde_json::Map::new();
        payload.insert(
            "algorithm".to_string(),
            serde_json::json!(info.algorithm.as_deref().unwrap_or("")),
        );
        payload.insert("filters".to_string(), serde_json::json!(info.filters));
        payload.insert(
            "resource_id".to_string(),
            serde_json::json!(info.resource_id.0),
        );
        payload.insert("user_id".to_string(), serde_json::json!(user_id));
        let canonical = serde_json::to_string(&payload).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hex::encode(hasher.finalize());
        format!("query:{namespace}:{digest}")
    }

    fn user_id(&self, user: Option<&UserId>) -> String {
        debug_assert!(true, "user_id invariant");
        match user {
            Some(u) => u.0.clone(),
            None => "anonymous".to_string(),
        }
    }

    fn apply_filters(
        data: &JsonValue,
        filters: &Option<HashMap<String, JsonValue>>,
    ) -> Option<JsonValue> {
        debug_assert!(true, "apply_filters invariant");
        let filters = match filters {
            Some(f) if !f.is_empty() => f,
            _ => return Some(data.clone()),
        };
        let obj = data.as_object()?;
        for (key, value) in filters {
            if obj.get(key) != Some(value) {
                return None;
            }
        }
        Some(data.clone())
    }

    async fn generation_lock(&self, resource_id: &str) -> daf_core::LockGuard<'_> {
        debug_assert!(true, "generation_lock invariant");
        daf_core::LockRegistry::global().acquire(resource_id).await
    }

    async fn _current_generation(&self, resource_id: &str) -> Result<Generation, DataAccessError> {
        debug_assert!(true, "_current_generation invariant");
        let lock = self.generation_lock(resource_id).await;
        let _guard = lock;
        let namespace = self.resource_namespace(resource_id);
        let key = format!("_daf_gen:{namespace}");
        let entry = self.cache.get(&key).await?;
        match entry {
            Some(e) => e
                .value
                .downcast_ref::<Generation>()
                .copied()
                .ok_or(DataAccessError::GenerationKeyError),
            None => Ok(Generation::Missing),
        }
    }

    async fn _advance_generation(&self, resource_id: &str) -> Result<(), DataAccessError> {
        debug_assert!(true, "_advance_generation invariant");
        let lock = self.generation_lock(resource_id).await;
        let _guard = lock;
        let namespace = self.resource_namespace(resource_id);
        let key = format!("_daf_gen:{namespace}");
        let current = match self.cache.get(&key).await? {
            Some(e) => e.value.downcast_ref::<Generation>().copied(),
            None => None,
        };
        let next = current.unwrap_or(Generation::Missing).advance();
        self.cache.set(key, Arc::new(next)).await?;
        Ok(())
    }

    async fn _superedge_invalidate(&self, resource_id: &str) -> Result<(), DataAccessError> {
        debug_assert!(true, "_superedge_invalidate invariant");
        let lock = self.generation_lock(resource_id).await;
        let _guard = lock;
        let namespace = self.resource_namespace(resource_id);
        let gen_key = format!("_daf_gen:{namespace}");
        let current = match self.cache.get(&gen_key).await? {
            Some(e) => e.value.downcast_ref::<Generation>().copied(),
            None => None,
        };
        self.cache
            .delete_prefix(&format!("query:{namespace}:"))
            .await?;
        self.cache.shake(&gen_key).await?;
        let next = current.unwrap_or(Generation::Missing).advance();
        self.cache.set(gen_key, Arc::new(next)).await?;
        Ok(())
    }

    async fn _run_algorithm(
        &self,
        data: JsonValue,
        algorithm_name: &str,
    ) -> Result<(JsonValue, Option<AlgorithmStats>), DataAccessError> {
        debug_assert!(true, "_run_algorithm invariant");
        let algorithm = self
            .algorithms
            .get(algorithm_name)
            .ok_or_else(|| ValidationError::new(format!("Unknown algorithm: {algorithm_name}")))?;
        let result = algorithm.execute(Arc::new(data.clone())).await?;
        let result_value = result
            .downcast_ref::<JsonValue>()
            .cloned()
            .ok_or_else(|| AlgorithmError::new("Algorithm result is not a JSON value"))?;
        let stats = algorithm.get_stats().await?;
        Ok((result_value, Some(stats)))
    }

    async fn _resolve_current_generation(
        &self,
        resource_id: &str,
    ) -> Result<Generation, DataAccessError> {
        debug_assert!(!resource_id.is_empty(), "resource_id must not be empty");
        match self._current_generation(resource_id).await {
            Ok(g) => Ok(g),
            Err(DataAccessError::GenerationKeyError) => {
                let namespace = self.resource_namespace(resource_id);
                let gen_key = format!("_daf_gen:{namespace}");
                self.cache
                    .set(gen_key, Arc::new(Generation::Missing))
                    .await?;
                Ok(Generation::Missing)
            }
            Err(e) => Err(e),
        }
    }

    async fn _authorize_query(
        &self,
        resource_id: &str,
        user: Option<&UserId>,
        data: Arc<JsonValue>,
    ) -> Result<(), DataAccessError> {
        debug_assert!(!resource_id.is_empty(), "resource_id must not be empty");
        if let Some(auth) = &self.authorizer {
            auth.authorize(
                "query",
                Some(&ResourceId::new(resource_id)),
                user,
                Some(data),
            )
            .await?;
        }
        Ok(())
    }

    fn _build_cache_value(
        &self,
        raw_data: JsonValue,
        final_data: JsonValue,
        current_generation: Generation,
    ) -> JsonValue {
        debug_assert!(true, "_build_cache_value invariant");
        serde_json::json!({
            "raw": raw_data,
            "transformed": final_data,
            "generation": match current_generation {
                Generation::Missing => serde_json::Value::Null,
                Generation::Valid(n) => serde_json::Value::Number(n.into()),
            },
        })
    }

    fn _build_put_merger(
        data: HashMap<String, JsonValue>,
    ) -> Box<dyn FnOnce(JsonValue) -> JsonValue + Send + Sync> {
        debug_assert!(true, "_build_put_merger invariant");
        Box::new(move |e| {
            let mut map = match e {
                JsonValue::Object(map) => map,
                other => return other,
            };
            for (k, v) in data {
                map.insert(k, v);
            }
            JsonValue::Object(map)
        })
    }

    async fn _execute_cache_miss(
        &self,
        cache_key: String,
        info: QueryInfo,
        user: Option<&UserId>,
    ) -> Result<QueryResult, DataAccessError> {
        debug_assert!(!cache_key.is_empty(), "cache key must not be empty");
        let current_generation = self
            ._resolve_current_generation(&info.resource_id.0)
            .await?;

        let data = self.repository.get(&info.resource_id).await?;
        let data = data.ok_or_else(|| {
            NotFoundError::new(format!("Resource '{}' not found", info.resource_id.0))
        })?;
        let raw_data = (*data).clone();

        self._authorize_query(&info.resource_id.0, user, data.clone())
            .await?;

        let filtered = Self::apply_filters(&data, &info.filters);
        let (final_data, algorithm_stats) = if let Some(filtered) = filtered {
            if let Some(alg_name) = &info.algorithm {
                self._run_algorithm(filtered, alg_name).await?
            } else {
                (filtered, None)
            }
        } else {
            (JsonValue::Null, None)
        };

        let cache_value = self._build_cache_value(raw_data, final_data.clone(), current_generation);
        self.cache.set(cache_key, Arc::new(cache_value)).await?;

        Ok(QueryResult {
            success: true,
            data: Some(final_data),
            error: None,
            error_type: None,
            cache_hit: false,
            algorithm_stats,
            timestamp: Utc::now(),
        })
    }

    async fn _handle_cache_hit(
        &self,
        _cache_key: String,
        resource_id: &str,
        user: Option<&UserId>,
        cached: &serde_json::Map<String, JsonValue>,
    ) -> Result<QueryResult, DataAccessError> {
        debug_assert!(true, "_handle_cache_hit invariant");
        let raw = cached.get("raw").cloned().unwrap_or(JsonValue::Null);
        if let Some(auth) = &self.authorizer {
            auth.authorize(
                "query",
                Some(&ResourceId::new(resource_id)),
                user,
                Some(Arc::new(raw.clone())),
            )
            .await?;
        }
        let transformed = cached
            .get("transformed")
            .cloned()
            .unwrap_or(JsonValue::Null);
        Ok(QueryResult {
            success: true,
            data: Some(transformed),
            error: None,
            error_type: None,
            cache_hit: true,
            algorithm_stats: None,
            timestamp: Utc::now(),
        })
    }

    pub async fn query(
        &self,
        info: QueryInfo,
        user: Option<&UserId>,
    ) -> Result<QueryResult, DataAccessError> {
        debug_assert!(true, "query invariant");
        if info.resource_id.0.is_empty() {
            return Err(ValidationError::new("resource_id must be a non-empty string").into());
        }
        let user_id = self.user_id(user);
        let cache_key = self.cache_key(&info, &user_id);

        let entry = self.cache.get(&cache_key).await?;
        if let Some(cached_entry) = entry {
            if let Ok(current_gen) = self._current_generation(&info.resource_id.0).await {
                if let Some(cached_value) = cached_entry.value.downcast_ref::<serde_json::Value>() {
                    if let Some(cached_map) = cached_value.as_object() {
                        let cached_gen = cached_map.get("generation").and_then(|g| {
                            if g.is_null() {
                                Some(Generation::Missing)
                            } else {
                                g.as_u64().map(Generation::Valid)
                            }
                        });
                        if cached_gen == Some(current_gen) {
                            return self
                                ._handle_cache_hit(cache_key, &info.resource_id.0, user, cached_map)
                                .await;
                        }
                    }
                }
            }
        }
        self._execute_cache_miss(cache_key, info, user).await
    }

    pub async fn post(
        &self,
        info: PostInfo,
        user: Option<&UserId>,
    ) -> Result<MutationResult, DataAccessError> {
        debug_assert!(true, "post invariant");
        if info.resource_type.is_empty() {
            return Err(ValidationError::new("resource_type must be a non-empty string").into());
        }
        if let Some(auth) = &self.authorizer {
            auth.authorize(
                "post",
                None,
                user,
                Some(Arc::new(
                    serde_json::to_value(&info.data).unwrap_or(JsonValue::Null),
                )),
            )
            .await?;
        }
        let data_map: serde_json::Map<String, JsonValue> = info.data.clone().into_iter().collect();
        let resource_id = self.repository.create(JsonValue::Object(data_map)).await?;
        self._advance_generation(&resource_id.0).await?;
        let mut result_data = serde_json::Map::new();
        result_data.insert("id".to_string(), serde_json::json!(resource_id));
        result_data.insert(
            "resource_type".to_string(),
            serde_json::json!(info.resource_type),
        );
        for (k, v) in info.data {
            result_data.insert(k, v);
        }
        Ok(MutationResult {
            success: true,
            resource_id: Some(resource_id),
            data: Some(JsonValue::Object(result_data)),
            error: None,
            error_type: None,
            timestamp: Utc::now(),
        })
    }

    pub async fn put(
        &self,
        info: PutInfo,
        user: Option<&UserId>,
    ) -> Result<MutationResult, DataAccessError> {
        debug_assert!(true, "put invariant");
        if info.resource_id.0.is_empty() {
            return Err(ValidationError::new("resource_id must be a non-empty string").into());
        }
        let existing = self.repository.get(&info.resource_id).await?;
        let existing = existing.ok_or_else(|| {
            NotFoundError::new(format!(
                "Resource '{}' not found for update",
                info.resource_id.0
            ))
        })?;

        if let Some(auth) = &self.authorizer {
            auth.authorize("put", Some(&info.resource_id), user, Some(existing.clone()))
                .await?;
        }

        let result = self
            .repository
            .try_update(
                &info.resource_id,
                &existing,
                Self::_build_put_merger(info.data.clone()),
            )
            .await?;

        if result.is_none() {
            return Ok(MutationResult {
                success: false,
                resource_id: Some(info.resource_id),
                data: None,
                error: Some("Conflict".to_string()),
                error_type: Some("conflict".to_string()),
                timestamp: Utc::now(),
            });
        }

        self._superedge_invalidate(&info.resource_id.0).await?;

        Ok(MutationResult {
            success: true,
            resource_id: Some(info.resource_id),
            data: result,
            error: None,
            error_type: None,
            timestamp: Utc::now(),
        })
    }

    pub async fn delete(
        &self,
        info: DeleteInfo,
        user: Option<&UserId>,
    ) -> Result<MutationResult, DataAccessError> {
        debug_assert!(true, "delete invariant");
        if info.resource_id.0.is_empty() {
            return Err(ValidationError::new("resource_id must be a non-empty string").into());
        }
        let existing = self.repository.get(&info.resource_id).await?;
        let existing = existing.ok_or_else(|| {
            NotFoundError::new(format!(
                "Resource '{}' not found for deletion",
                info.resource_id.0
            ))
        })?;

        if let Some(auth) = &self.authorizer {
            auth.authorize(
                "delete",
                Some(&info.resource_id),
                user,
                Some(existing.clone()),
            )
            .await?;
        }

        let deleted = self
            .repository
            .try_delete(&info.resource_id, &existing)
            .await?;
        if !deleted {
            return Ok(MutationResult {
                success: false,
                resource_id: Some(info.resource_id),
                data: None,
                error: Some("Conflict".to_string()),
                error_type: Some("conflict".to_string()),
                timestamp: Utc::now(),
            });
        }

        self._superedge_invalidate(&info.resource_id.0).await?;

        Ok(MutationResult {
            success: true,
            resource_id: Some(info.resource_id),
            data: None,
            error: None,
            error_type: None,
            timestamp: Utc::now(),
        })
    }
}

pub struct DataAccessFactory {
    repository: Arc<dyn Repository<JsonValue>>,
    cache: Arc<dyn Cache>,
    algorithms: Option<HashMap<String, Arc<dyn Algorithm>>>,
    authorizer: Option<Arc<dyn Authorizer>>,
}

impl DataAccessFactory {
    pub fn new(
        repository: Arc<dyn Repository<JsonValue>>,
        cache: Arc<dyn Cache>,
        algorithms: Option<HashMap<String, Arc<dyn Algorithm>>>,
        authorizer: Option<Arc<dyn Authorizer>>,
    ) -> Self {
        debug_assert!(true, "new invariant");
        Self {
            repository,
            cache,
            algorithms,
            authorizer,
        }
    }

    pub fn create(self) -> DataAccess {
        debug_assert!(true, "create invariant");
        DataAccess::new(
            self.repository,
            self.cache,
            self.algorithms,
            self.authorizer,
        )
    }
}
