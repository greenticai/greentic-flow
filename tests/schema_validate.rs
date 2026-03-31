use ciborium::value::Value as CborValue;
use greentic_flow::schema_validate::{Severity, validate_value_against_schema};
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};

#[test]
fn schema_validate_reports_required_missing() {
    let schema = SchemaIr::Object {
        properties: [(
            "name".to_string(),
            SchemaIr::String {
                min_len: None,
                max_len: None,
                regex: None,
                format: None,
            },
        )]
        .into_iter()
        .collect(),
        required: vec!["name".to_string()],
        additional: AdditionalProperties::Allow,
    };
    let value = CborValue::Map(Vec::new());
    let diags = validate_value_against_schema(&schema, &value);
    assert!(diags.iter().any(|d| d.code == "SCHEMA_REQUIRED_MISSING"));
}

#[test]
fn schema_validate_reports_type_mismatch() {
    let schema = SchemaIr::String {
        min_len: None,
        max_len: None,
        regex: None,
        format: None,
    };
    let value = CborValue::Bool(true);
    let diags = validate_value_against_schema(&schema, &value);
    assert!(diags.iter().any(|d| d.code == "SCHEMA_TYPE_MISMATCH"));
}

#[test]
fn schema_validate_forbids_additional_properties() {
    let schema = SchemaIr::Object {
        properties: std::collections::BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Forbid,
    };
    let value = CborValue::Map(vec![(
        CborValue::Text("extra".to_string()),
        CborValue::Bool(true),
    )]);
    let diags = validate_value_against_schema(&schema, &value);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "SCHEMA_ADDITIONAL_FORBIDDEN")
    );
}

#[test]
fn schema_validate_warns_on_regex() {
    let schema = SchemaIr::String {
        min_len: None,
        max_len: None,
        regex: Some("^foo$".to_string()),
        format: None,
    };
    let value = CborValue::Text("foo".to_string());
    let diags = validate_value_against_schema(&schema, &value);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "SCHEMA_REGEX_UNSUPPORTED" && d.severity == Severity::Warning)
    );
}

#[test]
fn schema_validate_reports_array_bounds_and_nested_item_errors() {
    let schema = SchemaIr::Array {
        items: Box::new(SchemaIr::Int {
            min: Some(2),
            max: Some(4),
        }),
        min_items: Some(2),
        max_items: Some(2),
    };
    let value = CborValue::Array(vec![CborValue::Integer(1.into())]);
    let diags = validate_value_against_schema(&schema, &value);
    assert!(diags.iter().any(|d| d.code == "SCHEMA_ARRAY_MIN_ITEMS"));
    assert!(diags.iter().any(|d| d.code == "SCHEMA_INT_MIN"));
}

#[test]
fn schema_validate_checks_float_enum_and_one_of() {
    let float_diags = validate_value_against_schema(
        &SchemaIr::Float {
            min: Some(1.5),
            max: Some(2.0),
        },
        &CborValue::Float(2.5),
    );
    assert!(float_diags.iter().any(|d| d.code == "SCHEMA_FLOAT_MAX"));

    let enum_diags = validate_value_against_schema(
        &SchemaIr::Enum {
            values: vec![CborValue::Text("red".to_string())],
        },
        &CborValue::Text("blue".to_string()),
    );
    assert!(enum_diags.iter().any(|d| d.code == "SCHEMA_ENUM"));

    let one_of_diags = validate_value_against_schema(
        &SchemaIr::OneOf {
            variants: vec![
                SchemaIr::String {
                    min_len: Some(4),
                    max_len: None,
                    regex: None,
                    format: None,
                },
                SchemaIr::Bool,
            ],
        },
        &CborValue::Integer(1.into()),
    );
    assert!(one_of_diags.iter().any(|d| d.code == "SCHEMA_ONE_OF"));
}

#[test]
fn schema_validate_reports_invalid_object_keys_additional_schema_and_refs() {
    let schema = SchemaIr::Object {
        properties: std::collections::BTreeMap::new(),
        required: Vec::new(),
        additional: AdditionalProperties::Schema(Box::new(SchemaIr::String {
            min_len: Some(3),
            max_len: None,
            regex: None,
            format: Some("email".to_string()),
        })),
    };
    let value = CborValue::Map(vec![
        (CborValue::Integer(1.into()), CborValue::Bool(true)),
        (
            CborValue::Text("extra".to_string()),
            CborValue::Text("x".to_string()),
        ),
    ]);
    let diags = validate_value_against_schema(&schema, &value);
    assert!(diags.iter().any(|d| d.code == "SCHEMA_INVALID_KEY"));
    assert!(diags.iter().any(|d| d.code == "SCHEMA_STRING_MIN_LEN"));
    assert!(diags.iter().any(|d| d.code == "SCHEMA_FORMAT_UNSUPPORTED"));

    let ref_diags = validate_value_against_schema(
        &SchemaIr::Ref {
            id: "schema://ref".to_string(),
        },
        &CborValue::Null,
    );
    assert!(ref_diags.iter().any(|d| d.code == "SCHEMA_REF_UNSUPPORTED"));
}
