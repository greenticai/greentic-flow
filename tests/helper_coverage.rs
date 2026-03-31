use ciborium::value::Value as CborValue;
use greentic_flow::{
    path_safety::normalize_under_root,
    resolve::resolve_parameters,
    schema_mode::SchemaMode,
    util::is_valid_component_key,
    wizard_ops::{
        WizardAbi, WizardMode, abi_version_from_abi, answers_to_cbor, canonicalize_answers_map,
        cbor_to_json, cbor_value_to_json, decode_component_qa_spec, describe_exports_for_meta,
        empty_cbor_map, ensure_answers_object, json_to_cbor, merge_default_answers,
        qa_spec_to_questions,
    },
};
use greentic_types::i18n_text::I18nText;
use greentic_types::schemas::component::v0_6_0::{ChoiceOption, ComponentQaSpec, QaMode, Question, QuestionKind};
use serde_json::{Map, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn normalize_under_root_accepts_nested_files_and_rejects_escape_attempts() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("safe");
    std::fs::create_dir_all(&nested).unwrap();
    let file = nested.join("schema.json");
    std::fs::write(&file, "{}").unwrap();

    let resolved = normalize_under_root(dir.path(), Path::new("safe/schema.json")).unwrap();
    assert_eq!(resolved, file.canonicalize().unwrap());

    let err = normalize_under_root(dir.path(), Path::new("../escape.json"))
        .expect_err("escape path should fail");
    assert!(format!("{err:#}").contains("failed to canonicalize"));

    let err = normalize_under_root(dir.path(), file.as_path()).expect_err("absolute path should fail");
    assert!(format!("{err:#}").contains("absolute paths are not allowed"));

    #[cfg(unix)]
    {
        let outside = dir.path().join("../outside-schema.json");
        std::fs::write(&outside, "{}").unwrap();
        std::os::unix::fs::symlink(&outside, dir.path().join("escape-link")).unwrap();
        let err = normalize_under_root(dir.path(), Path::new("escape-link"))
            .expect_err("symlink escape should fail");
        assert!(format!("{err:#}").contains("path escapes root"));
    }
}

#[test]
fn schema_mode_uses_cli_and_environment_controls() {
    unsafe { std::env::remove_var("GREENTIC_FLOW_STRICT") };
    assert_eq!(SchemaMode::resolve(false).unwrap(), SchemaMode::Strict);
    assert!(SchemaMode::resolve(true).unwrap().is_permissive());

    unsafe { std::env::set_var("GREENTIC_FLOW_STRICT", "0") };
    assert_eq!(SchemaMode::resolve(false).unwrap(), SchemaMode::Permissive);

    unsafe { std::env::set_var("GREENTIC_FLOW_STRICT", "nope") };
    let err = SchemaMode::resolve(false).expect_err("invalid env value should fail");
    assert!(format!("{err}").contains("must be '0' or '1'"));
    unsafe { std::env::remove_var("GREENTIC_FLOW_STRICT") };
}

#[test]
fn resolve_parameters_only_expands_parameters_references() {
    let value = json!({
        "kept": "state.name",
        "nested": ["parameters.user.name", "literal"]
    });
    let parameters = json!({
        "user": {
            "name": "Ada"
        }
    });

    let resolved = resolve_parameters(&value, &parameters, "$").unwrap();
    assert_eq!(resolved["kept"], "state.name");
    assert_eq!(resolved["nested"][0], "Ada");

    let err = resolve_parameters(&json!("parameters.user.name"), &json!(null), "$.field")
        .expect_err("non-object parameters should fail");
    assert!(format!("{err}").contains("not an object"));
}

#[test]
fn wizard_ops_convert_and_validate_answer_payloads() {
    let mut answers = HashMap::new();
    answers.insert("enabled".to_string(), json!(true));
    let cbor = answers_to_cbor(&answers).unwrap();
    assert_eq!(cbor_to_json(&cbor).unwrap(), json!({"enabled": true}));

    let json = json!({"nested": [1, 2], "name": "Ada"});
    let json_cbor = json_to_cbor(&json).unwrap();
    assert_eq!(cbor_to_json(&json_cbor).unwrap(), json);

    assert_eq!(cbor_value_to_json(&CborValue::Bytes(vec![1, 2])).unwrap(), json!([1, 2]));
    assert_eq!(cbor_value_to_json(&CborValue::Tag(42, Box::new(CborValue::Bool(true)))).unwrap(), json!(true));

    let err = cbor_value_to_json(&CborValue::Map(vec![(CborValue::Integer(1.into()), CborValue::Null)]))
        .expect_err("non-string map keys should fail");
    assert!(format!("{err}").contains("non-string map key"));

    let wide = cbor_value_to_json(&CborValue::Integer(u64::MAX.into())).unwrap();
    assert!(wide.is_string(), "wide integers should round-trip as strings");

    let err = cbor_value_to_json(&CborValue::Float(f64::NAN)).expect_err("nan should fail");
    assert!(format!("{err}").contains("float out of range"));
}

