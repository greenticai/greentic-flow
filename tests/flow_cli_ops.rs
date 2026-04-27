use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::prelude::*;
use greentic_flow::loader::load_ygtc_from_path;
use greentic_types::cbor::canonical;
use greentic_types::i18n_text::I18nText;
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};
use greentic_types::schemas::component::v0_6_0::{
    ComponentDescribe, ComponentInfo, ComponentOperation, ComponentQaSpec, ComponentRunInput,
    ComponentRunOutput, QaMode, schema_hash,
};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value as JsonValue;
use serde_json::json;
use serde_yaml_bw::Value;
use std::collections::BTreeMap;
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

fn read_yaml(path: &Path) -> Value {
    serde_yaml_bw::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn seed_wizard_pack(pack_dir: &Path, adaptive_card_wasm: &Path) {
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::create_dir_all(pack_dir.join("components")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .unwrap();
    fs::copy(
        adaptive_card_wasm,
        pack_dir.join("components/component_adaptive_card__0_6_0.wasm"),
    )
    .unwrap();
}

fn write_minimal_pack_yaml(pack_dir: &Path) {
    fs::write(
        pack_dir.join("pack.yaml"),
        r#"pack_id: ai.greentic.test
version: 0.1.0
kind: application
publisher: Greentic
components: []
flows:
  - id: main
    file: flows/main.ygtc
    tags: [default]
    entrypoints: [default]
dependencies: []
assets: []
"#,
    )
    .unwrap();
}

fn write_fixture_wizard_component(
    fixture_dir: &Path,
    reference: &str,
    operations: &[&str],
    applied_config: serde_json::Value,
) {
    fs::create_dir_all(fixture_dir).unwrap();
    let key = reference
        .trim_start_matches("oci://")
        .trim_start_matches("repo://")
        .trim_start_matches("store://")
        .trim_start_matches("file://")
        .replace(['/', ':', '@'], "_");

    let config_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let op_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let op_schema_hash = schema_hash(&op_schema, &op_schema, &config_schema).unwrap();
    let describe = ComponentDescribe {
        info: ComponentInfo {
            id: "acme.widget".to_string(),
            version: "0.1.0".to_string(),
            role: "tool".to_string(),
            display_name: None,
        },
        provided_capabilities: Vec::new(),
        required_capabilities: Vec::new(),
        metadata: BTreeMap::new(),
        operations: operations
            .iter()
            .map(|id| ComponentOperation {
                id: (*id).to_string(),
                display_name: None,
                input: ComponentRunInput {
                    schema: op_schema.clone(),
                },
                output: ComponentRunOutput {
                    schema: op_schema.clone(),
                },
                defaults: BTreeMap::new(),
                redactions: Vec::new(),
                constraints: BTreeMap::new(),
                schema_hash: op_schema_hash.clone(),
            })
            .collect(),
        config_schema,
    };
    let describe_cbor = canonical::to_canonical_cbor_allow_floats(&describe).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.describe.cbor")),
        describe_cbor,
    )
    .unwrap();

    let spec = ComponentQaSpec {
        mode: QaMode::Default,
        title: I18nText::new("title", Some("Fixture Wizard".to_string())),
        description: None,
        questions: Vec::new(),
        defaults: BTreeMap::new(),
    };
    let qa_spec_cbor = canonical::to_canonical_cbor(&spec).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.qa-spec.cbor")),
        qa_spec_cbor,
    )
    .unwrap();
    let apply_cbor = canonical::to_canonical_cbor(&applied_config).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.apply-answers.cbor")),
        apply_cbor,
    )
    .unwrap();
    fs::write(fixture_dir.join(format!("{key}.abi")), "0.6.0").unwrap();
}

#[test]
fn version_flag_prints_version() {
    let expected = format!("greentic-flow {}", env!("CARGO_PKG_VERSION"));
    cargo_bin_cmd!("greentic-flow")
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(expected));
}

#[test]
fn wizard_help_renders_with_pack_entrypoint() {
    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("[PACK]"));
}

#[test]
fn wizard_help_accepts_double_dash_before_pack() {
    let dir = tempdir().unwrap();
    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--")
        .arg(dir.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("[PACK]"));
}

#[test]
fn wizard_help_explains_schema_for_agentic_workflows() {
    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Write a strict wizard action-plan schema"))
        .stdout(contains("Codex and Claude"));
}

#[test]
fn component_schema_help_explains_agentic_schema_usage() {
    cargo_bin_cmd!("greentic-flow")
        .arg("component-schema")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains(
            "Emit strict JSON schema for a component wizard answer contract",
        ))
        .stdout(contains("embed those answers into a flow wizard plan"));
}

#[test]
fn wizard_double_dash_pack_allows_flags_after_pack() {
    let dir = tempdir().unwrap();
    let answers_path = dir.path().join("answers.json");
    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--")
        .arg(dir.path())
        .arg("--dry-run")
        .arg("--answers")
        .arg(&answers_path)
        .arg("--locale")
        .arg("nl")
        .write_stdin("0\n")
        .assert()
        .success();
    assert!(
        answers_path.exists(),
        "wizard should honor --answers when using `wizard -- <pack> ...`"
    );
}

#[test]
fn wizard_menu_allows_exit_from_main_menu() {
    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(".")
        .write_stdin("0\n")
        .assert()
        .success()
        .stdout(contains("Main Menu"));
}

