//! Regression tests for the add-step / update-step / delete-step audit findings.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const MESSAGING_FLOW: &str = "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  start:\n    op: {}\n    routing: out\n";

fn registry_resolver() -> String {
    let registry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("registry");
    format!("fixture://{}", registry.display())
}

fn read_json(path: &Path) -> JsonValue {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
}

fn flow_yaml(path: &Path) -> serde_yaml_bw::Value {
    serde_yaml_bw::from_str(&fs::read_to_string(path).expect("read flow")).expect("parse flow")
}

fn node_names(path: &Path) -> Vec<String> {
    let doc = flow_yaml(path);
    doc.get("nodes")
        .and_then(serde_yaml_bw::Value::as_mapping)
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// H1: manifest defaults must only fill missing keys, never override user values.
#[test]
fn update_step_keeps_user_values_over_manifest_defaults() {
    let dir = tempdir().expect("tempdir");
    let comp_dir = dir.path().join("comp");
    fs::create_dir_all(&comp_dir).expect("mkdir");
    fs::write(comp_dir.join("comp.wasm"), b"wasm").expect("wasm");
    fs::write(
        comp_dir.join("component.manifest.json"),
        r#"{"id":"acme.comp","config_schema":{"properties":{"foo":{"default":"manifest"},"bar":{"default":"filled"}}}}"#,
    )
    .expect("manifest");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  n:\n    op:\n      foo: user\n    routing: out\n",
    )
    .expect("flow");
    fs::write(
        dir.path().join("flow.ygtc.resolve.json"),
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"n":{"source":{"kind":"local","path":"comp/comp.wasm"}}}}"#,
    )
    .expect("sidecar");

    cargo_bin_cmd!("greentic-flow")
        .arg("--permissive")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("n")
        .arg("--non-interactive")
        .assert()
        .success();

    let doc = flow_yaml(&flow_path);
    let payload = doc
        .get("nodes")
        .and_then(|n| n.get("n"))
        .and_then(|n| n.get("op"))
        .expect("op payload");
    assert_eq!(payload.get("foo").and_then(|v| v.as_str()), Some("user"));
    assert_eq!(payload.get("bar").and_then(|v| v.as_str()), Some("filled"));
}

/// M2: deleting keeps the order of the remaining nodes and moves the entrypoint to the successor.
#[test]
fn delete_step_preserves_order_and_moves_entrypoint_to_successor() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nentrypoints:\n  default: b\nnodes:\n  a:\n    op: {}\n    routing: out\n  b:\n    op: {}\n    routing:\n      - to: c\n  c:\n    op: {}\n    routing: out\n  d:\n    op: {}\n    routing: out\n",
    )
    .expect("flow");

    cargo_bin_cmd!("greentic-flow")
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("b")
        .arg("--write")
        .assert()
        .success();

    assert_eq!(node_names(&flow_path), vec!["a", "c", "d"]);
    let doc = flow_yaml(&flow_path);
    // The `default` entrypoint is serialised as `start`.
    assert_eq!(doc.get("start").and_then(|v| v.as_str()), Some("c"));
}

/// L3: a self-loop on the deleted node must not leak dangling routes into predecessors.
#[test]
fn delete_step_with_self_loop_does_not_leave_dangling_routes() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  a:\n    op: {}\n    routing:\n      - to: mid\n  mid:\n    op: {}\n    routing:\n      - to: mid\n      - to: z\n  z:\n    op: {}\n    routing: out\n",
    )
    .expect("flow");

    cargo_bin_cmd!("greentic-flow")
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .arg("--write")
        .assert()
        .success();

    let text = fs::read_to_string(&flow_path).expect("read");
    assert!(!text.contains("mid"), "dangling reference to mid:\n{text}");
    let doc = flow_yaml(&flow_path);
    let routing = doc
        .get("nodes")
        .and_then(|n| n.get("a"))
        .and_then(|n| n.get("routing"))
        .and_then(|r| r.as_sequence())
        .expect("a routing");
    assert_eq!(routing.len(), 1);
    assert_eq!(routing[0].get("to").and_then(|v| v.as_str()), Some("z"));
}

