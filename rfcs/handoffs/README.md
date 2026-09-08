# Developer Handoffs

This directory holds **developer handoffs**: implementation-ready companions to
the RFCs in `rfcs/`. The RFC answers *what and why* (requirement / external
design); the handoff answers *how* (internal / program design), so an
implementer can go straight to coding per the project workflow:

> Requirement (RFC) → External Design → **Internal/Program Design (handoff)** →
> Implementation → Testing

## Convention

- One handoff per RFC, named `HANDOFF-0NN-<slug>.md`. Larger RFCs may add
  companion files (e.g. a task/PR plan, a QA checklist, or a separate external
  design) sharing the same `0NN` number.
- Each handoff is self-contained: exact crates/files touched, function
  signatures, an ordered task list, the test plan, and a definition-of-done
  checklist.
- Handoffs assume the **release-discipline rule**: all work lands in the current
  release version; no version number is created without explicit instruction.
- Handoffs respect the **boundary rules**: `orbok-ui` does no filesystem or
  database access (RFC-027); platform I/O (OS theme / locale / reduce-motion
  probing, settings persistence, folder picker, downloads) lives in `orbok-app`.
- Every change keeps the build **warning-free** (including `--tests`) and the
  full suite green before the step is considered done.

## Design-system program (RFC-032 → 035) — implemented

| RFC | Handoff | Theme |
|-----|---------|-------|
| 032 | HANDOFF-032 | Design token foundation + theming (substrate) |
| 033 | HANDOFF-033 | Component primitive migration (snora as primitive gateway) |
| 034 | HANDOFF-034 | Accessibility conformance (WCAG 2.1 AA) |
| 035 | HANDOFF-035 | Inclusive design (text scale, reduced motion, CVD-safe, i18n formatting) |

Shipped across v0.12.0–v0.14.0; the RFCs now live in `rfcs/done/`.

## Stabilization program (RFC-036 → 040) — implemented

| RFC | Handoff | Theme |
|-----|---------|-------|
| 036 | HANDOFF-036 | Resource-aware indexing scheduler and backpressure |
| 037 | HANDOFF-037 | Source lifecycle, refresh policy, change-detection UX |
| 038 | HANDOFF-038 | Result freshness, trust badges, recovery actions |
| 039 | HANDOFF-039 | Privacy modes and local data visibility |
| 040 | HANDOFF-040 | Safe diagnostics and redacted support bundle |

Shipped across v0.17.0–v0.19.0; the RFCs now live in `rfcs/done/`.

## Foundation & Search-UX program (RFC-041 → 045) — implemented

| RFC | Handoff | Theme |
|-----|---------|-------|
| 041 | HANDOFF-041 | Search, narrow results, and browse around |
| 042 | HANDOFF-042 (+ `RFC-042-search-history-external-design.md`) | Search history and reopen recent searches |
| 043 | HANDOFF-043 | Model download readiness and bounded concurrency |
| 044 | HANDOFF-044 | orbok-extract production hardening and boundary cleanup |
| 045 | HANDOFF-045-implementation, -task-breakdown-pr-plan, -acceptance-qa-checklist | Search-in-folder flow and friendly folder management |
| 046 | HANDOFF-046-candle-backend-removal | Candle backend cleanup (RFC-046, Option B1) |

RFC-044 shipped in v0.16.0, RFC-041 in v0.18.0, RFC-043 in v0.19.0,
RFC-045 in v0.20.0, RFC-042 in v0.21.0, and RFC-046 in v0.22.0 — all now in
`rfcs/done/`. The whole program is complete.

**Numbering vs. dependency order:** the 041–045 foundation RFCs were authored
before the 036–040 stabilization RFCs but received later numbers (032–035 were
already taken by the design-system program). Dependencies flow from cross-
references, not numeric order — 036–040 reference 041–045, which is expected
under RFC-000.

## v1.0.0 readiness and stabilization program (RFC-047 → 052) — proposed

| RFC | Handoff | Theme |
|-----|---------|-------|
| 047 | HANDOFF-047-v1-rc-evidence-collection | v1.0.0 RC evidence collection and review |
| 048 | HANDOFF-048-real-model-performance-recovery | real-model benchmark performance recovery |
| 049 | HANDOFF-049-portable-runtime-data-isolation | one runtime data context and standard/portable isolation |
| 050 | HANDOFF-050-trusted-atomic-model-delivery + Appendix B | trusted manifest, generation transaction, and crash recovery |
| 051 | HANDOFF-051-reproducible-reviewed-source-packaging | reviewed tracked inputs, lockfile, and deterministic archives |
| 052 | HANDOFF-052-ui-localization-and-design-gate-compliance | complete En/Ja UI copy and mandatory token/i18n gates |
| 053 | HANDOFF-053-rusqlite-line-and-msrv | rusqlite 0.39 line, measured MSRV, and the localcache upgrade it unblocks |
| 054 | HANDOFF-054-runtime-data-override-profile-scope | `ORBOK_DATA_DIR` profile scope, and the settings path it does not relocate |
| 055 | HANDOFF-055-settings-path-fail-closed | fail-closed settings-path resolution, and portable mode with no platform config dir |
| 056 | HANDOFF-056-hosting-the-indexing-scheduler | hosting RFC-036's scheduler in the application, which had never been connected to it |
| 057 | HANDOFF-057-live-resource-signals | user-activity and battery signal sources feeding RFC-036's existing policy |
| 058 | HANDOFF-058-verifying-the-wired-application | the end-to-end reachability test, the benchmark's timing boundary, and the release gate |
| 060 | HANDOFF-060-slice1-pdf-extraction-and-location-quality | Slice 1 only: PDF extraction by page number, and the chunk quality the chunker never reads |
| 061 | HANDOFF-061-catalog-access-and-application-boundary | one shared catalog, one model per process, failures surfaced, and the censored latency instrument |

**Index corrected 2026-09-07.** Handoffs 054–057 existed on disk and were
missing from the table above; 058 is new. The closing note below was also stale
— it described RFC-049–052 as unresolved blockers, and all four have been
implemented since.

RFC-047 evidence collection remains paused, now behind a different and larger
set of blockers than the one it was paused behind: RFC-058 through RFC-063,
opened by the 2026-09-01 external architecture audit. **RFC-048's performance
gate should not be re-measured until RFC-058 §7 lands** — the benchmark
constructs its search service outside its own timing loop, so every number the
project holds excludes the cost the application pays on every search.
