use ciborium::value::Value as CborValue;
use greentic_flow::cache::disk::DiskCache;
use greentic_flow::cache::metadata::ArtifactMetadata;
use greentic_flow::cache::{
    ArtifactKey, CacheConfig, CacheManager, CpuPolicy, EngineProfile, WarmupItem, WarmupMode,
};
use greentic_flow::schema_validate::validate_value_against_schema;
use greentic_types::schemas::common::schema_ir::SchemaIr;
use tempfile::tempdir;
use wasmtime::Engine;

fn profile(engine: &Engine) -> EngineProfile {
    EngineProfile::from_engine(engine, CpuPolicy::Baseline, "round2".to_string())
}

fn key_for(profile: &EngineProfile, digest: &str) -> ArtifactKey {
    ArtifactKey::new(profile.id().to_string(), digest.to_string())
}

// --- schema_validate: OneOf is "at least one variant matches" ---

#[test]
fn one_of_int_float_accepts_integer_and_float() {
    let schema = SchemaIr::OneOf {
        variants: vec![
            SchemaIr::Int {
                min: None,
                max: None,
            },
            SchemaIr::Float {
                min: None,
                max: None,
            },
        ],
    };
    let int = validate_value_against_schema(&schema, &CborValue::Integer(3.into()));
    assert!(int.is_empty(), "{int:?}");
    let float = validate_value_against_schema(&schema, &CborValue::Float(1.5));
    assert!(float.is_empty(), "{float:?}");
    let text = validate_value_against_schema(&schema, &CborValue::Text("x".into()));
    assert!(text.iter().any(|d| d.code == "SCHEMA_ONE_OF"));
}

#[test]
fn one_of_overlapping_variants_remain_valid() {
    let string = || SchemaIr::String {
        min_len: None,
        max_len: None,
        regex: None,
        format: None,
    };
    let enum_string = SchemaIr::OneOf {
        variants: vec![
            SchemaIr::Enum {
                values: vec![CborValue::Text("auto".into())],
            },
            string(),
        ],
    };
    let diags = validate_value_against_schema(&enum_string, &CborValue::Text("auto".into()));
    assert!(diags.is_empty(), "{diags:?}");

    let int_enum = SchemaIr::OneOf {
        variants: vec![
            SchemaIr::Int {
                min: None,
                max: None,
            },
            SchemaIr::Enum {
                values: vec![CborValue::Integer(0.into())],
            },
        ],
    };
    let diags = validate_value_against_schema(&int_enum, &CborValue::Integer(0.into()));
    assert!(diags.is_empty(), "{diags:?}");
}

// --- cache ---

#[cfg(unix)]
#[test]
fn group_writable_own_cache_is_repaired_on_write_then_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    let cache = DiskCache::new(root.clone(), profile.clone(), None);
    let key = key_for(&profile, "sha256:repair");
    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &meta).unwrap();
    let artifacts = root.join("artifacts");
    std::fs::set_permissions(&artifacts, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        cache.try_read(&key).unwrap().is_none(),
        "loose dir untrusted"
    );
    // Writing repairs directories we own, after which the entry is readable.
    cache.write_atomic(&key, b"abc", &meta).unwrap();
    let mode = std::fs::metadata(&artifacts).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0);
    assert_eq!(cache.try_read(&key).unwrap(), Some(b"abc".to_vec()));
}

#[cfg(unix)]
#[test]
fn world_writable_root_ancestor_disables_read_and_write() {
    use std::os::unix::fs::PermissionsExt;
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let loose = dir.path().join("loose");
    std::fs::create_dir(&loose).unwrap();
    std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o777)).unwrap();
    let root = loose.join("cache");
    let cache = DiskCache::new(root.clone(), profile.clone(), None);
    let key = key_for(&profile, "sha256:anc");
    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &meta).unwrap();
    assert!(
        !root.exists(),
        "nothing may be written under a loose ancestor"
    );
    assert!(cache.try_read(&key).unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn root_owned_by_someone_else_is_never_written() {
    let engine = Engine::default();
    let profile = profile(&engine);
    // /usr exists and is owned by root; skip when running as root.
    if rustix::process::geteuid().is_root() {
        eprintln!("SKIP root_owned_by_someone_else_is_never_written: running as root");
        return;
    }
    if !std::path::Path::new("/usr").is_dir() {
        eprintln!("SKIP root_owned_by_someone_else_is_never_written: /usr missing");
        return;
    }
    let cache = DiskCache::new("/usr".into(), profile.clone(), None);
    let key = key_for(&profile, "sha256:foreign");
    let meta = ArtifactMetadata::new(&profile, key.wasm_digest.clone(), 3);
    cache.write_atomic(&key, b"abc", &meta).unwrap();
    assert!(cache.try_read(&key).unwrap().is_none());
}

#[tokio::test]
async fn prune_disk_is_noop_when_disk_disabled() {
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let config = CacheConfig {
        root: dir.path().to_path_buf(),
        disk_enabled: false,
        ..CacheConfig::default()
    };
    let manager = CacheManager::new(config, profile);
    let report = manager.prune_disk(false).await.unwrap();
    assert_eq!((report.removed_entries, report.removed_bytes), (0, 0));
}

#[tokio::test]
async fn warmup_best_effort_skips_io_errors_but_strict_propagates() {
    let engine = Engine::default();
    let profile = profile(&engine);
    let dir = tempdir().unwrap();
    let config = CacheConfig {
        root: dir.path().to_path_buf(),
        disk_enabled: true,
        ..CacheConfig::default()
    };
    let manager = CacheManager::new(config, profile);
    // A key minted for a different engine profile makes the disk layer return
    // `Err` deterministically (also when running as root, unlike chmod tricks),
    // which exercises the error branch of `warmup`.
    let foreign = ArtifactKey::new("other-profile".to_string(), "sha256:warm".to_string());
    let items = [WarmupItem { key: foreign }];

    let report = manager
        .warmup(&engine, &items, WarmupMode::BestEffort)
        .await
        .expect("best effort must not abort on I/O errors");
    assert_eq!((report.warmed, report.skipped), (0, 1));

    // Strict also fails for a plain cache miss, so assert on the underlying I/O error
    // message to prove that the error itself is propagated.
    let err = manager
        .warmup(&engine, &items, WarmupMode::Strict)
        .await
        .expect_err("strict mode must propagate the error");
    assert!(
        format!("{err:#}").contains("engine_profile_id mismatch"),
        "unexpected strict error: {err:#}"
    );
}

// --- loader ---

#[test]
fn in_memory_schema_loaders_need_no_file() {
    let yaml =
        "id: x\ntype: messaging\nnodes:\n  in:\n    qa.process:\n      a: 1\n    routing: out\n";
    let schema = include_str!("../schemas/ygtc.flow.schema.json");
    let a = greentic_flow::loader::load_ygtc_from_str_with_embedded_schema(yaml, "mem").unwrap();
    let b = greentic_flow::loader::load_ygtc_from_str_with_schema_text(yaml, schema, "mem", "mem")
        .unwrap();
    assert_eq!(a.id, b.id);
}

#[cfg(unix)]
#[test]
fn config_schema_path_lives_in_private_dir() {
    use std::os::unix::fs::MetadataExt;
    let path = greentic_flow::loader::ensure_config_schema_path().unwrap();
    let parent = path.parent().unwrap();
    let meta = std::fs::metadata(parent).unwrap();
    assert_eq!(meta.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(meta.mode() & 0o077, 0, "schema dir must be owner-only");
}
