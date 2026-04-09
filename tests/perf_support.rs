use ciborium::value::Value as CborValue;
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};

pub fn medium_flow_yaml() -> String {
    let mut yaml = String::from("id: perf_flow\ntype: messaging\nnodes:\n");
    for idx in 0..24 {
        let node_id = format!("node_{idx:02}");
        let next_id = if idx + 1 < 24 {
            format!("node_{:02}", idx + 1)
        } else {
            "done".to_string()
        };
        let component = if idx % 2 == 0 {
            "qa.process"
        } else {
            "template"
        };
        yaml.push_str(&format!("  {node_id}:\n"));
        match component {
            "qa.process" => {
                yaml.push_str("    qa.process:\n");
                yaml.push_str(&format!("      prompt: \"node {idx}\"\n"));
            }
            _ => {
                yaml.push_str(&format!("    template: \"node {idx}\"\n"));
            }
        }
        yaml.push_str("    routing:\n");
        if idx % 3 == 0 {
            yaml.push_str("      - status: ok\n");
            yaml.push_str(&format!("        to: {next_id}\n"));
            yaml.push_str("      - to: done\n");
        } else {
            yaml.push_str(&format!("      - to: {next_id}\n"));
        }
    }
    yaml.push_str("  done:\n");
    yaml.push_str("    template: \"done\"\n");
    yaml.push_str("    routing: out\n");
    yaml.push_str("start: node_00\n");
    yaml
}

#[allow(dead_code)]
pub fn nested_schema() -> SchemaIr {
    SchemaIr::Object {
        properties: std::collections::BTreeMap::from([
            (
                "name".to_string(),
                SchemaIr::String {
                    min_len: Some(3),
                    max_len: Some(64),
                    regex: None,
                    format: None,
                },
            ),
            ("enabled".to_string(), SchemaIr::Bool),
            (
                "retries".to_string(),
                SchemaIr::Int {
                    min: Some(0),
                    max: Some(10),
                },
            ),
            (
                "thresholds".to_string(),
                SchemaIr::Array {
                    items: Box::new(SchemaIr::Float {
                        min: Some(0.0),
                        max: Some(1.0),
                    }),
                    min_items: Some(2),
                    max_items: Some(8),
                },
            ),
            (
                "labels".to_string(),
                SchemaIr::Object {
                    properties: std::collections::BTreeMap::new(),
                    required: Vec::new(),
                    additional: AdditionalProperties::Schema(Box::new(SchemaIr::String {
                        min_len: Some(1),
                        max_len: Some(32),
                        regex: None,
                        format: None,
                    })),
                },
            ),
        ]),
        required: vec![
            "name".to_string(),
            "enabled".to_string(),
            "retries".to_string(),
        ],
        additional: AdditionalProperties::Forbid,
    }
}

#[allow(dead_code)]
pub fn nested_value() -> CborValue {
    CborValue::Map(vec![
        (
            CborValue::Text("name".to_string()),
            CborValue::Text("performance-check".to_string()),
        ),
        (
            CborValue::Text("enabled".to_string()),
            CborValue::Bool(true),
        ),
        (
            CborValue::Text("retries".to_string()),
            CborValue::Integer(3.into()),
        ),
        (
            CborValue::Text("thresholds".to_string()),
            CborValue::Array(vec![
                CborValue::Float(0.25),
                CborValue::Float(0.5),
                CborValue::Float(0.75),
            ]),
        ),
        (
            CborValue::Text("labels".to_string()),
            CborValue::Map(vec![
                (
                    CborValue::Text("team".to_string()),
                    CborValue::Text("flow".to_string()),
                ),
                (
                    CborValue::Text("env".to_string()),
                    CborValue::Text("ci".to_string()),
                ),
            ]),
        ),
    ])
}
