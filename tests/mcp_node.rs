//! End-to-end coverage for MCP flow nodes.
//!
//! The authoring shape is a flattened single-op-key node whose op key is the
//! literal `mcp`; `server`, `tool`, `arguments` and `output` live in the node
//! payload. After lowering the runtime node's `component` is exactly `mcp`,
//! which is a valid `greentic_types::ComponentId` (so it survives pack/runtime
//! load). These tests exercise the offline compiler path only: load -> validate
//! -> compile. No MCP server is ever contacted.

use std::str::FromStr;

use greentic_flow::error::FlowError;
use greentic_flow::flow_ir::parse_flow_to_ir;
use greentic_flow::ir::{NodeKind, classify_node_type};
use greentic_flow::{compile_flow, loader::load_ygtc_from_str};
use greentic_types::ComponentId;

const MCP_OK: &str = include_str!("data/mcp_node_ok.ygtc");

#[test]
fn mcp_node_lowers_to_mcp_component_and_keeps_payload() {
    let doc = load_ygtc_from_str(MCP_OK).expect("mcp flow should load");

    // The mcp node keeps its literal `mcp` op key on the doc.
    let node = doc.nodes.get("lookup_issue").expect("lookup_issue present");
    assert!(
        node.raw.contains_key("mcp"),
        "mcp op key preserved on the node, keys: {:?}",
        node.raw.keys().collect::<Vec<_>>()
    );

    let flow = compile_flow(doc).expect("mcp flow should compile");
    let node_id = greentic_types::NodeId::new("lookup_issue").expect("valid node id");
    let runtime_node = flow.nodes.get(&node_id).expect("runtime node present");

    // CRITICAL: the runtime component string must be exactly "mcp" AND parse as
    // a greentic_types::ComponentId (the runner runs this through
    // ComponentId::from_str at flow_adapter.rs:126).
    let component = runtime_node.component.id.as_str();
    assert_eq!(component, "mcp", "runtime component must be exactly 'mcp'");
    ComponentId::from_str(component)
        .expect("runtime mcp component must parse via greentic_types::ComponentId::from_str");

    // server/tool/arguments/output survive in the runtime node payload (input).
    let input = &runtime_node.input.mapping;
    assert_eq!(
        input.get("server").and_then(|v| v.as_str()),
        Some("github"),
        "server survives in payload: {input:?}"
    );
    assert_eq!(
        input.get("tool").and_then(|v| v.as_str()),
        Some("get_issue"),
        "tool survives in payload: {input:?}"
    );
    assert!(
        input
            .get("arguments")
            .map(|v| v.is_object())
            .unwrap_or(false),
        "arguments survive as an object: {input:?}"
    );
    assert_eq!(
        input.get("output").and_then(|v| v.as_str()),
        Some("issue"),
        "output survives in payload: {input:?}"
    );
}

#[test]
fn mcp_node_survives_ir_roundtrip() {
    // FlowDoc -> IR -> FlowDoc must keep the `mcp` op key and payload intact and
    // recompile to the same `mcp` runtime component.
    let ir = parse_flow_to_ir(MCP_OK).expect("parse to ir");
    let back = ir.to_doc().expect("ir back to doc");
    let back_node = back.nodes.get("lookup_issue").expect("node survives ir");
    assert!(
        back_node.raw.contains_key("mcp"),
        "mcp op key must survive the IR round-trip"
    );

    let flow = compile_flow(back).expect("round-tripped doc compiles");
    let node_id = greentic_types::NodeId::new("lookup_issue").expect("valid node id");
    let runtime_node = flow.nodes.get(&node_id).expect("runtime node present");
    assert_eq!(runtime_node.component.id.as_str(), "mcp");
}

#[test]
fn mcp_op_key_classifies_as_mcp() {
    assert_eq!(
        classify_node_type("mcp"),
        NodeKind::Mcp {
            server_id: String::new(),
            tool: String::new(),
        }
    );
}

#[test]
fn mcp_node_missing_server_is_rejected() {
    let yaml = r#"
id: mcp_no_server
type: messaging
schema_version: 2
nodes:
  lookup_issue:
    mcp:
      tool: get_issue
      output: issue
    routing:
      - out: true
"#;
    let err = load_ygtc_from_str(yaml).expect_err("missing server must fail");
    match err {
        FlowError::McpConfig {
            node_id, message, ..
        } => {
            assert_eq!(node_id, "lookup_issue");
            assert!(message.contains("server"), "message: {message}");
        }
        other => panic!("expected McpConfig error, got: {other:?}"),
    }
}

#[test]
fn mcp_node_missing_tool_is_rejected() {
    let yaml = r#"
id: mcp_no_tool
type: messaging
schema_version: 2
nodes:
  lookup_issue:
    mcp:
      server: github
      output: issue
    routing:
      - out: true
"#;
    let err = load_ygtc_from_str(yaml).expect_err("missing tool must fail");
    match err {
        FlowError::McpConfig {
            node_id, message, ..
        } => {
            assert_eq!(node_id, "lookup_issue");
            assert!(message.contains("tool"), "message: {message}");
        }
        other => panic!("expected McpConfig error, got: {other:?}"),
    }
}

#[test]
fn mcp_node_with_non_object_arguments_is_rejected() {
    let yaml = r#"
id: mcp_bad_args
type: messaging
schema_version: 2
nodes:
  lookup_issue:
    mcp:
      server: github
      tool: get_issue
      arguments: "not-an-object"
      output: issue
    routing:
      - out: true
"#;
    let err = load_ygtc_from_str(yaml).expect_err("non-object arguments must fail");
    match err {
        FlowError::McpConfig {
            node_id, message, ..
        } => {
            assert_eq!(node_id, "lookup_issue");
            assert!(message.contains("arguments"), "message: {message}");
        }
        other => panic!("expected McpConfig error, got: {other:?}"),
    }
}

#[test]
fn mcp_node_with_non_string_output_is_rejected() {
    let yaml = r#"
id: mcp_bad_output
type: messaging
schema_version: 2
nodes:
  lookup_issue:
    mcp:
      server: github
      tool: get_issue
      arguments:
        owner: "{{ flow.owner }}"
      output: 42
    routing:
      - out: true
"#;
    let err = load_ygtc_from_str(yaml).expect_err("non-string output must fail");
    match err {
        FlowError::McpConfig {
            node_id, message, ..
        } => {
            assert_eq!(node_id, "lookup_issue");
            assert!(message.contains("output"), "message: {message}");
        }
        other => panic!("expected McpConfig error, got: {other:?}"),
    }
}
