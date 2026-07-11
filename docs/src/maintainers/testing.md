# Testing

## Running tests

```sh
# Standard workspace library gate
cargo test --workspace --lib

# Headless backend gate, using a scratch data directory
ORBOK_DATA_DIR=.git-exclude/tmp/orbok-check cargo run -p orbok -- --check

# All non-GUI crates (fast — no iced compile)
cargo test --workspace --exclude orbok --exclude orbok-ui --exclude orbok-bench

# Single crate
cargo test -p orbok-workers

# Specific test category
cargo test -p orbok-workers security
cargo test -p orbok-workers benchmark

# With logging
RUST_LOG=debug cargo test -p orbok-workers -- --nocapture
```

## Test organisation

Each crate's tests live in `src/tests.rs` (module) or `src/tests/`
(subdirectory). Tests validate design specs from the RFCs, not merely
the written code. Every test that exercises a security property is
labelled `// RFC-NNN §N test N: ...`.

## Test categories

| Category | Location | Coverage |
|---|---|---|
| Unit | Per-crate `tests.rs` | RFC acceptance criteria |
| Integration | `orbok-workers/src/tests/` | End-to-end pipeline |
| Security | `v05_features::security` | RFC-015 §19 |
| Benchmark smoke | `v05_features::benchmark` | RFC-016 §17 |

## CI gates

See [`docs/src/maintainers/release_readiness.md`](release_readiness.md)
for the full CI gate definition.
