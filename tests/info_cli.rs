use std::process::Command;

fn bin() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
}

#[test]
fn human_output_on_unbound_flow() {
    let out = bin()
        .args(["info", "fixtures/flow_ok.ygtc"])
        .output()
        .expect("run info");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    // Must contain the ID row and either bound/partial/unbound.
    assert!(
        s.contains("ID") || s.contains("Nodes"),
        "unexpected human output:\n{s}"
    );
    assert!(
        s.contains("bound") || s.contains("unbound") || s.contains("partial"),
        "resolve status missing:\n{s}"
    );
}

#[test]
fn json_output_has_schema_version() {
    let out = bin()
        .args(["--format", "json", "info", "fixtures/flow_ok.ygtc"])
        .output()
        .expect("run info --format json");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert_eq!(v["info_schema_version"], 1);
    assert!(v["resolve"].is_object());
    assert!(v["nodes"].is_array());
}

#[test]
fn missing_file_exits_2() {
    let out = bin()
        .args(["info", "/nope/does-not-exist.ygtc"])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn wrong_extension_exits_2() {
    let out = bin().args(["info", "Cargo.toml"]).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn corrupt_ygtc_exits_5() {
    // Write a file with .ygtc extension but invalid contents.
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("bad.ygtc");
    std::fs::write(&path, b"this is not yaml: : :\n  garbage").unwrap();
    let out = bin()
        .args(["info", path.to_str().unwrap()])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(5));
}
