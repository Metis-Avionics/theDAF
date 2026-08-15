use std::collections::HashMap;
use std::sync::Arc;

use daf_core::Repository;
use daf_core::{RepositoryError, ResourceId};

#[derive(Debug, Clone, Default)]
pub struct MemoryRepository<T> {
    store: Arc<tokio::sync::RwLock<HashMap<String, T>>>,
}

impl<T: Clone + Send + Sync + 'static + serde::Serialize + std::fmt::Debug> MemoryRepository<T> {
    pub fn new() -> Self {
        Self {
            store: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl<T: Clone + Send + Sync + 'static + serde::Serialize + std::fmt::Debug> Repository<T>
    for MemoryRepository<T>
{
    async fn get(&self, key: &ResourceId) -> Result<Option<Arc<T>>, RepositoryError> {
        let store = self.store.read().await;
        let value = store.get(&key.0).cloned();
        Ok(value.map(Arc::new))
    }

    async fn save(&self, key: &ResourceId, value: T) -> Result<(), RepositoryError> {
        let mut store = self.store.write().await;
        store.insert(key.0.clone(), value);
        Ok(())
    }

    async fn delete(&self, key: &ResourceId) -> Result<(), RepositoryError> {
        let mut store = self.store.write().await;
        store.remove(&key.0);
        Ok(())
    }

    async fn create(&self, value: T) -> Result<ResourceId, RepositoryError> {
        let resource_id = ResourceId::new(ulid::Ulid::generate().to_string());
        let mut store = self.store.write().await;
        store.insert(resource_id.0.clone(), value);
        Ok(resource_id)
    }

    async fn try_update(
        &self,
        key: &ResourceId,
        expected: &T,
        update: Box<dyn FnOnce(T) -> T + Send + 'static>,
    ) -> Result<Option<T>, RepositoryError> {
        let mut store = self.store.write().await;
        let current = match store.get(&key.0) {
            Some(v) => v,
            None => return Ok(None),
        };

        if !Self::values_equal(current, expected) {
            return Ok(None);
        }

        let new_value = update(current.clone());
        store.insert(key.0.clone(), new_value.clone());
        Ok(Some(new_value))
    }

    async fn try_delete(&self, key: &ResourceId, expected: &T) -> Result<bool, RepositoryError> {
        let mut store = self.store.write().await;
        let current = match store.get(&key.0) {
            Some(v) => v,
            None => return Ok(false),
        };

        if !Self::values_equal(current, expected) {
            return Ok(false);
        }

        store.remove(&key.0);
        Ok(true)
    }
}

impl<T: Clone + Send + Sync + 'static + serde::Serialize + std::fmt::Debug> MemoryRepository<T> {
    fn values_equal(a: &T, b: &T) -> bool {
        if let (Some(a_json), Some(b_json)) =
            (serde_json::to_value(a).ok(), serde_json::to_value(b).ok())
        {
            return a_json == b_json;
        }
        format!("{:?}", a) == format!("{:?}", b)
    }
}
