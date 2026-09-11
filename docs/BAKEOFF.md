# Quiet-host Criterion / Divan bake-off

rbench keeps Criterion and Divan **out of the default dependency graph**.
Compare them on a quiet machine with the same useful work (`mix64` × 256).

## Built-in table (no extra deps)

```sh
RBENCH_PIN_CPU=0 cargo run -p rbench --example bakeoff --release --offline
```

This prints handwritten Instant vs `Suite::bench` vs `Suite::bench_batch`
medians and relative gaps. It is a bookkeeping diagnostic, not a claim that
hosted CI is quiet.

## External Criterion / Divan (local only)

1. Create a scratch crate next to this repo (do not add deps to rbench itself).
2. Copy the `mix64` function and fixed batch of 256 from `examples/bakeoff.rs`.
3. Time N=40 samples under:
   - handwritten `Instant`
   - `rbench` `Suite::bench_batch`
   - Criterion `bench_function` with identical useful work
   - Divan `#[divan::bench]` with identical useful work
4. Publish medians + relative gaps into a short note under `docs/` after a
   quiet-host run (fans settled, `performance` governor, pin to one CPU).

rbench wins the bake-off when **decision quality** (multi-metric + process
units + inconclusive + Callgrind) matters more than the absolute nanosecond
gap for that workload.

## Combined ship gate

After a multi-metric Suite run that emits `wall`, `throughput`, and
`os.rss_peak`:

```sh
cargo rbench gate --run .rbench/run --config docs/examples/budgets-and.json
```

The `ship` group requires **all** member budgets to pass (AND).
