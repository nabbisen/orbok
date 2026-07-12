# Release Readiness (RFC-019)

This document defines the release gates and QA checklist for orbok releases.

---

## Release Readiness Levels

| Level | Name | Description |
|---|---|---|
| **RL-0** | Dev build | Compiles. Fast gate passes. Not for distribution. |
| **RL-1** | Alpha | All unit tests pass. `--check` runs cleanly. |
| **RL-2** | Beta | Release gate passes on all 3 platforms. Benchmark report present. |
| **RL-3** | RC | Manual QA checklist signed off. Security audit clean. |
| **RL-4** | Release | Checksums published. Changelog finalized. |

---

## Current Blocking Gates

These are the gates treated as release-blocking for the active post-v0.23
readiness track:

- `cargo fmt --all --check` — zero formatting violations across the workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` — zero clippy
  warnings across workspace library, binary, and test targets.
- `cargo test --workspace --lib` — all workspace library tests pass.
- Headless backend check —
  `ORBOK_DATA_DIR=<fresh-temp-dir> cargo run -p orbok -- --check` exits 0.
- Feature matrix — `cargo check -p orbok-embed --features tract` passes for
  the workspace's only currently declared package feature.
- RFC lifecycle integrity — `bash scripts/check-rfc-lifecycle.sh` verifies that
  status fields, folder placement, index links, and RFC numbers remain coherent.
- Version and lockfile coherence — workspace version and `Cargo.lock` package
  versions agree.
- Supply-chain vulnerability baseline — `cargo audit --deny warnings` passes with only
  documented waivers from `.cargo/audit.toml`.
- Release archive checks — archive name includes version, layout is flat, and
  generated checksums accompany the archive.

## Automation Coverage

The CI workflow covers the repository-verifiable blocking gates. Release
publication, platform sign-off, and real-model evidence still require owner
review before a release is cut.

| Gate | CI coverage | Manual / owner evidence |
|---|---|---|
| Formatting | `fast` job: `fmt` | None. |
| Strict clippy | `release` job: `strict clippy` | None. |
| Workspace library tests | `release` job: `workspace library tests` | None. |
| Headless backend check | `release` job: `--check headless` with a fresh `ORBOK_DATA_DIR` | Local release-prep rerun may be recorded when cutting an RC. |
| Feature matrix | `release` job: `feature matrix (tract)` | Real-model benchmark evidence remains separate. |
| RFC lifecycle integrity | `release` job: `RFC lifecycle integrity` | None. |
| Version and lockfile coherence | `release` job: `version and lockfile coherence`, plus `--version` after release build | Tag and release-note correctness remain owner responsibilities. |
| Supply-chain vulnerability baseline | `security` job: `audit dependencies` | Advisory `cargo deny` policy is not blocking. |
| Release archive checks | `release` job: `release archive checks` | Publishing the archive and checksum remains manual. |

## Advisory / Not Yet Blocking

These checks are useful and should be run when relevant, but are not currently
documented as release-blocking for the active post-v0.23 readiness track:

- `cargo deny` — useful for license, source, duplicate-version, and broader
  dependency-policy review, but not release-blocking until the project records
  acceptable license rationale, advisory-waiver ownership, duplicate-version
  escalation rules, allowed source policy, and maintenance expectations.

## Future Gate Alignment

Before v1.0.0, decide which advisory checks become blocking and update this
document in the same change that makes them green or explicitly waives them.
At minimum, the open decisions are:

- Whether `cargo deny` should become blocking after a formal
  license/source/dependency policy exists.
- Which Cargo feature combinations must compile for every release when new
  package features are added.

---

## Manual QA Checklist (required: RC → Release)

### Accessibility (RFC-034)

Run the full QA steps from [`docs/src/maintainers/accessibility.md`](accessibility.md)
before signing off, including:

- [ ] Keyboard-only walkthrough (all shortcuts, result navigation, Escape for overlays)
- [ ] High-contrast visual pass (all four theme presets)
- [ ] Grayscale status-distinguishability pass (badges distinguishable by icon + label)
- [ ] Screen reader spot check (VoiceOver / Orca)

### First launch

- [ ] Welcome screen appears
- [ ] Local-only badge visible
- [ ] Source selection works; sensitive path warning fires for `.ssh`
- [ ] Indexing starts after source selection

### Search

- [ ] Keyword search returns results for exact terms
- [ ] Identifier search (`ERR-4042`, `client_secret`) returns results
- [ ] Empty query state shows add-source prompt when no sources exist
- [ ] Search mode selector switches between Auto / Exact / Conceptual
- [ ] Source-missing badge appears when a source file is deleted

### Storage

- [ ] Storage view shows per-category MiB breakdown
- [ ] Safe cleanup removes snippets (source files unaffected)
- [ ] Reset catalog dialog requires confirmation
- [ ] Post-reset: sources list is empty; source files are intact

### Models

- [ ] Models view shows embedding and reranker rows with status
- [ ] Keyword-only notice appears when no embedding model is registered
- [ ] `locate` model action registers an on-disk file

### Settings

- [ ] Language switch to Japanese changes all UI text
- [ ] Language preference persists after restart

### Privacy

- [ ] Logs contain no document body text (check `RUST_LOG=debug`)
- [ ] `orbok --check` exits 0 on a fresh install
- [ ] No network requests observed during indexing

---

## Retrieval Benchmark Requirements (RFC-016)

A release candidate must include a benchmark report (`orbok-bench-report.md`)
showing:

- recall@5 ≥ 0.75 on the labeled query set
- p99 search latency ≤ 200 ms on a 1,000-document corpus
- indexing throughput ≥ 10 files/s on a modern laptop

Current v0.23 keyword-only evidence is tracked in
[`benchmark_report.md`](benchmark_report.md). The latest 1,000-document
release-mode keyword-only snapshot meets recall, p99 latency, and indexing
throughput targets. Real-model v1.0 validation uses the same benchmark command
with `--features orbok-embed/tract -- --model-dir <model-dir>` and remains a
separate required evidence item. Its JSON report must show
`"mode": "hybrid-real-model"` and a non-null `model` object recording model id,
name, version, and dimension.

---

## Packaging Checklist (RFC-017)

- [ ] Checksum file accompanies every archive (`orbok-vX.Y.Z.tar.gz.sha256`)
- [ ] Archive name includes version: `orbok-vX.Y.Z.tar.gz`
- [ ] Archive contains: `Cargo.toml`, all `crates/`, `rfcs/`, `docs/`, `scripts/`
- [ ] Archive does **not** contain: `target/`, `.git/`, `.git-exclude/`,
      `.agents/`, `.codex/`, `dist/`, `docs/book/`, `Cargo.lock`
- [ ] `orbok --version` output matches the Cargo.toml version

---

## RFC Status Lifecycle

New RFCs start in `rfcs/proposed/`. They move to `rfcs/done/` when the
implementation ships in a tagged release. The status field in each RFC
records the release version: `Implemented (v0.5.0)`.

No RFC is ever deleted. Withdrawn or superseded RFCs move to `rfcs/archive/`.
