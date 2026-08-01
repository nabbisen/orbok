# Local Development

```sh
# Requires Rust 1.85+ (install via rustup.rs)

# Run the full test suite
cargo test --workspace --lib

# Run the GUI
cargo run

# Portable mode (catalog, cache, models, AND settings in ./orbok-data/
# instead of the platform app-data/config dirs; combining with
# ORBOK_DATA_DIR below is rejected, not silently resolved)
cargo run -- --portable

# Headless backend check (no display needed)
cargo run -- --check

# With a custom data dir (standard mode only)
ORBOK_DATA_DIR=/tmp/orbok-dev cargo run -- --check
```

### Portable/standard precedence (RFC-049)

One immutable runtime context is resolved before any catalog, cache,
settings, model, or recovery operation, and every later operation in the
process uses only that context — see
`crates/app/src/runtime_context.rs`/`crates/app/src/runtime_storage.rs`.
Two points worth knowing when touching this area:

- Standard mode's settings live in the platform config directory, separate
  from `ORBOK_DATA_DIR` (which only affects catalog/cache/models). Portable
  mode relocates settings alongside everything else under `./orbok-data/` —
  the two modes are not symmetric in what `ORBOK_DATA_DIR` covers.
- `--portable` combined with a non-empty `ORBOK_DATA_DIR` fails closed
  (rejected during argument resolution, before any profile filesystem
  access) rather than one silently winning.
- The relative `./orbok-data/` label on portable startup and the full
  absolute path printed by `--check` are both intentional, not an
  inconsistency: the interactive startup message stays relative per the
  diagnostics/privacy minimal-disclosure default, while `--check` is an
  explicit headless diagnostic command where showing the resolved path is
  the point (RFC-049 §4.7).

## Testing Philosophy

Tests validate design specifications (RFC acceptance criteria), not
merely the written code. Each crate's `tests.rs` cites the RFC
section it targets.

Test organisation mirrors the module structure:

- Inline tests live in `src/tests.rs`.
- If `tests.rs` exceeds ~300 ELOC, contents move into submodules
  under `src/tests/`.
- The same line-count rules apply inside `tests/`.

## Module Style

orbok uses Rust 2018+ module layout throughout:

- A `foo.rs` file and a `foo/` subdirectory may coexist.
- `mod.rs` is never used. Place the module router in `foo.rs` and
  submodule files inside `foo/`.

## Packaging

```sh
bash scripts/package.sh 0.17.0
# Produces dist/orbok-v0.17.0.tar.gz and dist/orbok-v0.17.0.tar.gz.sha256
```

The archive is flat (no parent directory). Files unpack directly into
the extraction destination.
