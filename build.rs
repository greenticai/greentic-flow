use std::env;
use std::fs;
use std::path::PathBuf;

fn trusted_env_path(var: &str) -> PathBuf {
    let raw = env::var(var).unwrap_or_else(|_| panic!("{var}"));
    let path = PathBuf::from(raw);
    assert!(
        path.is_absolute(),
        "{var} must be an absolute path provided by cargo"
    );
    path
}

fn main() {
    println!("cargo:rerun-if-changed=frequent-components.json");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let manifest_dir = trusted_env_path("CARGO_MANIFEST_DIR")
        .canonicalize()
        .expect("canonical CARGO_MANIFEST_DIR");
    let source_path = manifest_dir.join("frequent-components.json");
    let raw = fs::read_to_string(&source_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", source_path.display()));

    let mut json: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("parse {}: {err}", source_path.display()));
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    json["catalog_version"] = serde_json::Value::String(version);

    let rendered =
        serde_json::to_string_pretty(&json).expect("serialize embedded frequent-components.json");
    let out_dir = trusted_env_path("OUT_DIR")
        .canonicalize()
        .expect("canonical OUT_DIR");
    let out_path = out_dir.join("frequent-components.embedded.json");
    fs::write(&out_path, format!("{rendered}\n"))
        .unwrap_or_else(|err| panic!("write {}: {err}", out_path.display()));
}
