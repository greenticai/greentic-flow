use greentic_flow::answers::{answers_paths, write_answers};
use greentic_types::cbor::canonical;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

#[test]
fn write_answers_writes_json_and_cbor() {
    let dir = tempdir().unwrap();
    let mut answers = BTreeMap::new();
    answers.insert("name".to_string(), Value::String("Widget".to_string()));
    answers.insert("count".to_string(), Value::Number(3.into()));

    let paths = write_answers(
        dir.path(),
        "flow-main",
        "node-1",
        "default",
        &answers,
        false,
    )
    .unwrap();

    let json_text = fs::read_to_string(paths.json).unwrap();
    let json_value: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(json_value["name"], "Widget");
    assert_eq!(json_value["count"], 3);

    let cbor_bytes = fs::read(paths.cbor).unwrap();
    let cbor_value: Value = canonical::from_cbor(&cbor_bytes).unwrap();
    assert_eq!(cbor_value, json_value);
}

#[test]
fn answers_paths_nest_by_flow_node_and_mode() {
    let paths = answers_paths(tempdir().unwrap().path(), "flow-main", "node-1", "setup");
    assert!(paths.json.ends_with("flow-main/node-1/setup.answers.json"));
    assert!(paths.cbor.ends_with("flow-main/node-1/setup.answers.cbor"));
}

#[test]
fn write_answers_rejects_existing_files_without_overwrite() {
    let dir = tempdir().unwrap();
    let mut answers = BTreeMap::new();
    answers.insert("name".to_string(), Value::String("Widget".to_string()));

    write_answers(
        dir.path(),
        "flow-main",
        "node-1",
        "default",
        &answers,
        false,
    )
    .unwrap();

    let err = write_answers(
        dir.path(),
        "flow-main",
        "node-1",
        "default",
        &answers,
        false,
    )
    .expect_err("existing answers should be protected");
    assert!(format!("{err}").contains("--overwrite-answers"));
}

#[test]
fn write_answers_overwrites_when_enabled() {
    let dir = tempdir().unwrap();
    let mut first = BTreeMap::new();
    first.insert("name".to_string(), Value::String("First".to_string()));
    write_answers(dir.path(), "flow-main", "node-1", "default", &first, false).unwrap();

    let mut second = BTreeMap::new();
    second.insert("name".to_string(), Value::String("Second".to_string()));
    let paths = write_answers(dir.path(), "flow-main", "node-1", "default", &second, true)
        .expect("overwrite should succeed");

    let json_text = fs::read_to_string(paths.json).unwrap();
    let json_value: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(json_value["name"], "Second");
}

#[test]
fn write_answers_reports_directory_creation_errors() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("flow-main"), "occupied").unwrap();

    let err = write_answers(
        dir.path(),
        "flow-main",
        "node-1",
        "default",
        &BTreeMap::new(),
        false,
    )
    .expect_err("blocking file should make parent creation fail");

    assert!(format!("{err}").contains("create answers directory"));
}