/// L3: a self-loop is not a second predecessor.
#[test]
fn delete_step_self_loop_is_not_a_second_predecessor() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  a:\n    op: {}\n    routing:\n      - to: mid\n  mid:\n    op: {}\n    routing:\n      - to: mid\n      - out: true\n",
    )
    .expect("flow");

    cargo_bin_cmd!("greentic-flow")
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .arg("--write")
        .assert()
        .success();
    assert_eq!(node_names(&flow_path), vec!["a"]);
}

/// M1: a flow id from YAML must not be usable to escape the answers directory.
#[test]
fn wizard_add_step_rejects_path_traversal_flow_id() {
    let dir = tempdir().expect("tempdir");
    let work = dir.path().join("work");
    fs::create_dir_all(&work).expect("mkdir");
    let flow_path = work.join("flow.ygtc");
    // `<answers_dir>/../../escape` is where an unchecked flow id would land.
    let answers_dir = work.join("ans").join("nested");
    fs::create_dir_all(&answers_dir).expect("mkdir answers");
    fs::write(
        &flow_path,
        "id: ../../escape\ntype: messaging\nschema_version: 2\nnodes:\n  start:\n    op: {}\n    routing: out\n",
    )
    .expect("flow");

    let output = cargo_bin_cmd!("greentic-flow")
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--wizard-mode")
        .arg("default")
        .arg("--node-id")
        .arg("widget")
        .arg("--component")
        .arg("oci://acme/widget:1")
        .arg("--resolver")
        .arg(registry_resolver())
        .arg("--routing-out")
        .arg("--answers-dir")
        .arg(&answers_dir)
        .output()
        .expect("run");
    assert!(
        !output.status.success(),
        "traversal flow id must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not safe to use as a path segment"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Actual escape target: <answers_dir>/../../escape == <work>/escape.
    assert!(!work.join("escape").exists());
    assert!(!dir.path().join("escape").exists());
    assert_eq!(
        fs::read_dir(&answers_dir)
            .expect("read answers dir")
            .count(),
        0,
        "answers dir must stay empty"
    );
}

/// Non-wizard delete-step must not build wizard-state paths from an unsafe flow id.
#[test]
fn non_wizard_delete_step_skips_wizard_state_for_traversal_flow_id() {
    let dir = tempdir().expect("tempdir");
    let work = dir.path().join("work");
    // Wizard state lives at <work>/.greentic/cache/flow_wizard/<id>.cbor, so
    // id `../../escape` resolves to <work>/.greentic/escape.cbor.
    let state_dir = work.join(".greentic").join("cache").join("flow_wizard");
    fs::create_dir_all(&state_dir).expect("mkdir");
    let canary = work.join(".greentic").join("escape.cbor");
    fs::write(&canary, b"canary-not-cbor").expect("canary");
    let flow_path = work.join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: ../../escape\ntype: messaging\nschema_version: 2\nnodes:\n  a:\n    op: {}\n    routing:\n      - to: b\n  b:\n    op: {}\n    routing: out\n",
    )
    .expect("flow");

    let output = cargo_bin_cmd!("greentic-flow")
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("a")
        .arg("--write")
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!node_names(&flow_path).contains(&"a".to_string()));
    assert!(
        stderr.contains("skipped wizard state cleanup"),
        "unsafe flow id must skip wizard state cleanup: {stderr}"
    );
    assert_eq!(
        fs::read(&canary).expect("canary"),
        b"canary-not-cbor",
        "file outside the state dir must stay untouched"
    );
}

