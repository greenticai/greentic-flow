//! Regression tests for the wizard / add-step audit findings.

use std::collections::HashMap;
use std::path::Path;

use greentic_flow::{
    add_step::{
        AddStepSpec, apply_and_validate,
        modes::{AddStepModeInput, materialize_node},
        plan_add_step,
    },
    component_catalog::{ComponentMetadata, MemoryCatalog},
    flow_ir::parse_flow_to_ir,
    splice::NEXT_NODE_PLACEHOLDER,
    wizard::{ApplyOptions, MODE_SCAFFOLD, ProviderContext, wizard_provider},
};
use serde_json::{Map, Value, json};

fn catalog(ids: &[&str]) -> MemoryCatalog {
    let mut catalog = MemoryCatalog::default();
    for id in ids {
        catalog.insert(ComponentMetadata {
            id: (*id).to_string(),
            required_fields: Vec::new(),
        });
    }
    catalog
}

#[test]
fn m2_config_mode_accepts_config_flow_without_type() {
    // No top-level `type`: run_config_flow normalizes it, so materialize_node must not
    // validate the raw text first.
    let config_flow = r#"id: cfg
start: in
nodes:
  in:
    questions:
      fields: []
    routing:
      - to: emit
  emit:
    template: |
      {
        "node_id": "hello-world",
        "node": {
          "component.exec": { "component": "ai.greentic.hello-world" },
          "operation": "handle_message",
          "routing": [ { "to": "NEXT_NODE_PLACEHOLDER" } ]
        }
      }
"#;
    let (hint, node) = materialize_node(
        AddStepModeInput::Config {
            config_flow: config_flow.to_string(),
            schema_path: Path::new("schemas/ygtc.flow.schema.json").into(),
            answers: Map::new(),
            manifest_id: None,
            manifest_path: None,
        },
        &catalog(&[]),
    )
    .expect("config flow without type should materialize");
    assert_eq!(hint.as_deref(), Some("hello-world"));
    assert!(node.get("component.exec").is_some());
}

#[test]
fn m3_placeholder_on_empty_flow_becomes_terminal() {
    let ir =
        parse_flow_to_ir("id: main\ntype: messaging\nstart: start\nnodes: {}\n").expect("parse");
    let cat = catalog(&["ai.greentic.echo"]);
    let spec = AddStepSpec {
        after: None,
        node_id_hint: None,
        node: json!({
            "ai.greentic.echo": {},
            "routing": [ { "to": NEXT_NODE_PLACEHOLDER } ]
        }),
        allow_cycles: false,
        require_placeholder: true,
    };
    let plan = plan_add_step(&ir, spec, &cat).expect("plan on empty flow");
    let updated = apply_and_validate(&ir, plan, &cat, false).expect("apply");
    let (_, node) = updated.nodes.get_index(0).expect("node");
    assert_eq!(node.routing.len(), 1);
    assert!(node.routing[0].out);
}

#[test]
fn m3_placeholder_after_anchor_without_routing_still_requires_routing() {
    // Existing anchors without routing keep failing loudly (see flow_cli_ops::add_step_requires_routing).
    let flow = r#"id: main
type: messaging
start: start
nodes:
  start:
    ai.greentic.echo: {}
"#;
    let ir = parse_flow_to_ir(flow).expect("parse");
    let cat = catalog(&["ai.greentic.echo"]);
    let spec = AddStepSpec {
        after: Some("start".to_string()),
        node_id_hint: Some("tail".to_string()),
        node: json!({
            "ai.greentic.echo": {},
            "routing": [ { "to": NEXT_NODE_PLACEHOLDER } ]
        }),
        allow_cycles: false,
        require_placeholder: true,
    };
    let diags = plan_add_step(&ir, spec, &cat).expect_err("must require routing");
    assert!(diags.iter().any(|d| d.code == "ADD_STEP_ROUTING_MISSING"));
}

fn m4_spec(routing: Value) -> AddStepSpec {
    AddStepSpec {
        after: None,
        node_id_hint: Some("pre".to_string()),
        node: json!({ "ai.greentic.echo": {}, "routing": routing }),
        allow_cycles: false,
        require_placeholder: false,
    }
}

const M4_FLOW: &str = r#"id: main
type: messaging
start: start
nodes:
  start:
    ai.greentic.echo: {}
    routing:
      - out: true
"#;

#[test]
fn m4_insert_before_entrypoint_with_explicit_routing_not_reaching_anchor_is_rejected() {
    // --routing-out without --after: the new node would become the entrypoint and `start`
    // would be orphaned.
    let ir = parse_flow_to_ir(M4_FLOW).expect("parse");
    let cat = catalog(&["ai.greentic.echo"]);
    let diags = plan_add_step(&ir, m4_spec(json!([{ "out": true }])), &cat)
        .expect_err("orphaning the old entrypoint must be rejected");
    assert!(
        diags
            .iter()
            .any(|d| d.code == "ADD_STEP_ENTRYPOINT_ORPHANED"),
        "{diags:?}"
    );
}

#[test]
fn m4_insert_before_entrypoint_with_route_to_anchor_is_accepted() {
    let ir = parse_flow_to_ir(M4_FLOW).expect("parse");
    let cat = catalog(&["ai.greentic.echo"]);
    let plan = plan_add_step(&ir, m4_spec(json!([{ "to": "start" }])), &cat)
        .expect("explicit route to the anchor keeps it reachable");
    assert!(plan.insert_before_entrypoint);
    let plan = plan_add_step(&ir, m4_spec(json!([{ "to": NEXT_NODE_PLACEHOLDER }])), &cat)
        .expect("placeholder resolves to the anchor");
    assert!(plan.insert_before_entrypoint);
}

