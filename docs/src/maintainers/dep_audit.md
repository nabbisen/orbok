# Dependency Audit

## 2026-08-03 Task 003 Part A: event-listener security fix

`event-listener` moved `5.4.1 → 5.4.2`, resolving `RUSTSEC-2026-0221`
(`StackSlot<'_, T>` unconditionally implementing `Send`/`Sync`, letting a
`!Send` tag set via `Event::with_tag` cross a thread boundary; classified
unsound, not known-exploitable). The advisory declares
`patched = [">= 5.4.2"]`, a semver-compatible patch inside the existing
`event-listener` requirement — no manifest change needed.

Reaches orbok only transitively, through `rfd`→`ashpd`→`zbus` and
`iced_winit`→`mundy`; orbok never calls it directly. `cargo update
--dry-run -p event-listener` confirmed the delta was this package alone
before applying. `cargo audit --deny warnings` is now clean.

## 2026-08-02 RFC-053 Slice 2: localcache moved to its current line

`localcache` moved `0.20.1 → 0.21.1` (workspace requirement `^0.20.0` →
`^0.21`), unblocked by Slice 1's rusqlite move — both `localcache` 0.20.1
and 0.21.1 declare `rusqlite = "0.39"`, so this was always going to resolve
to the same `rusqlite 0.39.0` / `libsqlite3-sys 0.37.0` Slice 1 already
locked; `cargo update -p localcache` confirms no other package moved.

