//! Shared test helpers. Each integration test imports this via `mod common;`
//! and `use common::*;`.

use jsonschema::{Draft, Validator};
use serde_json::Value;

pub fn build_validator(schema: &Value) -> Validator {
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(schema)
        .expect("compile flow schema")
}