#[test]
fn wizard_can_emit_plan_schema_from_answers_file() {
    let dir = tempdir().unwrap();
    let answers_path = dir.path().join("wizard.answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-flow",
                "flow": "flows/global/messaging/main.ygtc",
                "flow_id": "main",
                "flow_type": "messaging"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(dir.path())
        .arg("--answers")
        .arg(&answers_path)
        .arg("--schema")
        .output()
        .expect("wizard should run");
    assert!(
        output.status.success(),
        "wizard should succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let schema: JsonValue =
        serde_json::from_slice(&output.stdout).expect("schema JSON should be printed to stdout");
    assert_eq!(
        schema.get("schema_id").and_then(JsonValue::as_str),
        Some("greentic-flow.wizard.plan")
    );
    assert_eq!(
        schema
            .get("properties")
            .and_then(|v| v.get("actions"))
            .and_then(|v| v.get("type"))
            .and_then(JsonValue::as_str),
        Some("array")
    );
}

#[test]
fn wizard_schema_without_answers_prints_generic_schema_and_exits() {
    let dir = tempdir().unwrap();

    let output = cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--schema")
        .arg(dir.path())
        .output()
        .expect("wizard should run");
    assert!(
        output.status.success(),
        "wizard should succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let schema: JsonValue =
        serde_json::from_slice(&output.stdout).expect("schema JSON should be printed to stdout");
    assert_eq!(
        schema.get("schema_id").and_then(JsonValue::as_str),
        Some("greentic-flow.wizard.plan")
    );
    assert_eq!(
        schema
            .get("properties")
            .and_then(|v| v.get("actions"))
            .and_then(|v| v.get("items"))
            .and_then(|v| v.get("oneOf"))
            .and_then(JsonValue::as_array)
            .map(Vec::len),
        Some(7)
    );
}

#[test]
fn wizard_schema_without_pack_prints_generic_schema_and_exits() {
    let output = cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg("--schema")
        .output()
        .expect("wizard should run");
    assert!(
        output.status.success(),
        "wizard should succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let schema: JsonValue =
        serde_json::from_slice(&output.stdout).expect("schema JSON should be printed to stdout");
    assert_eq!(
        schema.get("schema_id").and_then(JsonValue::as_str),
        Some("greentic-flow.wizard.plan")
    );
}

#[test]
fn wizard_can_apply_declarative_answers_plan() {
    let dir = tempdir().unwrap();
    let answers_path = dir.path().join("wizard.answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-flow",
                "flow": "flows/global/messaging/main.ygtc",
                "flow_id": "main",
                "flow_type": "messaging"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(dir.path())
        .arg("--answers")
        .arg(&answers_path)
        .assert()
        .success();

    let flow_path = dir.path().join("flows/global/messaging/main.ygtc");
    assert!(flow_path.exists(), "plan should create the flow");
}

#[test]
fn new_writes_v2_empty_flow() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");

    cargo_bin_cmd!("greentic-flow")
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .arg("--description")
        .arg("test flow")
        .assert()
        .success();

    let doc = load_ygtc_from_path(&flow_path).expect("load flow");
    assert_eq!(doc.id, "main");
    assert_eq!(doc.flow_type, "messaging");
    assert_eq!(doc.schema_version, Some(2));
    assert!(doc.nodes.is_empty());
}

#[test]
fn add_step_into_empty_flow_succeeds() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"input":"hi"}"#)
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    assert_eq!(nodes.len(), 1);
    let (id, node) = nodes.iter().next().unwrap();
    assert_eq!(id.as_str().unwrap(), "comp");
    assert_eq!(
        node.get(Value::from("routing")).unwrap().as_str(),
        Some("out")
    );
}

#[test]
fn add_step_wizard_uses_fixture_resolver() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let fixture_dir = dir.path().join("fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    let reference = "oci://acme/widget:1";
    let key = reference
        .trim_start_matches("oci://")
        .trim_start_matches("repo://")
        .trim_start_matches("store://")
        .trim_start_matches("file://")
        .replace(['/', ':', '@'], "_");

    let config_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let op_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let op_schema_hash = schema_hash(&op_schema, &op_schema, &config_schema).unwrap();
    let describe = ComponentDescribe {
        info: ComponentInfo {
            id: "acme.widget".to_string(),
            version: "0.1.0".to_string(),
            role: "tool".to_string(),
            display_name: None,
        },
        provided_capabilities: Vec::new(),
        required_capabilities: Vec::new(),
        metadata: BTreeMap::new(),
        operations: vec![ComponentOperation {
            id: "run".to_string(),
            display_name: None,
            input: ComponentRunInput {
                schema: op_schema.clone(),
            },
            output: ComponentRunOutput { schema: op_schema },
            defaults: BTreeMap::new(),
            redactions: Vec::new(),
            constraints: BTreeMap::new(),
            schema_hash: op_schema_hash,
        }],
        config_schema,
    };
    let describe_cbor = canonical::to_canonical_cbor_allow_floats(&describe).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.describe.cbor")),
        describe_cbor,
    )
    .unwrap();

    let spec = ComponentQaSpec {
        mode: QaMode::Default,
        title: I18nText::new("title", Some("Fixture Wizard".to_string())),
        description: None,
        questions: Vec::new(),
        defaults: BTreeMap::new(),
    };
    let qa_spec_cbor = canonical::to_canonical_cbor(&spec).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.qa-spec.cbor")),
        qa_spec_cbor,
    )
    .unwrap();
    let config = json!({"foo":"bar"});
    let apply_cbor = canonical::to_canonical_cbor(&config).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.apply-answers.cbor")),
        apply_cbor,
    )
    .unwrap();
    fs::write(fixture_dir.join(format!("{key}.abi")), "0.6.0").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--node-id")
        .arg("widget")
        .arg("--component")
        .arg(reference)
        .arg("--wizard-mode")
        .arg("default")
        .arg("--routing-out")
        .arg("--resolver")
        .arg(format!("fixture://{}", fixture_dir.display()))
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let node = nodes.get(Value::from("widget")).unwrap();
    let component_exec = node
        .get(Value::from("run"))
        .and_then(Value::as_mapping)
        .unwrap();
    assert_eq!(
        component_exec
            .get(Value::from("component"))
            .and_then(Value::as_str),
        Some(reference)
    );
    assert_eq!(
        serde_json::to_value(component_exec.get(Value::from("config")).unwrap()).unwrap(),
        json!({"foo":"bar"})
    );
}

#[test]
fn add_step_setup_rejects_component_when_required_input_is_missing() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let fixture_dir = dir.path().join("fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    let reference = "oci://acme/llm-openai:1";
    let key = reference
        .trim_start_matches("oci://")
        .trim_start_matches("repo://")
        .trim_start_matches("store://")
        .trim_start_matches("file://")
        .replace(['/', ':', '@'], "_");

    let config_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let input_schema = SchemaIr::Object {
        properties: BTreeMap::from([
            ("config".to_string(), config_schema.clone()),
            (
                "input".to_string(),
                SchemaIr::Object {
                    properties: BTreeMap::from([(
                        "messages".to_string(),
                        SchemaIr::Array {
                            items: Box::new(SchemaIr::Object {
                                properties: BTreeMap::from([
                                    (
                                        "role".to_string(),
                                        SchemaIr::String {
                                            min_len: Some(1),
                                            max_len: None,
                                            regex: None,
                                            format: None,
                                        },
                                    ),
                                    (
                                        "content".to_string(),
                                        SchemaIr::String {
                                            min_len: Some(1),
                                            max_len: None,
                                            regex: None,
                                            format: None,
                                        },
                                    ),
                                ]),
                                required: vec!["role".to_string(), "content".to_string()],
                                additional: AdditionalProperties::Forbid,
                            }),
                            min_items: Some(1),
                            max_items: None,
                        },
                    )]),
                    required: vec!["messages".to_string()],
                    additional: AdditionalProperties::Forbid,
                },
            ),
        ]),
        required: vec!["input".to_string()],
        additional: AdditionalProperties::Forbid,
    };
    let output_schema = SchemaIr::Object {
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Allow,
    };
    let op_schema_hash = schema_hash(&input_schema, &output_schema, &config_schema).unwrap();
    let describe = ComponentDescribe {
        info: ComponentInfo {
            id: "component-llm-openai".to_string(),
            version: "0.1.0".to_string(),
            role: "tool".to_string(),
            display_name: None,
        },
        provided_capabilities: Vec::new(),
        required_capabilities: Vec::new(),
        metadata: BTreeMap::new(),
        operations: vec![ComponentOperation {
            id: "handle_message".to_string(),
            display_name: None,
            input: ComponentRunInput {
                schema: input_schema.clone(),
            },
            output: ComponentRunOutput {
                schema: output_schema,
            },
            defaults: BTreeMap::new(),
            redactions: Vec::new(),
            constraints: BTreeMap::new(),
            schema_hash: op_schema_hash,
        }],
        config_schema,
    };
    let describe_cbor = canonical::to_canonical_cbor_allow_floats(&describe).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.describe.cbor")),
        describe_cbor,
    )
    .unwrap();

    let qa_spec = ComponentQaSpec {
        mode: QaMode::Setup,
        title: I18nText::new("title", Some("Setup".to_string())),
        description: None,
        questions: Vec::new(),
        defaults: BTreeMap::new(),
    };
    let qa_spec_cbor = canonical::to_canonical_cbor(&qa_spec).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.qa-setup.cbor")),
        qa_spec_cbor,
    )
    .unwrap();

    let config = json!({"provider":"ollama"});
    let apply_cbor = canonical::to_canonical_cbor_allow_floats(&config).unwrap();
    fs::write(
        fixture_dir.join(format!("{key}.apply-setup-config.cbor")),
        apply_cbor,
    )
    .unwrap();
    fs::write(fixture_dir.join(format!("{key}.abi")), "0.6.0").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("component-llm-openai")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--node-id")
        .arg("llm")
        .arg("--component")
        .arg(reference)
        .arg("--operation")
        .arg("handle_message")
        .arg("--wizard-mode")
        .arg("setup")
        .arg("--routing-out")
        .arg("--resolver")
        .arg(format!("fixture://{}", fixture_dir.display()))
        .assert()
        .failure()
        .stderr(contains("requires invocation input"))
        .stderr(contains("handle_message"))
        .stderr(contains("input"));
}

#[test]
fn add_step_on_legacy_writes_v2_and_shorthand() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
nodes:
  start:
    component.exec:
      component: ai.greentic.echo
      input: {}
    operation: run
    routing:
      - out: true
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .assert()
        .success();

    let yaml = fs::read_to_string(&flow_path).unwrap();
    assert!(!yaml.contains("component.exec"));
    assert!(yaml.contains("routing: out"));
}

#[test]
fn add_step_requires_routing() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    qa.process:
      payload: true
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("next")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .failure()
        .stderr(predicates::str::contains("ADD_STEP_ROUTING_MISSING"));
}

#[test]
fn add_step_creates_sidecar_local() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .assert()
        .success();

    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    assert_eq!(nodes.len(), 1);
    let entry = nodes.values().next().unwrap();
    assert_eq!(
        entry
            .get("source")
            .and_then(|s| s.get("path"))
            .and_then(JsonValue::as_str)
            .unwrap(),
        "file://comp.wasm"
    );
}

