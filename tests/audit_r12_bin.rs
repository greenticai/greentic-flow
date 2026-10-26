//! Round-12 CLI regressions.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn wizard_plan_in_json_format_keeps_stdout_pure_json() {
    let dir = tempdir().expect("tempdir");
    let answers = dir.path().join("plan.json");
    fs::write(
        &answers,
        serde_json::to_string(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-flow",
                "flow": "flows/main.ygtc",
                "flow_id": "main",
                "flow_type": "messaging"
            }]
        }))
        .expect("plan json"),
    )
    .expect("write plan");

    let output = cargo_bin_cmd!("greentic-flow")
        .args(["--format", "json", "wizard"])
        .arg(dir.path())
        .arg("--answers")
        .arg(&answers)
        .output()
        .expect("run wizard");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        serde_json::from_str::<serde_json::Value>(stdout.trim())
            .unwrap_or_else(|err| panic!("stdout must be pure JSON ({err}): {stdout}"));
    }
}
