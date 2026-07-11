# Dependency Audit

## 2026-07-11 cargo-deny deferral

`cargo-deny` remains advisory for the active post-v0.23/v1.0 readiness track.
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
| RUSTSEC-2025-0141 | `bincode` 2.0.1 | Pulled through `localcache` 0.20.0; advisory is unmaintained status and the cache engine is pinned. |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | Transitive proc-macro helper in GUI/model-support paths; no direct orbok usage. |
| RUSTSEC-2026-0173 | `proc-macro-error2` 2.0.1 | Retained in `Cargo.lock` through a stale `defmt-macros` branch; not present in the active all-target dependency tree. |
| RUSTSEC-2026-0192 | `ttf-parser` 0.25.1 | Pulled by GUI/font and PDF stacks; replacement requires upstream dependency movement. |
| RUSTSEC-2026-0190 | `anyhow` 1.0.102 | Pulled by tract/prost real embedding paths; orbok does not directly call `anyhow::Error::downcast_mut`. |
| RUSTSEC-2026-0186 | `memmap2` 0.9.10 | Pulled by GUI/windowing/font stacks and `tract-onnx`; replacement requires upstream dependency movement. |
| RUSTSEC-2026-0194 | `quick-xml` 0.39.4 | Pulled through `wayland-scanner` 0.31.10 in the Linux GUI stack; `wayland-scanner` still requires `quick-xml ^0.39`. |
| RUSTSEC-2026-0195 | `quick-xml` 0.39.4 | Same `wayland-scanner` path as RUSTSEC-2026-0194. |

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
| tokio | 1.52.3 (orbok-app) | Async runtime for download |
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
| localcache | 0.20.0 | Is a newer release available? If so, note any schema migration required. |
| app-json-settings | 2.0.3 | Pending `.with_app_name("orbok")` builder consideration (see `settings.rs` note). |

## Dual-version transitive deps (normal, no action)

- `sha2`: 0.10.9 (transitive cryptography chain) + 0.11.0 (orbok direct)
- `thiserror`: 1.0.69 (transitive) + 2.0.18 (orbok direct)
- `zip`: 2.4.2 (orbok direct for DOCX) + 7.2.0 (some transitive dep)