/// M3: updating a step twice must succeed without --overwrite-answers.
#[test]
fn wizard_update_step_twice_overwrites_answers() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .expect("flow");
    let resolver = registry_resolver();

    cargo_bin_cmd!("greentic-flow")
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--wizard-mode")
        .arg("default")
        .arg("--node-id")
        .arg("widget")
        .arg("--component")
        .arg("oci://acme/widget:1")
        .arg("--resolver")
        .arg(&resolver)
        .arg("--routing-out")
        .assert()
        .success();

    for run in ["one", "two"] {
        let answers = format!(r#"{{"marker":"{run}"}}"#);
        cargo_bin_cmd!("greentic-flow")
            .arg("update-step")
            .arg("--flow")
            .arg(&flow_path)
            .arg("--step")
            .arg("widget")
            .arg("--wizard-mode")
            .arg("update")
            .arg("--component")
            .arg("oci://acme/widget:1")
            .arg("--resolver")
            .arg(&resolver)
            .arg("--non-interactive")
            .arg("--answers")
            .arg(&answers)
            .assert()
            .success();
        // Each run replaces the stored update answers with its own payload.
        let stored =
            read_json(&answers_dir_for(&flow_path, "main", "widget").join("update.answers.json"));
        assert_eq!(stored, serde_json::json!({ "marker": run }));
    }
}

/// M2/M3: delete --write removes the node's stored answers and does not recreate them.
#[test]
fn wizard_delete_step_removes_answers_and_remove_mode_honors_assume_yes() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .expect("flow");
    let resolver = registry_resolver();

    cargo_bin_cmd!("greentic-flow")
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--wizard-mode")
        .arg("default")
        .arg("--node-id")
        .arg("widget")
        .arg("--component")
        .arg("oci://acme/widget:1")
        .arg("--resolver")
        .arg(&resolver)
        .arg("--routing-out")
        .assert()
        .success();
    let answers_dir = answers_dir_for(&flow_path, "main", "widget");
    assert!(answers_dir.join("default.answers.json").exists());

    let delete = |assume_yes: bool| {
        let mut cmd = cargo_bin_cmd!("greentic-flow");
        cmd.arg("delete-step")
            .arg("--flow")
            .arg(&flow_path)
            .arg("--step")
            .arg("widget")
            .arg("--wizard-mode")
            .arg("remove")
            .arg("--component")
            .arg("oci://acme/widget:1")
            .arg("--resolver")
            .arg(&resolver)
            .arg("--write");
        if assume_yes {
            cmd.arg("--assume-yes");
        }
        cmd.output().expect("run")
    };

    let refused = delete(false);
    assert!(!refused.status.success());
    assert!(node_names(&flow_path).contains(&"widget".to_string()));
    // Nothing is persisted before confirmation/validation.
    assert!(!answers_dir.join("remove.answers.json").exists());

    let accepted = delete(true);
    assert!(
        accepted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(!node_names(&flow_path).contains(&"widget".to_string()));
    assert!(!answers_dir.join("default.answers.json").exists());
    assert!(!answers_dir.join("remove.answers.json").exists());
}

fn answers_dir_for(flow_path: &Path, flow_id: &str, node_id: &str) -> PathBuf {
    // Default answers base dir lives next to the flow file.
    let parent = flow_path.parent().expect("parent");
    for candidate in [
        parent.join("answers"),
        parent.join(".greentic").join("answers"),
        parent.to_path_buf(),
    ] {
        let dir = candidate.join(flow_id).join(node_id);
        if dir.exists() {
            return dir;
        }
    }
    parent.join("answers").join(flow_id).join(node_id)
}