0.21.0 makes `LocalFileCacheError` `#[non_exhaustive]` and adds a
`Poisoned` variant, and reclassifies JSON codec (de)serialization failures
from `UnsupportedFeature` to `Serialization`. Verified against the tree
(not assumed from the handoff's note): `crates/data/cache/src/service.rs`'s
`cache_err` is the only place in the workspace that names
`LocalFileCacheError`, and it does a blanket `e.to_string()` with no `match`
on variants — both changes are inert for orbok's compile-time behavior and
control flow. The only observable effect is the wrapped error text changing
from `"unsupported feature: json serialization error: …"` to
`"serialization error: json serialization error: …"` inside
`OrbokError::Cache(String)`; nothing in the workspace parses or matches on
that text (`git grep` for both substrings returns nothing outside
`localcache`'s own source). Confirmed by building and testing against
0.21.1 with zero `.rs` changes required.

`.cargo/audit.toml`'s `RUSTSEC-2025-0141` (`bincode`) waiver comment is
corrected: it previously said localcache "cannot be moved independently
here," which was true only while blocked on the rusqlite line (pre-Slice 1).
`bincode` is still 2.0.1 after the localcache move, so the waiver itself is
unchanged, but the stale rationale is fixed.

`ReadPool` and any other new 0.21.x capability are available but not
adopted — RFC-053 §5 scopes this to the line move only; adoption needs its
own RFC.

## 2026-08-01 RFC-053 rusqlite line reversal and measured MSRV

`rusqlite` moved `0.40.1 → 0.39.0` (`libsqlite3-sys` `0.38.1 → 0.37.0`),
superseding DEC-006. `rusqlite 0.40`'s `libsqlite3-sys 0.38.x` uses the
`cfg_select!` macro in its build script, which requires Rust 1.95 — an
undeclared floor neither crate reports, so `cargo`'s own diagnostics never
surfaced it. Manifest declared `1.85`; the true floor was ten releases
newer.

`localcache` incidentally moved `0.20.0 → 0.20.1` as part of the same
re-resolution (still within the workspace's unchanged `^0.20.0`
requirement — not the separate current-line upgrade RFC-053 Slice 2
tracks).

Workspace `rust-version` changed `1.85 → 1.91`, measured by building at
each candidate toolchain rather than inferred (`cargo +X check --workspace
--all-targets --locked`):

| Toolchain | Result | Cause |
|---|---|---|
| 1.94 | ✓ pass | — |
| 1.93 | ✓ pass | — |
| 1.92 | ✓ pass | — |
| 1.91 | ✓ pass | — |
| 1.88 | ✗ fail | `app-json-settings` 2.0.3 requires 1.90; `tract-core`/`tract-data`/`tract-extra`/`tract-nnef`/`tract-onnx`/`tract-transformers` 0.23.3 require 1.91 |
| 1.87 | ✗ fail | additionally `iced`/`iced_program`/`iced_selector`/`iced_test`, `time`/`time-core`/`time-macros`, `wgpu` 27.0.1 require 1.88 |
| 1.85 | ✗ fail | additionally `wayland-protocols` requires 1.86; `zbus`/`zbus_macros`/`zbus_names`/`zvariant`/`zvariant_derive`/`zvariant_utils` require 1.87 |

1.89 and 1.90 were not separately walked: `tract-core`/`tract-data`/
`tract-extra`/`tract-nnef`/`tract-onnx`/`tract-transformers` 0.23.3 declare
`rust-version = "1.91"`, and cargo enforces a declared `rust-version` by
refusing to build below it, so both toolchains would fail for the same
reason 1.88 does regardless of any other factor. 1.91 is the floor, and the
pass recorded at 1.91 is an actual build, not an inference from the gap.

The measured floor (1.91) is gated by `tract-core` and `app-json-settings`,
not by rusqlite/libsqlite3-sys — the rusqlite-specific 1.95 floor is fully
removed, but the workspace's true floor was already higher than the
declared 1.85 independent of this change. See
`rfcs/done/053-rusqlite-line-and-msrv-policy.md`.

Exactly one `rusqlite` and one `libsqlite3-sys` resolve (`cargo tree -d`),
preserving RFC-002 §16's single-`libsqlite3-sys` rule.

## 2026-07-11 cargo-deny deferral

`cargo-deny` remains advisory for the active post-v0.24/v1.0 readiness track.
Do not promote `cargo deny check` to a release-blocking gate until the project
records the policy that would make a `deny.toml` durable:

- acceptable license rationale
- advisory-waiver ownership and review cadence
- duplicate-version escalation rules
- allowed registry and git source policy
- maintenance expectations when dependency updates change the checked graph

`cargo audit --deny warnings` remains the required lockfile-wide RustSec
vulnerability baseline.

## 2026-07-10 security baseline

`cargo audit --deny warnings` is now configured as the supply-chain baseline.
The repository keeps the waiver list in `.cargo/audit.toml`; unwaived
vulnerabilities and warnings should fail CI.

Fixes applied:

- `lopdf`: 0.41.0 → 0.42.0 for RUSTSEC-2026-0187.
- `crossbeam-epoch`: 0.9.18 → 0.9.20 for RUSTSEC-2026-0204.
- `quinn-proto`: 0.11.14 → 0.11.16 for RUSTSEC-2026-0185.

Active waivers:

| Advisory | Crate | Reason |
|---|---|---|
| RUSTSEC-2025-0141 | `bincode` 2.0.1 | Pulled through `localcache` 0.21.1 (still `bincode` 2.0.1 after the RFC-053 Slice 2 line move); advisory is unmaintained status. |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | Transitive proc-macro helper in GUI/model-support paths; no direct orbok usage. |
| RUSTSEC-2026-0173 | `proc-macro-error2` 2.0.1 | Retained in `Cargo.lock` through a stale `defmt-macros` branch; not present in the active all-target dependency tree. |
| RUSTSEC-2026-0192 | `ttf-parser` 0.25.1 | Pulled by GUI/font and PDF stacks; replacement requires upstream dependency movement. |
| RUSTSEC-2026-0190 | `anyhow` 1.0.102 | Pulled by tract/prost real embedding paths; orbok does not directly call `anyhow::Error::downcast_mut`. |
| RUSTSEC-2026-0186 | `memmap2` 0.9.10 | Pulled by GUI/windowing/font stacks and `tract-onnx`; replacement requires upstream dependency movement. |
| RUSTSEC-2026-0194 | `quick-xml` 0.39.4 | Pulled through `wayland-scanner` 0.31.10 in the Linux GUI stack; `wayland-scanner` still requires `quick-xml ^0.39`. |
| RUSTSEC-2026-0195 | `quick-xml` 0.39.4 | Same `wayland-scanner` path as RUSTSEC-2026-0194. |
| RUSTSEC-2026-0206 | `rustybuzz` 0.20.1 | Pulled through the GUI SVG/text rendering stack (`iced` → `resvg`/`usvg`); advisory is unmaintained status and `cargo info rustybuzz` reports 0.20.1 as the current crate version. |

## 2026-06-20 dependency currency audit

Performed manually against crates.io / docs.rs.
(`cargo-outdated` could not be installed in the build environment due to
`openssl-sys` compile issues; the index was queried via `cargo update
--verbose` and `cargo generate-lockfile`.)

## Direct workspace dependencies

| Crate | Locked | Latest | Status |
|---|---|---|---|
| rusqlite | 0.40.1 | 0.40.1 | ✓ current |
| serde | 1.0.228 | 1.0.228 | ✓ current |
| serde_json | 1.0.150 | 1.0.150 | ✓ current |
| thiserror | 2.0.18 | 2.0.18 | ✓ current (1.0.69 is transitive only) |
| uuid | 1.23.2 | 1.23.2 | ✓ current (May 2026) |
| tokio | 1.52.3 | 1.52+ | ✓ current (LTS 1.51.x valid until Mar 2027) |
| tracing | 0.1.44 | 0.1.44 | ✓ current |
| tracing-subscriber | 0.3.23 | 0.3.23 | ✓ current |
| dirs | 6.0.0 | 6.x | ✓ current |
| time | 0.3.47 | 0.3.x | ✓ current |
| tempfile | 3.27.0 | 3.27 | ✓ current |
| **lopdf** | **0.42.0** | **0.42.0** | ✅ upgraded from 0.41 for RustSec baseline |
| **sha2** | **0.11.0** | **0.11.0** | ✅ upgraded from 0.10 |

## Added after initial audit

| Crate | Locked | Notes |
|---|---|---|
| rfd | 0.15 | Native OS folder picker dialog |
| reqwest | 0.12 (rustls-tls) | HuggingFace model download |
| futures | 0.3 | Async stream for download progress |
| tokio | 1.52.3 (`orbok` app crate) | Async runtime for download |
| iced_test | 0.14 (dev) | Headless view smoke tests |
| tokenizers | 0.23.1 | Optional under `orbok-embed/tract`; `default-features = false` with `fancy-regex` instead of native `onig` |

## Deferred upgrades (intentional)

| Crate | Locked | Available | Reason deferred |
|---|---|---|---|
| zip | 2.4.2 | 8.6.0 | Breaking API rewrite across 6 major versions; `FileOptions` → `SimpleFileOptions` → new builder API. Spec `"2"` is intentional. Upgrade when time allows full API migration. |
| generic-array | 0.14.7 | 0.14.9 | Pinned to exact `=0.14.7` by a transitive dep; cannot unilaterally update. |

## Author-owned crates (check with nabbisen)

| Crate | Locked | Notes |
|---|---|---|
| localcache | 0.21.1 | Current line as of RFC-053 Slice 2 (2026-08-02); no schema migration required — `cache_err`'s blanket `to_string()` mapping absorbed 0.21.0's error-enum changes with no `.rs` edits. |
| app-json-settings | 2.0.3 | Pending `.with_app_name("orbok")` builder consideration (see `settings.rs` note). |

## Dual-version transitive deps (normal, no action)

- `sha2`: 0.10.9 (transitive cryptography chain) + 0.11.0 (orbok direct)
- `thiserror`: 1.0.69 (transitive) + 2.0.18 (orbok direct)
- `zip`: 2.4.2 (orbok direct for DOCX) + 7.2.0 (some transitive dep)
