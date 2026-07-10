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

These are the gates treated as release-blocking for the current v0.22 line:

- `cargo fmt --check` — zero formatting violations.
- `cargo test --workspace --lib` — all workspace library tests pass.
- RFC lifecycle integrity — status fields, folder placement, index links, and
  RFC numbers remain coherent.
- Version and lockfile coherence — workspace version and `Cargo.lock` package
  versions agree.
- Supply-chain vulnerability baseline — `cargo audit` passes with only
  documented waivers from `.cargo/audit.toml`.
- Release archive checks — archive name includes version, layout is flat, and
  generated checksums accompany the archive.

## Advisory / Not Yet Blocking

These checks are useful and should be run when relevant, but are not currently
documented as release-blocking for the v0.22 line:

- `cargo clippy --workspace --all-targets` — advisory today; warnings exist.
- `cargo clippy --workspace --all-targets -- -D warnings` — target gate, not
  green yet.
- `cargo audit --deny warnings` — informational advisories are visible but not
  denied by the current baseline.
- `cargo deny` — not configured as a blocking gate yet.
- `cargo run -p orbok -- --check` — useful headless smoke check, but not part
  of the current blocking gate set.
- Feature matrix checks — `cargo check -p orbok-embed --features tract` is
  useful coverage, but the required feature set is not yet a blocking policy.

## Future Gate Alignment

Before v1.0.0, decide which advisory checks become blocking and update this
document in the same change that makes them green or explicitly waives them.
At minimum, the open decisions are:

- Whether clippy is advisory or `-D warnings`.
- Whether supply-chain checks add `cargo deny`, and whether `cargo audit`
  should deny informational warnings.
- Whether `orbok --check` is required for every release.
- Which Cargo feature combinations must compile for every release.

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

---

## Packaging Checklist (RFC-017)

- [ ] Checksum file accompanies every archive (`orbok-vX.Y.Z.tar.gz.sha256`)
- [ ] Archive name includes version: `orbok-vX.Y.Z.tar.gz`
- [ ] Archive contains: `Cargo.toml`, all `crates/`, `rfcs/`, `docs/`, `scripts/`
- [ ] Archive does **not** contain: `target/`, `.git/`, `Cargo.lock`
- [ ] `orbok --version` output matches the Cargo.toml version

---

## RFC Status Lifecycle

New RFCs start in `rfcs/proposed/`. They move to `rfcs/done/` when the
implementation ships in a tagged release. The status field in each RFC
records the release version: `Implemented (v0.5.0)`.

No RFC is ever deleted. Withdrawn or superseded RFCs move to `rfcs/archive/`.
