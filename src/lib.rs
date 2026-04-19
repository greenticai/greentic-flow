//! Downstream runtimes must set the current tenant telemetry context via
//! `greentic_types::telemetry::set_current_tenant_ctx` before executing flows
//! (for example, prior to `FlowEngine::run` in the host runner).
#![deny(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod add_step;
pub mod answers;
pub mod cache;
pub mod component_catalog;
pub mod component_schema;
pub mod component_setup;
pub mod config_flow;
pub mod contracts;
pub mod error;
pub mod flow_bundle;
pub mod flow_ir;
pub mod flow_meta;
pub mod i18n;
pub mod ir;
pub mod json_output;
pub mod lint;
pub mod loader;
pub mod model;
pub mod path_safety;
pub mod qa_runner;
pub mod questions;
pub mod questions_schema;
pub mod registry;
pub mod resolve;
pub mod resolve_summary;
pub mod schema_mode;
pub mod schema_validate;
pub mod splice;
pub mod template;
pub mod util;
pub mod wizard;
pub mod wizard_ops;
pub mod wizard_state;

pub use flow_bundle::{
    ComponentPin, FlowBundle, NodeRef, blake3_hex, canonicalize_json, extract_component_pins,
    load_and_validate_bundle, load_and_validate_bundle_with_flow,
};
pub use json_output::{JsonDiagnostic, LintJsonOutput, lint_to_stdout_json};
pub use splice::{NEXT_NODE_PLACEHOLDER, splice_node_after};

use crate::{error::Result, model::FlowDoc};
use greentic_types::{
    ComponentId, Flow, FlowComponentRef, FlowId, FlowKind, FlowMetadata, InputMapping, Node,
    NodeId, OutputMapping, Routing, TelemetryHints, flow::FlowHasher,
};
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

/// Map a YAML flow type string to [`FlowKind`].
pub fn map_flow_type(flow_type: &str) -> Result<FlowKind> {
    match flow_type {
        "messaging" => Ok(FlowKind::Messaging),
        "event" | "events" => Ok(FlowKind::Event),
        "component-config" => Ok(FlowKind::ComponentConfig),
        "job" => Ok(FlowKind::Job),
        "http" => Ok(FlowKind::Http),
        other => Err(crate::error::FlowError::UnknownFlowType {
            flow_type: other.to_string(),
            location: crate::error::FlowErrorLocation::at_path("type"),
        }),
    }
}

/// Compile a validated [`FlowDoc`] into the canonical [`Flow`] model.
pub fn compile_flow(doc: FlowDoc) -> Result<Flow> {
    let FlowDoc {
        id,
        title,
        description,
        flow_type,
        start,
        parameters,
        tags,
        schema_version,
        mut entrypoints,
        meta: _,
        nodes: node_docs,
    } = doc;

    let kind = map_flow_type(&flow_type)?;
    let known_nodes: HashSet<String> = node_docs.keys().cloned().collect();
    if let Some(entry) = start
        .clone()
        .or_else(|| known_nodes.contains("in").then(|| "in".to_string()))
        .or_else(|| node_docs.keys().next().cloned())
    {
        entrypoints
            .entry("default".to_string())
            .or_insert_with(|| Value::String(entry));
    }

    let mut nodes: IndexMap<NodeId, Node, FlowHasher> = IndexMap::default();
    for (node_id_str, node_doc) in node_docs {
        let node_id = NodeId::new(node_id_str.as_str()).map_err(|e| {
            crate::error::FlowError::InvalidIdentifier {
                kind: "node",
                value: node_id_str.clone(),
                detail: e.to_string(),
                location: crate::error::FlowErrorLocation::at_path(format!("nodes.{node_id_str}")),
            }
        })?;
        let routing = compile_routing(&node_doc.routing, &known_nodes, node_id_str.as_str())?;
        let telemetry = node_doc
            .telemetry
            .map(|t| TelemetryHints {
                span_name: t.span_name,
                attributes: t.attributes,
                sampling: t.sampling,
            })
            .unwrap_or_default();
        let mut op_key: Option<String> = None;
        let mut payload: Option<Value> = None;
        let mut input_mapping: Option<Value> = None;
        let mut output_mapping: Option<Value> = None;
        let mut err_mapping: Option<Value> = None;
        for (k, v) in node_doc.raw {
            match k.as_str() {
                "in_map" => {
                    input_mapping = Some(v);
                    continue;
                }
                "out_map" | "output" => {
                    output_mapping = Some(v);
                    continue;
                }
                "err_map" => {
                    err_mapping = Some(v);
                    continue;
                }
                _ => {}
            }
            op_key = Some(k);
            payload = Some(v);
        }
        let operation = op_key.ok_or_else(|| crate::error::FlowError::Internal {
            message: format!("node '{node_id_str}' missing operation key"),
            location: crate::error::FlowErrorLocation::at_path(format!("nodes.{node_id_str}")),
        })?;
        let is_builtin = matches!(operation.as_str(), "questions" | "template");
        let is_legacy = schema_version.unwrap_or(1) < 2;
        let (component_id, op_field) = if is_builtin || is_legacy {
            (operation, None)
        } else {
            ("component.exec".to_string(), Some(operation))
        };
        let node = Node {
            id: node_id.clone(),
            component: FlowComponentRef {
                id: ComponentId::new(&component_id).unwrap(),
                pack_alias: None,
                operation: op_field,
            },
            input: InputMapping {
                mapping: input_mapping
                    .or(payload)
                    .unwrap_or_else(|| Value::Object(Default::default())),
            },
            output: OutputMapping {
                mapping: output_mapping.unwrap_or_else(|| Value::Object(Default::default())),
            },
            err_map: err_mapping.map(|mapping| OutputMapping { mapping }),
            routing,
            telemetry,
        };
        nodes.insert(node_id, node);
    }

    let flow_id =
        FlowId::new(id.as_str()).map_err(|e| crate::error::FlowError::InvalidIdentifier {
            kind: "flow",
            value: id.clone(),
            detail: e.to_string(),
            location: crate::error::FlowErrorLocation::at_path("id"),
        })?;

    let entrypoints_map: BTreeMap<String, Value> = entrypoints.into_iter().collect();

    Ok(Flow {
        schema_version: "flow-v1".to_string(),
        id: flow_id,
        kind,
        entrypoints: entrypoints_map,
        nodes,
        metadata: FlowMetadata {
            title,
            description,
            tags: tags.into_iter().collect::<BTreeSet<_>>(),
            extra: parameters,
        },
    })
}

