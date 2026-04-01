use greentic_flow::{
    compile_ygtc_str, flow_bundle::load_and_validate_bundle,
    schema_validate::validate_value_against_schema,
};
use std::time::{Duration, Instant};

mod perf_support;

#[test]
fn core_perf_workloads_should_finish_quickly() {
    let yaml = perf_support::medium_flow_yaml();
    let schema = perf_support::nested_schema();
    let value = perf_support::nested_value();

    let start = Instant::now();

    for _ in 0..10 {
        let flow = compile_ygtc_str(&yaml).expect("compile medium flow");
        std::hint::black_box(flow);
    }
    for _ in 0..6 {
        let bundle = load_and_validate_bundle(&yaml, None).expect("load and validate bundle");
        std::hint::black_box(bundle);
    }
    for _ in 0..100 {
        let diags = validate_value_against_schema(&schema, &value);
        assert!(diags.is_empty(), "expected valid schema payload");
        std::hint::black_box(diags);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "core perf workloads took too long: {:?}",
        elapsed
    );
}
