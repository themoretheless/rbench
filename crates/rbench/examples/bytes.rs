//! Example consumer: byte-oriented workloads with declared work units, so
//! `cargo rbench throughput` reports MiB/s. Run under the runner, e.g.:
//!
//!   cargo build --release --examples
//!   cargo rbench run --program target/release/examples/bytes --protocol \
//!     --repetitions 8 -o .rbench/bytes -- --json
//!   cargo rbench throughput .rbench/bytes
//!   cargo rbench stat .rbench/bytes
use rbench::{black_box, DropPolicy, Suite};

fn main() -> rbench::Result<()> {
    let mut s = Suite::new("bytes");
    for size in [4096u64, 65536] {
        s.bench_with_input(
            &format!("sum/{size}"),
            move || (0..size).map(|i| i as u8).collect::<Vec<u8>>(),
            |data| {
                let total = data.iter().fold(0u64, |acc, &b| acc + b as u64);
                black_box(total);
            },
            DropPolicy::InsideTiming,
        )
        .parameter("bytes", size)
        .work_units("bytes", size);
        s.bench_with_input(
            &format!("count/{size}"),
            move || (0..size).map(|i| (i % 251) as u8).collect::<Vec<u8>>(),
            |data| {
                // Full O(n) scan, so per-operation time scales with the input size.
                black_box(data.iter().filter(|&&b| b == 7).count());
            },
            DropPolicy::InsideTiming,
        )
        .parameter("bytes", size)
        .work_units("bytes", size);
    }
    s.main()
}
