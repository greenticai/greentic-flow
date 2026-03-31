use greentic_flow::cache::disk::DiskCache;
use greentic_flow::cache::memory::MemoryCache;
use greentic_flow::cache::metadata::ArtifactMetadata;
use greentic_flow::cache::{ArtifactKey, CacheConfig, CacheManager, CpuPolicy, EngineProfile};
use std::sync::Arc;
use tempfile::tempdir;
use wasmtime::{Config, Engine};
use wasmtime::component::Component;

fn test_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model(true);
    Engine::new(&config).expect("engine")
}

fn test_profile(engine: &Engine) -> EngineProfile {
    EngineProfile::from_engine(engine, CpuPolicy::Baseline, "test-config".to_string())
}

fn test_key(profile: &EngineProfile, digest: &str) -> ArtifactKey {
    ArtifactKey::new(profile.id().to_string(), digest.to_string())
}

fn test_component(engine: &Engine) -> Arc<Component> {
    Arc::new(Component::new(engine, "(component)").expect("component"))
}

#[test]
fn disk_cache_round_trips_and_prunes_entries() {
    let engine = test_engine();
    let profile = test_profile(&engine);
    let dir = tempdir().unwrap();
    let cache = DiskCache::new(dir.path().to_path_buf(), profile.clone(), Some(4));

    let key_a = test_key(&profile, "sha256:a");
    let meta_a = ArtifactMetadata::new(&profile, key_a.wasm_digest.clone(), 3);
    cache.write_atomic(&key_a, b"abc", &meta_a).unwrap();

    let key_b = test_key(&profile, "sha256:b");
    let meta_b = ArtifactMetadata::new(&profile, key_b.wasm_digest.clone(), 3);
    cache.write_atomic(&key_b, b"xyz", &meta_b).unwrap();

    assert_eq!(cache.try_read(&key_a).unwrap(), Some(b"abc".to_vec()));
    assert_eq!(cache.artifact_count().unwrap(), 2);
    assert_eq!(cache.approx_size_bytes().unwrap(), 6);

    let dry_run = cache.prune_to_limit(true).unwrap();
    assert_eq!(dry_run.removed_entries, 1);
    assert_eq!(cache.artifact_count().unwrap(), 2);

    let pruned = cache.prune_to_limit(false).unwrap();
    assert_eq!(pruned.removed_entries, 1);
    assert_eq!(cache.artifact_count().unwrap(), 1);
}

#[test]
fn disk_cache_rejects_mismatched_metadata_and_cleans_corrupt_entries() {
    let engine = test_engine();
    let profile = test_profile(&engine);
    let dir = tempdir().unwrap();
    let cache = DiskCache::new(dir.path().to_path_buf(), profile.clone(), None);
    let key = test_key(&profile, "sha256:abc");

    let wrong_digest = ArtifactMetadata::new(&profile, "sha256:def".to_string(), 3);
    let err = cache
        .write_atomic(&key, b"abc", &wrong_digest)
        .expect_err("mismatched digest should fail");
    assert!(format!("{err}").contains("digest does not match"));

    let good_meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &good_meta).unwrap();
    let meta_path = cache.root().join("artifacts/sha256_abc.json");
    std::fs::write(&meta_path, "{not-json").unwrap();
    assert_eq!(cache.try_read(&key).unwrap(), None);
    assert!(!meta_path.exists(), "corrupt metadata should be removed");
}

#[test]
fn memory_cache_tracks_hits_misses_and_eviction_policy() {
    let engine = test_engine();
    let profile = test_profile(&engine);
    let component = test_component(&engine);

    let cache = MemoryCache::new(10, 2);
    let key_a = test_key(&profile, "sha256:a");
    let key_b = test_key(&profile, "sha256:b");

    assert!(cache.get(&key_a).is_none());
    cache.insert(key_a.clone(), Arc::clone(&component), 6, false);
    assert!(cache.get(&key_a).is_some());
    assert!(cache.get(&key_a).is_some());
    cache.insert(key_b.clone(), Arc::clone(&component), 6, false);

    let stats = cache.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 2);
    assert_eq!(stats.evictions, 1);
    assert_eq!(stats.entries, 1);
    assert!(cache.get(&key_a).is_some(), "frequently used item should be protected");
    assert!(cache.get(&key_b).is_none(), "newer item should have been evicted");
}

#[test]
fn memory_cache_can_evict_previously_pinned_items_on_second_pass() {
    let engine = test_engine();
    let profile = test_profile(&engine);
    let component = test_component(&engine);

    let cache = MemoryCache::new(5, 1);
    let key = test_key(&profile, "sha256:pinned");
    cache.insert(key.clone(), component, 6, true);

    let stats = cache.stats();
    assert_eq!(stats.evictions, 1);
    assert_eq!(stats.entries, 0);
    assert!(cache.get(&key).is_none());
}

#[tokio::test]
async fn cache_manager_reports_stats_without_compiling_components() {
    let engine = test_engine();
    let profile = test_profile(&engine);
    let dir = tempdir().unwrap();
    let manager = CacheManager::new(
        CacheConfig {
            root: dir.path().to_path_buf(),
            disk_enabled: false,
            memory_enabled: false,
            disk_max_bytes: Some(1),
            memory_max_bytes: 1,
            lfu_protect_hits: 1,
        },
        profile,
    );

    assert_eq!(manager.disk_stats().unwrap().artifact_count, 0);
    assert_eq!(manager.memory_stats().entries, 0);
    assert_eq!(manager.engine_profile_id().starts_with("sha256:"), true);

    let warm = manager.warmup(&engine, &[], greentic_flow::cache::WarmupMode::Strict).await.unwrap();
    assert_eq!(warm.warmed, 0);
    assert_eq!(manager.doctor().entries_checked, 0);

    let pruned = manager.prune_disk(false).await.unwrap();
    assert_eq!(pruned.removed_entries, 0);
    assert_eq!(manager.metrics().compiles, 0);
}