#[test]
fn add_step_local_wasm_is_relativized_from_flow_dir() {
    let dir = tempdir().unwrap();
    let other_dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(other_dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg(&wasm_path)
        .arg("--after")
        .arg("start")
        .arg("--write")
        .assert()
        .success();

    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    let entry = nodes.values().next().unwrap();
    assert_eq!(
        entry
            .get("source")
            .and_then(|s| s.get("path"))
            .and_then(JsonValue::as_str)
            .unwrap(),
        "file://comp.wasm"
    );
}

#[test]
fn add_step_local_wasm_relativizes_from_flow_dir_when_called_elsewhere() {
    let dir = tempdir().unwrap();
    let flow_dir = dir.path().join("flows");
    let component_dir = dir.path().join("components/hello-world/target");
    fs::create_dir_all(&flow_dir).unwrap();
    fs::create_dir_all(&component_dir).unwrap();
    let flow_path = flow_dir.join("main.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = component_dir.join("component_hello_world.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("hello-world")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("components/hello-world/target/component_hello_world.wasm")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .assert()
        .success();

    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    let entry = nodes.values().next().unwrap();
    assert_eq!(
        entry
            .get("source")
            .and_then(|s| s.get("path"))
            .and_then(JsonValue::as_str)
            .unwrap(),
        "file://../components/hello-world/target/component_hello_world.wasm"
    );
}

#[test]
fn add_step_local_wasm_relativizes_from_nested_flow_dir() {
    let dir = tempdir().unwrap();
    let flow_dir = dir.path().join("flow/flow-a");
    let component_dir = dir.path().join("components/hello-world/target");
    fs::create_dir_all(&flow_dir).unwrap();
    fs::create_dir_all(&component_dir).unwrap();
    let flow_path = flow_dir.join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = component_dir.join("component_hello_world.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("hello-world")
        .arg("--operation")
        .arg("handle_message")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("components/hello-world/target/component_hello_world.wasm")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .assert()
        .success();

    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    let entry = nodes.values().next().unwrap();
    assert_eq!(
        entry
            .get("source")
            .and_then(|s| s.get("path"))
            .and_then(JsonValue::as_str)
            .unwrap(),
        "file://../../components/hello-world/target/component_hello_world.wasm"
    );
}

#[test]
fn add_step_remote_pin_uses_env_digest() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("remote")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--component")
        .arg("oci://example.com/component:latest")
        .arg("--pin")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .env(
            "GREENTIC_FLOW_TEST_DIGEST",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .assert()
        .success();

    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    let entry = nodes.values().next().unwrap();
    assert_eq!(
        entry
            .get("source")
            .and_then(|s| s.get("digest"))
            .and_then(JsonValue::as_str)
            .unwrap(),
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn add_step_default_prompts_for_questions() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let template = json!({
        "node_id": "card1",
        "node": {
            "card": { "msg": "{{state.msg}}" },
            "routing": [ { "to": "NEXT_NODE_PLACEHOLDER" } ]
        }
    });
    let manifest = json!({
        "id": "ai.greentic.card",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "ask",
                    "nodes": {
                        "ask": {
                            "questions": {
                                "fields": [
                                    {
                                        "id": "msg",
                                        "prompt": "Message?",
                                        "type": "string",
                                        "default": "hi",
                                        "writes_to": "msg"
                                    }
                                ]
                            },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            }
        }
    });
    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(&manifest_path, manifest.to_string()).unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("card1")
        .arg("--operation")
        .arg("card")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--manifest")
        .arg(&manifest_path)
        .write_stdin("hello\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("Question (msg):"));

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let inserted = nodes
        .get(Value::from("card1"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let card = inserted
        .get(Value::from("card"))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(
        card.get(Value::from("msg")).unwrap().as_str(),
        Some("hello")
    );
}

#[test]
fn add_step_default_skips_prompt_without_dev_flow() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("card1")
        .arg("--operation")
        .arg("card")
        .arg("--payload")
        .arg(r#"{"msg":"hi"}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .success()
        .stdout(predicates::str::contains("Question (").not());
}

#[test]
fn add_step_config_uses_cached_component_manifest() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();

    let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let cache_dir = dir.path().join("cache");
    let digest_dir =
        cache_dir.join("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    fs::create_dir_all(&digest_dir).unwrap();
    fs::write(digest_dir.join("component.wasm"), b"wasm-bytes").unwrap();

    let template = json!({
        "node_id": "adaptive-card",
        "node": {
            "component.exec": {
                "component": "ai.greentic.card",
                "input": { "text": "hi" }
            },
            "operation": "handle_message",
            "routing": [ { "to": "NEXT_NODE_PLACEHOLDER" } ]
        }
    });
    let manifest = json!({
        "id": "ai.greentic.card",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "in",
                    "nodes": {
                        "in": {
                            "questions": { "fields": [] },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            },
            "custom": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "in",
                    "nodes": {
                        "in": {
                            "questions": { "fields": [] },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            }
        }
    });
    fs::write(
        digest_dir.join("component.manifest.json"),
        manifest.to_string(),
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("config")
        .arg("--node-id")
        .arg("adaptive-card")
        .arg("--component")
        .arg("oci://example.com/component:latest")
        .arg("--pin")
        .arg("--after")
        .arg("start")
        .arg("--write")
        .env("GREENTIC_FLOW_TEST_DIGEST", digest)
        .env("GREENTIC_DIST_CACHE_DIR", &cache_dir)
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let start = nodes
        .get(Value::from("start"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let routing = start
        .get(Value::from("routing"))
        .unwrap()
        .as_sequence()
        .unwrap();
    let to = routing[0]
        .as_mapping()
        .unwrap()
        .get(Value::from("to"))
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(to, "adaptive-card");

    let inserted = nodes
        .get(Value::from("adaptive-card"))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(
        inserted.get(Value::from("routing")).and_then(Value::as_str),
        Some("out")
    );
}

#[test]
fn add_step_defaults_to_appending_after_terminal_node() {
    // Default behaviour change: when `--after` is omitted the wizard
    // appends the new step at the end of the entrypoint-rooted chain
    // instead of prepending before the entrypoint. Sequential `add-step`
    // calls now build a forward-ordered flow, fixing the reversed-chain
    // bug that broke greentic-demo flow_wizard_answers (hr-onboarding,
    // sales-crm, supply-chain, …).
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
start: a
type: messaging
schema_version: 2
nodes:
  a:
    op: {}
    routing:
      - to: b
  b:
    tail: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("inserted")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--write")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let default_target = yaml
        .get("entrypoints")
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::from("default")))
        .and_then(Value::as_str)
        .or_else(|| yaml.get("start").and_then(Value::as_str))
        .expect("entrypoint or start");
    assert_eq!(
        default_target, "a",
        "entrypoint must remain stable when appending"
    );

    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    assert!(nodes.contains_key(Value::from("a")));
    assert!(nodes.contains_key(Value::from("b")));
    assert!(nodes.contains_key(Value::from("inserted")));

    // a → b unchanged, b now routes forward to the new step, new step
    // inherits the terminal `out: true` that b used to own.
    let a_routing = nodes
        .get(Value::from("a"))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::from("routing")))
        .and_then(Value::as_sequence)
        .unwrap();
    assert_eq!(a_routing[0].get("to").and_then(Value::as_str).unwrap(), "b");

    let b_routing = nodes
        .get(Value::from("b"))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::from("routing")))
        .and_then(Value::as_sequence)
        .unwrap();
    assert_eq!(
        b_routing[0].get("to").and_then(Value::as_str).unwrap(),
        "inserted",
        "former terminal node should now route forward to the new step"
    );
}

#[test]
fn add_step_inserts_after_anchor_in_order() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  first:
    hop: {}
    routing:
      - to: second
  second:
    hop: {}
    routing:
      - to: third
  third:
    hop: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(dir.path().join("comp.wasm"), b"bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("inserted")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--after")
        .arg("second")
        .assert()
        .success();

    let content = fs::read_to_string(&flow_path).unwrap();
    let mut order = Vec::new();
    for line in content.lines() {
        if line.starts_with("  ")
            && !line.starts_with("    ")
            && let Some(id) = line[2..].strip_suffix(':')
            && !id.starts_with('-')
        {
            order.push(id.to_string());
        }
    }
    assert_eq!(
        order,
        vec![
            "first".to_string(),
            "second".to_string(),
            "inserted".to_string(),
            "third".to_string()
        ]
    );
}

#[test]
fn add_step_routing_out_flag() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    hop: {}
    routing:
      - to: end
  end:
    hop: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(dir.path().join("comp.wasm"), b"bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--after")
        .arg("start")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let start = nodes
        .get(Value::from("start"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let routing = start
        .get(Value::from("routing"))
        .and_then(Value::as_sequence)
        .unwrap();
    let inserted_id = routing[0].get("to").and_then(Value::as_str).unwrap();
    assert_eq!(inserted_id, "comp");
    let inserted = nodes
        .get(Value::from(inserted_id))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(
        inserted.get(Value::from("routing")).unwrap().as_str(),
        Some("out")
    );
}

#[test]
fn add_step_routing_next_flag() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  a:
    hop: {}
    routing:
      - to: b
  b:
    hop: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(dir.path().join("comp.wasm"), b"bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("op")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--routing-next")
        .arg("b")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--after")
        .arg("a")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let a = nodes.get(Value::from("a")).unwrap().as_mapping().unwrap();
    let a_routing = a
        .get(Value::from("routing"))
        .unwrap()
        .as_sequence()
        .unwrap();
    let inserted_id = a_routing[0].get("to").and_then(Value::as_str).unwrap();
    assert_eq!(inserted_id, "comp");
    let inserted = nodes
        .get(Value::from(inserted_id))
        .unwrap()
        .as_mapping()
        .unwrap();
    let ins_routing = inserted
        .get(Value::from("routing"))
        .and_then(Value::as_sequence)
        .unwrap();
    assert_eq!(
        ins_routing[0].get("to").and_then(Value::as_str).unwrap(),
        "b"
    );
}

#[test]
fn update_metadata_changes_name_only() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
title: old
type: messaging
schema_version: 2
start: hello
nodes:
  hello:
    op:
      field: keep
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--name")
        .arg("new-name")
        .assert()
        .success();

    let yaml = fs::read_to_string(&flow_path).unwrap();
    assert!(yaml.contains("title: new-name"));
    assert!(yaml.contains("field: keep"));
    assert!(yaml.contains("routing: out"));
}

#[test]
fn update_type_on_empty_flow_succeeds() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--type")
        .arg("events")
        .assert()
        .success();

    let doc = load_ygtc_from_path(&flow_path).expect("load flow");
    assert_eq!(doc.flow_type, "events");
}

#[test]
fn update_type_on_non_empty_fails() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
start: hello
nodes:
  hello:
    op: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--type")
        .arg("events")
        .assert()
        .failure();
}

#[test]
fn update_fails_when_missing_file() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("missing.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--name")
        .arg("noop")
        .assert()
        .failure();
}

