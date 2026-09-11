# How rbench beats competitors

Honest scorecard against Criterion, Divan, iai/iai-callgrind, and hyperfine.
Goal: **measurement validity for confirmatory product gates**.

## Scorecard

| Dimension | rbench | Criterion | Divan | iai | hyperfine |
|---|---|---|---|---|---|
| Independent process A/B units | **Lead** | Weak (in-process default) | Weak | N/A | Strong for commands |
| Typed multi-metric + Availability | **Lead** | DIY | DIY | Counters only | Wall only |
| Inconclusive-aware CI gates | **Lead** | DIY | N/A | Different failure mode | DIY |
| Hot-loop ergonomics | **Lead** (Suite) / Competitive (plots) | Deepest HTML plots | Lowest absolute overhead on tiny bodies | N/A | N/A |
| Valgrind-deterministic CI | **Lead** (tied; Callgrind adapter) | Trail | Trail | **Lead** (tied) | Trail |
| Simple command wall timing | **Lead** (contracts) / Competitive (CLI) | N/A | N/A | N/A | Competitive |

```sh
cargo rbench compete
cargo rbench doctor
```

## Closed gaps (this slice)

1. **Callgrind / iai-class CI counters** — `rbench::callgrind` + `cargo rbench callgrind --program PATH`.
2. **Hot-loop ergonomics** — `#[rbench::bench]`, `#[rbench::main]`, `Suite::bench_batch`.
3. **Hyperfine-class command timing** — `cargo rbench time`.
4. **Stronger isolation** — best-effort cgroup v2 via `RBENCH_CGROUP*` (snapshot + doctor).
5. **Combined ship gates** — budget `groups` with AND semantics (`docs/examples/budgets-and.json`).
6. **Bake-off harness** — `examples/bakeoff.rs` + `docs/BAKEOFF.md` (Criterion/Divan stay external).

## Still refuse to claim

- Zero overhead vs handwritten Instant / Divan on every micro-body.
- Treating shared GitHub-hosted runners as controlled performance acceptance.
- Equating `RBENCH_PIN_CPU` / `RBENCH_CGROUP` / loadavg snapshots with BenchExec-grade isolation.
- Callgrind Ir equals wall-time or uninstrumented instruction counts.
- Hosted GitHub Actions as controlled performance acceptance.
- `RBENCH_PIN_CPU` / loadavg == BenchExec isolation.
- Callgrind Ir == wall time or uninstrumented instruction counts.

## Reproduce

```sh
cargo rbench doctor
cargo rbench compete -o .rbench/compete.json
cargo rbench time --runs 30 --warmup 3 -- /bin/true
cargo rbench callgrind --program ./target/release/examples/overhead -o .rbench/callgrind.json
cargo run --release -p rbench --example overhead --offline
cargo run --release -p rbench --example compete --offline
```
