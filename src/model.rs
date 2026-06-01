use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

fn default_parameters() -> Value {
    Value::Object(Default::default())
}

fn default_entrypoints() -> IndexMap<String, Value> {
    IndexMap::new()
}

fn default_routing() -> Value {
    Value::Array(Vec::new())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub flow_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default = "default_parameters")]
    pub parameters: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default = "default_entrypoints")]
    pub entrypoints: IndexMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// Slot definitions for utterance-driven prefill. Free-form `Value` to
    /// avoid coupling greentic-flow to a specific extractor crate; expected
    /// to be a JSON array of `SlotDefinition` objects matching the
    /// `component-slot-extractor` wire shape (`name`, `slot_type`,
    /// `pattern?`, `required`, `enum_values?`, `default_value?`).
    /// Consumed by the canonical M2 chain: Fast2Flow `Dispatch{utterance}`
    /// → slot-extractor node (utterance + this schema) → adaptive-card node
    /// with `prefill = extractor output`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_schema: Option<Value>,
    pub nodes: IndexMap<String, NodeDoc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeDoc {
    #[serde(default = "default_routing")]
    pub routing: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryDoc>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub operation: Option<String>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub payload: Value,
    #[serde(flatten, default)]
    pub raw: IndexMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelemetryDoc {
    #[serde(default)]
    pub span_name: Option<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub sampling: Option<String>,
}
