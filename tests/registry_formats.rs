use greentic_flow::registry::AdapterCatalog;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Creates a temp dir under the current dir (the registry loader's root) so paths can be made
/// relative to it; the loader rejects absolute paths.
fn tmp_dir() -> TempDir {
    let cwd = std::env::current_dir().expect("cwd");
    tempfile::Builder::new()
        .prefix("registry-formats-")
        .tempdir_in(cwd)
        .expect("create temp dir")
}

fn rel_to_cwd(path: &Path) -> PathBuf {
    let cwd = std::env::current_dir().expect("cwd");
    path.strip_prefix(&cwd)
        .unwrap_or_else(|_| panic!("{} is not under cwd {}", path.display(), cwd.display()))
        .to_path_buf()
}

#[test]
fn loads_json_registry() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.json");
    std::fs::write(&file, r#"{"adapters":{"acme.mail":["send","draft"]}}"#).unwrap();

    let catalog = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect("json should load");
    assert!(catalog.contains("acme", "mail", "send"));
    assert!(catalog.contains("acme", "mail", "draft"));
    assert!(!catalog.contains("acme", "mail", "delete"));
    assert!(!catalog.contains("acme", "chat", "send"));
}

#[test]
fn rejects_invalid_registry_content() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.bad");
    std::fs::write(&file, "this is neither json nor toml {{{").unwrap();

    let err = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect_err("garbage must fail");
    assert!(
        format!("{err:#}").contains("unsupported registry format"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_garbage_content_in_json_named_registry() {
    // Right extension, wrong content: the loader must judge content, not the file name.
    let dir = tmp_dir();
    let file = dir.path().join("registry.json");
    std::fs::write(&file, "<<< not json at all {{{").unwrap();

    let err = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect_err("garbage must fail");
    assert!(
        format!("{err:#}").contains("unsupported registry format"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_wrong_schema_registry() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.json");
    std::fs::write(&file, r#"{"adapters":["not","a","map"]}"#).unwrap();

    let err = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect_err("schema must fail");
    assert!(
        format!("{err:#}").contains("unsupported registry format"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_missing_registry_file() {
    let dir = tmp_dir();
    let file = dir.path().join("missing.json");
    let err = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect_err("missing must fail");
    assert!(
        format!("{err:#}").contains("failed to canonicalize"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_path_traversal_outside_root() {
    // A valid registry that exists outside the root, reached through `..` components, so the
    // only possible failure is the root check (not a missing-file error).
    let outside = tempfile::tempdir().expect("outside dir");
    let target = outside.path().join("registry.json");
    std::fs::write(&target, r#"{"adapters":{"evil.x":["y"]}}"#).unwrap();

    let cwd = std::env::current_dir()
        .expect("cwd")
        .canonicalize()
        .unwrap();
    let mut escaping = PathBuf::new();
    for _ in cwd.components().skip(1) {
        escaping.push("..");
    }
    escaping.push(target.canonicalize().unwrap().strip_prefix("/").unwrap());

    let err = AdapterCatalog::load_from_file(&escaping).expect_err("escape must be rejected");
    assert!(
        format!("{err:#}").contains("path escapes root"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_absolute_registry_path() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.json");
    std::fs::write(&file, r#"{"adapters":{}}"#).unwrap();

    let err = AdapterCatalog::load_from_file(&file).expect_err("absolute path must be rejected");
    assert!(
        format!("{err:#}").contains("absolute paths are not allowed"),
        "unexpected error: {err:#}"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escaping_root() {
    let dir = tmp_dir();
    let outside = tempfile::tempdir().expect("outside dir");
    let target = outside.path().join("registry.json");
    std::fs::write(&target, r#"{"adapters":{"evil.x":["y"]}}"#).unwrap();
    let link = dir.path().join("link.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(AdapterCatalog::load_from_file(rel_to_cwd(&link)).is_err());
}

#[cfg(feature = "toml")]
#[test]
fn loads_toml_registry() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.toml");
    std::fs::write(&file, "[adapters]\n\"acme.mail\" = [\"send\"]\n").unwrap();

    let catalog = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect("toml should load");
    assert!(catalog.contains("acme", "mail", "send"));
    assert!(!catalog.contains("acme", "mail", "draft"));
}

#[cfg(not(feature = "toml"))]
#[test]
fn toml_registry_requires_feature() {
    let dir = tmp_dir();
    let file = dir.path().join("registry.toml");
    std::fs::write(&file, "[adapters]\n\"acme.mail\" = [\"send\"]\n").unwrap();

    let err = AdapterCatalog::load_from_file(rel_to_cwd(&file)).expect_err("toml needs feature");
    assert!(format!("{err:#}").contains("enable `toml` feature"));
}
