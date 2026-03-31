use std::env;
use std::{
    fs,
    path::{Component, PathBuf},
};

fn trusted_env_path(var: &str) -> PathBuf {
    let raw = env::var(var).unwrap_or_else(|_| panic!("{var}"));
    let path = PathBuf::from(raw);
    assert!(
        path.is_absolute(),
        "{var} must be an absolute path provided by cargo"
    );
    path
}

fn trusted_out_dir() -> PathBuf {
    let out_dir = trusted_env_path("OUT_DIR")
        .canonicalize()
        .expect("canonical OUT_DIR");
    let manifest_dir = trusted_env_path("CARGO_MANIFEST_DIR")
        .canonicalize()
        .expect("canonical CARGO_MANIFEST_DIR");
    let target_root = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                manifest_dir.join(path)
            }
        })
        .unwrap_or_else(|| manifest_dir.join("target"));
    let target_root = if target_root.exists() {
        target_root
            .canonicalize()
            .expect("canonical CARGO_TARGET_DIR/target")
    } else {
        assert!(
            target_root.is_absolute(),
            "target root must resolve to an absolute path"
        );
        assert!(
            !target_root
                .components()
                .any(|component| matches!(component, Component::ParentDir)),
            "target root must not contain parent traversal segments"
        );
        target_root
    };
    assert!(
        out_dir.starts_with(&target_root),
        "OUT_DIR must stay under the cargo target directory"
    );
    out_dir
}

fn main() {
    println!("cargo:rerun-if-changed=frequent-components.json");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let raw = include_str!("frequent-components.json");

    let mut json: serde_json::Value = serde_json::from_str(raw)
        .unwrap_or_else(|err| panic!("parse frequent-components.json: {err}"));
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    json["catalog_version"] = serde_json::Value::String(version);

    let rendered =
        serde_json::to_string_pretty(&json).expect("serialize embedded frequent-components.json");
    let out_dir = trusted_out_dir();
    let out_path = out_dir.join("frequent-components.embedded.json");
    fs::write(&out_path, format!("{rendered}\n"))
        .unwrap_or_else(|err| panic!("write {}: {err}", out_path.display()));
}
