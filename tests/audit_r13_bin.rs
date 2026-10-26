//! Round-13 regression tests for the greentic-flow binary.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn flow_yaml(path: &Path) -> serde_yaml_bw::Value {
    serde_yaml_bw::from_str(&fs::read_to_string(path).expect("read flow")).expect("parse flow")
}

fn run_update_step(dir: &Path, flow_path: &Path, operation: Option<&str>) {
    let mut cmd = cargo_bin_cmd!("greentic-flow");
    cmd.current_dir(dir)
        .arg("--permissive")
        .arg("update-step")
        .arg("--flow")
        .arg(flow_path)
        .arg("--step")
        .arg("n")
        .arg("--non-interactive");
    if let Some(operation) = operation {
        cmd.arg("--operation").arg(operation);
    }
    cmd.assert().success();
}

/// `update-step --operation <new>` must drop the stale `operation:` sibling (it described the
/// previous operation) while an update that does not change the operation keeps it.
#[test]
fn update_step_operation_change_drops_stale_operation_sibling() {
    let dir = tempdir().expect("tempdir");
    let comp_dir = dir.path().join("comp");
    fs::create_dir_all(&comp_dir).expect("mkdir");
    fs::write(comp_dir.join("comp.wasm"), b"wasm").expect("wasm");
    fs::write(
        comp_dir.join("component.manifest.json"),
        r#"{"id":"acme.comp","config_schema":{"properties":{"foo":{"default":"manifest"}}}}"#,
    )
    .expect("manifest");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  n:\n    op:\n      foo: user\n    operation: support\n    retry:\n      max_attempts: 3\n    routing: out\n",
    )
    .expect("flow");
    fs::write(
        dir.path().join("flow.ygtc.resolve.json"),
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"n":{"source":{"kind":"local","path":"comp/comp.wasm"}}}}"#,
    )
    .expect("sidecar");

    let node_of = |path: &Path| {
        flow_yaml(path)
            .get("nodes")
            .and_then(|n| n.get("n"))
            .cloned()
            .expect("node n")
    };

    // No operation change: the sibling is preserved.
    run_update_step(dir.path(), &flow_path, None);
    let node = node_of(&flow_path);
    assert_eq!(
        node.get("operation").and_then(|v| v.as_str()),
        Some("support"),
        "sibling must survive an update that keeps the operation: {node:?}"
    );
    assert!(node.get("retry").is_some(), "retry must be preserved");

    // Operation change: the stale sibling is dropped.
    run_update_step(dir.path(), &flow_path, Some("run"));
    let node = node_of(&flow_path);
    assert!(
        node.get("operation").is_none(),
        "stale operation sibling must be dropped: {node:?}"
    );
    assert!(node.get("retry").is_some(), "retry must be preserved");
}
