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
            _lock: lock,
            guard: Some(guard),
            locks: Arc::clone(&self.locks),
        }
    }
}

pub struct SingleflightGuard {
    key: ArtifactKey,
    _lock: Arc<Mutex<()>>,
    guard: Option<OwnedMutexGuard<()>>,
    locks: Arc<DashMap<ArtifactKey, Arc<Mutex<()>>>>,
}

impl Drop for SingleflightGuard {
    fn drop(&mut self) {
        // Release the owned guard first: it holds its own Arc clone of the lock.
        self.guard = None;
        // The map and `self.lock` account for 2 strong refs; any more means a
        // waiter (or a fresh acquirer) still uses the entry. `remove_if` checks
        // and removes atomically under the shard lock, which `acquire` also takes
        // when cloning the Arc, so a concurrent acquirer cannot be orphaned.
        self.locks
            .remove_if(&self.key, |_, lock| Arc::strong_count(lock) == 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(digest: &str) -> ArtifactKey {
        ArtifactKey::new("profile".to_string(), digest.to_string())
    }

    #[tokio::test]
    async fn entries_are_removed_after_last_guard_drops() {
        let sf = Singleflight::new();
        for i in 0..10 {
            let guard = sf.acquire(key(&format!("sha256:{i}"))).await;
            drop(guard);
        }
        assert_eq!(sf.locks.len(), 0);
    }

    #[tokio::test]
    async fn entry_survives_while_a_waiter_exists_then_is_removed() {
        let sf = Singleflight::new();
        let first = sf.acquire(key("sha256:x")).await;
        let sf2 = sf.clone();
        let waiter = tokio::spawn(async move { sf2.acquire(key("sha256:x")).await });
        // Wait until the waiter has actually cloned the entry: while `first` is
        // held the map, its `_lock` and its guard account for 3 strong refs,
        // so any more means the waiter is queued on the mutex.
        let queued = async {
            loop {
                let count = sf
                    .locks
                    .get(&key("sha256:x"))
                    .map(|entry| Arc::strong_count(entry.value()))
                    .unwrap_or(0);
                if count > 3 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), queued)
            .await
            .expect("waiter never queued on the singleflight lock");
        drop(first);
        let second = waiter.await.expect("waiter task");
        assert_eq!(
            sf.locks.len(),
            1,
            "entry must stay while second guard lives"
        );
        drop(second);
        assert_eq!(sf.locks.len(), 0);
    }
}
