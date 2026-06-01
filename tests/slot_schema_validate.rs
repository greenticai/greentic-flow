use jsonschema::Draft;
use serde_json::{Value, json};

fn flow_schema() -> Value {
    serde_json::from_str(include_str!("../schemas/ygtc.flow.schema.json"))
        .expect("parse embedded schema")
}

/// Build a minimal valid flow document with the given `slot_schema` array.
fn flow_with_slots(slots: Value) -> Value {
    json!({
        "id": "test-flow",
        "type": "flow",
        "slot_schema": slots,
        "nodes": {
            "start": {
                "component": "echo",
                "routing": "out"
            }
        }
    })
}

fn validates(schema: &Value, instance: &Value) -> bool {
    let compiled = jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(schema)
        .expect("compile schema");
    compiled.validate(instance).is_ok()
}

// ── reject cases ────────────────────────────────────────────────────

#[test]
fn slot_schema_rejects_string_slot_without_pattern() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "city", "slot_type": "string" }
    ]));
    assert!(
        !validates(&schema, &doc),
        "string slot without pattern must be rejected"
    );
}

#[test]
fn slot_schema_rejects_enum_slot_without_enum_values() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "color", "slot_type": "enum" }
    ]));
    assert!(
        !validates(&schema, &doc),
        "enum slot without enum_values must be rejected"
    );
}

#[test]
fn slot_schema_rejects_enum_slot_with_empty_enum_values() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "color", "slot_type": "enum", "enum_values": [] }
    ]));
    assert!(
        !validates(&schema, &doc),
        "enum slot with empty enum_values must be rejected"
    );
}

#[test]
fn slot_schema_rejects_string_slot_with_empty_pattern() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "city", "slot_type": "string", "pattern": "" }
    ]));
    assert!(
        !validates(&schema, &doc),
        "string slot with empty pattern must be rejected"
    );
}

// ── accept cases ────────────────────────────────────────────────────

#[test]
fn slot_schema_accepts_valid_string_slot() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "city", "slot_type": "string", "pattern": "^[A-Z].+" }
    ]));
    assert!(
        validates(&schema, &doc),
        "valid string slot with non-empty pattern must be accepted"
    );
}

#[test]
fn slot_schema_accepts_valid_enum_slot() {
    let schema = flow_schema();
    let doc = flow_with_slots(json!([
        { "name": "color", "slot_type": "enum", "enum_values": ["red", "blue"] }
    ]));
    assert!(
        validates(&schema, &doc),
        "valid enum slot with non-empty enum_values must be accepted"
    );
}

#[test]
fn slot_schema_accepts_unconstrained_types_without_pattern_or_enum_values() {
    let schema = flow_schema();
    for slot_type in &["number", "date", "boolean"] {
        let doc = flow_with_slots(json!([
            { "name": "field", "slot_type": slot_type }
        ]));
        assert!(
            validates(&schema, &doc),
            "{slot_type} slot without pattern or enum_values must be accepted"
        );
    }
}
