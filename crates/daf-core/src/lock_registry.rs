use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex;

const NUM_STRIPES: usize = 16;

#[derive(Debug)]
pub struct LockRegistry {
    stripes: [Mutex<()>; NUM_STRIPES],
    next_id: AtomicU64,
}

impl LockRegistry {
    pub fn new() -> Self {
        Self {
            stripes: std::array::from_fn(|_| Mutex::new(())),
            next_id: AtomicU64::new(0),
        }
    }

    pub fn global() -> &'static Self {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<LockRegistry> = OnceLock::new();
        let registry = INSTANCE.get_or_init(LockRegistry::new);
        debug_assert!(INSTANCE.get().is_some(), "LockRegistry singleton must be initialized");
        registry
    }

    fn stripe_index(&self, resource_id: &str) -> usize {
        debug_assert!(!resource_id.is_empty(), "resource_id must not be empty");
        let mut hasher = DefaultHasher::new();
        resource_id.hash(&mut hasher);
        (hasher.finish() as usize) % NUM_STRIPES
    }

    pub async fn acquire(&self, resource_id: &str) -> LockGuard<'_> {
        debug_assert!(!resource_id.is_empty(), "resource_id must not be empty");
        let idx = self.stripe_index(resource_id);
        let guard = self.stripes[idx].lock().await;
        LockGuard {
            _guard: guard,
            _id: self.next_id.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl Default for LockRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LockGuard<'a> {
    _guard: tokio::sync::MutexGuard<'a, ()>,
    _id: u64,
}

impl fmt::Debug for LockGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockGuard").field("id", &self._id).finish()
    }
}