#[test]
fn wizard_ops_misc_helpers_preserve_contract_shapes() {
    ensure_answers_object(&json!({"ok": true})).unwrap();
    let err = ensure_answers_object(&json!(["bad"])).expect_err("arrays should fail");
    assert!(format!("{err}").contains("JSON object"));

    let mut map = Map::new();
    map.insert("b".to_string(), json!(2));
    map.insert("a".to_string(), json!(1));
    let canonical = canonicalize_answers_map(&map).unwrap();
    assert_eq!(cbor_to_json(&canonical).unwrap(), json!({"a": 1, "b": 2}));

    assert_eq!(empty_cbor_map(), vec![0xa0]);
    assert_eq!(describe_exports_for_meta(WizardAbi::V6), vec!["describe", "invoke"]);
    assert_eq!(abi_version_from_abi(WizardAbi::V6), "0.6.0");
    assert_eq!(WizardMode::Default.as_qa_mode(), QaMode::Default);
    assert_eq!(WizardMode::Setup.as_str(), "setup");
    assert_eq!(WizardMode::Setup.as_qa_mode(), QaMode::Setup);
    assert_eq!(WizardMode::Update.as_str(), "update");
    assert_eq!(WizardMode::Remove.as_str(), "remove");
    assert_eq!(WizardMode::Update.as_qa_mode(), QaMode::Update);
    assert_eq!(WizardMode::Remove.as_qa_mode(), QaMode::Remove);
}

#[test]
fn component_key_validation_allows_builtins_and_rejects_partial_names() {
    assert!(is_valid_component_key("acme.widget.run"));
    assert!(is_valid_component_key("questions"));
    assert!(is_valid_component_key("template"));
    assert!(!is_valid_component_key("widget"));
    assert!(!is_valid_component_key("1acme.widget.run"));
}

fn text(key: &str, fallback: &str) -> I18nText {
    I18nText::new(key, Some(fallback.to_string()))
}

#[test]
fn wizard_ops_decode_spec_and_project_questions_and_defaults() {
    let spec = ComponentQaSpec {
        mode: QaMode::Setup,
        title: text("setup.title", "Setup"),
        description: Some(text("setup.description", "Describe setup")),
        questions: vec![
            Question {
                id: "name".to_string(),
                label: text("question.name", "Name"),
                help: None,
                error: None,
                kind: QuestionKind::Text,
                required: true,
                default: Some(CborValue::Text("Ada".to_string())),
                skip_if: None,
            },
            Question {
                id: "mode".to_string(),
                label: text("question.mode", "Mode"),
                help: Some(text("question.mode.help", "Pick one")),
                error: None,
                kind: QuestionKind::Choice {
                    options: vec![
                        ChoiceOption {
                            value: "fast".to_string(),
                            label: text("option.fast", "Fast"),
                        },
                        ChoiceOption {
                            value: "safe".to_string(),
                            label: text("option.safe", "Safe"),
                        },
                    ],
                },
                required: true,
                default: Some(CborValue::Text("safe".to_string())),
                skip_if: None,
            },
            Question {
                id: "enabled".to_string(),
                label: text("question.enabled", "Enabled"),
                help: None,
                error: None,
                kind: QuestionKind::Bool,
                required: false,
                default: None,
                skip_if: None,
            },
            Question {
                id: "count".to_string(),
                label: text("question.count", "Count"),
                help: None,
                error: None,
                kind: QuestionKind::Number,
                required: false,
                default: None,
                skip_if: None,
            },
            Question {
                id: "payload".to_string(),
                label: text("question.payload", "Payload"),
                help: None,
                error: None,
                kind: QuestionKind::InlineJson { schema: None },
                required: false,
                default: None,
                skip_if: None,
            },
            Question {
                id: "asset".to_string(),
                label: text("question.asset", "Asset"),
                help: None,
                error: None,
                kind: QuestionKind::AssetRef {
                    file_types: vec!["json".to_string()],
                    base_path: Some("assets".to_string()),
                    check_exists: true,
                    allow_remote: true,
                },
                required: false,
                default: None,
                skip_if: None,
            },
        ],
        defaults: BTreeMap::from([("mode".to_string(), CborValue::Text("fast".to_string()))]),
    };

    let encoded = greentic_types::cbor::canonical::to_canonical_cbor(&spec).unwrap();
    let decoded = decode_component_qa_spec(&encoded, WizardMode::Setup).unwrap();
    assert_eq!(decoded.mode, QaMode::Setup);

    let questions = qa_spec_to_questions(&decoded, &Default::default(), "en");
    assert_eq!(questions.len(), 6);
    assert_eq!(questions[0].kind, greentic_flow::questions::QuestionKind::String);
    assert_eq!(questions[0].default, Some(json!("Ada")));
    assert_eq!(questions[1].choices, vec![json!("fast"), json!("safe")]);
    assert_eq!(questions[1].default, Some(json!("safe")));
    assert_eq!(questions[2].kind, greentic_flow::questions::QuestionKind::Bool);
    assert_eq!(questions[3].kind, greentic_flow::questions::QuestionKind::Float);
    assert_eq!(questions[4].kind, greentic_flow::questions::QuestionKind::String);
    assert_eq!(questions[5].kind, greentic_flow::questions::QuestionKind::String);

    let mut seed = HashMap::from([("enabled".to_string(), json!(false))]);
    merge_default_answers(&decoded, &mut seed);
    assert_eq!(seed.get("mode"), Some(&json!("fast")));
    assert_eq!(seed.get("enabled"), Some(&json!(false)));

    let invalid_defaults = ComponentQaSpec {
        defaults: BTreeMap::from([(
            "broken".to_string(),
            CborValue::Map(vec![(CborValue::Integer(1.into()), CborValue::Null)]),
        )]),
        ..decoded
    };
    merge_default_answers(&invalid_defaults, &mut seed);
    assert!(!seed.contains_key("broken"), "invalid default cbor should be ignored");
}

#[test]
fn wizard_ops_decode_spec_rejects_invalid_payloads() {
    let err = decode_component_qa_spec(b"not cbor or json", WizardMode::Default)
        .expect_err("invalid payload should fail");
    assert!(format!("{err}").contains("adapt legacy qa-spec json"));

    let err = decode_component_qa_spec(&[], WizardMode::Default)
        .expect_err("empty payload should fail");
    assert!(format!("{err}").contains("not valid CBOR or legacy JSON"));
}