#[test]
fn update_step_requires_sidecar_mapping() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op:
      field: old
    routing: out
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--non-interactive")
        .assert()
        .failure();
}

#[test]
fn doctor_uses_embedded_schema_by_default() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes: {}
parameters: {}
tags: []
entrypoints: {}
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .success();
}

#[test]
fn doctor_fails_on_raw_summary_literals() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: raw-summary
type: messaging
schema_version: 2
title: Raw title
description: Raw description
nodes: {}
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(
            contains("title must be an i18n tag").and(contains("description must be an i18n tag")),
        );
}

#[test]
fn doctor_fails_when_i18n_summary_key_missing_from_pack_source() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flows/global/messaging/welcome.ygtc");
    fs::create_dir_all(flow_path.parent().unwrap()).unwrap();
    fs::write(
        &flow_path,
        r#"id: welcome
type: messaging
schema_version: 2
title: i18n:flow.welcome.title
description: i18n:flow.welcome.description
nodes: {}
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("i18n")).unwrap();
    fs::write(
        dir.path().join("i18n/en-GB.json"),
        r#"{"flow.welcome.title":"Welcome"}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(contains(
            "description i18n key 'flow.welcome.description' missing from pack i18n/en-GB.json",
        ));
}

#[test]
fn doctor_accepts_i18n_summary_keys_present_in_pack_source() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flows/global/messaging/welcome.ygtc");
    fs::create_dir_all(flow_path.parent().unwrap()).unwrap();
    fs::write(
        &flow_path,
        r#"id: welcome
type: messaging
schema_version: 2
title: i18n:flow.welcome.title
description: i18n:flow.welcome.description
nodes: {}
parameters: {}
tags: []
entrypoints: {}
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("i18n")).unwrap();
    fs::write(
        dir.path().join("i18n/en-GB.json"),
        r#"{"flow.welcome.title":"Welcome","flow.welcome.description":"Greeting flow"}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .success();
}

#[test]
fn doctor_json_reports_raw_summary_i18n_lint_errors() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: raw-summary
type: messaging
schema_version: 2
title: Raw title
description: Raw description
nodes: {}
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg("--json")
        .arg(&flow_path)
        .assert()
        .failure()
        .stdout(
            contains("title must be an i18n tag").and(contains("description must be an i18n tag")),
        );
}

#[test]
fn doctor_reports_component_config_schema_errors() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &manifest_path,
        r#"{"id":"ai.greentic.test","config_schema":{"type":"object","required":["message"],"properties":{"message":{"type":"string"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    run:
      message: 42
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("component_config"));
}

#[test]
fn doctor_accepts_component_config_schema_matches() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &manifest_path,
        r#"{"id":"ai.greentic.test","config_schema":{"type":"object","required":["message"],"properties":{"message":{"type":"string"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    run:
      message: "ok"
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .success();
}

#[test]
fn add_step_rejects_component_payload_schema_mismatch() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    cargo_bin_cmd!("greentic-flow")
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(
        &manifest_path,
        r#"{"id":"ai.greentic.test","config_schema":{"type":"object","required":["message"],"properties":{"message":{"type":"string"}}}}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("hello")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{"message":42}"#)
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .failure()
        .stderr(predicates::str::contains("component_config"));
}

#[test]
fn update_step_rejects_component_payload_schema_mismatch() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    cargo_bin_cmd!("greentic-flow")
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(
        &manifest_path,
        r#"{"id":"ai.greentic.test","config_schema":{"type":"object","required":["message"],"properties":{"message":{"type":"string"}}}}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("hello")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{"message":"ok"}"#)
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .success();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--answers")
        .arg(r#"{"message":42}"#)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("component_config"));
}

#[test]
fn update_step_preserves_when_no_answers() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    let template = json!({
        "node_id": "hello",
        "node": {
            "op": { "field": "{{state.field}}" },
            "routing": [ { "to": "NEXT_NODE_PLACEHOLDER" } ]
        }
    });
    let manifest = json!({
        "id": "ai.greentic.test",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "in",
                    "nodes": {
                        "in": {
                            "questions": { "fields": [ { "id": "field", "default": "old" } ] },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            },
            "custom": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "in",
                    "nodes": {
                        "in": {
                            "questions": { "fields": [ { "id": "field", "default": "old" } ] },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            }
        }
    });
    fs::write(
        dir.path().join("component.manifest.json"),
        manifest.to_string(),
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
        )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op:
      field: old
    routing: out
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--mode")
        .arg("config")
        .arg("--non-interactive")
        .arg("--write")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .expect("nodes map");
    let hello = nodes
        .get(Value::from("hello"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let op = hello.get(Value::from("op")).unwrap().as_mapping().unwrap();
    assert_eq!(op.get(Value::from("field")).unwrap().as_str(), Some("old"));
    assert_eq!(
        hello.get(Value::from("routing")).unwrap().as_str(),
        Some("out")
    );
}

#[test]
fn update_step_config_prompts_for_questions() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let template = json!({
        "node_id": "hello",
        "node": {
            "card": { "msg": "{{state.msg}}" },
            "routing": [ { "to": "NEXT_NODE_PLACEHOLDER" } ]
        }
    });
    let manifest = json!({
        "id": "ai.greentic.card",
        "dev_flows": {
            "custom": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "ask",
                    "nodes": {
                        "ask": {
                            "questions": {
                                "fields": [
                                    {
                                        "id": "msg",
                                        "prompt": "Message?",
                                        "type": "string"
                                    }
                                ]
                            },
                            "routing": [ { "to": "emit" } ]
                        },
                        "emit": {
                            "template": serde_json::to_string(&template).unwrap()
                        }
                    }
                }
            }
        }
    });
    fs::write(
        dir.path().join("component.manifest.json"),
        manifest.to_string(),
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    card: {}
    routing: out
"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--mode")
        .arg("config")
        .write_stdin("new\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("Question (msg):"));

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let hello = nodes
        .get(Value::from("hello"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let card = hello
        .get(Value::from("card"))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(card.get(Value::from("msg")).unwrap().as_str(), Some("new"));
}

#[test]
fn add_step_config_requires_custom_dev_flow() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let manifest = json!({
        "id": "ai.greentic.card",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "nodes": {}
                }
            }
        }
    });
    let manifest_path = dir.path().join("component.manifest.json");
    fs::write(&manifest_path, manifest.to_string()).unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("config")
        .arg("--node-id")
        .arg("card1")
        .arg("--operation")
        .arg("card")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--manifest")
        .arg(&manifest_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("dev_flows.custom"));
}

