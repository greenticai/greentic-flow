use greentic_flow::{compile_flow, loader::load_ygtc_from_str, resolve::resolve_parameters};
use greentic_types::{FlowKind, NodeId};
use serde_json::json;

#[test]
fn load_weather_ir_and_resolve_params() {
    let yaml = std::fs::read_to_string("fixtures/weather_bot.ygtc").unwrap();
    let doc = load_ygtc_from_str(&yaml).unwrap();
    let flow = compile_flow(doc).unwrap();

    assert_eq!(flow.id.as_str(), "weather_bot");
    assert_eq!(flow.kind, FlowKind::Messaging);
    assert_eq!(flow.entrypoints.get("default"), Some(&json!("in")));

    let fw = flow
        .nodes
        .get(&NodeId::new("forecast_weather").unwrap())
        .unwrap();
    assert_eq!(fw.component.id.as_str(), "component.exec");
    assert_eq!(fw.component.operation.as_deref(), Some("mcp.exec"));

    let resolved = resolve_parameters(
        &fw.input.mapping,
        &flow.metadata.extra,
        "nodes.forecast_weather",
    )
    .unwrap();
    assert_eq!(resolved.pointer("/args/days").unwrap(), &json!(3));
    assert_eq!(
        resolved.pointer("/args/q").unwrap(),
        &json!("in.q_location")
    );
}

#[test]
fn entrypoints_output_and_telemetry_round_trip() {
    let yaml = r#"
id: extras_flow
type: messaging
tags: ["demo"]
entrypoints:
  default: "start"
nodes:
  start:
    qa.process:
      payload: true
    output:
      select: "$.foo"
    routing:
      - out: true
    telemetry:
      span_name: "demo"
      attributes:
        k: v
      sampling: "high"
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    assert_eq!(flow.entrypoints.get("default"), Some(&json!("start")));
    let node = flow.nodes.get(&NodeId::new("start").unwrap()).unwrap();
    assert_eq!(
        node.output.mapping.pointer("/select"),
        Some(&json!("$.foo"))
    );
    assert_eq!(node.telemetry.span_name.as_deref(), Some("demo"));
    assert_eq!(
        node.telemetry.attributes.get("k").map(String::as_str),
        Some("v")
    );
    assert_eq!(node.telemetry.sampling.as_deref(), Some("high"));
    assert!(flow.metadata.tags.contains("demo"));
}

#[test]
fn branch_and_reply_routing() {
    let yaml = r#"
id: routing_flow
type: messaging
nodes:
  in:
    qa.process:
      payload: true
    routing:
      - status: ok
        to: next
      - to: fallback
  next:
    qa.process:
      payload: true
    routing:
      - reply: true
  fallback:
    qa.process: {}
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    use greentic_types::Routing;
    match &flow.nodes.get(&NodeId::new("in").unwrap()).unwrap().routing {
        Routing::Branch { on_status, default } => {
            assert!(on_status.contains_key("ok"));
            assert_eq!(on_status.get("ok").unwrap(), &NodeId::new("next").unwrap());
            assert_eq!(default.as_ref(), Some(&NodeId::new("fallback").unwrap()));
        }
        other => panic!("expected branch routing, got {other:?}"),
    }
    match &flow
        .nodes
        .get(&NodeId::new("next").unwrap())
        .unwrap()
        .routing
    {
        Routing::Reply => {}
        other => panic!("expected reply routing, got {other:?}"),
    }
}

#[test]
fn v2_dotted_operation_stays_as_operation() {
    let yaml = r#"
id: dotted_op
type: messaging
schema_version: 2
nodes:
  start:
    templating.handlebars:
      text: "hi"
    routing: out
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    let node = flow.nodes.get(&NodeId::new("start").unwrap()).unwrap();
    assert_eq!(node.component.id.as_str(), "component.exec");
    assert_eq!(
        node.component.operation.as_deref(),
        Some("templating.handlebars")
    );
}

#[test]
fn compile_flow_prefers_alias_maps_when_present() {
    let yaml = r#"
id: alias_maps
type: messaging
schema_version: 2
nodes:
  start:
    component.exec:
      component: repo://demo/component
      config:
        greeting: hi
    operation: run
    in_map:
      source: "$.input"
    out_map:
      target: "$.output"
    err_map:
      target: "$.error"
    routing: out
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    let node = flow.nodes.get(&NodeId::new("start").unwrap()).unwrap();
    assert_eq!(node.input.mapping, json!({ "source": "$.input" }));
    assert_eq!(node.output.mapping, json!({ "target": "$.output" }));
    assert_eq!(
        node.err_map.as_ref().map(|mapping| mapping.mapping.clone()),
        Some(json!({ "target": "$.error" }))
    );
}

