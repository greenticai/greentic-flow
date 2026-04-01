use greentic_flow::{
    error::FlowError,
    load_and_validate_bundle,
    loader::{load_ygtc_from_str, load_ygtc_from_str_with_source},
};
use std::path::Path;

#[test]
fn two_components_is_error() {
    let yaml = std::fs::read_to_string("fixtures/invalid_node_shape.ygtc").unwrap();
    let err = load_ygtc_from_str(&yaml).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must contain exactly one component key"));
}

#[test]
fn location_includes_source_label() {
    let yaml = std::fs::read_to_string("fixtures/invalid_node_shape.ygtc").unwrap();
    let err = load_ygtc_from_str_with_source(
        &yaml,
        Path::new("schemas/ygtc.flow.schema.json"),
        "fixtures/invalid_node_shape.ygtc",
    )
    .unwrap_err();
    match err {
        FlowError::NodeComponentShape { location, .. } => {
            assert_eq!(
                location.path.as_deref(),
                Some("fixtures/invalid_node_shape.ygtc::nodes.x")
            );
        }
        other => panic!("expected location aware error, got {other:?}"),
    }
}

#[test]
fn schema_error_exposes_details() {
    let yaml = "id: missing_type\nnodes: {}\n";
    let err = load_and_validate_bundle(yaml, None).unwrap_err();
    match err {
        FlowError::Schema { details, .. } => {
            assert!(!details.is_empty());
            assert!(details.iter().any(|detail| detail.location.path.is_some()));
        }
        other => panic!("expected schema error, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn absolute_schema_path_outside_allowlist_is_rejected() {
    let yaml = "id: flow\nnodes: {}\n";
    let err = load_ygtc_from_str_with_source(yaml, Path::new("/etc/hosts"), "inline")
        .expect_err("schema path outside allowed roots should fail");
    assert!(
        format!("{err}").contains("outside allowed roots"),
        "unexpected error: {err}"
    );
}
