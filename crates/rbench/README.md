# rbench

Explicit, reproducible Rust benchmarks and scenario observations. Each operation
gets a fresh input; input creation and teardown stay outside the measured region,
and a `DropPolicy` decides when the operation's result is dropped. Release builds
are required for measurement.

This crate is the library API. The companion runner and offline reports live in
the `cargo-rbench` crate (`cargo install cargo-rbench`).

## Add the dependency

Depend on it straight from Git (Cargo pins the resolved commit in your
`Cargo.lock`; `cargo update` moves it forward):

```toml
[dependencies]
rbench = { git = "https://github.com/themoretheless/rbench", branch = "release" }
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

## Using it from another project via Git

No crates.io needed. Cargo pins the resolved commit in your `Cargo.lock`;
`cargo update` moves it forward.

```toml
# latest RELEASED version: the `release` branch advances on each release
rbench = { git = "https://github.com/themoretheless/rbench", branch = "release" }
# pinned version (v* tags are created automatically on a version bump)
rbench = { git = "https://github.com/themoretheless/rbench", tag = "v0.1.0" }
# tip of the default branch (includes unreleased work)
rbench = { git = "https://github.com/themoretheless/rbench" }
```

Cargo cannot pick the highest semver *tag* from Git — semver ranges such as
`rbench = "0.1"` only work through a registry. "Latest release" is the moving
`release` branch, exact versions are `v*` tags, and `main` is the latest commit.
The `release` branch appears after the first release; until then use `main`.

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.