#[test]
fn add_step_default_fixture_prompts_and_applies_answers() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let fixture_flow =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flows/simple.ygtc");
    fs::write(&flow_path, fs::read_to_string(&fixture_flow).unwrap()).unwrap();
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/manifests/component.manifest.json");

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--node-id")
        .arg("card1")
        .arg("--mode")
        .arg("default")
        .arg("--operation")
        .arg("card")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--local-wasm")
        .arg("comp.wasm")
        .arg("--manifest")
        .arg(&manifest_path)
        .write_stdin("assets/cards/card.json\ny\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("Question (asset_path):"))
        .stdout(predicates::str::contains("Question (needs_interaction):"));

    let yaml = read_yaml(&flow_path);
    let nodes = yaml.get("nodes").and_then(Value::as_mapping).unwrap();
    let inserted = nodes
        .get(Value::from("card1"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let card = inserted
        .get(Value::from("card"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let card_spec = card
        .get(Value::from("card_spec"))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(
        card_spec.get(Value::from("asset_path")).unwrap().as_str(),
        Some("assets/cards/card.json")
    );
    let interaction = card
        .get(Value::from("interaction"))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(
        interaction
            .get(Value::from("enabled"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn update_step_non_interactive_missing_required_reports_template() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    let manifest_fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/manifests/component.manifest.json");
    fs::write(
        dir.path().join("component.manifest.json"),
        fs::read_to_string(&manifest_fixture).unwrap(),
    )
    .unwrap();

    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"card1":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  card1:
    card: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("card1")
        .arg("--mode")
        .arg("default")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("missing required answers"))
        .stderr(predicates::str::contains("--answers"))
        .stderr(predicates::str::contains("asset_path"));
}

#[test]
fn wizard_answers_plan_copies_adaptive_card_asset_from_local_file_and_remote_url() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-adaptive-card")
        .join("dist")
        .join("component_adaptive_card__0_6_0.wasm");
    if !wasm_path.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let local_source = dir.path().join("adaptive-card-local.json");
    fs::write(
        &local_source,
        r#"{"type":"AdaptiveCard","version":"1.6","body":[{"type":"TextBlock","text":"Local"}]}"#,
    )
    .unwrap();

    let remote_url = "https://github.com/greenticai/component-adaptive-card/releases/latest/download/adaptive-card.json";

    let local_pack = dir.path().join("pack-local");
    seed_wizard_pack(&local_pack, &wasm_path);
    let local_answers = local_pack.join("answers.json");
    fs::write(
        &local_answers,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "step_id": "adaptive_local",
                "component": "components/component_adaptive_card__0_6_0.wasm",
                "mode": "default",
                "answers": {
                    "card_source": "remote",
                    "default_card_remote": local_source.display().to_string(),
                    "multilingual": false
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&local_pack)
        .arg("--answers")
        .arg(&local_answers)
        .assert()
        .success();

    let copied_local_asset = local_pack.join("assets/cards/adaptive-card-local.json");
    assert!(copied_local_asset.exists());
    assert_eq!(
        fs::read_to_string(&copied_local_asset).unwrap(),
        fs::read_to_string(&local_source).unwrap()
    );

    let remote_pack = dir.path().join("pack-remote");
    seed_wizard_pack(&remote_pack, &wasm_path);
    let remote_answers = remote_pack.join("answers.json");
    fs::write(
        &remote_answers,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "step_id": "adaptive_remote",
                "component": "components/component_adaptive_card__0_6_0.wasm",
                "mode": "default",
                "answers": {
                    "card_source": "remote",
                    "default_card_remote": remote_url,
                    "multilingual": false
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&remote_pack)
        .arg("--answers")
        .arg(&remote_answers)
        .assert()
        .success();

    let copied_remote_asset = remote_pack.join("assets/cards/adaptive-card.json");
    assert!(copied_remote_asset.exists());
    let copied_remote_asset_text = fs::read_to_string(&copied_remote_asset).unwrap();
    assert!(
        copied_remote_asset_text.contains("\"AdaptiveCard\""),
        "remote URL should be materialized into pack assets"
    );
}

#[test]
fn wizard_answers_plan_registers_referenced_asset_in_pack_yaml() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-adaptive-card")
        .join("dist")
        .join("component_adaptive_card__0_6_0.wasm");
    if !wasm_path.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    seed_wizard_pack(&pack_dir, &wasm_path);
    write_minimal_pack_yaml(&pack_dir);

    let asset_path = pack_dir.join("assets/cards/welcome_card.json");
    fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
    fs::write(
        &asset_path,
        r#"{"type":"AdaptiveCard","version":"1.6","body":[{"type":"TextBlock","text":"asset repro"}]}"#,
    )
    .unwrap();

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [
                {
                    "action": "add-flow",
                    "flow": "flows/on_message.ygtc",
                    "flow_id": "on_message",
                    "flow_type": "messaging"
                },
                {
                    "action": "add-step",
                    "flow": "flows/on_message.ygtc",
                    "component": "components/component_adaptive_card__0_6_0.wasm",
                    "mode": "default",
                    "answers": {
                        "card_source": "asset",
                        "default_card_inline": {
                            "type": "AdaptiveCard",
                            "version": "1.6",
                            "body": [{"type":"TextBlock","text":"inline fallback"}]
                        },
                        "default_card_asset": "assets/cards/welcome_card.json",
                        "default_card_remote": "",
                        "multilingual": true,
                        "language_mode": "custom",
                        "supported_locales": "en"
                    }
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    let flow_path = pack_dir.join("flows/on_message.ygtc");
    assert!(flow_path.exists(), "plan should create on_message flow");
    assert!(
        fs::read_to_string(&flow_path)
            .unwrap()
            .contains("default_card_asset: assets/cards/welcome_card.json"),
        "flow should reference the asset path"
    );

    let pack_yaml = read_yaml(&pack_dir.join("pack.yaml"));
    let assets = pack_yaml
        .get(Value::from("assets"))
        .and_then(Value::as_sequence)
        .expect("pack.yaml assets sequence");
    let has_asset_entry = assets.iter().any(|entry| {
        entry
            .as_mapping()
            .and_then(|m| m.get(Value::from("path")))
            .and_then(Value::as_str)
            == Some("assets/cards/welcome_card.json")
    });
    assert!(
        has_asset_entry,
        "pack.yaml assets should include assets/cards/welcome_card.json"
    );
}

#[test]
fn add_step_local_wasm_setup_rejects_required_runtime_input() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-llm-openai")
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("component_llm_openai.wasm");
    if !wasm_path.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    write_minimal_pack_yaml(&pack_dir);
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .unwrap();

    let answers_dir = dir.path().join("answers");
    let answers_file = dir.path().join("setup.answers.json");
    fs::write(
        &answers_file,
        serde_json::to_string_pretty(&json!({
            "provider": "ollama",
            "base_url": "http://127.0.0.1:11434/v1",
            "default_model": "llama3.2",
            "endpoint_requires_api_key": false,
            "api_key_secret": "dummy_ollama_key"
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("add-step")
        .arg("component-llm-openai")
        .arg("--flow")
        .arg(pack_dir.join("flows/main.ygtc"))
        .arg("--node-id")
        .arg("llm")
        .arg("--local-wasm")
        .arg(&wasm_path)
        .arg("--operation")
        .arg("handle_message")
        .arg("--wizard-mode")
        .arg("setup")
        .arg("--answers-file")
        .arg(&answers_file)
        .arg("--answers-dir")
        .arg(&answers_dir)
        .arg("--overwrite-answers")
        .arg("--routing-out")
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .stderr(contains("requires invocation input"))
        .stderr(contains("handle_message"))
        .stderr(contains("input"));
}

#[test]
fn wizard_answers_plan_registers_local_wasm_component_in_pack_yaml() {
    let source_wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-llm-openai")
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("component_llm_openai.wasm");
    if !source_wasm.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::create_dir_all(pack_dir.join("components")).unwrap();
    write_minimal_pack_yaml(&pack_dir);
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .unwrap();

    let pack_wasm = pack_dir.join("components/component_llm_openai.wasm");
    fs::copy(&source_wasm, &pack_wasm).unwrap();

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "step_id": "llm",
                "component": "components/component_llm_openai.wasm",
                "mode": "setup",
                "answers": {
                    "provider": "ollama",
                    "base_url": "http://127.0.0.1:11434/v1",
                    "default_model": "llama3.2",
                    "endpoint_requires_api_key": false,
                    "api_key_secret": "dummy_ollama_key"
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    let pack_yaml = read_yaml(&pack_dir.join("pack.yaml"));
    let components = pack_yaml
        .get(Value::from("components"))
        .and_then(Value::as_sequence)
        .expect("pack.yaml components sequence");
    let component_entry = components.iter().find_map(|entry| {
        let map = entry.as_mapping()?;
        let wasm = map.get(Value::from("wasm")).and_then(Value::as_str)?;
        if wasm == "components/component_llm_openai.wasm" {
            Some(map)
        } else {
            None
        }
    });
    let component_entry = component_entry.expect("pack.yaml component entry");
    assert_eq!(
        component_entry
            .get(Value::from("id"))
            .and_then(Value::as_str),
        Some("component_llm_openai")
    );
}

#[test]
fn wizard_answers_plan_add_step_persists_mapping_aliases() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-adaptive-card")
        .join("dist")
        .join("component_adaptive_card__0_6_0.wasm");
    if !wasm_path.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    seed_wizard_pack(&pack_dir, &wasm_path);

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "step_id": "adaptive_mapped",
                "component": "components/component_adaptive_card__0_6_0.wasm",
                "mode": "default",
                "answers": {
                    "card_source": "inline",
                    "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Mapped\"}]}",
                    "multilingual": false
                },
                "in_map": {
                    "source": "$.input"
                },
                "out_map": {
                    "target": "$.output"
                },
                "err_map": {
                    "target": "$.error"
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .assert()
        .success();

    let doc = load_ygtc_from_path(&pack_dir.join("flows/main.ygtc")).expect("load flow");
    let raw = &doc.nodes.get("adaptive_mapped").expect("added node").raw;
    assert_eq!(raw.get("in_map"), Some(&json!({ "source": "$.input" })));
    assert_eq!(raw.get("out_map"), Some(&json!({ "target": "$.output" })));
    assert_eq!(raw.get("err_map"), Some(&json!({ "target": "$.error" })));
}

#[test]
fn wizard_answers_plan_update_step_persists_mapping_aliases() {
    let wasm_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-adaptive-card")
        .join("dist")
        .join("component_adaptive_card__0_6_0.wasm");
    if !wasm_path.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    seed_wizard_pack(&pack_dir, &wasm_path);

    let add_answers_path = pack_dir.join("add-answers.json");
    fs::write(
        &add_answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "step_id": "adaptive_update_mapped",
                "component": "components/component_adaptive_card__0_6_0.wasm",
                "mode": "default",
                "answers": {
                    "card_source": "inline",
                    "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Before update\"}]}",
                    "multilingual": false
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&add_answers_path)
        .assert()
        .success();

    let update_answers_path = pack_dir.join("update-answers.json");
    fs::write(
        &update_answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "update-step",
                "flow": "flows/main.ygtc",
                "step_id": "adaptive_update_mapped",
                "component": "components/component_adaptive_card__0_6_0.wasm",
                "mode": "default",
                "answers": {
                    "card_source": "inline",
                    "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"After update\"}]}",
                    "multilingual": false
                },
                "in_map": {
                    "source": "$.session.input"
                },
                "out_map": {
                    "target": "$.session.output"
                },
                "err_map": {
                    "target": "$.session.error"
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&update_answers_path)
        .assert()
        .success();

    let doc = load_ygtc_from_path(&pack_dir.join("flows/main.ygtc")).expect("load flow");
    let raw = &doc
        .nodes
        .get("adaptive_update_mapped")
        .expect("updated node")
        .raw;
    assert_eq!(
        raw.get("in_map"),
        Some(&json!({ "source": "$.session.input" }))
    );
    assert_eq!(
        raw.get("out_map"),
        Some(&json!({ "target": "$.session.output" }))
    );
    assert_eq!(
        raw.get("err_map"),
        Some(&json!({ "target": "$.session.error" }))
    );
}

#[test]
fn wizard_answers_plan_add_step_preserves_operation_and_routing() {
    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    begin: {}
    routing:
      - to: done
  done:
    finish: {}
    routing: out
"#,
    )
    .unwrap();

    let fixture_dir = dir.path().join("fixtures");
    let reference = "oci://acme/widget:1";
    write_fixture_wizard_component(
        &fixture_dir,
        reference,
        &["run", "send"],
        json!({"foo":"bar"}),
    );

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "after": "start",
                "step_id": "widget",
                "component": reference,
                "mode": "default",
                "operation": "send",
                "routing": [{ "to": "done" }],
                "answers": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .env(
            "GREENTIC_FLOW_WIZARD_RESOLVER",
            format!("fixture://{}", fixture_dir.display()),
        )
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let widget = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .and_then(|nodes| nodes.get(Value::from("widget")))
        .and_then(Value::as_mapping)
        .expect("widget node");
    assert!(widget.contains_key(Value::from("send")));
    assert_eq!(
        serde_json::to_value(widget.get(Value::from("routing")).unwrap()).unwrap(),
        json!([{ "to": "done" }])
    );
}

#[test]
fn wizard_answers_plan_add_step_preserves_route_conditions() {
    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        r#"id: main
type: messaging
schema_version: 2
nodes:
  start:
    begin: {}
    routing:
      - to: third
  third:
    finish: {}
    routing: out
"#,
    )
    .unwrap();

    let fixture_dir = dir.path().join("fixtures");
    let reference = "oci://acme/widget:1";
    write_fixture_wizard_component(
        &fixture_dir,
        reference,
        &["run", "send"],
        json!({"foo":"bar"}),
    );

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "add-step",
                "flow": "flows/main.ygtc",
                "after": "start",
                "step_id": "widget",
                "component": reference,
                "mode": "default",
                "operation": "send",
                "routing": [
                    { "condition": "response.action == \"go\"", "to": "third" },
                    { "out": true }
                ],
                "answers": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .env(
            "GREENTIC_FLOW_WIZARD_RESOLVER",
            format!("fixture://{}", fixture_dir.display()),
        )
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let widget = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .and_then(|nodes| nodes.get(Value::from("widget")))
        .and_then(Value::as_mapping)
        .expect("widget node");
    let routing = serde_json::to_value(widget.get(Value::from("routing")).unwrap()).unwrap();
    assert_eq!(
        routing,
        json!([
            { "condition": "response.action == \"go\"", "to": "third" },
            { "out": true }
        ])
    );
}

#[test]
fn wizard_answers_plan_update_step_preserves_operation_and_route_array() {
    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        r#"id: main
type: messaging
schema_version: 2
nodes:
  widget:
    run:
      component: oci://acme/widget:1
      config: {}
    routing: out
"#,
    )
    .unwrap();

    let fixture_dir = dir.path().join("fixtures");
    let reference = "oci://acme/widget:1";
    write_fixture_wizard_component(
        &fixture_dir,
        reference,
        &["run", "send"],
        json!({"foo":"bar"}),
    );

    let answers_path = pack_dir.join("update-answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "update-step",
                "flow": "flows/main.ygtc",
                "step_id": "widget",
                "component": reference,
                "mode": "default",
                "operation": "send",
                "routing": "reply",
                "answers": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .env(
            "GREENTIC_FLOW_WIZARD_RESOLVER",
            format!("fixture://{}", fixture_dir.display()),
        )
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let widget = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .and_then(|nodes| nodes.get(Value::from("widget")))
        .and_then(Value::as_mapping)
        .expect("widget node");
    assert!(widget.contains_key(Value::from("send")));
    assert_eq!(
        widget.get(Value::from("routing")).and_then(Value::as_str),
        Some("reply")
    );
}

#[test]
fn wizard_answers_plan_update_step_preserves_route_conditions() {
    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        r#"id: main
type: messaging
schema_version: 2
nodes:
  widget:
    run:
      component: oci://acme/widget:1
      config: {}
    routing: out
  third:
    finish: {}
    routing: out
"#,
    )
    .unwrap();

    let fixture_dir = dir.path().join("fixtures");
    let reference = "oci://acme/widget:1";
    write_fixture_wizard_component(
        &fixture_dir,
        reference,
        &["run", "send"],
        json!({"foo":"bar"}),
    );

    let answers_path = pack_dir.join("update-answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [{
                "action": "update-step",
                "flow": "flows/main.ygtc",
                "step_id": "widget",
                "component": reference,
                "mode": "default",
                "operation": "send",
                "routing": [
                    { "condition": "response.action == \"go\"", "to": "third" },
                    { "out": true }
                ],
                "answers": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .env(
            "GREENTIC_FLOW_WIZARD_RESOLVER",
            format!("fixture://{}", fixture_dir.display()),
        )
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let widget = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .and_then(|nodes| nodes.get(Value::from("widget")))
        .and_then(Value::as_mapping)
        .expect("widget node");
    let routing = serde_json::to_value(widget.get(Value::from("routing")).unwrap()).unwrap();
    assert_eq!(
        routing,
        json!([
            { "condition": "response.action == \"go\"", "to": "third" },
            { "out": true }
        ])
    );
}

#[test]
fn wizard_answers_plan_two_phase_llm_handle_message_authoring() {
    let source_wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-llm-openai")
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("component_llm_openai.wasm");
    if !source_wasm.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::create_dir_all(pack_dir.join("components")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .unwrap();
    fs::copy(
        &source_wasm,
        pack_dir.join("components/component_llm_openai.wasm"),
    )
    .unwrap();

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [
                {
                    "action": "add-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "research_planner",
                    "component": "components/component_llm_openai.wasm",
                    "mode": "setup",
                    "answers": {
                        "provider": "ollama",
                        "base_url": "http://127.0.0.1:11434/v1",
                        "default_model": "llama3.2",
                        "endpoint_requires_api_key": false
                    }
                },
                {
                    "action": "update-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "research_planner",
                    "component": "components/component_llm_openai.wasm",
                    "mode": "setup",
                    "operation": "handle_message",
                    "answers": {
                        "provider": "ollama",
                        "base_url": "http://127.0.0.1:11434/v1",
                        "default_model": "llama3.2",
                        "endpoint_requires_api_key": false
                    },
                    "in_map": {
                        "config": "$.config.llm",
                        "input": {
                            "messages": "$.input.messages"
                        }
                    },
                    "routing": "out"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let llm = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .and_then(|nodes| nodes.get(Value::from("research_planner")))
        .and_then(Value::as_mapping)
        .expect("research_planner node");
    assert!(llm.contains_key(Value::from("handle_message")));
    assert_eq!(
        serde_json::to_value(llm.get(Value::from("in_map")).unwrap()).unwrap(),
        json!({
            "config": "$.config.llm",
            "input": {
                "messages": "$.input.messages"
            }
        })
    );
}

#[test]
fn wizard_answers_plan_deep_research_slice_preserves_orchestration() {
    let adaptive_card_wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-adaptive-card")
        .join("dist")
        .join("component_adaptive_card__0_6_0.wasm");
    let llm_wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace parent")
        .join("component-llm-openai")
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("component_llm_openai.wasm");
    if !adaptive_card_wasm.exists() || !llm_wasm.exists() {
        return;
    }

    let dir = tempdir().unwrap();
    let pack_dir = dir.path().join("pack");
    fs::create_dir_all(pack_dir.join("flows")).unwrap();
    fs::create_dir_all(pack_dir.join("components")).unwrap();
    fs::write(
        pack_dir.join("flows/main.ygtc"),
        "id: main\ntype: messaging\nschema_version: 2\nnodes: {}\n",
    )
    .unwrap();
    fs::copy(
        &adaptive_card_wasm,
        pack_dir.join("components/component_adaptive_card__0_6_0.wasm"),
    )
    .unwrap();
    fs::copy(
        &llm_wasm,
        pack_dir.join("components/component_llm_openai.wasm"),
    )
    .unwrap();

    let answers_path = pack_dir.join("answers.json");
    fs::write(
        &answers_path,
        serde_json::to_string_pretty(&json!({
            "schema_id": "greentic-flow.wizard.plan",
            "schema_version": "2.0.0",
            "actions": [
                {
                    "action": "add-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "main_menu",
                    "component": "components/component_adaptive_card__0_6_0.wasm",
                    "mode": "default",
                    "answers": {
                        "card_source": "inline",
                        "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Menu\"}]}",
                        "multilingual": false
                    }
                },
                {
                    "action": "add-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "research_planner",
                    "component": "components/component_llm_openai.wasm",
                    "mode": "setup",
                    "answers": {
                        "provider": "ollama",
                        "base_url": "http://127.0.0.1:11434/v1",
                        "default_model": "llama3.2",
                        "endpoint_requires_api_key": false
                    }
                },
                {
                    "action": "add-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "show_final_report",
                    "component": "components/component_adaptive_card__0_6_0.wasm",
                    "mode": "default",
                    "answers": {
                        "card_source": "inline",
                        "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Report\"}]}",
                        "multilingual": false
                    }
                },
                {
                    "action": "update-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "main_menu",
                    "component": "components/component_adaptive_card__0_6_0.wasm",
                    "mode": "default",
                    "answers": {
                        "card_source": "inline",
                        "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Menu\"}]}",
                        "multilingual": false
                    },
                    "routing": [
                        { "condition": "response.action == \"create_research_plan\"", "to": "research_planner" },
                        { "out": true }
                    ]
                },
                {
                    "action": "update-step",
                    "flow": "flows/main.ygtc",
                    "step_id": "research_planner",
                    "component": "components/component_llm_openai.wasm",
                    "mode": "setup",
                    "operation": "handle_message",
                    "answers": {
                        "provider": "ollama",
                        "base_url": "http://127.0.0.1:11434/v1",
                        "default_model": "llama3.2",
                        "endpoint_requires_api_key": false
                    },
                    "in_map": {
                        "config": "$.config.llm",
                        "input": {
                            "messages": "$.input.messages"
                        }
                    },
                    "routing": [{ "to": "show_final_report" }]
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("wizard")
        .arg(&pack_dir)
        .arg("--answers")
        .arg(&answers_path)
        .assert()
        .success();

    let yaml = read_yaml(&pack_dir.join("flows/main.ygtc"));
    let nodes = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .expect("nodes mapping");

    let main_menu = nodes
        .get(Value::from("main_menu"))
        .and_then(Value::as_mapping)
        .expect("main_menu node");
    assert_eq!(
        serde_json::to_value(main_menu.get(Value::from("routing")).unwrap()).unwrap(),
        json!([
            { "condition": "response.action == \"create_research_plan\"", "to": "research_planner" },
            { "out": true }
        ])
    );

    let planner = nodes
        .get(Value::from("research_planner"))
        .and_then(Value::as_mapping)
        .expect("research_planner node");
    assert!(planner.contains_key(Value::from("handle_message")));
    assert_eq!(
        serde_json::to_value(planner.get(Value::from("in_map")).unwrap()).unwrap(),
        json!({
            "config": "$.config.llm",
            "input": {
                "messages": "$.input.messages"
            }
        })
    );
}

#[test]
fn add_step_rejects_invalid_component_scheme() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    fs::write(dir.path().join("comp.wasm"), b"bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--component")
        .arg("badref")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "--component must start with oci://, repo://, or store://",
        ));
}

#[test]
fn add_step_rejects_private_oci_host() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    fs::write(dir.path().join("comp.wasm"), b"bytes").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("comp")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg(r#"{}"#)
        .arg("--component")
        .arg("oci://localhost/component:latest")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "oci:// references must use a public registry host",
        ));
}

#[test]
fn update_step_rejects_invalid_component_scheme() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op:
      field: old
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--component")
        .arg("badref")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "--component must start with oci://, repo://, or store://",
        ));
}

#[test]
fn update_step_rejects_private_oci_host() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op:
      field: old
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--component")
        .arg("oci://localhost/component:latest")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "oci:// references must use a public registry host",
        ));
}

#[test]
fn doctor_prunes_sidecar_to_match_flow() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  keep:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"old.ygtc","nodes":{"keep":{"source":{"kind":"local","path":"comp.wasm"}},"stale":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&flow_path)
        .write_stdin("y\n")
        .assert()
        .success();

    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    assert_eq!(
        sidecar.get("flow").and_then(JsonValue::as_str),
        flow_path.file_name().and_then(|s| s.to_str())
    );
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    assert!(nodes.contains_key("keep"));
    assert!(!nodes.contains_key("stale"));
}

#[test]
fn doctor_reports_unused_sidecar_when_denied() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  keep:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"keep":{"source":{"kind":"local","path":"comp.wasm"}},"stale":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .arg("doctor")
        .arg(&flow_path)
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(predicates::str::contains("unused sidecar entries"));

    let sidecar: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    let nodes = sidecar.get("nodes").and_then(JsonValue::as_object).unwrap();
    assert!(nodes.contains_key("stale"));
}

#[test]
fn doctor_reports_invalid_sidecar_source() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"oci","ref":"oci://localhost/component:latest"}}}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid sidecar entries"));
}

#[test]
fn doctor_reports_missing_sidecar_entries() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("missing sidecar entries"));
}

#[test]
fn doctor_reports_missing_local_wasm() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"missing.wasm"}}}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid sidecar entries"));
}

