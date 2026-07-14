# orbok Implementation Roadmap

## Current Status (2026-07-14)

Shipped: **v0.24.0**. Latest tagged release: **0.24.0**. RFCs
**000–046 implemented** (see
[`rfcs/README.md`](rfcs/README.md)). The design-system program (RFC-032–035:
design tokens, component primitives, WCAG 2.1 AA accessibility, inclusive
design) completed across v0.12.0–v0.14.0; the stabilization and
search-foundation programs landed across v0.16.0–v0.24.0:

- v0.16.0 — RFC-044 (orbok-extract production hardening).
- v0.17.0 — RFC-036 (resource-aware scheduler and backpressure).
- v0.18.0 — RFC-041 (search / narrow / browse), RFC-037 (source lifecycle), RFC-038 (result trust).
- v0.19.0 — RFC-043 (model download readiness), RFC-039 (privacy modes), RFC-040 (safe diagnostics).
- v0.20.0 — RFC-045 (search-in-folder flow and friendly folder management).
- v0.21.0 — RFC-042 (search history and reopen recent searches).
- v0.22.0 — RFC-046 (Candle backend cleanup, Option B1).
- v0.23.0 — release gate stabilization, real `tract` ONNX inference, and
  keyword-only benchmark p99 stabilization.
- v0.24.0 — v1.0.0 evidence workflow, CI/release-gate alignment, real-model
  benchmark guards, model evidence, and timing diagnostics.

Stack: snora 0.25 / iced 0.14, localcache 0.20.0 + rusqlite 0.40.

## Forward Plan — v1.0.0 readiness and stabilization RFCs in progress

Every RFC through 046 has shipped. RFC-047 through RFC-052 are proposed
v1.0.0 readiness/stabilization RFCs. RFC-047 defines evidence collection and
RFC-048 tracks real-model performance recovery. RFC-049 through RFC-052 address
the architecture review's release blockers: portable runtime isolation,
trusted atomic model delivery, reviewed-source packaging, and complete UI
localization/design-gate enforcement. None marks v1.0.0 ready.

The `tract` feature build finding is resolved: `cargo check -p orbok-embed
--features tract` is now a blocking release gate, and `orbok-embed` contains
real tokenizer-backed local ONNX inference. Empirical validation with a local
`multilingual-e5-small` artifact has identified real-model p99 and indexing
throughput failures. RFC-048 tracks the measurement-first recovery path.
Future work (new features, stabilization, or the v1.0.0 push) will be opened
as new RFCs in creation order (RFC-000).

### Stabilization order before RC evidence collection

1. Review and accept RFC-049 through RFC-052 and their handoffs.
2. Implement and independently review the four stabilization boundaries.
3. Continue RFC-048 measured performance recovery until real-model thresholds
   pass or a later accepted RFC changes policy.
4. Resume RFC-047 evidence collection only after those blockers are closed.

### v1.0.0 gate (unchanged — awaiting owner confirmation)

1. recall@5 ≥ 0.75 with a real embedding model on a user corpus.
2. p99 ≤ 200 ms and indexing throughput ≥ 10 files/s in release mode on a
   1,000-document corpus. Current keyword-only evidence is green; real-model
   artifact validation is blocked on RFC-048 performance recovery.
3. Manual QA checklist signed off on Linux, Windows, and macOS.

v1.0.0 is not released without explicit project-owner confirmation.

### Future process hardening candidates

- Reusable owner-run evidence checklist template: extract the pattern from the
  RFC-048 timing evidence checklist so future owner-run benchmarks, manual QA,
  and release evidence requests are recorded in project files instead of only
  in chat. Open a dedicated RFC only if the template changes release policy or
  adds new gates.

---

> The sections below are historical milestone tracking (v0.1–v0.9 RC), retained
> as a record. Current planning lives in the two sections above and in
> [`rfcs/README.md`](rfcs/README.md).

## Milestone Status

| M | Name | v0.1 | v0.2 |
|---|---|:---:|:---:|
| M0 | Project Skeleton and Architecture Boundaries | ✓ | |
| M1 | Local Data Lifecycle and SQLite Catalog | ✓ | |
| M2 | Source Registration and Safe File Access | ✓ | |
| M3 | File Scanner and Change Detection | ✓ | |
| M4 | Document Extraction Pipeline | ✓ | |
| M5 | Adaptive Chunking and Location Metadata | | ✓ |
| M6 | Keyword Search MVP | Proto | ✓ |
| M7 | Embedding and Vector Search MVP | | |
| M8 | Hybrid Search and RRF | | |
| M9 | Search UI MVP | Shell | Partial |
| M10 | Storage Dashboard and Cleanup | Partial | |
| M11 | Optional Reranking | | |
| M12 | Model Registry and Installation UX | Types | |
| M13 | Hardening, Benchmarks, and Packaging | | |

## Next (v0.3 targets)

### M7 — Embeddings and Vector Search

