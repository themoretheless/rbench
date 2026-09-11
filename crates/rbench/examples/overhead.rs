//! Suite hot-loop overhead vs a handwritten Instant loop on identical useful work.
//!
//! Diagnostic only: both paths execute a fixed batch of 256 `mix64` calls per
//! sample so the medians are comparable. Relative gap still includes Suite
//! bookkeeping; it is not a zero-overhead claim.
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

fn handwritten(samples: u32, batch: u64) -> Vec<u128> {
    for _ in 0..10 {
        for i in 0..batch {
            black_box(mix64(i));
        }
    }
    (0..samples)
        .map(|_| {
            let start = Instant::now();
            for i in 0..batch {
                black_box(mix64(i));
            }
            start.elapsed().as_nanos()
        })
        .collect()
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

fn main() -> rbench::Result<()> {
    let samples = 40u32;
    let batch = 256u64;
    let mut hand = handwritten(samples, batch);
    let hand_med = median(&mut hand);

    let mut suite = Suite::new("overhead");
    // One Suite operation = the whole batch, matching the handwritten sample.
    suite
        .bench("mix64_batch", move || {
            for i in 0..batch {
                black_box(mix64(i));
            }
        })
        .parameter("batch", batch);
    suite.config(Config {
        samples,
        warmup: Duration::from_millis(5),
        sample_time: Duration::from_nanos(1),
        max_iterations: 1,
    });
    let run = suite.run("mix64_batch")?;
    let mut suite_ns: Vec<u128> = Vec::new();
    for o in run.observations.iter().filter(|o| o.metric == "wall") {
        if let Some(v) = o.number()? {
            // batch_total wall for operations==1 equals the batch duration.
            suite_ns.push(v.round() as u128);
        }
    }
    let suite_med = median(&mut suite_ns);
    let gap = (suite_med as f64 / hand_med.max(1) as f64 - 1.0) * 100.0;
    println!("handwritten_median_ns={hand_med}");
    println!("suite_median_ns={suite_med}");
    println!("relative_gap_percent={gap:.3}");
    println!("batch={batch}");
    println!("samples={samples}");
    println!("note=diagnostic; positive gap means Suite bookkeeping cost on top of useful work");
    Ok(())
}
