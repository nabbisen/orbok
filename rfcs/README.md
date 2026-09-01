# orbok RFC Index

Managed by RFC-000. Last updated: 2026-09-01 (main at `3e26f92`).

The folder an RFC lives in is the source of truth for its state
(`done/` = Implemented, `proposed/` = under review, `accepted/` = review
complete and the implementer may start, `archive/` = Withdrawn/Superseded).
Each RFC's `Status` field mirrors its folder.

This project uses RFC-000's [5-folder variant](done/000-rfc-lifecycle-policy.md#folder-layout-5-folder-variant)
(adopted 2026-08-04 — see that RFC's adoption note for why) because every RFC
accepted for implementation here (049, 050, 051, 052, 053) shipped while
still sitting in `proposed/`, with authorization coming from task directives
instead of the folder. `accepted/` makes "the owner signed off, implementer
may start" a checked state rather than an implicit convention.

## Implemented

| ID | Title | Release |
|---|---|---|
| 000 | [RFC lifecycle policy](done/000-rfc-lifecycle-policy.md) | v0.6.0 |
| 001 | [Local Data Classification and Lifecycle](done/001-local-data-classification-and-lifecycle.md) | v0.1.0 |
| 002 | [SQLite Catalog Schema and Migration Policy](done/002-sqlite-catalog-schema-and-migration-policy.md) | v0.1.0 |
| 003 | [Source Registration and File Access Boundary](done/003-source-registration-and-file-access-boundary.md) | v0.1.0 |
| 004 | [File Scanner and Change Detection](done/004-file-scanner-and-change-detection.md) | v0.1.0 |
| 005 | [Document Extraction Pipeline](done/005-document-extraction-pipeline.md) | v0.1.0 |
| 006 | [Adaptive Chunking and Location Metadata](done/006-adaptive-chunking-and-location-metadata.md) | v0.2.0 |
| 007 | [Keyword Search Engine Selection](done/007-keyword-search-engine-selection.md) | v0.2.0 |
| 008 | [Embedding Model and Vector Storage](done/008-embedding-model-and-vector-storage.md) | v0.3.0 |
| 009 | [Hybrid Search and RRF Fusion](done/009-hybrid-search-and-rrf-fusion.md) | v0.3.0 |
| 010 | [Optional Local Reranking](done/010-optional-local-reranking.md) | v0.4.0 |
| 011 | [Storage Dashboard and Cleanup UX](done/011-storage-dashboard-and-cleanup-ux.md) | v0.4.0 |
| 012 | [Model Registry and Installation Workflow](done/012-model-registry-and-installation-workflow.md) | v0.5.0 |
| 013 | [Search View and Result Explanation UX](done/013-search-view-and-result-explanation-ux.md) | v0.4.0 |
| 014 | [Japanese and Mixed-Language Search Strategy](done/014-japanese-and-mixed-language-search-strategy.md) | v0.4.0 |
| 015 | [Security Hardening for Local Files and Local API](done/015-security-hardening-for-local-files-and-local-api.md) | v0.5.0 |
| 016 | [Benchmark and Retrieval Evaluation Plan](done/016-benchmark-and-retrieval-evaluation-plan.md) | v0.5.0 |
| 017 | [Packaging and Distribution Strategy](done/017-packaging-and-distribution-strategy.md) | v0.5.0 |
| 018 | [Crash Recovery, Diagnostics, and Repair Tools](done/018-crash-recovery-diagnostics-and-repair-tools.md) | v0.5.0 |
| 019 | [Test Matrix and Release Readiness](done/019-test-matrix-and-release-readiness.md) | v0.6.0 |
| 020 | [Documentation and User Guidance Structure](done/020-documentation-and-user-guidance-structure.md) | v0.6.0 |
| 021 | [Default Embedding Model Selection](done/021-default-embedding-model-selection.md) | v0.7.0 |
| 022 | [PDF Extraction Backend Selection](done/022-pdf-extraction-backend-selection.md) | v0.7.0 |
| 023 | [Vector ANN Indexing](done/023-vector-ann-indexing.md) | v0.8.0 |
| 024 | [Vector Quantization](done/024-vector-quantization.md) | v0.8.0 |
| 025 | [OCR Pipeline](done/025-ocr-pipeline.md) | v0.8.0 |
| 027 | [GUI Framework Finalization](done/027-gui-framework-finalization.md) | v0.1.0 |
| 028 | [Plugin Extractor Architecture](done/028-plugin-extractor-architecture.md) | v0.8.0 |
| 029 | [Model Download Integrity and Trust Policy](done/029-model-download-integrity-and-trust-policy.md) | v0.7.0 |
| 030 | [Portable Mode](done/030-portable-mode.md) | v0.8.0 |
| 031 | [GUI Internationalization (i18n)](done/031-gui-internationalization.md) | v0.1.0 |
| 032 | [Design Token Foundation and Theming](done/032-design-token-foundation-and-theming.md) | v0.12.0 |
| 033 | [Component Primitive Migration](done/033-component-primitive-migration.md) | v0.12.0 |
| 034 | [Accessibility Conformance (WCAG 2.1 AA)](done/034-accessibility-conformance.md) | v0.13.0 |
| 035 | [Inclusive Design](done/035-inclusive-design.md) | v0.14.0 |
| 036 | [Resource-Aware Indexing Scheduler and Backpressure](done/036-resource-aware-indexing-scheduler-and-backpressure.md) | v0.17.0 |
| 037 | [Source Lifecycle, Refresh Policy, and Change Detection UX](done/037-source-lifecycle-refresh-policy-and-change-detection-ux.md) | v0.18.0 |
| 038 | [Result Freshness, Trust Badges, and Recovery Actions](done/038-result-freshness-trust-badges-and-recovery-actions.md) | v0.18.0 |
| 039 | [Privacy Modes and Local Data Visibility](done/039-privacy-modes-and-local-data-visibility.md) | v0.19.0 |
| 040 | [Safe Diagnostics and Redacted Support Bundle](done/040-safe-diagnostics-and-redacted-support-bundle.md) | v0.19.0 |
| 041 | [Search, Narrow Results, and Browse Around](done/041-search-narrow-and-browse-around.md) | v0.18.0 |
| 043 | [Model Download Readiness and Bounded Concurrency](done/043-model-download-readiness-and-concurrency.md) | v0.19.0 |
| 044 | [orbok-extract Production Hardening](done/044-orbok-extract-production-hardening.md) | v0.16.0 |
| 045 | [Search-in-Folder Flow and Friendly Folder Management](done/045-search-in-folder-flow-and-friendly-folder-management.md) | v0.20.0 |
| 042 | [Search History and Reopen Recent Searches](done/042-search-history-and-reopen.md) | v0.21.0 |
| 046 | [Declared Candle Embedding Backend — Status and Options](done/046-candle-embedding-backend-status.md) | v0.22.0 |
| 049 | [Portable Runtime Data Isolation](done/049-portable-runtime-data-isolation.md) | main at `2fba6a9`; release pending |
| 050 | [Trusted Atomic Model Delivery](done/050-trusted-atomic-model-delivery.md) | main at `902f33a`; release pending |
| 053 | [rusqlite Line and Rust MSRV Policy](done/053-rusqlite-line-and-msrv-policy.md) | main at `6342cde`; release pending |
| 051 | [Reproducible Reviewed-Source Packaging](done/051-reproducible-reviewed-source-packaging.md) | main at `56d63a6`; release pending |
| 054 | [Runtime Data Override Profile Scope](done/054-runtime-data-override-profile-scope.md) | main at `6bcedd9`; release pending |
| 055 | [Fail-Closed Settings Path Resolution](done/055-settings-path-fail-closed.md) | main at `9da7a4c`; release pending |
| 052 | [UI Localization and Design-Gate Compliance](done/052-ui-localization-and-design-gate-compliance.md) | main at `2204caa`; release pending |
| 056 | [Hosting the Indexing Scheduler in the Application](done/056-hosting-the-indexing-scheduler.md) | main at `190a5a7`; release pending |
| 057 | [Live Resource Signals for the Indexing Scheduler](done/057-live-resource-signals.md) | main at `1d2b234`; release pending — §7 manual battery criterion deferred to Owner Task 003 Part C |

## Accepted

| ID | Title | Status |
|---|---|---|
| 048 | [Real-Model Benchmark Performance Recovery](accepted/048-real-model-performance-recovery.md) | Accepted — measurement-first recovery sequence in progress (Owner Task 003) |

## Proposed

| ID | Title | Status |
|---|---|---|
| 047 | [v1.0.0 RC Evidence Collection and Review](proposed/047-v1-rc-evidence-collection.md) | Proposed |
| 058 | [Verifying the Wired Application](proposed/058-verifying-the-wired-application.md) | Proposed — review first; the control that stops the unwired-feature class recurring |
| 059 | [Erasure Completeness and Cache Lifetime](proposed/059-erasure-completeness-and-cache-lifetime.md) | Proposed — Reset does not erase the trigram index or the extraction cache |
| 060 | [Search Result Integrity](proposed/060-search-result-integrity.md) | Proposed — snippets, trust, filters, folder scope, source status, reranker decision |
| 061 | [Catalog Access and the Application Boundary](proposed/061-catalog-access-and-application-boundary.md) | Proposed — one shared catalog, one model per process, failures surfaced |
| 062 | [Migration Integrity and Schema Guards](proposed/062-migration-integrity-and-schema-guards.md) | Proposed — a released migration was edited; no downgrade guard |
| 063 | [Evidence for the Implemented Transition](proposed/063-evidence-for-the-implemented-transition.md) | Proposed — **owner decision**; `done/` is entered by assertion, not evidence, which is why nine entries make a false claim |

**2026-09-01 — external audit.** An independent architecture audit
(76 findings: 6 Critical, 23 High) was reviewed and its load-bearing claims
verified by execution. RFC-058 through RFC-063 are its design output. Two
things the index itself must carry:

1. **Nine RFCs in `done/` currently make a false claim about the product** —
   RFC-023/024/025/028 (nothing shipped), RFC-010/037/038 (designed, central
   mechanism never connected), and RFC-041/045 (shipped with a named gap).
   RFC-063 traces this to its cause — the `Implemented` transition requires no
   evidence and names no verifier — and puts the fix to the owner. Until it is
   decided, **this index overstates the shipped feature set** and should be read
   alongside RFC-063 §7.
2. **RFC-037 already contains the full design for the audit's single blocking
   issue** (registered folders are never re-scanned). It does not need a new RFC;
   it needs wiring. Routed as a dev-team task, not as RFC-058–063.

Developer handoffs live in [`handoffs/`](handoffs/). The v0.23.0 resolved
finding note for the separate `--features tract` recovery is in
[`appendices/FINDING-tract-feature-build.md`](appendices/FINDING-tract-feature-build.md).
The v0.24.0 readiness trail is represented by RFC-047 and RFC-048. The
architecture preparation review opened RFC-049 through RFC-052 for the
portable-data, model-delivery, release-provenance, and UI-compliance blockers
that must be resolved before release-candidate promotion. RFC-049, RFC-050,
and RFC-051 are now implemented on `main`; RFC-050's normative model trust
data and provenance evidence are recorded in
[`APPENDIX-B-default-model-trust-root.md`](appendices/APPENDIX-B-default-model-trust-root.md).
RFC-052 is implemented: the dev-team work landed and CI-green, and the project
owner completed §9's manual Japanese QA on 2026-08-08, accepting all 21 Phase 2
strings without amendment. RFC-053 (rusqlite line and Rust MSRV policy) is a separate
dependency-maintenance track, not part of the architecture-review blocker
set; it is also now implemented on `main`. RFC-048's real-model measurement
work is accepted and in progress (Owner Task 003); RFC-047 remains paused
behind the other blockers and has not been accepted. RFC-054 is implemented on `main`; it narrowed a cross-platform gap found in RFC-049 after
it shipped: `ORBOK_DATA_DIR` relocates a profile's data but not its settings,
and the workaround for that exists only on Linux. RFC-056 is accepted and awaiting implementation; it hosts
RFC-036's scheduler in the application: the policy engine and the background
execution pattern both already exist, and indexing has never been connected to
either — see RFC-008 §27 and RFC-009 §24 for how that went unnoticed. RFC-055 is implemented on `main`; it is the
`app-json-settings` dependency track — it makes settings-path resolution
fail-closed and decides what portable mode does where no platform configuration
directory exists.

## Archive

| ID | Title | Reason |
|---|---|---|
| 026 | [Encrypted Local Indexes](archive/026-encrypted-local-indexes.md) | Withdrawn — key-management design needs a dedicated security audit; deferred to post-v1.0.0. |
