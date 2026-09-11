# How rbench beats (and does not beat) competitors

Honest positioning against Criterion, Divan, iai/iai-callgrind, and hyperfine.
The goal is **measurement validity for confirmatory product gates**, not a blanket
“fastest microbench crate” claim.

## Scorecard (capability, not marketing)

| Dimension | rbench | Criterion | Divan | iai | hyperfine |
|---|---|---|---|---|---|
| Independent process A/B units | **Lead** | Weak (in-process default) | Weak | N/A | Strong for commands |
| Typed multi-metric + Availability | **Lead** | DIY | DIY | Counters only | Wall only |
| Inconclusive-aware CI gates | **Lead** | DIY | N/A | Different failure mode | DIY |
| Hot-loop ergonomics / plots | Trail | **Lead** | **Lead** (overhead) | N/A | N/A |
| Valgrind-deterministic CI | Trail (perf_event optional) | Trail | Trail | **Lead** | Trail |
| Simple command wall timing | Capable | N/A | N/A | N/A | **Lead** |

Reproduce the machine-readable form:

```sh
cargo rbench compete
cargo rbench doctor
```

## Killer differentiators (implemented)

1. **Process-paired confirmatory design** — AB/BA, leases, hashes, incomplete/cancelled states.
2. **Multi-metric contracts in one artifact** — wall, throughput (`work_units`), OS RSS/CPU, Linux `perf.instructions`/`perf.cycles` when permitted, scenario metrics with explicit Availability.
3. **Gates that admit ignorance** — `Inconclusive` is not equivalence and is not a silent pass.
4. **Host honesty** — `doctor` reports perf_event probe, loadavg/governor, pin env; `accept --hardware` runs live Instant A/A under current noise.
5. **Atomic publish / recover** — interrupted runs are classified, not silently “complete”.

## What we refuse to claim

- Beating Criterion/Divan on day-to-day microbench UX or absolute hot-loop overhead.
- Beating iai on Valgrind-deterministic CI until a Callgrind adapter ships.
- Treating shared GitHub-hosted runners as controlled performance acceptance.
- Equating `RBENCH_PIN_CPU` / loadavg snapshots with BenchExec-grade isolation.

## Reproduce locally

```sh
# Capability matrix + host probe
cargo rbench compete -o .rbench/compete-scorecard.json
cargo rbench doctor

# Synthetic statistical battery (offline)
cargo rbench accept --seed 42 -o .rbench/accept.json

# Live host A/A (noisy; optional pin)
RBENCH_PIN_CPU=0 cargo rbench accept --hardware --pairs 8 --trials 20 -o .rbench/accept-hw.json

# Multi-metric Suite demo (wall + throughput + process + perf sibling batches)
cargo run --release -p rbench --example compete --offline > .rbench/compete-demo.json

# Suite bookkeeping vs handwritten Instant (not vs Criterion)
cargo run --release -p rbench --example overhead --offline
```

## Suggested Criterion/Divan bake-off (external)

Keep Criterion/Divan out of the default dependency graph. On a quiet machine, time
the **same** `mix64` batch (256 iterations × N samples) under:

1. handwritten `Instant` loop (`examples/overhead.rs`)
2. `rbench` Suite (`examples/compete.rs` / `overhead.rs`)
3. a local Criterion bench with identical useful work
4. a local Divan bench with identical useful work

Publish medians + relative gaps. rbench wins the bake-off only if the **decision
quality** (multi-metric + process units + inconclusive) matters more than the
absolute nanosecond gap for that workload.

## Next competitive moves (open)

- Callgrind/iai adapter for deterministic CI counters.
- Stronger isolation (cgroup freeze / cpuset) beyond affinity.
- Published Criterion/Divan numbers checked into `docs/` after a quiet-host run.
- Combined wall ∩ throughput ∩ RSS gate helpers in CLI.