#[test]
fn doctor_accepts_file_uri_local_wasm() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op: {}
    routing: out
"#,
    )
    .unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"file://comp.wasm"}}}}"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .current_dir(dir.path())
        .arg("doctor")
        .arg(&flow_path)
        .assert()
        .success();
}

#[test]
fn update_step_overrides_payload_and_routing() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar_path = flow_path.with_extension("ygtc.resolve.json");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    fs::write(
        &sidecar_path,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"hello":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
        )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  hello:
    op:
      field: old
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("update-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("hello")
        .arg("--answers")
        .arg(r#"{"field":"new","extra":1}"#)
        .arg("--routing-reply")
        .arg("--write")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .expect("nodes map");
    let hello = nodes
        .get(Value::from("hello"))
        .unwrap()
        .as_mapping()
        .unwrap();
    let op = hello.get(Value::from("op")).unwrap().as_mapping().unwrap();
    assert_eq!(op.get(Value::from("field")).unwrap().as_str(), Some("new"));
    assert_eq!(
        hello.get(Value::from("routing")).unwrap().as_str(),
        Some("reply")
    );
}

#[test]
fn delete_step_splices_single_predecessor() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();
    let sidecar = dir.path().join("flow.ygtc.resolve.json");
    fs::write(
        &sidecar,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"mid":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  a:
    hop: {}
    routing:
      - to: mid
  mid:
    op: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .arg("--write")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .expect("nodes map");
    assert!(!nodes.contains_key(Value::from("mid")));
    let a = nodes.get(Value::from("a")).unwrap().as_mapping().unwrap();
    if let Some(r) = a.get(Value::from("routing")) {
        if let Some(s) = r.as_str() {
            assert_eq!(s, "out");
        } else if let Some(seq) = r.as_sequence() {
            assert!(
                seq.is_empty()
                    || seq
                        .iter()
                        .any(|v| v.get("out").and_then(Value::as_bool) == Some(true))
            );
        }
    }
}

#[test]
fn delete_step_errors_on_multiple_predecessors() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  a:
    hop: {}
    routing:
      - to: mid
  b:
    hop: {}
    routing:
      - to: mid
  mid:
    op: {}
    routing:
      - to: end
  end:
    noop: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .assert()
        .failure();
}

