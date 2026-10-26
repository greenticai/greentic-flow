#[cfg(unix)]
use greentic_flow::cache::disk::DiskCache;
#[cfg(unix)]
use greentic_flow::cache::metadata::ArtifactMetadata;
#[cfg(unix)]
use greentic_flow::cache::{ArtifactKey, CpuPolicy, EngineProfile};
#[cfg(unix)]
use tempfile::tempdir;
#[cfg(unix)]
use wasmtime::Engine;

#[cfg(unix)]
fn setup() -> (tempfile::TempDir, DiskCache, ArtifactKey) {
    let engine = Engine::default();
    let profile = EngineProfile::from_engine(&engine, CpuPolicy::Baseline, "r11".to_string());
    let dir = tempdir().expect("tempdir");
    let cache = DiskCache::new(dir.path().to_path_buf(), profile.clone(), None);
    let key = ArtifactKey::new(profile.id().to_string(), "sha256:t".to_string());
    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &meta).expect("write");
    (dir, cache, key)
}

#[cfg(unix)]
#[test]
fn written_entries_are_owner_only_and_readable() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, cache, key) = setup();
    assert!(cache.try_read(&key).expect("read").is_some());
    for entry in std::fs::read_dir(dir.path().join("artifacts")).expect("dir") {
        let mode = entry
            .expect("entry")
            .metadata()
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "entry must be owner-only: {mode:o}");
    }
    // A second hit rewrites the metadata; it must stay readable.
    assert!(cache.try_read(&key).expect("read").is_some());
}

#[cfg(unix)]
#[test]
fn group_writable_entry_in_repaired_dir_is_not_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, cache, key) = setup();
    let artifacts = dir.path().join("artifacts");
    for entry in std::fs::read_dir(&artifacts).expect("dir") {
        let path = entry.expect("entry").path();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).expect("chmod");
    }
    assert!(cache.try_read(&key).expect("read").is_none());
}

#[test]
fn disk_root_has_no_colon_in_profile_dir() {
    let config = greentic_flow::cache::CacheConfig::default();
    let root = config.disk_root("sha256:abcdef");
    let name = root.file_name().and_then(|n| n.to_str()).expect("name");
    assert_eq!(name, "sha256_abcdef");
}
