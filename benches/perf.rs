use criterion::{Criterion, criterion_group, criterion_main};
use greentic_flow::{
    compile_ygtc_str, flow_bundle::load_and_validate_bundle,
    schema_validate::validate_value_against_schema,
};
use std::hint::black_box;

#[path = "../tests/perf_support.rs"]
mod perf_support;

fn bench_compile_flow(c: &mut Criterion) {
    let yaml = perf_support::medium_flow_yaml();
    c.bench_function("compile_ygtc_str/medium_flow", |b| {
        b.iter(|| {
            let flow = compile_ygtc_str(black_box(&yaml)).expect("compile medium flow");
            black_box(flow);
        })
    });
}

fn bench_load_bundle(c: &mut Criterion) {
    let yaml = perf_support::medium_flow_yaml();
    c.bench_function("load_and_validate_bundle/medium_flow", |b| {
        b.iter(|| {
            let bundle = load_and_validate_bundle(black_box(&yaml), None)
                .expect("load and validate medium bundle");
            black_box(bundle);
        })
    });
}

fn bench_schema_validation(c: &mut Criterion) {
    let schema = perf_support::nested_schema();
    let value = perf_support::nested_value();
    c.bench_function("validate_value_against_schema/nested_object", |b| {
        b.iter(|| {
            let diags = validate_value_against_schema(black_box(&schema), black_box(&value));
            assert!(diags.is_empty(), "expected valid schema payload");
            black_box(diags);
        })
    });
}

criterion_group!(
    benches,
    bench_compile_flow,
    bench_load_bundle,
    bench_schema_validation
);
criterion_main!(benches);
