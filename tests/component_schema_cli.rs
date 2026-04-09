use assert_cmd::cargo::cargo_bin_cmd;
use greentic_flow::questions_schema::{example_for_questions, schema_for_questions};
use greentic_flow::wizard_ops::{WizardMode, decode_component_qa_spec, qa_spec_to_questions};
use jsonschema::Draft;
use std::{fs, path::PathBuf};

fn fixture_registry_resolver() -> String {
    format!(
        "fixture://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("registry")
            .display()
    )
}

fn validate_with_schema(schema: &serde_json::Value, instance: &serde_json::Value) -> bool {
    let compiled = jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(schema)
        .expect("compile schema");
    compiled.validate(instance).is_ok()
}

#[test]
fn component_schema_matches_fixture_contract_and_validates_example() {
    let output = cargo_bin_cmd!("greentic-flow")
        .arg("component-schema")
        .arg("oci://acme/widget:1")
        .arg("--resolver")
        .arg(fixture_registry_resolver())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_schema: serde_json::Value =
        serde_json::from_slice(&output).expect("parse CLI schema output");

    let qa_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("registry")
        .join("components")
        .join("acme_widget_1")
        .join("qa_default.cbor");
    let qa_bytes = fs::read(&qa_path).expect("read fixture qa spec");
    let qa_spec = decode_component_qa_spec(&qa_bytes, WizardMode::Default).expect("decode qa");
    let questions = qa_spec_to_questions(&qa_spec, &Default::default(), "en");
    let expected_schema = schema_for_questions(&questions);
    let expected_example = example_for_questions(&questions);

    assert_eq!(cli_schema, expected_schema);
    assert!(validate_with_schema(&cli_schema, &expected_example));
}
