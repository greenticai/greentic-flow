//! Round-2 regression: `doctor <dir>` must keep going after a malformed flow.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::tempdir;

#[test]
fn doctor_dir_continues_after_unreadable_flow_and_counts_it_once() {
    let dir = tempdir().expect("tempdir");
    // Not valid UTF-8: reading it as text fails before any lint can run.
    fs::write(dir.path().join("a_bad.ygtc"), [0xff, 0xfe, 0x00, 0x01]).expect("bad flow");
    fs::write(
        dir.path().join("z_good.ygtc"),
        "id: good\ntype: messaging\nschema_version: 2\nstart: s\nnodes:\n  s:\n    templates.text:\n      text: hi\n    routing: out\n",
    )
    .expect("good flow");
    let output = cargo_bin_cmd!("greentic-flow")
        .args(["doctor"])
        .arg(dir.path())
        .output()
        .expect("run doctor");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stderr.contains("a_bad.ygtc"), "bad flow reported: {stderr}");
    // The bad flow is counted once (lint failure only, no second contract-check count)
    // and the walk still reaches the second flow (which lacks a sidecar, hence 2 total).
    assert!(
        stderr.contains("2 flow(s) failed validation"),
        "bad flow counted exactly once: {stderr}"
    );
    assert!(
        stderr.contains("z_good.ygtc"),
        "second flow still processed: {stdout} {stderr}"
    );
}
