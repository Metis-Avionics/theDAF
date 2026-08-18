use std::sync::Arc;

use daf_core::{Generation, ResourceId};
use dashmap::DashMap;

pub struct GenerationRegistry {
    inner: Arc<DashMap<ResourceId, Generation>>,
}

impl GenerationRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }
}

impl Default for GenerationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl GenerationRegistry {
    pub async fn current(&self, resource_id: &ResourceId) -> Generation {
        debug_assert!(
            !resource_id.0.is_empty(),
            "resource_id must not be empty for current"
        );
        self.inner
            .get(resource_id)
            .map(|entry| *entry)
            .unwrap_or(Generation::Missing)
    }

    pub async fn advance(&self, resource_id: &ResourceId) -> Generation {
        debug_assert!(
            !resource_id.0.is_empty(),
            "resource_id must not be empty for advance"
        );
        let mut entry = self
            .inner
            .entry(resource_id.clone())
            .or_insert(Generation::Missing);
        let next = entry.advance();
        *entry = next;
        next
    }
}
