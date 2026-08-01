# RFC-053: rusqlite Line and Rust MSRV Policy

**Project:** orbok\
**RFC:** 053\
**Title:** rusqlite Line and Rust MSRV Policy\
**Status:** Proposed\
**Target milestone:** v1.0.0 stabilization\
**Date:** 2026-08-01\
**Related RFCs:** RFC-002 SQLite Catalog Schema and Migration Policy (§16 one-`libsqlite3-sys` rule); RFC-017 Packaging and Distribution Strategy; RFC-019 Test Matrix and Release Readiness\
**Supersedes decision:** DEC-006 (pin `rusqlite 0.40` to match `localcache 0.20.0`)\
**Handoff:** [`HANDOFF-053-rusqlite-line-and-msrv.md`](../handoffs/HANDOFF-053-rusqlite-line-and-msrv.md)

---

## 1. Summary

Move orbok from `rusqlite 0.40` to `rusqlite 0.39`, and replace the declared
Rust MSRV with a measured one.

This reverses DEC-006. The pin existed to match `localcache`; localcache has
since moved its own requirement to `^0.39`, so the pin now does the opposite of
what it was written to do. The move also removes an undeclared Rust 1.95 floor
that orbok has been carrying, unannounced, since it adopted rusqlite 0.40.

No schema, storage format, or SQL changes.

## 2. Triggering evidence

**The MSRV floor is 1.95, not the declared 1.85.** `libsqlite3-sys 0.38.x`,
required by `rusqlite 0.40`, uses the `cfg_select!` macro in its build script.
Measured in orbok's own graph:

```
cargo +1.94 check -p orbok-db --locked
  → error[E0658]: use of unstable library feature `cfg_select`
    error: could not compile `libsqlite3-sys` (build script)
cargo +1.95 check -p orbok-db --locked
  → Finished
```

This never surfaced because `cargo` only reports crates that *declare*
`rust-version`, and neither `rusqlite` nor `libsqlite3-sys` does. The failure is
at compile time, not resolution time. `Cargo.toml` declares `1.85` and the
README advertises "requires Rust 1.85+"; both have been wrong by ten releases.

**The pin no longer matches its own rationale.** DEC-006 pinned 0.40 "to match
localcache 0.20.0". localcache's requirement history: 0.19.1 `^0.40` → 0.20.1
`^0.39` → 0.21.0 `^0.39`. Because `libsqlite3-sys` declares `links = "sqlite3"`
and cargo permits exactly one such package per graph, orbok cannot adopt any
localcache past 0.20.0 — a hard resolution failure, not a tolerable duplicate.

**The upstream fix was requested and declined, with good reason.** localcache
measured the same 1.95 floor and declined to impose it on every consumer; a
second downstream project had concurrently asked them to restore 1.85. Their
decision is sound and orbok has no local remedy on the 0.40 line. The request,
their response, and orbok's withdrawal are retained outside version control per
the upstream-request convention.

## 3. Decision

1. orbok requires `rusqlite 0.39` with the `bundled` feature.
2. orbok's declared `rust-version` is set to a **measured** floor — the lowest
   toolchain on which the full workspace actually builds — not an aspirational
   one.
3. orbok tracks localcache's current line rather than pinning against a specific
   localcache release.
4. **MSRV is a measured property, not a declaration.** Any future change to a
   dependency line that moves the floor must re-measure and update both
   `Cargo.toml` and the README in the same change.

## 4. Rationale

orbok is an end-user desktop application installed with `cargo install`. A floor
three releases below current stable is an adoption barrier in a way it would not
be for a library. Once rusqlite 0.40 was identified as the sole cause of the
1.95 floor, roughly seven releases of headroom outweighed a two-year-newer
bundled SQLite (3.53.2 → 3.51.3).

Feasibility is established rather than assumed. orbok's entire rusqlite surface
across four crates (`orbok-db`, `orbok-cache`, `orbok-workers`, `orbok-search`)
is `params`, `params_from_iter`, `query_row`, `query_map`, `transaction`,
`execute_batch`, `pragma_update`, `Connection::open`, `Row`, `Result`, and
`Error::QueryReturnedNoRows` — all long-stable. SQLite feature use is limited to
WAL, `foreign_keys`, `synchronous`, `temp_store`, and FTS5 with `unicode61`, all
long predating 3.51.

## 5. Scope

**In scope**

- The `rusqlite` requirement and the workspace `rust-version`.
- README's stated Rust requirement.
- Upgrading `localcache` from 0.20.0 to its current line, once unblocked.
- Updating DEC-006's record and the RFC-002 §16 note.

**Out of scope**

- Schema, migration, storage format, or SQL changes.
- Adopting `ReadPool` or any other new localcache capability. Availability is not
  adoption; that needs its own RFC if wanted.
- The `event-listener` / RUSTSEC-2026-0221 advisory, which reaches orbok through
  the GUI stack rather than localcache, and is tracked separately.
- Raising MSRV again to obtain a newer SQLite.

## 6. Implementation slices

Two independently reviewable slices; do not combine.

**Slice 1 — rusqlite line and MSRV.** Move to `rusqlite 0.39`. Measure the
resulting floor by building at candidate toolchains and set `rust-version` to the
lowest that passes. Update the README. Full gate set.

**Slice 2 — localcache upgrade.** Move to localcache's current line. Note its
0.21.0 breaking changes: `LocalFileCacheError` became `#[non_exhaustive]`, and
JSON codec failures return `Serialization` rather than `UnsupportedFeature`.
orbok has no exhaustive match on that type — `cache_err` in
`crates/data/cache/src/service.rs` maps via `to_string()` — so no code change is
expected, but this must be confirmed rather than assumed.

## 7. Testing and verification

1. Full gate set per `docs/src/maintainers/release_readiness.md`, plus the
   RFC-049 isolation suite.
2. **MSRV verified by building, not by reading manifests.** The declared value
   must be the lowest toolchain on which `cargo check --workspace --all-targets`
   actually succeeds. The `cfg_select` case proves manifest inspection is
   insufficient: cargo cannot see an undeclared floor.
3. `cargo tree -d` shows exactly one `rusqlite` and one `libsqlite3-sys`
   (RFC-002 §16).
4. Catalog and cache databases created under the previous SQLite open and
   migrate unchanged; the migration suite passes from empty and is idempotent.
5. `cargo audit` shows no new advisory introduced by the older line.
6. **Security review of the SQLite delta.** Confirm no security-relevant fix
   between bundled 3.51.3 and 3.53.2 affects orbok's usage. If one does, it is an
   input to the decision and must be reported before the change lands.

## 8. Risks

| Risk | Assessment |
|---|---|
| Older bundled SQLite lacks a needed fix | Requires the §7.6 check. Only the WAL/FTS5/pragma surface is used. |
| rusqlite 0.39 API gap | Low — the surface is long-stable, and localcache verified both lines compile identically. |
| Measured floor is higher than the ~1.88 estimate | Possible. The estimate comes from *declared* versions; other undeclared floors may exist, exactly as `libsqlite3-sys` did. §7.2 measures rather than predicts. |
| Perceived as going backwards | Mitigated by recording the rationale here, so a future maintainer does not "fix" the old pin and silently restore the 1.95 floor. |

## 9. Acceptance criteria

Accepted when the rusqlite-line reversal and the measured-MSRV policy are
approved.

Implemented when: orbok builds and passes all gates on `rusqlite 0.39`; the
declared `rust-version` equals a verified build floor; the README matches it;
exactly one `libsqlite3-sys` resolves; localcache is on its current line; and
DEC-006's record notes its supersession by this RFC.
