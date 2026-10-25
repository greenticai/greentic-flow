use greentic_flow::cache::disk::DiskCache;
use greentic_flow::cache::metadata::ArtifactMetadata;
use greentic_flow::cache::{
    ArtifactKey, CacheConfig, CacheManager, CpuPolicy, EngineProfile, WarmupItem, WarmupMode,
};
use std::sync::Arc;
use tempfile::tempdir;
use wasmtime::Engine;

fn profile(engine: &Engine) -> EngineProfile {
    EngineProfile::from_engine(engine, CpuPolicy::Baseline, "audit".to_string())
}

#[test]
fn default_cache_root_is_never_cwd_relative() {
    let config = CacheConfig::default();
    assert!(
        config.root.is_absolute() || !config.disk_enabled,
        "default cache root must be absolute or the disk cache disabled: {}",
        config.root.display()
    );
}

#[test]
// Smoke test; the TMP_COUNTER uniqueness itself is locked by cache::disk::tmp_path_tests.
fn concurrent_writes_to_same_key_do_not_collide_on_tmp_files() {
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let cache = Arc::new(DiskCache::new(
        dir.path().to_path_buf(),
        profile.clone(),
        None,
    ));
    let key = ArtifactKey::new(profile.id().to_string(), "sha256:same".to_string());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            let profile = profile.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
                    cache.write_atomic(&key, b"abc", &meta).unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(cache.try_read(&key).unwrap(), Some(b"abc".to_vec()));
    let leftovers = std::fs::read_dir(dir.path().join("tmp"))
        .map(|rd| rd.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0, "tmp files must be cleaned up");
}

#[cfg(unix)]
#[test]
fn world_writable_cache_dir_is_not_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let cache = DiskCache::new(dir.path().to_path_buf(), profile.clone(), None);
    let key = ArtifactKey::new(profile.id().to_string(), "sha256:t".to_string());
    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &meta).unwrap();
    assert!(cache.try_read(&key).unwrap().is_some());
    let artifacts = dir.path().join("artifacts");
    std::fs::set_permissions(&artifacts, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(cache.try_read(&key).unwrap().is_none());
}

#[tokio::test]
async fn warmup_reports_uncached_items_as_skipped_not_warmed() {
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let config = CacheConfig {
        root: dir.path().to_path_buf(),
        ..CacheConfig::default()
    };
    let manager = CacheManager::new(config, profile.clone());
    let items = [WarmupItem {
        key: ArtifactKey::new(profile.id().to_string(), "sha256:missing".to_string()),
    }];
    let report = manager
        .warmup(&engine, &items, WarmupMode::BestEffort)
        .await
        .unwrap();
    assert_eq!((report.warmed, report.skipped), (0, 1));
    assert!(
        manager
            .warmup(&engine, &items, WarmupMode::Strict)
            .await
            .is_err()
    );
}
