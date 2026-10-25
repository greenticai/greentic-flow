use ciborium::value::Value as CborValue;
use greentic_flow::i18n::{I18nCatalog, resolve_keys};
use greentic_flow::schema_validate::validate_value_against_schema;
use greentic_types::schemas::common::schema_ir::SchemaIr;
use std::collections::BTreeSet;

fn string_schema() -> SchemaIr {
    SchemaIr::String {
        min_len: None,
        max_len: None,
        regex: None,
        format: None,
    }
}

#[test]
fn one_of_accepts_value_matching_multiple_variants() {
    let schema = SchemaIr::OneOf {
        variants: vec![string_schema(), string_schema()],
    };
    let diags = validate_value_against_schema(&schema, &CborValue::Text("x".into()));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn one_of_accepts_exactly_one_match() {
    let schema = SchemaIr::OneOf {
        variants: vec![string_schema(), SchemaIr::Bool],
    };
    let diags = validate_value_against_schema(&schema, &CborValue::Text("x".into()));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn resolve_keys_errors_on_missing_translation() {
    let mut catalog = I18nCatalog::default();
    catalog.insert("a", "en", "Alpha".to_string());
    let keys: BTreeSet<String> = ["a".to_string(), "missing".to_string()].into();
    assert!(resolve_keys(&keys, &catalog, "en").is_err());
    let only_a: BTreeSet<String> = ["a".to_string()].into();
    assert_eq!(resolve_keys(&only_a, &catalog, "en").unwrap()["a"], "Alpha");
}

#[test]
fn config_schema_temp_file_matches_embedded_schema() {
    let path = greentic_flow::loader::ensure_config_schema_path().unwrap();
    let on_disk = std::fs::read_to_string(&path).unwrap();
    let embedded = include_str!("../schemas/ygtc.flow.schema.json");
    assert_eq!(on_disk, embedded);
}

#[test]
/// Smoke test only: the path-based loader re-reads the schema file on every call (the validator
/// cache is keyed by schema text), so this checks repeated loads stay correct, not cache hits.
fn custom_schema_loads_repeatedly() {
    let dir = tempfile::tempdir().unwrap();
    let schema_path = dir.path().join("schema.json");
    std::fs::write(
        &schema_path,
        include_str!("../schemas/ygtc.flow.schema.json"),
    )
    .unwrap();
    let yaml =
        "id: x\ntype: messaging\nnodes:\n  in:\n    qa.process:\n      a: 1\n    routing: out\n";
    for _ in 0..3 {
        greentic_flow::loader::load_ygtc_from_str_with_schema(yaml, &schema_path).unwrap();
    }
}
