use greentic_flow::{
    add_step::{AddStepPlan, apply_plan, validate_flow},
    compile_ygtc_str,
    component_catalog::MemoryCatalog,
    flow_ir::{NodeIr, Route, parse_flow_to_ir},
    loader::load_ygtc_from_str,
    splice::splice_node_after,
};
use greentic_types::Routing;
use serde_json::{Value, json};

const FLOW: &str = r#"
id: rt
type: messaging
schema_version: 2
nodes:
  chat:
    dw.agent:
      user_text: hi
    operation: support
    conversational: true
    retry:
      max_attempts: 3
    timeout:
      ms: 100
    when:
      expr: "$.ok"
    annotations:
      note: keep
    meta:
      owner: me
    routing:
      - to: tail
  tail:
    templating.handlebars:
      text: hi
    conversational: false
    routing: out
"#;

const PRESERVED: &[&str] = &[
    "operation",
    "conversational",
    "retry",
    "timeout",
    "when",
    "annotations",
    "meta",
];

fn node_ir(id: &str) -> NodeIr {
    NodeIr {
        id: id.to_string(),
        operation: "dw.agent".to_string(),
        payload: json!({"user_text": "new"}),
        output: Value::Object(Default::default()),
        in_map: None,
        out_map: None,
        err_map: None,
        routing: vec![Route {
            out: true,
            ..Route::default()
        }],
        telemetry: None,
        extra: Default::default(),
    }
}

fn assert_preserved(flow: &greentic_flow::flow_ir::FlowIr) {
    let doc = flow.to_doc().expect("to_doc");
    let yaml = serde_yaml_bw::to_string(&doc).expect("yaml");
    let reloaded = load_ygtc_from_str(&yaml).expect("reload");
    let original = load_ygtc_from_str(FLOW).expect("orig");
    for id in ["chat", "tail"] {
        let (Some(a), Some(b)) = (original.nodes.get(id), reloaded.nodes.get(id)) else {
            panic!("node {id} missing");
        };
        for key in PRESERVED {
            assert_eq!(a.raw.get(*key), b.raw.get(*key), "{id}.{key}\n{yaml}");
        }
    }
}

#[test]
fn flow_ir_roundtrip_keeps_node_fields() {
    assert_preserved(&parse_flow_to_ir(FLOW).expect("ir"));
}

#[test]
fn add_step_keeps_untouched_node_fields() {
    let flow = parse_flow_to_ir(FLOW).expect("ir");
    let plan = AddStepPlan {
        anchor: "tail".to_string(),
        new_node: node_ir("added"),
        anchor_old_routing: flow.nodes["tail"].routing.clone(),
        insert_before_entrypoint: false,
    };
    let updated = apply_plan(&flow, plan, false).expect("apply");
    assert!(updated.nodes.contains_key("added"));
    assert_preserved(&updated);
}

#[test]
fn update_and_delete_step_keep_other_node_fields() {
    let mut flow = parse_flow_to_ir(FLOW).expect("ir");
    flow.nodes.get_mut("tail").expect("tail").payload = json!({"component": "repo://demo/other"});
    assert_preserved(&flow);
    flow.nodes.shift_remove("extra_missing");
    assert_preserved(&flow);
}

#[test]
fn compile_ignores_reserved_keys_after_operation() {
    let yaml = r#"
id: m1
type: messaging
schema_version: 2
nodes:
  a:
    dw.agent:
      user_text: hi
    timeout:
      ms: 100
    meta:
      x: 1
    annotations:
      y: 2
    routing: out
"#;
    let flow = compile_ygtc_str(yaml).expect("compile");
    let node = flow.nodes.values().next().expect("node");
    assert_eq!(node.component.id.as_str(), "dw.agent");
}

#[test]
fn validate_flow_accepts_terminal_out_route() {
    let mut flow = parse_flow_to_ir(FLOW).expect("ir");
    flow.nodes.get_mut("tail").expect("tail").routing = vec![Route {
        to: Some("out".to_string()),
        ..Route::default()
    }];
    let diags = validate_flow(&flow, &MemoryCatalog::default());
    assert!(
        diags.iter().all(|d| d.code != "ROUTE_TARGET_MISSING"),
        "{diags:?}"
    );
}

#[test]
fn splice_after_shorthand_routing() {
    for shorthand in ["out", "reply"] {
        let flow = format!(
            "id: main\ntype: messaging\nstart: start\nnodes:\n  start:\n    templating.handlebars:\n      text: hi\n    routing: {shorthand}\n"
        );
        let new_node = serde_yaml_bw::from_str(
            "tool:\n  component: x\nrouting:\n  - to: NEXT_NODE_PLACEHOLDER\n",
        )
        .expect("node");
        let out = splice_node_after(&flow, "hello", new_node, "start").expect("splice");
        assert!(out.contains(&format!("{shorthand}: true")), "{out}");
    }
}

#[test]
fn compile_routing_keeps_multiple_unconditional_routes() {
    let yaml = r#"
id: l2
type: messaging
schema_version: 2
nodes:
  a:
    dw.agent: {}
    routing:
      - to: b
      - to: c
  b:
    dw.agent: {}
    routing: out
  c:
    dw.agent: {}
    routing: out
"#;
    let flow = compile_ygtc_str(yaml).expect("compile");
    let a = flow.nodes.values().next().expect("a");
    assert!(matches!(a.routing, Routing::Custom(_)), "{:?}", a.routing);
}
