# orbok Implementation Roadmap

## Current Status (2026-09-01)

Shipped: **v0.24.0**. Latest tagged release: **0.24.0**. RFCs
**000–046 are indexed as implemented** (see
[`rfcs/README.md`](rfcs/README.md)) — **but nine of those entries make a
false claim about the product**; see the Forward Plan below and RFC-063. RFC-049 (portable runtime data
isolation), RFC-050 (trusted atomic model delivery), RFC-051
(reproducible reviewed-source packaging), and RFC-053 (rusqlite line and
Rust MSRV policy) are also now implemented on `main`,
pending the next release tag — see
[`rfcs/README.md`](rfcs/README.md) for their `Status` fields. The
design-system program (RFC-032–035: design tokens, component primitives,
WCAG 2.1 AA accessibility, inclusive design) completed across
v0.12.0–v0.14.0; the stabilization and search-foundation programs landed
across v0.16.0–v0.24.0:

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
- unreleased (`main`) — RFC-050 (trusted atomic model delivery), RFC-049
  (portable runtime data isolation), RFC-051 (reproducible reviewed-source
  packaging), and RFC-053 (rusqlite line and Rust MSRV policy); release
  pending.

Stack: snora 0.39 / iced 0.14, localcache 0.21 + rusqlite 0.39.

## Forward Plan — revised 2026-09-01 after an external architecture audit

An independent architecture audit at `3e26f92` returned 76 findings (6 Critical,
23 High). Its load-bearing claims were verified by execution, not accepted as
written. **Its substantive conclusion is that the blockers this roadmap tracked
were not the ones that mattered most**, and this section is rewritten
accordingly. The full assessment — including where the audit is wrong, imprecise,
or colliding with a project rule — is recorded outside the tree with the rest of
the review record.

### Why the tracked schedule was wrong

Not carelessness, and worth stating precisely because the mechanism is still
live:

**Nine RFCs in `rfcs/done/` carry `Status: Implemented` while making a false
claim about the product.** Four decided to defer and shipped nothing
(RFC-023 ANN, RFC-024 quantization, RFC-025 OCR, RFC-028 plugin extractors).
Three shipped a design whose central mechanism the application never calls
(RFC-010 reranking, RFC-037 source refresh, RFC-038 result trust). Two shipped
with a named gap (RFC-041 filtering, RFC-045 folder scoping).

Because the RFC index is where this roadmap reads what exists, re-scanning,
erasure, snippets and CJK ranking were all invisible from here.

**The cause is in the rules, not in the code.** RFC-000 specifies the
`Implemented` transition in full as *"the implementer or maintainer moves the
file from `proposed/` to `done/` and updates the Status field"*. No evidence is
required and no verifier is named — and it is the only consequential claim in
this project that works that way. Code has six CI jobs and an adversarial review;
a design token has a self-tested gate; a missing translation fails the build; an
RFC becoming Implemented has a file move. `check-rfc-lifecycle.sh` appears to
cover this and does not: it verifies that the Status field matches the folder,
which is internal consistency, not correspondence to the product.

Two things follow. The acceptance criteria decayed into capability phrasing —
*"X exists"*, *"X is exposed to UI"* — because nothing was ever going to run
them; RFC-058 §5 fixes the phrasing and RFC-063 supplies the missing evidence
step that gives it force. And the evidence that *does* exist for recently-closed
RFCs lives outside the tree, so the claim ships in the release archive and its
proof does not.

### Blocking issues for v1.0.0, in order

Replaces the previous list. Each names its instrument.