/// M4: --dry-run / --validate-only must not create the sidecar or vendor wasm into the pack.
#[test]
fn add_step_dry_run_and_validate_only_write_nothing() {
    let dir = tempdir().expect("tempdir");
    let pack = dir.path().join("pack");
    fs::create_dir_all(pack.join("flows")).expect("mkdir");
    fs::write(pack.join("pack.yaml"), "pack_id: demo\nversion: 0.1.0\n").expect("pack.yaml");
    let flow_path = pack.join("flows").join("main.ygtc");
    fs::write(&flow_path, MESSAGING_FLOW).expect("flow");
    let external = dir.path().join("external");
    fs::create_dir_all(&external).expect("mkdir");
    fs::write(external.join("comp.wasm"), b"wasm").expect("wasm");

    for flag in ["--dry-run", "--validate-only"] {
        cargo_bin_cmd!("greentic-flow")
            .arg("add-step")
            .arg("--flow")
            .arg(&flow_path)
            .arg("--mode")
            .arg("default")
            .arg("--node-id")
            .arg("next")
            .arg("--operation")
            .arg("run")
            .arg("--payload")
            .arg("{}")
            .arg("--local-wasm")
            .arg(external.join("comp.wasm"))
            .arg("--routing-out")
            .arg("--after")
            .arg("start")
            .arg(flag)
            .assert()
            .success();
        assert!(!pack.join("components").exists(), "{flag} vendored wasm");
        assert!(
            !pack.join("flows").join("main.ygtc.resolve.json").exists(),
            "{flag} wrote sidecar"
        );
        assert_eq!(
            fs::read_to_string(&flow_path).expect("flow"),
            MESSAGING_FLOW
        );
    }
}

/// M4: update-step / delete-step dry runs must not create a sidecar.
#[test]
fn delete_and_update_step_dry_runs_do_not_create_sidecar() {
    let dir = tempdir().expect("tempdir");
    // pack.yaml + flows/ make the pack root inferable, so a leaked vendor step would be visible.
    let pack = dir.path().join("pack");
    fs::create_dir_all(pack.join("flows")).expect("mkdir");
    fs::write(pack.join("pack.yaml"), "pack_id: demo\nversion: 0.1.0\n").expect("pack.yaml");
    let flow_path = pack.join("flows").join("main.ygtc");
    fs::write(&flow_path, MESSAGING_FLOW).expect("flow");

    cargo_bin_cmd!("greentic-flow")
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("start")
        .assert()
        .success();
    fs::write(dir.path().join("comp.wasm"), b"wasm").expect("wasm");
    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("--permissive")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("start")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--dry-run")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(!pack.join("flows").join("main.ygtc.resolve.json").exists());
    assert!(
        !pack.join("components").exists(),
        "dry-run must not vendor wasm into the pack"
    );
    assert_eq!(
        fs::read_to_string(&flow_path).expect("flow"),
        MESSAGING_FLOW
    );
}

/// M9: update-step --local-wasm replaces the sidecar source instead of being ignored.
#[test]
fn update_step_local_wasm_updates_sidecar_source() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  n:\n    op:\n      foo: user\n    routing: out\n",
    )
    .expect("flow");
    fs::write(dir.path().join("old.wasm"), b"old").expect("old");
    fs::write(dir.path().join("new.wasm"), b"new").expect("new");
    let sidecar_path = dir.path().join("flow.ygtc.resolve.json");
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"n":{"source":{"kind":"local","path":"old.wasm"}}}}"#,
    )
    .expect("sidecar");

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("--permissive")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("n")
        .arg("--local-wasm")
        .arg("new.wasm")
        .arg("--non-interactive")
        .assert()
        .success();

    let sidecar = read_json(&sidecar_path);
    let path = sidecar
        .pointer("/nodes/n/source/path")
        .and_then(JsonValue::as_str)
        .expect("source path");
    assert!(
        path.ends_with("new.wasm"),
        "sidecar source not updated: {path}"
    );
}

