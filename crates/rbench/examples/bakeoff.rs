//! Quiet-host bake-off table: handwritten Instant vs Suite::bench vs Suite::bench_batch.
//!
//! Keeps Criterion/Divan out of the default dependency graph. To compare those
//! tools on the same mix64 batch, follow `docs/BAKEOFF.md` with local crates.
use rbench::{Config, Suite};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn mix64(n: u64) -> u64 {
    let mut x = n ^ 0x9e3779b97f4a7c15;
    for i in 0..64u64 {
        x = x.wrapping_mul(0xBF58476D1CE4E5B9).wrapping_add(i);
        x ^= x >> 27;
    }
    x
}

fn work(batch: u64) {
    for i in 0..batch {
        black_box(mix64(i));
    }
}

fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 0 {
        (v[n / 2 - 1] / 2) + (v[n / 2] / 2)
    } else {
        v[n / 2]
    }
}

fn handwritten(samples: u32, batch: u64) -> u128 {
    for _ in 0..10 {
        work(batch);
    }
    let mut xs: Vec<u128> = (0..samples)
        .map(|_| {
            let start = Instant::now();
            work(batch);
            start.elapsed().as_nanos()
        })
        .collect();
    median(&mut xs)
}

fn suite_median(name: &str, batch: u64, samples: u32, batch_api: bool) -> rbench::Result<u128> {
    let mut suite = Suite::new(name);
    if batch_api {
        suite.bench_batch("mix64", move |n| {
            for _ in 0..n {
                work(batch);
            }
        });
    } else {
        suite.bench("mix64", move || work(batch));
    }
    suite.parameter("batch", batch);
    suite.config(Config {
        samples,
        warmup: Duration::from_millis(5),
        sample_time: Duration::from_nanos(1),
        max_iterations: 1,
    });
    let run = suite.run("mix64")?;
    let mut xs = Vec::new();
    for o in run.observations.iter().filter(|o| o.metric == "wall") {
        if let Some(v) = o.number()? {
            xs.push(v.round() as u128);
        }
    }
    Ok(median(&mut xs))
}

fn main() -> rbench::Result<()> {
    let _ = rbench::isolate::apply_env_pin()?;
    let _ = rbench::isolate::apply_env_cgroup()?;
    let snap = rbench::isolate::snapshot();
    let samples = 40u32;
    let batch = 256u64;
    let hand = handwritten(samples, batch);
    let bench = suite_median("bakeoff_bench", batch, samples, false)?;
    let batch_med = suite_median("bakeoff_batch", batch, samples, true)?;
    let gap = |x: u128| (x as f64 / hand.max(1) as f64 - 1.0) * 100.0;
    let out = serde_json::json!({
        "schema": 1,
        "case": "mix64_batch_256",
        "samples": samples,
        "batch": batch,
        "isolation": snap,
        "medians_ns": {
            "handwritten_instant": hand,
            "suite_bench": bench,
            "suite_bench_batch": batch_med,
        },
        "relative_gap_percent_vs_handwritten": {
            "suite_bench": gap(bench),
            "suite_bench_batch": gap(batch_med),
        },
        "note": "Criterion/Divan are intentionally not depended on here; see docs/BAKEOFF.md for an external quiet-host comparison of the same useful work.",
        "claims_forbidden": [
            "zero overhead on every micro-body",
            "this container is a quiet controlled host"
        ]
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