| # | Blocker | Instrument |
|---|---|---|
| 1 | **Registered folders are never re-scanned.** After the first scan, edited files stay stale, new files are never found, deleted files are never marked missing. A search index that never updates is not a shippable search product. | **Task 035** — the design already exists in full in RFC-037 §10–§20; it needs wiring, not a new RFC |
| 2 | **"Reset catalog" does not erase what was indexed.** The trigram index is never cleared (`DELETE FROM chunk_fts_trigram` occurs zero times in the workspace) and the extraction cache — full document text, no TTL, no LRU — survives every cleanup. | **RFC-059** |
| 3 | **Japanese results are ranked worst-first; PDF/DOCX/HTML snippets read the wrong bytes.** Both visible in the first minute of use, on the corpora orbok targets. | Task 034 (ranking), **RFC-060** (snippets) |
| 4 | **The benchmark cannot see the largest cost in the product.** The harness builds the search service outside its timing loop with a pre-loaded model; the application loads and graph-optimizes the ONNX model on *every search*, on the GUI thread. This fully explains green keyword-only p99 against failing real-model p99. | **RFC-058 §7**, then **RFC-061 §6** |
| 5 | **The DOCX decompression-bomb limit is enforced against an attacker-declared header field.** Measured 880× amplification on untrusted input processed automatically in the background. | Task 034 |
| 6 | **Result trust, filters, folder scope and source status are inert.** A filter UI that does not filter and badges that always read "Ready" are worse than shipping neither. | **RFC-060** |
| 7 | **The README asserts capabilities that do not exist**, two of them privacy claims and one the security-boundary claim. | Task 034 (text), RFC-059/RFC-060 (restore the claims) |

Structurally important and not in the list only because it is not user-visible:
**a released migration was edited in place** (`0001_baseline.sql`, commit
`c54e89d`), so catalogs created by orbok ≤ 0.16.0 reject `status='paused'` and
turning off background indexing silently pauses nothing. **RFC-062.**

### RFC review order

Six RFCs are open for owner review. The order is not arbitrary.

1. **RFC-058 — Verifying the Wired Application.** First, and its end-to-end test
   before any wiring work. It is the control. Wiring six features without it
   fixes six symptoms and leaves the cause.
2. **RFC-063 — Evidence for the Implemented Transition.** Owner decision.
   Everything this roadmap says about "what exists" depends on it, and it is the
   cause the other five are consequences of.
3. **RFC-061 — Catalog Access and the Application Boundary.** Before RFC-060 and
   before any parallelism work: the FTS row leak is driven in a tight loop by a
   dropped `SQLITE_BUSY`, and parallel indexing would make the connection defect
   load-bearing rather than intermittent.
4. **RFC-059 — Erasure Completeness and Cache Lifetime.**
5. **RFC-060 — Search Result Integrity.**
6. **RFC-062 — Migration Integrity and Schema Guards.**

### Sequencing

1. **Task 034 — the no-design batch.** One week, no owner decisions. Closes two
   Criticals, three Highs, and the entire "documentation asserts something
   false" class.
2. **RFC-058's end-to-end test**, with each assertion observed *failing* before
   its fix lands. Not after.
3. **Task 035 — wire RFC-037** (blocker 1), once that test exists to assert it.
4. **RFC-061 §5–§6**, then re-measure with the corrected harness.
5. **RFC-059, RFC-060, RFC-062** as accepted.
6. **RFC-048 measurement resumes only after step 4.** Commissioning more
   measurement from the current harness would produce numbers that exclude the
   dominant cost, as every number this project holds today does.
7. **RFC-047 evidence collection last**, unchanged in position but now behind a
   different and larger set of blockers.

### Where RFC-047, RFC-048 and RFC-052 now stand

RFC-052 is implemented. RFC-049, RFC-050, RFC-051 and RFC-053 shipped on `main`
(release pending). RFC-048 remains accepted and in progress, but **its
measurement instrument is now known to be measuring the wrong region** — see
blocker 4. RFC-047 remains proposed and paused. None marks v1.0.0 ready.

### v1.0.0 gate (unchanged in substance; one criterion re-qualified)

1. recall@5 ≥ 0.75 with a real embedding model on a user corpus. *Last measured
   at 100%, comfortably passing — but measured before the audit found that the
   e5 `query:`/`passage:` prefixes the model card requires are absent from both
   the indexing and the query path, and before the CJK ranking inversion was
   known. Re-measure after both are fixed.*
2. p99 ≤ 200 ms and indexing throughput ≥ 10 files/s in release mode on a
   1,000-document corpus. **Re-qualified:** the existing evidence was produced by
   a harness that excludes per-search model construction. The measurement is not
   invalid, but it attributes the cost to the wrong stage and cannot be used to
   direct optimization. RFC-058 §7 fixes the harness; the gate itself is
   unchanged.
3. Manual QA checklist signed off on Linux, Windows, and macOS. *Linux and
   Windows passes remain outstanding; macOS is automated-coverage-only by an
   owner decision recorded in
   [`docs/src/maintainers/release_readiness.md`](docs/src/maintainers/release_readiness.md).*

