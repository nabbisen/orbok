# Implementation Handoff — RFC-053: rusqlite Line and Rust MSRV Policy

**Project:** orbok  
**RFC:** 053  
**Lifecycle stage:** Design + handoff  
**Primary owner:** workspace manifests; `orbok-db` / `orbok-cache` / `orbok-workers` / `orbok-search` build surface  
**RFC:** [`../proposed/053-rusqlite-line-and-msrv-policy.md`](../proposed/053-rusqlite-line-and-msrv-policy.md)

> **Scope rule:** This is a dependency-line and MSRV change. Do not adopt new
> localcache capabilities, do not touch schema or SQL, and do not combine the
> two slices.

## 1. Expected Change Surface

- `Cargo.toml` (workspace): `rusqlite` requirement, `localcache` requirement,
  `rust-version`, and the pin comment recording DEC-006's supersession
- `Cargo.lock`
- `README.md` — the stated Rust requirement in Quick Start
- `docs/src/maintainers/development.md` — toolchain prerequisite, if stated there
- `docs/src/maintainers/dep_audit.md` — a dated entry recording the line change
- No `.rs` change is expected in either slice. If one proves necessary, stop
  (see §6).

## 2. Program Design

### Slice 1 — rusqlite line and MSRV

1. Change the workspace `rusqlite` requirement to `0.39`, keeping `bundled`.
2. Regenerate `Cargo.lock`. Confirm `cargo tree -d` resolves exactly one
   `rusqlite` and one `libsqlite3-sys` (RFC-002 §16).
3. **Measure the MSRV floor.** Do not infer it. Walk installed toolchains
   downward — 1.94, 1.93, 1.92, 1.91, 1.88, 1.87, 1.85 are available — running
   `cargo check --workspace --all-targets --locked` at each. The declared value
   is the lowest that passes. `cargo`'s own `rust-version` diagnostics are a
   *hint only*: `libsqlite3-sys` proved an undeclared floor can exist, so a
   toolchain that cargo does not object to may still fail to compile.
4. Set `rust-version` in `Cargo.toml` to the measured value and update
   `README.md` to match it exactly.
5. Update the `# database — pinned to 0.40 to match localcache 0.20.0` comment
   in `Cargo.toml` to record the new rationale and cite RFC-053.

### Slice 2 — localcache upgrade

1. Move `localcache` to its current published line.
2. Verify the 0.21.0 breaking changes do not reach orbok:
   `LocalFileCacheError` became `#[non_exhaustive]`, and JSON codec failures
   return `Serialization` instead of `UnsupportedFeature`. The only use in the
   workspace is `cache_err` in `crates/data/cache/src/service.rs`, which maps via
   `to_string()`, and there is no exhaustive `match` on the type. Confirm this
   against the tree at implementation time rather than trusting this note.
3. Confirm no new advisory appears in `cargo audit` from the moved graph.

## 3. Test Sequence

1. `cargo test --workspace --lib --locked` — full suite green.
2. Migration behavior: the `orbok-db` migration suite applies from empty and is
   idempotent; a catalog and cache created *before* the change still open and
   migrate afterwards. Use a database produced by the pre-change binary, not a
   freshly generated one.
3. `cargo test -p orbok --bin orbok --locked` — the RFC-049 isolation suite.
4. Fresh isolated Standard and Portable headless `--check` runs.
5. The MSRV walk from §2 Slice 1 step 3, with the result for each toolchain
   recorded — including the failures, since they are the evidence for the floor.

## 4. Review Slices

1. rusqlite line, measured MSRV, README and manifest updates.
2. localcache upgrade and advisory re-check.

## 5. Validation

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --lib --locked`
- `cargo test -p orbok --bin orbok --locked`
- `cargo tree -d` — one `rusqlite`, one `libsqlite3-sys`
- `cargo audit`
- `bash scripts/check-rfc-lifecycle.sh`
- `git diff --check`
- CI at the submitted head, per OS leg

## 6. Stop Conditions

Return to design review rather than deciding, if:

- any `.rs` change proves necessary to compile against rusqlite 0.39 — the RFC's
  feasibility case rests on the surface being long-stable, so a required source
  change invalidates a premise;
- the measured floor lands at or above 1.95, which would mean the move buys
  nothing and the decision should be revisited;
- an existing catalog or cache fails to open or migrate after the change;
- the §7.6 SQLite security-delta review finds a relevant fix between 3.51.3 and
  3.53.2 — report it before landing, as it is an input to the decision;
- `cargo audit` reports a new advisory attributable to the older line.

## 7. Definition of Done

Both slices reviewed; the workspace builds and passes all gates on
`rusqlite 0.39`; `rust-version` equals a floor demonstrated by an actual build;
`README.md` matches it; exactly one `libsqlite3-sys` resolves; localcache is on
its current line; pre-existing databases open and migrate unchanged; and
`Cargo.toml`'s pin comment plus `docs/src/maintainers/dep_audit.md` record the
change and supersede DEC-006.
