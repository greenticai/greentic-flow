use greentic_flow::{compile_ygtc_str, flow_ir::parse_flow_to_ir, loader::load_ygtc_from_str};
use greentic_types::Routing;

const FLOW: &str = r#"
id: rt
type: messaging
schema_version: 2
nodes:
  chat:
    dw.agent:
      user_text: hi
    operation: support
    retry:
      max_attempts: 3
    routing:
      - to: tail
  tail:
    templating.handlebars:
      text: bye
    timeout:
      ms: 5
    routing: out
"#;

fn reload(flow: &greentic_flow::flow_ir::FlowIr) -> greentic_flow::model::FlowDoc {
    let doc = flow.to_doc().expect("to_doc");
    let yaml = serde_yaml_bw::to_string(&doc).expect("yaml");
    load_ygtc_from_str(&yaml).expect("reload")
}

#[test]
fn set_operation_drops_stale_operation_sibling() {
    let mut flow = parse_flow_to_ir(FLOW).expect("ir");
    let chat = flow.nodes.get_mut("chat").expect("chat");
    assert!(chat.extra.contains_key("operation"));
    chat.set_operation("run");
    let doc = reload(&flow);
    let raw = &doc.nodes.get("chat").expect("chat").raw;
    assert!(raw.contains_key("run"));
    assert!(!raw.contains_key("operation"), "{raw:?}");
    assert!(raw.contains_key("retry"));
    // Unchanged operation keeps the legitimate sibling.
    let mut flow = parse_flow_to_ir(FLOW).expect("ir");
    flow.nodes
        .get_mut("chat")
        .expect("chat")
        .set_operation("dw.agent");
    assert!(
        reload(&flow)
            .nodes
            .get("chat")
            .expect("chat")
            .raw
            .contains_key("operation")
    );
}

#[test]
fn delete_step_keeps_other_node_extras() {
    let mut flow = parse_flow_to_ir(FLOW).expect("ir");
    flow.nodes.shift_remove("chat");
    let doc = reload(&flow);
    assert!(doc.nodes.get("chat").is_none());
    let tail = &doc.nodes.get("tail").expect("tail").raw;
    assert!(tail.contains_key("timeout"));
}

const OUT_NODE_FLOW: &str = r#"
id: o
type: messaging
schema_version: 2
nodes:
  a:
    templating.handlebars:
      text: hi
    routing:
      - to: out
  out:
    templating.handlebars:
      text: done
    routing: out
"#;

#[test]
fn single_route_to_real_node_named_out_is_next() {
    let flow = compile_ygtc_str(OUT_NODE_FLOW).expect("compile");
    let a = flow.nodes.values().next().expect("a");
    assert!(matches!(&a.routing, Routing::Next { node_id } if node_id.as_str() == "out"));
    // Without a real `out` node the sentinel is terminal.
    let sentinel = OUT_NODE_FLOW.split("  out:").next().expect("head");
    let flow = compile_ygtc_str(sentinel).expect("compile");
    let a = flow.nodes.values().next().expect("a");
    assert!(matches!(a.routing, Routing::End));
}