- `EmbeddingModel` trait + mock implementation (deterministic, test-safe).
- `EmbeddingWorker` in `orbok-workers`: reads chunk text from extraction
  cache, generates embeddings, stores them in the `embeddings` table.
- Exact cosine-similarity scan (no ANN in v0.3; dataset sizes are small).
- Vector storage as `sqlite_blob` in the catalog embeddings table.
- Model version tracking: changing the embedding model marks existing
  embeddings stale.
- **RFC-008** implementation target.

### M8 — Hybrid Search and RRF

- `HybridSearchService` merging keyword and vector candidates.
- Reciprocal Rank Fusion (k=60, configurable) — RFC-009.
- Candidate deduplication by chunk or parent context.
- Result explanation badges: Keyword / Semantic / Fused.
- Search mode selector in `orbok-ui` (Auto / Exact / Conceptual).

### M9 (complete) — Search UI

- Result preview panel with "why this result" explanation.
- Stale/missing source badges on result cards.
- Filter drawer (source, file type, date range).
- Open file / open folder actions wired to `orbok-app`.
- **RFC-013** implementation target.

### Other v0.3 candidates

- Persist locale preference to catalog settings on change.
- Source health banner in the UI (stale/missing file counts).
- Scan-on-startup option (configurable via settings).
- Storage accounting populated after index runs.
- RFC-014 scoping: evaluate unicode61 trigram vs Tantivy for Japanese.

## Design decisions (settled)

- **GUI**: iced 0.14 via snora 0.8 — no WebView, no local HTTP API (RFC-027).
- **i18n**: compile-time typed catalog, En+Ja (RFC-031).
- **DB pin**: localcache 0.20.0 + rusqlite 0.40 — one libsqlite3-sys (RFC-002 §16).
- **FTS**: SQLite FTS5 contentless + `keyword_index_records.fts_rowid` mapping (RFC-007).
- **Chunking**: structure-aware (Markdown headings) + paragraph fallback (RFC-006).
- **Pipeline**: extract → chunk+index in two worker steps, atomic per-file transactions (RFC-006 §12).

## v0.4 status

| RFC | Title | v0.4 |
|---|---|:---:|
| RFC-010 | Optional Local Reranking | ✓ |
| RFC-011 | Storage Dashboard and Cleanup UX | ✓ |
| RFC-013 | Search View and Result Explanation UX | ✓ |
| RFC-014 | Japanese and Mixed-Language Search | ✓ |

## v0.5 targets

- **RFC-012**: Model Registry and Installation Workflow — full model registry UI, locate/install/validate model files, reindex-on-change flow.
- **RFC-015**: Security Hardening — CSRF protection for local API (when applicable), path-traversal audit, HTML render sanitization hardening.
- **RFC-016**: Benchmarks and Retrieval Evaluation — search quality test corpus, indexing throughput, memory profiling.
- **RFC-017**: Packaging and Distribution — cross-platform release binaries, Debian/RPM packages, macOS .app bundle, Windows installer.
- **M9 complete**: Two-pane preview panel with full explanation (RFC-013 follow-through), file-open OS integration in orbok-app.
- **M10 complete**: Storage dashboard cleanup actions wired end-to-end (CleanupService combining catalog + cache).

## v0.5 status

| RFC | Title | v0.5 |
|---|---|:---:|
| RFC-012 | Model Registry and Installation Workflow | ✓ |
| RFC-015 | Security Hardening | ✓ |
| RFC-016 | Benchmark and Retrieval Evaluation | ✓ |
| RFC-017 | Packaging and Distribution | ✓ |
| RFC-018 | Crash Recovery and Diagnostics | ✓ |

## v0.6 targets (historical)

- **RFC-019**: Test Matrix and Release Readiness — cross-platform CI definition, integration test scenarios, release gate criteria.
- **RFC-020**: Documentation and User Guidance — complete mdbook docs, API reference, tutorial content for new/intermediate/maintainer paths.
- **RFC-019/020 complete**: these are the final RFCs in Part 4 (operational).
- **M10 complete**: Storage cleanup actions fully wired — CleanupService combining catalog + cache, one-click cleanup triggering both.
- **M12 complete**: Real embedding model loading via candle/ONNX backend (replaces MockEmbeddingModel in production paths).
- **Remaining Part 5 RFCs** (021–030): at this point in the historical plan,
  these were deferred future work. Current RFC state is tracked in
  [`rfcs/README.md`](rfcs/README.md).

## v0.6 status — All Part 1–4 RFCs complete ✓

| RFC | Title | Status |
|---|---|:---:|
| RFC-019 | Test Matrix and Release Readiness | ✓ v0.6 |
| RFC-020 | Documentation and User Guidance | ✓ v0.6 |
| M10 | Storage Cleanup (CleanupService end-to-end) | ✓ v0.6 |
| M12 | Backend Config (EmbeddingModelConfig, RerankerConfig) | ✓ v0.6 |

