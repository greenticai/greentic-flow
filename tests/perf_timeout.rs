use ciborium::value::Value as CborValue;
use greentic_flow::{
    compile_ygtc_str, flow_bundle::load_and_validate_bundle,
    schema_validate::validate_value_against_schema,
};
use greentic_types::schemas::common::schema_ir::SchemaIr;
use std::time::{Duration, Instant};

mod perf_support;

#[test]
fn core_perf_workloads_should_finish_quickly() {
    let yaml = perf_support::medium_flow_yaml();
    let schema = perf_support::nested_schema();
    let value = perf_support::nested_value();

    // Best of several samples: scheduler noise only ever makes a run slower, so the minimum is
    // the most stable estimate of the real cost.
    const SAMPLES: usize = 3;
    let elapsed = (0..SAMPLES)
        .map(|_| run_workloads(&yaml, &schema, &value))
        .min()
        .expect("at least one sample");
    assert!(
        elapsed < Duration::from_secs(5),
        "core perf workloads took too long (best of {SAMPLES}): {:?}",
        elapsed
    );
}

fn run_workloads(yaml: &str, schema: &SchemaIr, value: &CborValue) -> Duration {
    let start = Instant::now();

    for _ in 0..10 {
        let flow = compile_ygtc_str(yaml).expect("compile medium flow");
        std::hint::black_box(flow);
    }
    for _ in 0..6 {
        let bundle = load_and_validate_bundle(yaml, None).expect("load and validate bundle");
        std::hint::black_box(bundle);
    }
    for _ in 0..100 {
        let diags = validate_value_against_schema(schema, value);
        assert!(diags.is_empty(), "expected valid schema payload");
        std::hint::black_box(diags);
    }

    start.elapsed()
}
