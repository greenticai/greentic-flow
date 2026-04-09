use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::cache::keys::ArtifactKey;

#[derive(Clone, Debug, Default)]
pub struct Singleflight {
    locks: Arc<DashMap<ArtifactKey, Arc<Mutex<()>>>>,
}

impl Singleflight {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(DashMap::new()),
        }
    }

    pub async fn acquire(&self, key: ArtifactKey) -> SingleflightGuard {
        let lock = self
            .locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let guard = lock.clone().lock_owned().await;
        SingleflightGuard {
            key,
            lock,
            guard: Some(guard),
            locks: Arc::clone(&self.locks),
        }
    }
}

pub struct SingleflightGuard {
    key: ArtifactKey,
    lock: Arc<Mutex<()>>,
    guard: Option<OwnedMutexGuard<()>>,
    locks: Arc<DashMap<ArtifactKey, Arc<Mutex<()>>>>,
}

impl Drop for SingleflightGuard {
    fn drop(&mut self) {
        self.guard = None;
        if Arc::strong_count(&self.lock) == 1 {
            self.locks.remove(&self.key);
        }
    }
}
