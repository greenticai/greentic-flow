//! A route marked `on_message` is how a card answers a typed message instead of
//! a click. The runner takes it only on the turn that RESUMES the parked card,
//! never on the turn that renders it, so it must reach the compiled flow
//! intact and must never be collapsed into an unconditional next-hop (which
//! would make the card skip its own page).

use greentic_flow::{compile_flow, flow_ir::parse_flow_to_ir, loader::load_ygtc_from_str};
use greentic_types::Routing;

const MENU: &str = r#"
id: menu_flow
type: messaging
start: menu
nodes:
  menu:
    component.exec:
      operation: render
    routing:
      - condition: response.action == "go_a"
        to: page_a
      - to: fallback
        on_message: true
      - out: true
  page_a:
    component.exec:
      operation: render
    routing: out
  fallback:
    component.exec:
      operation: render
    routing: out
"#;

fn compile(yaml: &str) -> greentic_types::Flow {
    compile_flow(load_ygtc_from_str(yaml).expect("schema accepts on_message")).expect("compiles")
}

fn routing_of<'a>(flow: &'a greentic_types::Flow, id: &str) -> &'a Routing {
    &flow
        .nodes
        .iter()
        .find(|(node_id, _)| node_id.as_str() == id)
        .map(|(_, node)| node)
        .expect("node")
        .routing
}

#[test]
fn the_schema_accepts_on_message_and_the_arm_survives_compilation() {
    let flow = compile(MENU);
    match routing_of(&flow, "menu") {
        Routing::Custom(raw) => {
            let arms = raw.as_array().expect("array");
            assert_eq!(arms.len(), 3);
            assert_eq!(arms[1]["on_message"], true);
            assert_eq!(arms[1]["to"], "fallback");
        }
        other => panic!("expected custom routing, got {other:?}"),
    }
}

#[test]
fn a_lone_on_message_arm_is_not_collapsed_into_a_next_hop() {
    let yaml = MENU
        .replace(
            "      - condition: response.action == \"go_a\"\n        to: page_a\n",
            "",
        )
        .replace("      - out: true\n", "");
    let flow = compile(&yaml);
    assert!(
        matches!(routing_of(&flow, "menu"), Routing::Custom(_)),
        "an unconditional next-hop would skip the card: {:?}",
        routing_of(&flow, "menu")
    );
}

#[test]
fn an_arm_naming_an_unknown_node_is_still_refused() {
    let yaml = MENU.replace("to: fallback", "to: nowhere");
    assert!(
        load_ygtc_from_str(&yaml).is_err(),
        "a target that is not a node must still be refused"
    );
}

#[test]
fn the_ir_round_trip_keeps_on_message() {
    // The IR wants a plain operation key rather than the `component.exec` form.
    let yaml = MENU.replace(
        "component.exec:\n      operation: render",
        "emit.log:\n      message: hi",
    );
    let ir = parse_flow_to_ir(&yaml).expect("ir");
    let menu = ir.nodes.get("menu").expect("menu node");
    assert!(
        menu.routing
            .iter()
            .any(|r| r.on_message && r.to.as_deref() == Some("fallback"))
    );
    let doc = ir.to_doc().expect("doc");
    let text = serde_yaml_bw::to_string(&doc).expect("yaml");
    assert!(
        text.contains("on_message: true"),
        "export dropped the arm:\n{text}"
    );
}
