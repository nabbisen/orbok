# Testing

## Running tests

```sh
# Workspace formatting gate
cargo fmt --all --check

# Standard workspace library gate
cargo test --workspace --lib

# App binary gate — RFC-049 isolation and denial-harness matrix. This is
# the only command that reaches it: the app crate's tests live in its
# binary target (main.rs), not its library target, so neither
# `--workspace --lib` above nor `cargo test -p orbok --lib` runs them.
cargo test -p orbok --bin orbok --locked

# Strict clippy gate
cargo clippy --workspace --all-targets -- -D warnings

# Current feature-matrix gate
cargo check -p orbok-embed --features tract
cargo test -p orbok-embed --features tract --lib

# RFC lifecycle integrity gate
bash scripts/check-rfc-lifecycle.sh

# Design-token gate (RFC-032/052 §5) -- no literal font size, padding,
# spacing, radius, or colour in crates/ui/src/{views.rs, views/*.rs,
# shell.rs, components.rs}
bash scripts/check-design-tokens.sh
bash scripts/check-design-tokens.test.sh   # self-test: clean + planted-violation fixtures

# i18n literal-copy gate (RFC-052 §6) -- every designated UI/platform-
# integration file routes visible copy through the typed catalog; see
# scripts/check-i18n-literals.sh's own header for the exact designated
# directories and SCAN_FILES/EXCLUDED_FILES classification
bash scripts/check-i18n-literals.sh
bash scripts/check-i18n-literals.test.sh   # self-test: fixtures + discovery/classification exhaustiveness

# Exhaustive (Locale, MessageKey) catalog tests (RFC-052 §4 rule 2) --
# already included in "Standard workspace library gate" above; scoped
# separately here for the fast gate, which excludes orbok-ui otherwise
cargo test -p orbok-ui --lib tests::i18n

# Strict supply-chain audit gate
cargo audit --deny warnings

# Keyword-only benchmark evidence
cargo run -p orbok-bench --release -- \
  1000 target/orbok-bench/results --expect-mode keyword-only

# Real-model benchmark evidence, using a local recommended model directory
cargo run -p orbok-bench --release --features orbok-embed/tract -- \
  1000 target/orbok-bench/results-real-model \
  --model-dir /path/to/multilingual-e5-small \
  --expect-mode hybrid-real-model

# Headless backend gate, using a fresh scratch data directory
rm -rf .git-exclude/tmp/orbok-check
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

## The embedding model for development

Real-model tests (`RFC013_MODEL_DIR`, the real-model benchmark above, a
manual pass that includes search by meaning) need `multilingual-e5-small`
on disk. It lives at:

```
${XDG_CACHE_HOME:-$HOME/.cache}/orbok-dev/models/multilingual-e5-small/
```

(Review Request 319 §3.) This is a development fixture, not a profile:

- **Not the production data directory** (`~/.local/share/orbok/…`). That
  is a real profile, which the GUI rule file keeps every test out of.
- **Not the repository.** The model is several hundred megabytes; it does
  not belong in source control or in anything archived with it.
- **A cache directory**, because it can always be downloaded again. If it
  is cleared, the next task that needs it fills it again — that is the
  only cost.

**To fill it**, use orbok's own verified download rather than fetching
the files by hand: run the built binary once with a scratch
`ORBOK_DATA_DIR` (the GUI rule file's isolation applies, same as any
other manual pass), go through the wizard's model download, and let it
verify every file against Appendix B's pinned checksums. Then move the
two files it downloads (`tokenizer.json` and `onnx/model.onnx` —
`orbok_workers::model_verifier::REQUIRED_MODEL_FILES`) from that scratch profile's
`models/multilingual-e5-small/generations/<id>/` into the fixture path
above, flat (no `generations/<id>/` level, no `COMPLETE` or
`trusted-manifest.json`), and remove the scratch profile.

**To use it:** `RFC013_MODEL_DIR=$XDG_CACHE_HOME/orbok-dev/models/multilingual-e5-small`
(or the `$HOME/.cache` default) for the model-backed tests; the wizard's
"Point orbok at the folder" with the same path for a manual pass. orbok
only reads from this directory — it never writes into it.

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

## RFC-049 isolation and denial testing

`cargo test -p orbok --bin orbok --locked` runs the Standard/Portable
two-way isolation matrix
(`runtime_isolation_tests::standard_and_portable_startup_check_recovery_and_later_access_stay_isolated`)
and its supporting tests, in `crates/app/src/runtime_isolation_tests.rs`.

The matrix runs under an armed OS-level denial boundary around the
*inactive* profile — not just a before/after snapshot comparison, which
cannot detect a read-only access:

- **Unix/macOS:** `chmod 000` on the inactive profile's data and settings
  roots.
- **Windows:** an `icacls` deny ACE, deliberately excluding write-DAC so the
  same (denied) account retains the right to remove its own ACE on
  teardown.

Both self-check that the denial actually took effect — a known sentinel
read must fail — immediately after arming, and fail loudly rather than
silently pass if it cannot arm (`assert_denial_armed`). A green run on a
runner that cannot actually deny anything (e.g. an unexpectedly privileged
container) would be caught by this check, not mistaken for isolation
evidence.

`physical_bind_mount_identity_alias_is_rejected` (Linux only) additionally
needs unprivileged user-namespace creation (`bwrap --unshare-user`), which
some CI runners restrict. It probes this capability first; if unavailable,
it skips and writes a notice **directly to the stderr file descriptor**
(`emit_capability_skip_notice`, not `eprintln!` — libtest captures
`eprintln!`/`println!` output for a passing test and would silently drop
the notice). The skip is visible in plain `cargo test` output without
`--nocapture`, and is exercised deliberately by
`bind_mount_probe_skip_is_visible_without_nocapture`, which forces the
branch with a fake `bwrap` and asserts the notice appears in captured child
output.

## CI gates

See [`docs/src/maintainers/release_readiness.md`](release_readiness.md)
for the full CI gate definition.