#[test]
fn delete_step_splice_all_predecessors() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar = dir.path().join("flow.ygtc.resolve.json");
    fs::write(
        &sidecar,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"mid":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  a:
    hop: {}
    routing:
      - to: mid
  b:
    hop: {}
    routing:
      - to: mid
  mid:
    op: {}
    routing:
      - to: end
  end:
    noop: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .arg("--if-multiple-predecessors")
        .arg("splice-all")
        .arg("--write")
        .assert()
        .success();

    let yaml = read_yaml(&flow_path);
    let nodes = yaml
        .get("nodes")
        .and_then(Value::as_mapping)
        .expect("nodes map");
    assert!(!nodes.contains_key(Value::from("mid")));
    for pred in ["a", "b"] {
        let n = nodes.get(Value::from(pred)).unwrap().as_mapping().unwrap();
        let routing = n.get(Value::from("routing")).unwrap();
        if let Some(arr) = routing.as_sequence() {
            assert_eq!(
                arr[0].get("to").and_then(Value::as_str).expect("to target"),
                "end"
            );
        } else if let Some(s) = routing.as_str() {
            assert_eq!(s, "out");
        } else {
            panic!("unexpected routing shape");
        }
    }
}

#[test]
fn delete_step_removes_sidecar_mapping() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    let sidecar = flow_path.with_extension("ygtc.resolve.json");
    fs::write(
        &sidecar,
        r#"{"schema_version":1,"flow":"flow.ygtc","nodes":{"mid":{"source":{"kind":"local","path":"comp.wasm"}}}}"#,
    )
    .unwrap();
    fs::write(
        &flow_path,
        r#"id: main
type: messaging
schema_version: 2
nodes:
  a:
    hop: {}
    routing:
      - to: mid
  mid:
    op: {}
    routing: out
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("delete-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--step")
        .arg("mid")
        .arg("--write")
        .assert()
        .success();

    let sidecar_json: JsonValue =
        serde_json::from_str(&fs::read_to_string(&sidecar).unwrap()).unwrap();
    assert!(
        !sidecar_json
            .get("nodes")
            .and_then(JsonValue::as_object)
            .unwrap()
            .contains_key("mid")
    );
}

