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

## Upstream requests

- `snora-request-contrast-facade-v0.25.1.md` — request that produced the
  `snora::design::contrast` facade used by RFC-034 (delivered in snora 0.25.1).