#[test]
fn m4_insert_before_entrypoint_honors_explicit_routing() {
    let ir = parse_flow_to_ir(M4_FLOW).expect("parse");
    let cat = catalog(&["ai.greentic.echo"]);
    let spec = AddStepSpec {
        after: None,
        node_id_hint: Some("pre".to_string()),
        node: json!({
            "ai.greentic.echo": {},
            // Deliberately differs from the default `[{to: anchor}]` so that a regression
            // which overwrites the explicit routing is detected.
            "routing": [
                { "to": "start", "status": "ok" },
                { "out": true, "status": "err" }
            ]
        }),
        allow_cycles: false,
        require_placeholder: false,
    };
    let plan = plan_add_step(&ir, spec, &cat).expect("plan");
    let updated = apply_and_validate(&ir, plan, &cat, false).expect("apply");
    let (first_id, first) = updated.nodes.get_index(0).expect("first node");
    assert_eq!(first_id, "pre");
    assert_eq!(
        first.routing.len(),
        2,
        "explicit routing must be kept as-is"
    );
    assert_eq!(first.routing[0].to.as_deref(), Some("start"));
    assert_eq!(first.routing[0].status.as_deref(), Some("ok"));
    assert!(first.routing[1].out);
    assert_eq!(first.routing[1].status.as_deref(), Some("err"));
    assert_eq!(updated.entrypoints.get("default"), Some(&"pre".to_string()));
}

fn scaffold_answers(path: &str, entrypoint: &str, scaffold: bool) -> HashMap<String, Value> {
    let mut answers = HashMap::new();
    answers.insert("flow.name".to_string(), json!("demo"));
    answers.insert("flow.kind".to_string(), json!("messaging"));
    answers.insert("flow.path".to_string(), json!(path));
    answers.insert("flow.entrypoint".to_string(), json!(entrypoint));
    answers.insert("flow.nodes.scaffold".to_string(), json!(scaffold));
    answers.insert("flow.nodes.variant".to_string(), json!("start-end"));
    answers
}

#[test]
fn m8_entrypoint_colliding_with_starter_nodes_is_rejected() {
    let ctx = ProviderContext::default();
    let opts = ApplyOptions { validate: false };
    let err = wizard_provider()
        .apply(
            MODE_SCAFFOLD,
            &ctx,
            &scaffold_answers("flows/main.ygtc", "end", true),
            &opts,
        )
        .expect_err("entrypoint 'end' collides with the starter end node");
    assert!(err.to_string().contains("collides"), "{err}");
    wizard_provider()
        .apply(
            MODE_SCAFFOLD,
            &ctx,
            &scaffold_answers("flows/main.ygtc", "start", true),
            &opts,
        )
        .expect("distinct entrypoint is accepted");
}

#[test]
fn wizard_flow_path_must_stay_under_root() {
    let ctx = ProviderContext::default();
    let opts = ApplyOptions { validate: false };
    for bad in ["../escape.ygtc", "flows/../../escape.ygtc", "/tmp/abs.ygtc"] {
        let result = wizard_provider().apply(
            MODE_SCAFFOLD,
            &ctx,
            &scaffold_answers(bad, "start", false),
            &opts,
        );
        assert!(result.is_err(), "path '{bad}' must be rejected");
    }
    wizard_provider()
        .apply(
            MODE_SCAFFOLD,
            &ctx,
            &scaffold_answers("flows/main.ygtc", "start", false),
            &opts,
        )
        .expect("relative path under root is accepted");
}

// --- round 2 ---

const WASI_QA_COMPONENT: &[u8] = include_bytes!("fixtures/wasi_qa_component.wasm");

fn fetch_fixture_spec() -> anyhow::Result<Vec<u8>> {
    let spec = greentic_flow::wizard_ops::fetch_wizard_spec(
        WASI_QA_COMPONENT,
        greentic_flow::wizard_ops::WizardMode::Default,
    )?;
    Ok(spec.qa_spec_cbor)
}

#[test]
fn wizard_spec_runs_wasi_component_without_runtime() {
    assert_eq!(
        fetch_fixture_spec().expect("fixture must load"),
        vec![0xa1, 0xa2, 0xa3]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wizard_spec_runs_wasi_component_inside_multi_thread_runtime() {
    // The component calls a WASI import; the sync WASI shim uses `block_on` internally, which
    // panics on a runtime thread unless the whole call is moved off-runtime.
    assert_eq!(
        fetch_fixture_spec().expect("must not panic inside a runtime"),
        vec![0xa1, 0xa2, 0xa3]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wizard_spec_runs_wasi_component_inside_current_thread_runtime() {
    assert_eq!(
        fetch_fixture_spec().expect("must not panic inside a runtime"),
        vec![0xa1, 0xa2, 0xa3]
    );
}

#[test]
#[allow(deprecated)]
fn questions_merge_answers_is_still_exported() {
    use greentic_flow::questions::{Answers, merge_answers};
    let mut cli = Answers::new();
    cli.insert("a".to_string(), json!(1));
    cli.insert("b".to_string(), json!(1));
    let mut file = Answers::new();
    file.insert("b".to_string(), json!(2));
    let merged = merge_answers(Some(cli), Some(file));
    assert_eq!(merged.get("a"), Some(&json!(1)));
    assert_eq!(merged.get("b"), Some(&json!(2)));
}