#[test]
fn dw_agent_op_key_compiles_to_dw_agent_component() {
    let yaml = r#"
id: aw_flow
type: messaging
schema_version: 2
nodes:
  greet:
    dw.agent:
      user_text: "ping"
    operation: greeter
    routing: out
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    let node = flow.nodes.get(&NodeId::new("greet").unwrap()).unwrap();
    assert_eq!(node.component.id.as_str(), "dw.agent");
    assert_eq!(node.component.operation.as_deref(), Some("greeter"));
    assert_eq!(
        node.input.mapping.pointer("/user_text"),
        Some(&json!("ping"))
    );
}

#[test]
fn component_exec_with_operation_sibling_uses_operation_value() {
    let yaml = r#"
id: exec_flow
type: messaging
schema_version: 2
nodes:
  step:
    component.exec:
      component: "oci://acme/widget:1"
      config: {}
    operation: run
    routing: out
"#;
    let doc = load_ygtc_from_str(yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    let node = flow.nodes.get(&NodeId::new("step").unwrap()).unwrap();
    assert_eq!(node.component.id.as_str(), "component.exec");
    assert_eq!(node.component.operation.as_deref(), Some("run"));
}

/// Compile a one-node `schema_version: 2` flow whose node is `step` and
/// return that node's `(component id, operation, input mapping)`.
fn lower_single_step(node_body: &str) -> (String, Option<String>, serde_json::Value) {
    let yaml = format!(
        "id: lowering\ntype: messaging\nschema_version: 2\nnodes:\n  step:\n{node_body}    routing: out\n"
    );
    let doc = load_ygtc_from_str(&yaml).unwrap();
    let flow = compile_flow(doc).unwrap();
    let node = flow.nodes.get(&NodeId::new("step").unwrap()).unwrap();
    (
        node.component.id.as_str().to_string(),
        node.component.operation.clone(),
        node.input.mapping.clone(),
    )
}

#[test]
fn runtime_native_call_kinds_keep_their_op_key_and_take_the_sibling_operation() {
    // The runner dispatches these by component id (engine.rs `HostNode::from`
    // match arms) and reads its target from `component.operation`. Lowered to
    // `component.exec` they lose the id, and greentic-pack's builtin exemption
    // can no longer recognise them ("missing resolve summary entries").
    for (kind, target) in [
        ("operala.call", "w1"),
        ("sorla.call", "orders"),
        ("agentic.call", "a1"),
        ("approval.call", "gate1"),
        ("telco-x.call", "t1"),
    ] {
        let body = format!("    {kind}:\n      goal: \"hi\"\n    operation: {target}\n");
        let (component, operation, mapping) = lower_single_step(&body);
        assert_eq!(
            component, kind,
            "{kind} must keep its op key as the component id"
        );
        assert_eq!(
            operation.as_deref(),
            Some(target),
            "{kind} target comes from the sibling"
        );
        assert_eq!(
            mapping.pointer("/goal"),
            Some(&json!("hi")),
            "{kind} payload survives"
        );
    }
}

#[test]
fn a_sibling_less_approval_call_keeps_its_op_key_with_no_operation() {
    // The designer emits `approval.call` WITHOUT a sibling `operation`. It used to
    // lower to component.exec/approval.call; it now lowers to the shape the
    // runner documents for it (component "approval.call", operation None), which
    // its `approval.` prefix keeps from being dot-split. greentic-pack classifies
    // both shapes as builtin via `node_effective_id`.
    let body = "    approval.call:\n      reason: \"confirm\"\n";
    let (component, operation, _) = lower_single_step(body);
    assert_eq!(component, "approval.call");
    assert_eq!(operation, None);
}

#[test]
fn only_exact_runtime_native_kinds_are_exempt_from_component_exec() {
    // A 3-segment key is an adapter COMPONENT (greentic-pack builtin.rs
    // documents the `mcp.exec` / `telco-x.call.*` hazard). A prefix match here
    // would silently reclassify it as engine-dispatched.
    for key in ["x.call.y", "operala.call.extra", "sorla.caller"] {
        let body = format!("    {key}:\n      a: 1\n");
        let (component, operation, _) = lower_single_step(&body);
        assert_eq!(component, "component.exec", "{key} must stay a component");
        assert_eq!(operation.as_deref(), Some(key));
    }
    let (component, operation, _) =
        lower_single_step("    x.call.y:\n      a: 1\n    operation: run\n");
    assert_eq!(component, "component.exec");
    assert_eq!(operation.as_deref(), Some("run"));
}

#[test]
fn legacy_schema_keeps_runtime_native_call_kinds_as_before() {
    // schema_version < 2 already kept every op key; v2 now agrees with it for
    // the runtime-native kinds.
    let yaml = r#"
id: legacy_native
type: messaging
nodes:
  step:
    operala.call:
      goal: "hi"
    operation: w1
    routing: out
"#;
    let flow = compile_flow(load_ygtc_from_str(yaml).unwrap()).unwrap();
    let node = flow.nodes.get(&NodeId::new("step").unwrap()).unwrap();
    assert_eq!(node.component.id.as_str(), "operala.call");
    assert_eq!(node.component.operation.as_deref(), Some("w1"));
}
