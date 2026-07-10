# Dependency Audit

## 2026-07-10 security baseline

`cargo audit` is now configured as the supply-chain baseline. The repository
keeps the waiver list in `.cargo/audit.toml`; unwaived vulnerabilities should
fail CI.

Fixes applied:

- `lopdf`: 0.41.0 → 0.42.0 for RUSTSEC-2026-0187.
- `crossbeam-epoch`: 0.9.18 → 0.9.20 for RUSTSEC-2026-0204.
- `quinn-proto`: 0.11.14 → 0.11.16 for RUSTSEC-2026-0185.

Temporary waivers:

- RUSTSEC-2026-0194 and RUSTSEC-2026-0195 for `quick-xml` 0.39.4.
  This crate is pulled through `wayland-scanner` 0.31.10, a proc-macro
  dependency in the Linux GUI stack. `wayland-scanner` 0.31.10 is current and
  still requires `quick-xml ^0.39`, so this cannot be updated independently.

Observed remaining audit warnings are informational (`unmaintained` /
`unsound`) and are not denied by the current baseline.

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
