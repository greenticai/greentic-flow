//! End-to-end coverage for MCP flow nodes (`mcp:<server_id>/<tool_name>`).
//!
//! These tests exercise the offline compiler path only: load -> validate ->
//! compile. No MCP server is ever contacted.

use greentic_flow::error::FlowError;
use greentic_flow::flow_ir::parse_flow_to_ir;
use greentic_flow::ir::{NodeKind, classify_node_type};
use greentic_flow::{compile_flow, loader::load_ygtc_from_str};

const MCP_OK: &str = include_str!("data/mcp_node_ok.ygtc");

#[test]
fn mcp_node_compiles_and_survives_roundtrip() {
    let doc = load_ygtc_from_str(MCP_OK).expect("mcp flow should load");

    // The mcp node must keep its `mcp:` component key and not be dropped.
    let node = doc.nodes.get("lookup_issue").expect("lookup_issue present");
    let comp_key = node
        .raw
        .keys()
        .find(|k| k.starts_with("mcp:"))
        .expect("mcp component key preserved on the node");
    assert_eq!(comp_key, "mcp:github/get_issue");

    let flow = compile_flow(doc).expect("mcp flow should compile");
    assert!(
        flow.nodes
            .contains_key(&greentic_types::NodeId::new("lookup_issue").expect("valid node id"))
    );

    // FlowDoc -> IR -> FlowDoc must keep the mcp key intact and recompile.
    let ir = parse_flow_to_ir(MCP_OK).expect("parse to ir");
    let back = ir.to_doc().expect("ir back to doc");
    let back_node = back.nodes.get("lookup_issue").expect("node survives ir");
    assert!(
        back_node.raw.contains_key("mcp:github/get_issue"),
        "mcp key must survive the IR round-trip"
    );
    compile_flow(back).expect("round-tripped doc compiles");
}

#[test]
fn mcp_component_key_classifies_as_mcp() {
    assert_eq!(
        classify_node_type("mcp:github/get_issue"),
        NodeKind::Mcp {
            server_id: "github".to_string(),
            tool: "get_issue".to_string(),
        }
    );
}

#[test]
fn mcp_node_with_non_object_arguments_is_rejected() {
    let yaml = r#"
id: mcp_bad_args
type: messaging
schema_version: 2
nodes:
  lookup_issue:
    "mcp:github/get_issue":
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
    "mcp:github/get_issue":
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