/// Compile YGTC YAML text into [`Flow`].
pub fn compile_ygtc_str(src: &str) -> Result<Flow> {
    let doc = loader::load_ygtc_from_str(src)?;
    compile_flow(doc)
}

/// Compile a YGTC file into [`Flow`].
pub fn compile_ygtc_file(path: &Path) -> Result<Flow> {
    let doc = loader::load_ygtc_from_path(path)?;
    compile_flow(doc)
}

fn compile_routing(raw: &Value, nodes: &HashSet<String>, node_id: &str) -> Result<Routing> {
    #[derive(serde::Deserialize)]
    struct RouteDoc {
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        out: Option<bool>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        reply: Option<bool>,
        #[serde(default)]
        condition: Option<String>,
    }

    let routes: Vec<RouteDoc> = if raw.is_null() {
        Vec::new()
    } else if let Some(shorthand) = raw.as_str() {
        match shorthand {
            "out" => vec![RouteDoc {
                to: None,
                out: Some(true),
                status: None,
                reply: None,
                condition: None,
            }],
            "reply" => vec![RouteDoc {
                to: None,
                out: None,
                status: None,
                reply: Some(true),
                condition: None,
            }],
            other => {
                return Err(crate::error::FlowError::Routing {
                    node_id: node_id.to_string(),
                    message: format!("invalid routing shorthand '{other}'"),
                    location: crate::error::FlowErrorLocation::at_path(format!(
                        "nodes.{node_id}.routing"
                    )),
                });
            }
        }
    } else {
        serde_json::from_value(raw.clone()).map_err(|e| crate::error::FlowError::Routing {
            node_id: node_id.to_string(),
            message: e.to_string(),
            location: crate::error::FlowErrorLocation::at_path(format!("nodes.{node_id}.routing")),
        })?
    };

    // Any route with a condition expression → preserve as Custom routing
    if routes.iter().any(|r| r.condition.is_some()) {
        // Validate all target nodes exist
        for route in &routes {
            if let Some(to) = &route.to
                && !nodes.contains(to)
            {
                return Err(crate::error::FlowError::MissingNode {
                    target: to.clone(),
                    node_id: node_id.to_string(),
                    location: crate::error::FlowErrorLocation::at_path(format!(
                        "nodes.{node_id}.routing"
                    )),
                });
            }
        }
        return Ok(Routing::Custom(raw.clone()));
    }

    if routes.len() == 1 {
        let route = &routes[0];
        let is_out = route.out.unwrap_or(false);
        if route.reply.unwrap_or(false) {
            return Ok(Routing::Reply);
        }
        if route.status.is_some() {
            // A single status route is still conditional; preserve it as custom routing
            // instead of silently treating it as an unconditional next-hop.
            return Ok(Routing::Custom(raw.clone()));
        }
        if let Some(to) = &route.to {
            if to == "out" || is_out {
                return Ok(Routing::End);
            }
            if !nodes.contains(to) {
                return Err(crate::error::FlowError::MissingNode {
                    target: to.clone(),
                    node_id: node_id.to_string(),
                    location: crate::error::FlowErrorLocation::at_path(format!(
                        "nodes.{node_id}.routing"
                    )),
                });
            }
            return Ok(Routing::Next {
                node_id: NodeId::new(to.as_str()).map_err(|e| {
                    crate::error::FlowError::InvalidIdentifier {
                        kind: "node",
                        value: to.clone(),
                        detail: e.to_string(),
                        location: crate::error::FlowErrorLocation::at_path(format!(
                            "nodes.{node_id}.routing"
                        )),
                    }
                })?,
            });
        }
        if is_out {
            return Ok(Routing::End);
        }
    }

    if routes.is_empty() {
        return Ok(Routing::End);
    }

    // Attempt to build a Branch when multiple status routes are present.
    if routes.len() >= 2 {
        use std::collections::BTreeMap;
        let mut on_status: BTreeMap<String, NodeId> = BTreeMap::new();
        let mut default: Option<NodeId> = None;
        let mut any_status = false;
        for route in &routes {
            if route.reply.unwrap_or(false) || route.out.unwrap_or(false) {
                return Ok(Routing::Custom(raw.clone()));
            }
            let to = match &route.to {
                Some(t) => t,
                None => return Ok(Routing::Custom(raw.clone())),
            };
            if !nodes.contains(to) {
                return Err(crate::error::FlowError::MissingNode {
                    target: to.clone(),
                    node_id: node_id.to_string(),
                    location: crate::error::FlowErrorLocation::at_path(format!(
                        "nodes.{node_id}.routing"
                    )),
                });
            }
            let to_id = NodeId::new(to.as_str()).map_err(|e| {
                crate::error::FlowError::InvalidIdentifier {
                    kind: "node",
                    value: to.clone(),
                    detail: e.to_string(),
                    location: crate::error::FlowErrorLocation::at_path(format!(
                        "nodes.{node_id}.routing"
                    )),
                }
            })?;
            if let Some(status) = &route.status {
                any_status = true;
                on_status.insert(status.clone(), to_id);
            } else {
                default = Some(to_id);
            }
        }
        if any_status {
            return Ok(Routing::Branch { on_status, default });
        }
        if let Some(default) = default {
            return Ok(Routing::Branch {
                on_status,
                default: Some(default),
            });
        }
    }

    Ok(Routing::Custom(raw.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_ygtc_from_str;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn map_flow_type_supports_known_aliases() {
        assert_eq!(map_flow_type("messaging").unwrap(), FlowKind::Messaging);
        assert_eq!(map_flow_type("events").unwrap(), FlowKind::Event);
        assert_eq!(
            map_flow_type("component-config").unwrap(),
            FlowKind::ComponentConfig
        );
        assert!(matches!(
            map_flow_type("unknown").unwrap_err(),
            crate::error::FlowError::UnknownFlowType { .. }
        ));
    }

    #[test]
    fn compile_flow_builds_entrypoints_and_branch_routing() {
        let yaml = r#"id: demo
type: messaging
nodes:
  start:
    qa.process: {}
    routing:
      - status: ok
        to: done
      - to: fallback
  done:
    template: "ok"
    routing: out
  fallback:
    template: "fallback"
    routing: reply
"#;

        let flow = compile_ygtc_str(yaml).expect("compile flow");
        assert_eq!(flow.entrypoints.get("default"), Some(&json!("start")));
        match flow
            .nodes
            .get(&NodeId::new("start").unwrap())
            .unwrap()
            .routing
            .clone()
        {
            Routing::Branch { on_status, default } => {
                assert_eq!(on_status.get("ok").unwrap().as_str(), "done");
                assert_eq!(default.unwrap().as_str(), "fallback");
            }
            other => panic!("expected branch routing, got {other:?}"),
        }
    }

    #[test]
    fn compile_ygtc_file_reports_invalid_routing_targets() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.ygtc");
        std::fs::write(
            &path,
            r#"id: demo
type: messaging
nodes:
  start:
    qa.process: {}
    routing:
      - to: missing
"#,
        )
        .unwrap();

        let err = compile_ygtc_file(&path).expect_err("missing routing target should fail");
        assert!(matches!(err, crate::error::FlowError::MissingNode { .. }));
    }

    #[test]
    fn compile_flow_rejects_invalid_routing_shorthand() {
        let err = load_ygtc_from_str(
            r#"id: demo
type: messaging
nodes:
  start:
    qa.process: {}
    routing: invalid
"#,
        )
        .expect_err("invalid shorthand should fail during load");
        assert!(matches!(err, crate::error::FlowError::Routing { .. }));
    }
}
