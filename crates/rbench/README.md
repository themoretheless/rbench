# rbench

Explicit, reproducible Rust benchmarks and scenario observations. Each operation
gets a fresh input; input creation and teardown stay outside the measured region,
and a `DropPolicy` decides when the operation's result is dropped. Release builds
are required for measurement.

This crate is the library API. The companion runner and offline reports live in
the `cargo-rbench` crate (`cargo install cargo-rbench`).

## Add the dependency

```toml
[dependencies]
rbench = "0.1"
```

Optional features:

- `macros` — `#[rbench::bench]` attribute that generates a `register_NAME(&mut suite)` function.
- `memory` — bounded allocation tracking via `TrackingAllocator<System>`.

For a Cargo benchmark target, set `harness = false`.

## Example

```rust
use rbench::{DropPolicy, Suite};

fn main() -> rbench::Result<()> {
    let mut suite = Suite::new("collections");
    suite.bench_with_input(
        "sort/1000",
        || (0..1000u64).rev().collect::<Vec<_>>(),
        |input| input.sort_unstable(),
        DropPolicy::InsideTiming,
    ).parameter("elements", 1000);
    suite.main()
}
```

## Using it from another project before a crates.io release

Depend on it straight from Git:

```toml
[dependencies]
rbench = { git = "https://github.com/themoretheless/rbench" }
```

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.
