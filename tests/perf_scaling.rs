use greentic_flow::{compile_ygtc_str, flow_bundle::load_and_validate_bundle};
use std::{
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

mod perf_support;

fn run_compile_workload(threads: usize, iters_per_thread: usize) -> Duration {
    let yaml = Arc::new(perf_support::medium_flow_yaml());
    let barrier = Arc::new(Barrier::new(threads));
    let start = Instant::now();

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let yaml = Arc::clone(&yaml);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..iters_per_thread {
                    let flow = compile_ygtc_str(&yaml).expect("compile medium flow");
                    std::hint::black_box(flow);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("compile worker should finish");
    }

    start.elapsed()
}

fn run_bundle_workload(threads: usize, iters_per_thread: usize) -> Duration {
    let yaml = Arc::new(perf_support::medium_flow_yaml());
    let barrier = Arc::new(Barrier::new(threads));
    let start = Instant::now();

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let yaml = Arc::clone(&yaml);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..iters_per_thread {
                    let bundle =
                        load_and_validate_bundle(&yaml, None).expect("load and validate bundle");
                    std::hint::black_box(bundle);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("bundle worker should finish");
    }

    start.elapsed()
}

#[test]
fn compile_flow_scaling_should_not_collapse_under_concurrency() {
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2);
    if cpus < 2 {
        return;
    }

    let t1 = run_compile_workload(1, 12);
    let t2 = run_compile_workload(2, 12);
    let per_op_1 = t1.as_secs_f64() / 12.0;
    let per_op_2 = t2.as_secs_f64() / 24.0;

    assert!(
        per_op_2 <= per_op_1 * 1.75,
        "compile flow scaling regressed badly: t1/op={:?}, t2/op={:?}",
        Duration::from_secs_f64(per_op_1),
        Duration::from_secs_f64(per_op_2)
    );
}

#[test]
fn bundle_validation_scaling_should_not_collapse_under_concurrency() {
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2);
    if cpus < 2 {
        return;
    }

    let t1 = run_bundle_workload(1, 8);
    let t2 = run_bundle_workload(2, 8);
    let per_op_1 = t1.as_secs_f64() / 8.0;
    let per_op_2 = t2.as_secs_f64() / 16.0;

    assert!(
        per_op_2 <= per_op_1 * 1.9,
        "bundle validation scaling regressed badly: t1/op={:?}, t2/op={:?}",
        Duration::from_secs_f64(per_op_1),
        Duration::from_secs_f64(per_op_2)
    );
}