v1.0.0 is not released without explicit project-owner confirmation.

### Tracked technical debt — named, not scheduled

Neither of the original two items is a release blocker; both are named so they
are not rediscovered a third time. The 2026-09-01 audit added four more that are
recorded here rather than given an RFC, because their design is not in question.

- **RFC-050 durability guarantees are not continuously verified.** Three
  separate-process helpers (`crash_injection_child`, `proxy_client_child`,
  `lifecycle_interleaving_child`) are `#[ignore]`d, and two Windows volume tests
  require environment fixtures, so none run in CI. RFC-050 is Implemented, so a
  regression in those paths would turn no gate red. Documented in
  [`docs/src/maintainers/release_readiness.md`](docs/src/maintainers/release_readiness.md)
  under "Known Gate Coverage Limitation". Closing it means running the helpers
  in CI and provisioning the two Windows volume fixtures. **The audit sharpened
  this:** these paths contain the workspace's *only* `unsafe` — seven Win32 FFI
  blocks, all individually sound — so the code carrying the memory-safety burden
  is the code with the weakest continuous verification.

- **File-size rule (DEC-004) is broadly unmet.** Now **20** files exceed the
  500-line hard limit (11 production, 9 test), up from the 11 recorded on
  2026-07-18. `model_delivery.rs` (2,176) and `model_lifecycle.rs` (1,724) have
  both grown further; `scheduler_host/tests.rs` (1,651) is new. Split by stable
  responsibility as each subsystem stabilizes rather than as a dedicated
  campaign.

- **The FTS index grows without bound and cannot be reclaimed.** Re-indexing a
  file permanently doubles its FTS footprint, and the Storage view's
  "free space" action reports rows deleted while reclaiming nothing and
  orphaning the rows permanently. Root cause is that `insert_bundle` mints a
  fresh UUID `chunk_id` on every call, so the replace-on-reindex delete keyed on
  `chunk_id` has never matched anything. Fix lands with RFC-059 §6, which shares
  the prerequisite.

- **Storage accounting double-counts and mis-attributes.** The catalog file's
  whole size is reported as `PersistentCatalog` and the keyword and vector
  indexes inside it are then counted again; the keyword estimate is a flat
  `records × 256` that understates real Japanese-corpus usage by ~1.8×; the WAL
  is not counted. `dbstat` gives the real per-table page usage.

- **No integrity or repair path over orbok's own state.** No
  `PRAGMA integrity_check`, no FTS↔catalog consistency check, no rebuild action;
  `JobType::Rebuild` is defined and never enqueued; and a corrupt embedding is
  silently converted to an empty vector and filtered out, removing a document
  from semantic search with no error, no log and no counter. RFC-018 is titled
  *"Crash Recovery, Diagnostics, and Repair Tools"*.

- ~~**MSRV is declared and never measured.**~~ **Closed 2026-09-02 (Task 036).**
  The `msrv` CI job builds the workspace on a pinned `dtolnay/rust-toolchain@1.91`
  on every push, and `cargo-audit` is pinned to `0.22.2 --locked` with the
  failure-masking `|| true` removed. The declared 1.91 floor turned out to be
  correct rather than rotten — RFC-053's `rusqlite 0.39` pin is what kept it
  true, verified by `cargo tree -i libsqlite3-sys` → `0.37.0`, not the `0.38.x`
  line that carries an unmeasured 1.95 floor. The remaining half — the four
  `@stable` gate jobs, whose resolution at run time can turn an unchanged commit
  red — is dev-team Task 039.

### Considered and declined

- **`#[non_exhaustive]` on public types before 1.0** (external audit E-01).
  Declined 2026-09-01. All eleven library crates are published at 0.24.0, and
  every reverse dependency of every one of them on crates.io is another orbok
  crate — there are no outside consumers to protect from a variant addition. The
  build and release workflows are settled. Revisit only if the library crates
  are promoted as independently reusable, which the FAQ currently half-suggests
  for `orbok-workers`.

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
- **DB pin**: localcache 0.21 + rusqlite 0.39 — one libsqlite3-sys (RFC-002 §16, superseded by RFC-053).
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
