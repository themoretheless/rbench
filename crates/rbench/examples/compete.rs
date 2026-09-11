//! Competitive multi-metric demo: same useful work under rbench Suite with
//! wall + throughput + OS process metrics + optional Linux perf counters.
//!
//! This does **not** pull Criterion/Divan as dependencies. Those tools still
//! win day-to-day microbench ergonomics; see `docs/COMPETE.md` and
//! `cargo rbench compete` for an honest scorecard.
use rbench::{Config, Suite};
use std::hint::black_box;
use std::time::Duration;

fn mix64(n: u64) -> u64 {
    let mut x = n ^ 0x9e3779b97f4a7c15;
    for i in 0..64u64 {
        x = x.wrapping_mul(0xBF58476D1CE4E5B9).wrapping_add(i);
        x ^= x >> 27;
    }
    x
}

fn main() -> rbench::Result<()> {
    let _pin = rbench::isolate::apply_env_pin().ok().flatten();
    let snap = rbench::isolate::snapshot();
    let warnings = rbench::isolate::noise_warnings(&snap);
    let perf = rbench::perf::probe();

    let batch = 256u64;
    let samples = 24u32;
    let mut suite = Suite::new("compete");
    suite
        .process_metrics(true)
        .perf_counters(true)
        .bench("mix64_batch", move || {
            for i in 0..batch {
                black_box(mix64(i));
            }
        })
        .parameter("batch", batch)
        .work_units("mix64", batch);
    suite.config(Config {
        samples,
        warmup: Duration::from_millis(5),
        sample_time: Duration::from_nanos(1),
        max_iterations: 1,
    });
    let run = suite.run("mix64_batch")?;

    let mut by_metric: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for o in &run.observations {
        if let Some(v) = o.number()? {
            by_metric.entry(o.metric.clone()).or_default().push(v);
        }
    }
    let mut medians = serde_json::Map::new();
    for (metric, mut vals) in by_metric {
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = vals.len();
        let med = if n == 0 {
            continue;
        } else if n % 2 == 0 {
            (vals[n / 2 - 1] + vals[n / 2]) / 2.0
        } else {
            vals[n / 2]
        };
        medians.insert(metric, serde_json::json!(med));
    }

    let out = serde_json::json!({
        "schema": 1,
        "case": "mix64_batch",
        "batch": batch,
        "samples": samples,
        "isolation": snap,
        "noise_warnings": warnings,
        "perf_probe": {
            "availability": format!("{:?}", perf.availability),
            "note": perf.note,
        },
        "metric_medians": medians,
        "notes": run.notes,
        "positioning": {
            "wins": [
                "typed multi-metric observations in one run",
                "Availability never fabricates counter zeroes",
                "optional CPU pin + load/governor snapshot for confirmatory context"
            ],
            "does_not_claim": [
                "lower hot-loop overhead than Divan/Criterion",
                "Valgrind-deterministic CI without Callgrind adapter"
            ]
        }
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