## v0.7+ — Part 5 Deferred Future Work (historical)

At this point in the historical plan, these RFCs were tracked as deferred
future work. Current RFC state is tracked in [`rfcs/README.md`](rfcs/README.md).

| RFC | Title | Priority |
|---|---|---|
| RFC-021 | Default Embedding Model Selection | High |
| RFC-022 | PDF Extraction Backend | High |
| RFC-023 | Vector ANN Indexing | Medium |
| RFC-024 | Vector Quantization | Medium |
| RFC-025 | OCR Pipeline | Low |
| RFC-026 | Encrypted Local Indexes | Low |
| RFC-028 | Plugin Extractor Architecture | Low |
| RFC-029 | Model Download Integrity and Trust | Medium |
| RFC-030 | Portable Mode | Low |

## v1.0.0 readiness (historical criteria)

At this point in the historical plan, orbok was expected to reach v1.0.0 when:
1. RFC-021 (real embedding model) and RFC-022 (PDF backend) are implemented.
2. Benchmarks meet RFC-019 targets: recall@5 ≥ 0.75, p99 ≤ 200 ms.
3. All three platforms (Linux/Windows/macOS) pass the manual QA checklist.
4. Release level RL-4 is achieved.

## v0.7 status

| Item | Status |
|---|:---:|
| RFC-021 Default Embedding Model (multilingual-e5-small) | ✓ |
| RFC-022 PDF Extraction (lopdf) | ✓ |
| RFC-029 Model Integrity + Trust | ✓ |
| orbok-embed crate (feature-flagged backends) | ✓ |

## v0.8 targets (historical path to v1.0.0)

**Remaining draft RFCs at this point in the historical plan:**
- RFC-023: Vector ANN Indexing (HNSW for > 100k chunks)
- RFC-024: Vector Quantization (INT8 / binary)
- RFC-025: OCR Pipeline (image PDFs, screenshots)
- RFC-026: Encrypted Local Indexes
- RFC-028: Plugin Extractor Architecture
- RFC-030: Portable Mode (single-dir deployment)

**v1.0.0 gate (3 conditions — awaiting confirmation):**
1. recall@5 ≥ 0.75 on labeled query set with real model
2. p99 search latency ≤ 200 ms on 1,000-doc corpus
   (green for keyword-only release-mode benchmark; real-model run pending)
3. Manual QA checklist signed off on Linux + Windows + macOS

> v1.0.0 will not be released without explicit project owner confirmation.

## v0.8 status — All RFCs resolved ✓

| RFC | Title | Status |
|---|---|:---:|
| RFC-023 | ANN Indexing | Decision: exact scan ✓ |
| RFC-024 | Vector Quantization | INT8 implemented ✓ |
| RFC-025 | OCR Pipeline | Detection only ✓ |
| RFC-026 | Encrypted Indexes | Archived (post-v1.0) |
| RFC-028 | Plugin Architecture | Interface defined ✓ |
| RFC-030 | Portable Mode | --portable flag ✓ |

## v1.0.0 — Awaiting confirmation

Three conditions must be verified before v1.0.0 is released:

1. **recall@5 ≥ 0.75** with a real embedding model on a user corpus
   (currently 87.5% with keyword-only on the 1,000-document synthetic release
   corpus ✓)
2. **p99 ≤ 200 ms** in release mode on a 1,000-document corpus
   (currently 149.79 ms in release mode on the 1,000-document keyword-only
   synthetic release corpus ✓)
3. **Manual QA checklist** signed off on Linux, Windows, and macOS

**v1.0.0 requires explicit project owner confirmation.**

### Post-v1.0.0 backlog

- RFC-026 revisited: encrypted local indexes (key management design)
- RFC-023 revisited: HNSW ANN (when user corpora show > 200 ms)
- XLSX, PPTX extraction (new RFC)
- Plugin dynamic loading (RFC-028 full activation)
- Mobile/browser companion (new RFC)

## v0.9.0 RC status

| Item | Status |
|---|:---:|
| DOCX extractor (ZIP+XML) | ✓ |
| HTML extractor (tag stripper) | ✓ |
| End-to-end pipeline integration test | ✓ |
| Pre-release gate tests | ✓ |
| Zero compiler warnings | ✓ |
| 169 tests / 0 failures | ✓ |

## v1.0.0 checklist (awaiting owner confirmation)

- [ ] Real embedding model installed and validated
- [ ] Benchmark with real model: recall@5 ≥ 0.75
- [x] Release build p99 ≤ 200 ms on 1,000-document keyword-only corpus
- [ ] Benchmark with real model artifact on release hardware
- [ ] Manual QA signed off: Linux
- [ ] Manual QA signed off: macOS
- [ ] Manual QA signed off: Windows
- [ ] CHANGELOG finalized
- [ ] **Explicit owner confirmation received**