/// L1: the atomic-write temp file must never clobber an unrelated sibling such as `flow.tmp`.
#[test]
fn write_flow_file_does_not_clobber_sibling_tmp_file() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(&flow_path, MESSAGING_FLOW).expect("flow");
    let sibling = dir.path().join("flow.tmp");
    fs::write(&sibling, "precious").expect("sibling");

    cargo_bin_cmd!("greentic-flow")
        .arg("update")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--name")
        .arg("renamed")
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&sibling).expect("sibling"), "precious");
    assert!(
        fs::read_to_string(&flow_path)
            .expect("flow")
            .contains("renamed")
    );
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp") && n != "flow.tmp")
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}

/// L2: directory linting must not follow symlinks (cycles, escaping the lint root).
#[cfg(unix)]
#[test]
fn doctor_directory_walk_skips_symlinks() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("flows");
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(
        root.join("ok.ygtc"),
        "id: ok\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .expect("flow");
    std::os::unix::fs::symlink(&root, root.join("loop")).expect("symlink");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).expect("mkdir");
    fs::write(outside.join("bad.ygtc"), "not: [valid").expect("bad");
    std::os::unix::fs::symlink(&outside, root.join("escape")).expect("symlink");

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&root)
        .assert()
        .success();
}

/// Plan (no --write) delete-step in remove mode: confirmation is still required unless
/// --assume-yes is given, and the plan never persists anything.
#[test]
fn delete_step_plan_remove_mode_honors_assume_yes_without_writing() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .expect("flow");
    let resolver = registry_resolver();
    cargo_bin_cmd!("greentic-flow")
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--wizard-mode")
        .arg("default")
        .arg("--node-id")
        .arg("widget")
        .arg("--component")
        .arg("oci://acme/widget:1")
        .arg("--resolver")
        .arg(&resolver)
        .arg("--routing-out")
        .assert()
        .success();
    let before = fs::read_to_string(&flow_path).expect("flow");
    let answers_dir = answers_dir_for(&flow_path, "main", "widget");

    let plan = |assume_yes: bool| {
        let mut cmd = cargo_bin_cmd!("greentic-flow");
        cmd.arg("delete-step")
            .arg("--flow")
            .arg(&flow_path)
            .arg("--step")
            .arg("widget")
            .arg("--wizard-mode")
            .arg("remove")
            .arg("--component")
            .arg("oci://acme/widget:1")
            .arg("--resolver")
            .arg(&resolver);
        if assume_yes {
            cmd.arg("--assume-yes");
        }
        cmd.output().expect("run")
    };

    let refused = plan(false);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--assume-yes"),
        "stderr: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let accepted = plan(true);
    assert!(
        accepted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(fs::read_to_string(&flow_path).expect("flow"), before);
    assert!(answers_dir.join("default.answers.json").exists());
    assert!(!answers_dir.join("remove.answers.json").exists());
}

/// update-step --local-wasm must keep a pinned source pinned (digest refreshed, mode kept).
#[test]
fn update_step_local_wasm_keeps_existing_pin() {
    let dir = tempdir().expect("tempdir");
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        "id: main\ntype: messaging\nschema_version: 2\nnodes:\n  n:\n    op:\n      foo: user\n    routing: out\n",
    )
    .expect("flow");
    fs::write(dir.path().join("new.wasm"), b"new").expect("new");
    let sidecar_path = dir.path().join("flow.ygtc.resolve.json");
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"n":{"source":{"kind":"local","path":"new.wasm","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},"mode":"pinned"}}}"#,
    )
    .expect("sidecar");

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("--permissive")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("n")
        .arg("--local-wasm")
        .arg("new.wasm")
        .arg("--non-interactive")
        .assert()
        .success();

    let sidecar = read_json(&sidecar_path);
    let digest = sidecar
        .pointer("/nodes/n/source/digest")
        .and_then(JsonValue::as_str)
        .expect("digest kept");
    assert!(
        digest.starts_with("sha256:")
            && digest != "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "{digest}"
    );
    assert_eq!(
        sidecar.pointer("/nodes/n/mode").and_then(JsonValue::as_str),
        Some("pinned")
    );
}
