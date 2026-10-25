use greentic_flow::{compile_ygtc_str, flow_ir::parse_flow_to_ir};

#[test]
fn legacy_schema_v1_invalid_component_id_returns_error_not_panic() {
    let yaml = r#"
id: legacy
type: messaging
schema_version: 1
nodes:
  start:
    "oci://foo:bar":
      x: 1
    routing: out
"#;
    let result = compile_ygtc_str(yaml);
    assert!(result.is_err(), "invalid component id must be an error");
}

#[test]
fn conditional_routing_to_out_is_terminal() {
    let yaml = r#"
id: cond
type: messaging
start: a
nodes:
  a:
    qa.process:
      x: 1
    routing:
      - to: b
        condition: "x == 1"
      - to: out
  b:
    qa.process:
      x: 2
    routing: out
"#;
    compile_ygtc_str(yaml).expect("`to: out` alongside a condition must compile");
}

#[test]
fn multi_status_routing_to_out_is_terminal() {
    let yaml = r#"
id: multi
type: messaging
start: a
nodes:
  a:
    qa.process:
      x: 1
    routing:
      - status: ok
        to: b
      - to: out
  b:
    qa.process:
      x: 2
    routing: out
"#;
    compile_ygtc_str(yaml).expect("`to: out` in multi-route must compile");
}

#[test]
fn flow_ir_accepts_operation_sibling_key() {
    let yaml = r#"
id: opsib
type: messaging
start: a
nodes:
  a:
    qa.process:
      x: 1
    operation: run
    routing: out
"#;
    let ir = parse_flow_to_ir(yaml).expect("operation sibling must be reserved");
    assert_eq!(ir.nodes.len(), 1);
}
