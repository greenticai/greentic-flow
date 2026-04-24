use anyhow::Result;
use greentic_types::Flow;
use greentic_types::flow_resolve::{FlowResolveV1, read_flow_resolve, sidecar_path_for_flow};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoReport {
    pub info_schema_version: u32,
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub resolve: ResolveStatus,
    pub entrypoints: Vec<EntrypointInfo>,
    pub nodes: Vec<NodeInfo>,
    pub parameters: Vec<ParameterInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveStatus {
    /// "bound" | "partial" | "unbound"
    pub status: String,
    pub sidecar_path: Option<String>,
    pub resolved_nodes: u32,
    pub total_nodes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntrypointInfo {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub component_id: String,
    pub operation: Option<String>,
    pub pack_alias: Option<String>,
    pub routing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub required: bool,
}

impl InfoReport {
    pub fn from_flow(flow: &Flow, flow_path: &Path) -> Result<Self> {
        let sidecar_path = sidecar_path_for_flow(flow_path);
        let sidecar: Option<FlowResolveV1> = if sidecar_path.exists() {
            read_flow_resolve(&sidecar_path).ok()
        } else {
            None
        };

        let total = flow.nodes.len() as u32;
        let (status, resolved) = match &sidecar {
            None => ("unbound".to_string(), 0u32),
            Some(s) => {
                let r = s.nodes.len() as u32;
                if r == total {
                    ("bound".to_string(), r)
                } else {
                    ("partial".to_string(), r)
                }
            }
        };

        Ok(Self {
            info_schema_version: 1,
            id: flow.id.as_str().to_string(),
            kind: format!("{:?}", flow.kind).to_lowercase(),
            title: flow.metadata.title.clone(),
            description: flow.metadata.description.clone(),
            tags: flow.metadata.tags.iter().cloned().collect(),
            resolve: ResolveStatus {
                status,
                sidecar_path: sidecar.as_ref().map(|_| sidecar_path.display().to_string()),
                resolved_nodes: resolved,
                total_nodes: total,
            },
            entrypoints: flow
                .entrypoints
                .iter()
                .map(|(name, target_val)| EntrypointInfo {
                    name: name.clone(),
                    target: target_val
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                })
                .collect(),
            nodes: flow
                .nodes
                .iter()
                .map(|(id, n)| NodeInfo {
                    id: id.as_str().to_string(),
                    component_id: n.component.id.as_str().to_string(),
                    operation: n.component.operation.clone(),
                    pack_alias: n.component.pack_alias.clone(),
                    routing: format!("{:?}", n.routing),
                })
                .collect(),
            parameters: parameters_from_extra(&flow.metadata.extra),
        })
    }
}

fn parameters_from_extra(extra: &serde_json::Value) -> Vec<ParameterInfo> {
    // metadata.extra is expected to be a JSON object where keys are parameter
    // names and values are small schema objects with "type" and optional "required".
    // Flows without parameters have extra == null or {} — return empty in those cases.
    let map = match extra.as_object() {
        Some(m) => m,
        None => return vec![],
    };
    map.iter()
        .map(|(name, v)| ParameterInfo {
            name: name.clone(),
            ty: v
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string(),
            required: v.get("required").and_then(|r| r.as_bool()).unwrap_or(true),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn json_has_schema_version_one() {
        let r = InfoReport {
            info_schema_version: 1,
            id: "x".into(),
            kind: "messaging".into(),
            title: None,
            description: None,
            tags: vec![],
            resolve: ResolveStatus {
                status: "unbound".into(),
                sidecar_path: None,
                resolved_nodes: 0,
                total_nodes: 0,
            },
            entrypoints: vec![],
            nodes: vec![],
            parameters: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["info_schema_version"], 1);
        assert_eq!(v["kind"], "messaging");
    }

    #[test]
    fn unbound_flow_reports_unbound_and_zero_resolved() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("flow.ygtc");
        // Minimal valid .ygtc v1 flow: flat top-level metadata, flat nodes
        // with an operation key (`qa.process`) and an `out: true` routing.
        let yaml = r#"id: ex
title: Example
type: messaging
nodes:
  in:
    qa.process:
      welcome: "hello"
    routing:
      - out: true
"#;
        std::fs::File::create(&path)
            .unwrap()
            .write_all(yaml.as_bytes())
            .unwrap();
        let flow = crate::compile_ygtc_file(&path).expect("compile flow fixture");
        let info = InfoReport::from_flow(&flow, &path).expect("from_flow");
        assert_eq!(info.id, "ex");
        assert_eq!(info.title.as_deref(), Some("Example"));
        assert_eq!(info.resolve.status, "unbound");
        assert_eq!(info.resolve.total_nodes, 1);
        assert_eq!(info.resolve.resolved_nodes, 0);
        // compile_flow injects a "default" entrypoint pointing at the first
        // node when no explicit entrypoints are authored.
        assert_eq!(info.entrypoints.len(), 1);
        assert_eq!(info.entrypoints[0].name, "default");
        assert_eq!(info.entrypoints[0].target, "in");
        assert_eq!(info.nodes.len(), 1);
        assert_eq!(info.nodes[0].id, "in");
        // The loader defaults unset `schema_version` to 2 (see `loader.rs`),
        // so a dot-operation like `qa.process` is rewritten to the generic
        // `component.exec` component with `operation = Some("qa.process")`.
        assert_eq!(info.nodes[0].component_id, "component.exec");
        assert_eq!(info.nodes[0].operation.as_deref(), Some("qa.process"));
    }
}
