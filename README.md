# Poly workspace

## Cargo network requirements
- Builds and tests require network egress to `https://index.crates.io`. The workspace defaults to the sparse protocol via `.cargo/config.toml` to minimize bandwidth and improve resilience.
- If outbound access to crates.io is blocked in CI or runner environments, set a sparse mirror URL and direct Cargo to use it:
  1. Export `CRATES_IO_MIRROR_URL` to the mirror root (for example, `https://crates-mirror.example.com`).
  2. Set `CARGO_REGISTRIES_CRATES_IO_REPLACE_WITH=crates-io-mirror`.
  3. Set `CARGO_SOURCE_CRATES_IO_MIRROR_REGISTRY="sparse+${CRATES_IO_MIRROR_URL%/}/index"`.
- With either direct egress or a mirror configured, run the validation steps below.

## Local and CI validation steps
Run commands from the repository root:

1) Fetch dependencies (fail-fast on registry issues):
```bash
cargo fetch --locked
```

2) Execute the full workspace test suite and capture output:
```bash
mkdir -p artifacts
cargo test --workspace --locked --all-targets --all-features -- --nocapture | tee artifacts/cargo-test.log
```

The CI workflow uploads `artifacts/cargo-test.log` for diagnostics.
