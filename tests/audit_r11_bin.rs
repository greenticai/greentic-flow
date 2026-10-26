//! Round-11 CLI regressions.

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

#[test]
fn doctor_nonexistent_path_fails() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("typo.ygtc");
    let output = cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&missing)
        .output()
        .expect("run doctor");
    assert!(!output.status.success(), "missing target must fail");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("All flows valid"), "{stdout}");
    assert!(stderr.contains("path not found"), "{stderr}");
}

#[test]
fn bind_component_rejects_component_with_local_wasm() {
    let dir = tempdir().expect("tempdir");
    let output = cargo_bin_cmd!("greentic-flow")
        .args([
            "bind-component",
            "--flow",
            "x.ygtc",
            "--step",
            "s",
            "--local-wasm",
            "a.wasm",
            "--component",
            "oci://ghcr.io/o/c:1",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run bind-component");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be used with"), "{stderr}");
}