#[test]
fn add_step_rejects_empty_manifest_schema_by_default() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    cargo_bin_cmd!("greentic-flow")
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    fs::write(
        dir.path().join("component.manifest.json"),
        r#"{"id":"ai.greentic.empty","operations":[{"name":"run","input_schema":{}}]}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("empty")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg("{}")
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .failure()
        .stderr(predicates::str::contains("E_SCHEMA_EMPTY"));
}

#[test]
fn add_step_warns_on_empty_manifest_schema_with_permissive_flag() {
    let dir = tempdir().unwrap();
    let flow_path = dir.path().join("flow.ygtc");
    cargo_bin_cmd!("greentic-flow")
        .arg("new")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--id")
        .arg("main")
        .arg("--type")
        .arg("messaging")
        .assert()
        .success();

    let wasm_path = dir.path().join("comp.wasm");
    fs::write(&wasm_path, b"wasm-bytes").unwrap();

    fs::write(
        dir.path().join("component.manifest.json"),
        r#"{"id":"ai.greentic.empty","operations":[{"name":"run","input_schema":{}}]}"#,
    )
    .unwrap();

    cargo_bin_cmd!("greentic-flow")
        .current_dir(dir.path())
        .arg("--permissive")
        .arg("add-step")
        .arg("--flow")
        .arg(&flow_path)
        .arg("--mode")
        .arg("default")
        .arg("--node-id")
        .arg("empty")
        .arg("--operation")
        .arg("run")
        .arg("--payload")
        .arg("{}")
        .arg("--routing-out")
        .arg("--local-wasm")
        .arg("comp.wasm")
        .assert()
        .success()
        .stderr(predicates::str::contains("W_SCHEMA_EMPTY"));
}

#[test]
fn answers_error_on_empty_question_graph_by_default() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("component.manifest.json");
    let manifest = json!({
        "id": "ai.greentic.empty",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "ask",
                    "nodes": {
                        "ask": {
                            "template": "{}"
                        }
                    }
                }
            }
        }
    });
    fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("answers")
        .arg("--component")
        .arg(&manifest_path)
        .arg("--operation")
        .arg("default")
        .arg("--name")
        .arg("empty")
        .assert()
        .failure()
        .stderr(predicates::str::contains("E_SCHEMA_EMPTY"));
}

#[test]
fn answers_warns_on_empty_question_graph_with_permissive() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("component.manifest.json");
    let manifest = json!({
        "id": "ai.greentic.empty",
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "ask",
                    "nodes": {
                        "ask": {
                            "template": "{}"
                        }
                    }
                }
            }
        }
    });
    fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("--permissive")
        .arg("answers")
        .arg("--component")
        .arg(&manifest_path)
        .arg("--operation")
        .arg("default")
        .arg("--name")
        .arg("empty")
        .arg("--out-dir")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("W_SCHEMA_EMPTY"));
}

#[test]
fn answers_prefers_operations_schema_when_dev_flow_questions_empty() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("component.manifest.json");
    let manifest = json!({
        "id": "ai.greentic.empty-with-ops",
        "operations": [
            {
                "name": "default",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "foo": {
                            "type": "string"
                        }
                    }
                }
            }
        ],
        "dev_flows": {
            "default": {
                "graph": {
                    "id": "cfg",
                    "type": "component-config",
                    "start": "ask",
                    "nodes": {
                        "ask": {
                            "questions": {
                                "fields": []
                            },
                            "routing": [
                                { "to": "emit" }
                            ]
                        },
                        "emit": {
                            "template": "{}"
                        }
                    }
                }
            }
        }
    });
    fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("greentic-flow"))
        .arg("answers")
        .arg("--component")
        .arg(&manifest_path)
        .arg("--operation")
        .arg("default")
        .arg("--name")
        .arg("empty-with-ops")
        .arg("--out-dir")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("E_SCHEMA_EMPTY").not());
}
