# Local Development

```sh
# Requires Rust 1.91+ (install via rustup.rs)

# Run the full test suite
cargo test --workspace --lib

# App binary tests — RFC-049 isolation/denial matrix and everything else
# under crates/app/src/main.rs. NOT reached by --workspace --lib or -p
# orbok --lib (the app binary's tests live in its binary target); see
# testing.md for what this covers.
cargo test -p orbok --bin orbok --locked

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
  access) rather than one silently winning. An absent or empty
  `ORBOK_DATA_DIR` is treated as unset, not as an explicit override, so it
  never triggers that rejection.
- The startup directory is resolved to one absolute, normalized anchor and
  frozen into the `RuntimeContext` at construction, before any argument is
  acted on further. A later `chdir` in the same process cannot redirect
  either profile — see
  `frozen_startup_anchor_survives_a_later_current_directory_change`.
- An unwritable or otherwise invalid portable directory is a hard failure,
  never a silent downgrade to the standard profile.
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

## Packaging (RFC-051)

```sh
bash scripts/package.sh 0.17.0
# Produces dist/orbok-0.17.0.tar.gz and dist/orbok-0.17.0.tar.gz.sha256

bash scripts/verify-release-archive.sh dist/orbok-0.17.0.tar.gz
# Independently re-derives the expected file set from `git ls-tree` plus
# scripts/release-path-policy.sh -- never from package.sh's own output --
# and checks Cargo.lock is present and coherent (`cargo metadata --locked`
# against a fresh unpack).

bash scripts/package.test.sh
# Adversarial self-test: untracked/ignored files never leak in and never
# block packaging; a force-tracked local-only path is excluded anyway;
# dirty tracked content and tracked symlinks both fail packaging;
# executable bits and canonical ./ naming survive; two clean builds are
# byte-identical; a planted producer/policy disagreement is caught by the
# independent verifier.
```

The archive is flat (no parent directory), built only from the git-tracked
file set at the packaged commit — never the ambient working directory.
Packaging fails if any tracked file is dirty relative to `HEAD`; untracked
and ignored files are structurally absent from the input, never a reason to
fail. `Cargo.lock` is included, since release gates and audit run against
it. Files unpack directly into the extraction destination.

Metadata (path order, uid/gid, mode, mtime, gzip headers) is normalized so
two clean builds of the same commit on the same toolchain produce a
byte-identical archive — see `scripts/release-path-policy.sh` for the
shared inclusion/exclusion policy, and RFC-051 §5 for what reproducibility
does and does not cover (within-release, not across time or environments).
