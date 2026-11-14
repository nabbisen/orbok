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
on disk. It lives at, project-root relative:

```
.git-exclude/dev-fixtures/models/multilingual-e5-small/
```

(Review Request 319b §5: files the team creates live inside the project
or under `/tmp`, never elsewhere, so this replaced an earlier
`~/.cache/orbok-dev/` choice.) This is a development fixture, not a
profile:

- **Not the production data directory** (`~/.local/share/orbok/…`). That
  is a real profile, which the GUI rule file keeps every test out of.
- **Not tracked.** `.gitignore` excludes all of `/.git-exclude/`, and the
  release archive is built only from tracked files (`git archive`), so
  this never reaches a commit or a shipped archive despite living inside
  the project tree.

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

**To use it,** from the project root: `RFC013_MODEL_DIR=$PWD/.git-exclude/dev-fixtures/models/multilingual-e5-small`
for the model-backed tests; the wizard's "Point orbok at the folder" with
the same path for a manual pass. orbok only reads from this directory —
it never writes into it.

**To remove it** when no longer needed: `rm -rf .git-exclude/dev-fixtures/`.
The next task that needs the model fills it again the same way; that is
the only cost.

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

## The headless UI layout matrix

`crates/ui/src/tests/task148_layout_matrix.rs`: renders Search (with a
result), Folders, Settings, the wizard's first screen, and the notice
through the real shell (`OrbokApp::view`), across every text size
(`TextScale::ALL`), both locales (`Locale::ALL`), and the narrowest
window this crate tests against (450x1400) plus the GUI rule file's own
1280x720 manual-capture default — asserting, for each combination, that
every named element is present, lies inside the window, and is not
clipped (Task 106's property, generalized from one scale/width to the
whole matrix). It also exposes a `min` check for a control's hit area
(Task 140's house 44 px rule), proven against the file's own red-first
tests rather than asserted against today's controls, which do not meet
it yet.

Theme is deliberately not part of the matrix — nothing here lays out
differently by theme (color tokens, not typography or spacing); "Follow
system" resolving to the right concrete theme is covered separately by
`task138_follow_system_follows_the_system.rs`.

```sh
cargo test -p orbok-ui --lib task148_layout_matrix
```

This is part of `orbok-ui`'s own test suite, which the Fast gate excludes
entirely for build-time reasons (compiling `orbok-ui`'s `iced`/`wgpu`/
`snora` dependency tree costs much more than running any one test module
inside it) — not because of this matrix's own runtime, which is a couple
of seconds. It runs on `main` (the Release gate's workspace library
tests) and on all three OSes (the Cross-platform smoke job's own
`orbok-ui` step), the same two places the rest of `orbok-ui`'s tests
already run.

## The real-UI session script

`scripts/gui-session.sh` is the one reusable setup for a pass that needs
an actual rendered window (what the layout matrix above cannot reach —
font rendering, "does it read well", anything needing human eyes). It
starts a nested niri compositor sized for the request, isolates every
directory the app reads (`ORBOK_DATA_DIR`, `XDG_CONFIG_HOME`,
`XDG_STATE_HOME`, `XDG_CACHE_HOME`) under a scratch directory inside
`.git-exclude/`, launches one binary inside it, captures its window, and
tears everything down:

```sh
scripts/gui-session.sh <task> <width> <height> <capture-name.png> <binary> [args...]
# for example:
scripts/gui-session.sh task150 1280 720 en-default.png target/release/orbok
```

The capture lands at `.git-exclude/evidence/<task>/<capture-name.png>`.
See the script's own header for what it verified about niri (a nested
compositor's outer window is still owned by the *host's* tiling layout
and must be floated and resized there, not configured from inside; the
host's output scale changes the arithmetic between the outer window size
and the nested app's logical size) and for which input channel
(`wtype`/`xdotool`) reaches the session once it is up — driving that
input is the caller's job, not this script's.

`scripts/gui-session.test.sh` is its self-test (skips, rather than
failing, on a machine with no niri session running).

## The previous release's build

An upgrade pass needs the previous release's binary. Rebuilding it from
a worktree every time (as every pass before this one did) costs a full
release build per pass. Instead, it is kept at, project-root relative:

```
.git-exclude/dev-fixtures/bin/orbok-<version>
```

with its commit and checksum recorded beside it in
`orbok-<version>.sha256`. Not tracked, same reasoning as the embedding
model fixture above: `.gitignore` excludes all of `/.git-exclude/`.

**To fill it**, from a clean worktree at the release tag:

```sh
git worktree add /tmp/orbok-prev-release <tag>
(cd /tmp/orbok-prev-release && cargo build --release -p orbok)
mkdir -p .git-exclude/dev-fixtures/bin
cp /tmp/orbok-prev-release/target/release/orbok \
   .git-exclude/dev-fixtures/bin/orbok-<version>
sha256sum .git-exclude/dev-fixtures/bin/orbok-<version> \
   > .git-exclude/dev-fixtures/bin/orbok-<version>.sha256   # add the tag and commit as comments
git worktree remove --force /tmp/orbok-prev-release
```

Built once per release, on the release commit (the same "once per
release" rule the next section states for the upgrade pass itself).

**To remove it** when no longer needed: `rm -rf .git-exclude/dev-fixtures/bin/`.

## When the expensive checks run

(`.git-exclude/rules/project-instructions-gui-common.md`'s "Don't repeat
an expensive pass on unchanged code" rule, restated for this project's
own three passes):

- **The upgrade pass** (the previous release's binary opens a profile
  this release wrote, or vice versa): when a migration, the settings
  format, or the cache format changed since the last pass.
- **The real model download** (the wizard's own download and trust
  verification, not a test against the fixture above): when the
  download or trust code changed.
- **Otherwise**, the model-backed tests with the fixture
  (`RFC013_MODEL_DIR`) plus the headless UI layout matrix above — both
  cheap enough to run every time regardless.

Each of the first two still runs at least once per release, on the
release commit, whether or not its own trigger fired since the last one.

## CI gates

See [`docs/src/maintainers/release_readiness.md`](release_readiness.md)
for the full CI gate definition.
