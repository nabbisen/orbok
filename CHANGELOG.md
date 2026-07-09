# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.21.0] — 2026-06-21 — Search History and Reopen Recent Searches (RFC-042)

Local recent searches with "Search again" — no automatic result tabs.
Reopened searches restore the search words (and stored narrowing-choice
labels), then run again against current files. History is local-only,
clearable, and disabled by Strict privacy.

### Added

**Core types (`orbok-core::history`)**

- `SearchHistoryEntry` (id, search_text, filters, created_at, last_used_at,
  previous_result_count, locale) — stores *instructions*, never results
  (RFC-042 §7.1, §8.2).
- `StoredSearchFilter` — self-contained mirror of `ActiveFilter` so
  `orbok-core` stays free of an `orbok-search` dependency, plus the five
  storage-mirror enums (`StoredKindFilter`, `StoredChangedFilter`,
  `StoredReadyFilter`, `StoredSearchStyle`, `StoredLanguageFilter`).
  `folder_id()` / `label()` helpers for restore validity and display.
- `SearchHistoryId`, `SearchHistorySettings` (default `max_entries = 20`,
  `clear_when_privacy_strict = true`).
- `SearchHistoryEntry::accessible_label()` for screen-reader summaries
  (RFC-042 §15).

**Conversion (`orbok-search`)**

- `impl From<&ActiveFilter> for StoredSearchFilter` — maps each live filter
  variant + label to its stored mirror (correct dependency direction).

**Storage (`orbok-db`)**

- Migration `0004_search_history.sql` — `search_history` table (instructions
  + JSON filter labels; no snippets/embeddings/scores).
- `SearchHistoryRepository`: `upsert` (dedup on text+filters, refresh
  `last_used_at`/count, evict beyond `max_entries`, reject empty), `list`
  (newest first), `get`, `remove`, `clear`, `count`.

**UI (`orbok-ui`)**

- `SearchUiState` gains `history`, `history_panel_open`,
  `restoring_history_id` (RFC-042 §7.4).
- `AppState` gains `remember_recent_searches` and `confirm_clear_history`.
- 11 `Message` variants: open/close panel, `SearchAgain`,
  `RecentSearchRestored`, `RemoveRecentSearch`, ask/cancel/confirm clear,
  `RecentSearchesCleared`, `HistoryLoaded`, `ToggleRememberRecentSearches`.
- `recent_searches_panel` view: collapsed "Recent searches" button ↔ expanded
  list, each entry with a filter summary and "Search again"; "Clear recent
  searches" footer. No tabs (RFC-042 §5.2).
- Settings → Privacy: "Remember recent searches" toggle with local-only note,
  and a "Clear recent searches" control with inline confirmation (Cancel
  first, RFC-042 §11.6, §15).
- Two `UserNotice` variants: `RecentSearchesCleared`, `RecentSearchFilterDropped`.
- 18 i18n keys (En + Ja) covering all required RFC-042 §6.1 labels; forbidden
  vocabulary (§6.2) excluded and asserted in tests.

**App (`orbok` binary)**

- `history.rs`: record/load/get/remove/clear plus `restore_valid_filters`
  (drops folder filters whose source no longer exists — RFC-042 §9 step 3),
  all gated by `PrivacySettings::effective_recent_searches()` so Strict
  privacy disables history.
- `OrbokSettings::privacy_settings()` / `history_settings()` derivation.
- `SubmitSearch` records on success and refreshes the list; `SearchAgain`
  restores text + reruns against current files; clear/toggle wired with
  confirmation and notices; history loaded into state at startup.

### Tests

- `orbok-db`: +8 (dedup, max-entries eviction, empty rejection, clear, remove,
  get round-trip, serde round-trip, folder-filter validity).
- `orbok-search`: +4 (`From<&ActiveFilter>` variant/label mapping).
- `orbok-ui`: +9 (panel open/close, search-again restoring flow, clear
  confirmation, toggle-off clears, remove entry, forbidden-vocabulary copy,
  app-name copy).
- Workspace total: **387 tests / 0 failures**.

### Documentation

- RFC-042 moved `proposed/` → `done/`, Status `Implemented (v0.21.0)`.
  `rfcs/proposed/` is now empty — all RFCs through 045 are implemented.
- `rfcs/README.md`, `rfcs/handoffs/README.md`, `ROADMAP.md` updated.

---

## [0.20.1] — 2026-06-21 — Rename crate orbok-app → orbok

### Changed

- `crates/app/Cargo.toml`: package `name` changed from `orbok-app` to `orbok`.
  The compiled binary was already named `orbok` (via `[[bin]] name = "orbok"`);
  this aligns the crate package name with it so `cargo install orbok` works as
  expected and crates.io, docs.rs, and deps.rs badges all point to the right
  crate.
- Root `Cargo.toml`: workspace dependency key renamed `orbok-app` → `orbok`.
- `README.md`: crates.io, docs.rs, and deps.rs badge URLs updated to `orbok`.
- `docs/src/maintainers/architecture.md`: crate label updated.
- `docs/src/maintainers/release_readiness.md`: `-p orbok-app` → `-p orbok`.
- `docs/src/maintainers/testing.md`: `--exclude orbok-app` → `--exclude orbok`.
- All `// ... in orbok-app` / `` `orbok-app` `` references in `crates/**/*.rs`
  source comments updated to `orbok`.
- `crates/app/src/settings.rs`: resolved the open-question comment about binary
  naming — the crate package and binary are now both `orbok`.

No logic, API, or behaviour changes. No DB migrations. No new tests.

---

## [0.20.0] — 2026-06-21 — Search-in-Folder Flow and Friendly Folder Management (RFC-045)

### Added

**RFC-045: Search-in-Folder Flow and Friendly Folder Management.**

- `crates/ui/src/state/location.rs` — new module with:
  - `SearchFolderScope` (`FolderAndSubfolders` default / `FolderOnly`) with
    `includes_subfolders` helper; scope is a search-time restriction that never
    changes folder identity (RFC-045 §6.3).
  - `SearchLocation::Remembered { source_id, display_name, scope }` — P0
    variant; always backed by a remembered source record (RFC-045 §6.1, §13).
    `with_scope` returns a modified copy preserving folder identity.
    `source_id()` returns `Option<&SourceId>` so a future `Transient` variant
    (P1) can return `None` without a signature change.
  - `SearchLocationSummary` — compact entry for recent-folder chips (display
    name + source id).
  - `SearchLocationState { selected, recent_locations, picker_in_progress }` —
    defaults to no selection; `clear()` drops the location without touching the
    query; `set_scope()` applies the scope-change-in-place pattern.
  - All types are plain data (no iced imports), mirroring the `search` sibling.
- `AppState::search_location: SearchLocationState` — new field, default empty.
- `Message` variants (RFC-045 §17): `ChooseFolderRequested`,
  `FolderPickerCancelled`, `FolderPicked(PathBuf)`,
  `SearchLocationSelected(SearchLocation)`, `SearchLocationCleared`,
  `SearchScopeChanged(SearchFolderScope)`, `RecentFolderSelected(SourceId)`.
- `AppState::update` handles all seven variants: picker guard, neutral cancel,
  location commit + auto-search resume on `SearchLocationSelected`, chip clear,
  scope toggle, recent-chip promotion.
- `i18n::search_location_chip(locale, folder, scope)` — parameterized function
  producing "Documents and subfolders" / "Documents only" (En) and
  "Documents とサブフォルダー" / "Documents のみ" (Ja) — never the word
  "source" or "recursive" (RFC-045 §19.4).
- 5 new `MessageKey` variants (en + ja): `SearchInLabel`, `SearchChooseFolder`,
  `SearchScopeOnly`, `SearchScopeSubfolders`, `SearchRecentFoldersLabel`.
- `search_location_row` helper in `views.rs` renders the "Search in:" row:
  - No-selection state: passive `SearchInLabel / SearchChooseFolder` prompt.
  - Selected state: removable chip (`SearchLocationCleared`) + scope toggle
    (`SearchScopeChanged`) — progressive disclosure, no control shown until a
    folder is chosen.
- Recent-folder quick-select chip row below the location row: shown only when
  remembered folders exist and no location is selected (disappears on selection).
- `bootstrap::find_source_by_canonical_path` — reuse an existing source record
  rather than create a duplicate when the chosen folder path already exists in
  the catalog (RFC-045 §19.3).
- `orbok-app` `SubmitSearch` gate: if no location is selected, opens the OS
  folder picker via `rfd::AsyncFileDialog` as a non-blocking `iced::Task`
  (RFC-045 §19.0 — picker never called from view rendering). On picker return:
  reuse-or-create source → `SearchLocationSelected` → kick background scan →
  resume pending search (RFC-045 §8.1 "as soon as possible").
- 11 new tests in `tests/rfc045_location.rs` validating: default state (no
  selection, no recents), default scope (`FolderAndSubfolders`), chip labels in
  both locales, forbidden vocabulary ("source"/"recursive" absent from labels),
  scope-change preserves folder identity, clear preserves query text, recent
  summary fields.

### Changed

- Navigation and Sources view: "Sources" / "ソース" renamed to
  "Folders" / "フォルダー" (`NavSources`, `SourcesTitle`,
  `SourcesEmptyTitle`) — friendly user-facing copy throughout (RFC-045 §12,
  §19.4).
- `SubmitSearch` in `orbok-app` now gates on `search_location.has_selected()`
  before running the query; triggers the folder picker on first search when no
  location is set.

### Documentation

- `rfcs/done/045-…` — RFC-045 status updated to Implemented (v0.20.0); moved
  from `proposed/` to `done/`.
- `rfcs/README.md`, `rfcs/handoffs/README.md`, `ROADMAP.md` — RFC-045 reflected
  as shipped; RFC-042 remains the sole proposed RFC.
- Doc-sync (v0.19.0 carry-in, no code change): `ROADMAP.md` current-status
  section, `rfcs/README.md` contiguous Implemented table, `rfcs/handoffs/README.md`
  program-status labels — all updated to reflect the true v0.19.0 shipped state.

---

## [0.19.0] — 2026-06-21 — Phase 3: Model Readiness, Privacy Modes, Safe Diagnostics (RFC-043, RFC-039, RFC-040)

### Added

**RFC-043: Model Download Readiness and Bounded Concurrency.**

- `crates/search/models/src/readiness.rs` — `LocalFileStatus` (5 variants with
  `user_label`, `needs_work`), `FileReadiness`, `ModelReadiness` (4 variants),
  `ModelReadinessReport` (`files_needing_work`, `ready_count`, `total_count`),
  `check_model_readiness` — pure filesystem check, no network access, called at
  startup / before wizard / before download / after download / on retry.
- `crates/search/models/src/download_plan.rs` — `DownloadAction` (Skip /
  Download / Replace / Retry), `ModelFilePlan` with `.part` temp-file paths
  (RFC-043 §9.1 atomic write pattern), `DownloadPlan`, `build_download_plan`
  mapping readiness → action; `DEFAULT_MODEL_DOWNLOAD_CONCURRENCY = 2`
  (RFC-043 §11.1 bounded concurrency); progress types (`FileDownloadStatus`,
  `FileDownloadProgress`, `OverallDownloadProgress`);
  `FriendlyDownloadProblem` with 7 variants — all messages end with a period,
  avoid technical terms (RFC-043 §20).
- 10 RFC-043 i18n keys (en + ja): readiness states, download progress copy,
  failure messages — plain-language, no HTTP/TCP/DNS/URL terms.

**RFC-039: Privacy Modes and Local Data Visibility.**

- `crates/core/src/privacy.rs` — `PrivacyMode` (Standard / Strict / Portable /
  Diagnostics with `as_str`/`from_str` roundtrip, `allows_recent_searches`,
  `allows_snippet_persistence`, `allows_diagnostics_sensitive_optins`);
  `PrivacySettings` with `with_mode_applied` enforcing strict overrides and
  `effective_recent_searches` / `effective_snippet_persistence` helpers;
  `LocalDataCategory` (12 variants with plain-language `user_label` — no
  "cache/catalog/vector/fts"); `DiagnosticsPolicy` with `from_privacy` and
  `allows_sensitive_optins`.
- `OrbokSettings` extended with `privacy_mode`, `remember_recent_searches`,
  `persist_snippets`, `clear_temporary_previews_on_exit`.
- 19 RFC-039 i18n keys (en + ja): privacy mode descriptions, strict confirmation
  dialog, toggle labels — no technical terms, `orbok` throughout.
- `Message` variants: `SetPrivacyMode`, `PrivacySettingChanged`,
  `ClearTemporaryPreviews`.

**RFC-040: Safe Diagnostics and Redacted Support Bundle.**

- `crates/app/src/diagnostics.rs` — `DiagnosticsManifest` (records exactly what
  was included, defaults `redacted: true`), `DiagnosticsSectionKind` (11
  sections, each with a stable `filename`), `redact_text` engine (home
  directory, absolute paths → `<folder>/filename`, URL query strings → `?<redacted query>`),
  `collect_app_info`, `collect_platform_info`, `bundle_preview_text`.
  Never uploaded automatically; manual export only.
- `UserNotice` extended with `DiagnosticsFileCreated` (Info tone) and
  `DiagnosticsFileFailed` (Danger tone + retry action).
- `Message` variants: `DiagnosticsCreateBundle`, `DiagnosticsBundleCreated`,
  `DiagnosticsBundleFailed`, `DiagnosticsOptInChanged`.
- 13 RFC-040 i18n keys (en + ja): bundle preview, opt-in labels, result notices.

### Tests

- `orbok-models/src/rfc043_tests/rfc043_readiness.rs` — 8 tests: missing dir →
  NeedsDownload, complete valid files → Ready, `.part` → Partial, empty file →
  Invalid, ready count, label compliance, `needs_work` correctness.
- `orbok-models/src/rfc043_tests/rfc043_download_plan.rs` — 9 tests: skip ready,
  download missing, retry partial, replace invalid, concurrency ≤ 2, temp path
  `.part` suffix, friendly message technical-term compliance and punctuation.
- `orbok-core/src/tests.rs` extended — 10 RFC-039 tests: default mode, strict
  disables searches/snippets, strict `with_mode_applied` forces overrides,
  standard respects user choice, `PrivacyMode` roundtrip, `LocalDataCategory`
  label compliance, `DiagnosticsPolicy` strict restrictions.

**351 tests / 0 failures / 0 warnings.**

---

## [0.18.0] — 2026-06-21 — Phase 2: Search UX, Source Lifecycle, Result Trust (RFC-041, RFC-037, RFC-038)

### Added

**RFC-041: Search, Narrow Results, and Browse Around.**

- `crates/search/engine/src/filter.rs` — complete filter model: `ActiveFilter`,
  `KindFilter`, `ChangedFilter`, `ReadyFilter`, `SearchStyle`, `LanguageFilter`,
  `SuggestedFilter`, `extension_matches_kind`, `is_already_active`.
- `crates/ui/src/state/search.rs` — `SearchUiState` with `apply_suggested`,
  `remove_filter`, `clear_filters`; `ResultsStatus` (7 variants covering the
  full results lifecycle); `ResultTrustDisplay`.
- `SearchResultDisplay.trust` field added (RFC-038 integration).
- `AppState.search_ui: SearchUiState` added; `Message` extended with
  `ApplySuggestedFilter`, `RemoveFilter`, `ClearFilters`, `OpenMoreWays`,
  `CloseMoreWays`, `SearchInResultFolder`, `ShowNearbyFiles`, `ShowSimilarFiles`,
  `TrustRecoveryAction`.
- `update()` now syncs `query` ↔ `search_ui.text`, advances `results_status`
  through Searching → Ready / EmptyAfterSearch / EmptyAfterFiltering, and
  handles all new filter messages.
- `components::filter_chip` — narrowing chip with `×` active state.
- `components::result_trust_badge` — plain-text trust badge, returns `None`
  for Ready results to keep them uncluttered.
- 67 new i18n keys in `en.rs` and `ja.rs`: filter labels, trust badges,
  source lifecycle copy, recovery actions — all avoiding forbidden technical terms.

**RFC-037: Source Lifecycle, Refresh Policy, and Change Detection UX.**

- `crates/data/fs/src/source_lifecycle.rs` — `SourceState` (7 variants with
  `user_label`, `is_searchable`, `can_refresh`, `as_str`), `FileState` (8 variants
  with `user_label`, `from_catalog_status`), `FileFingerprint` (metadata change
  detection, no content-hash by default), `SourceCheckResult`, `check_source_path`.

**RFC-038: Result Freshness, Trust Badges, and Recovery Actions.**

- `crates/search/engine/src/result_trust.rs` — `ResultTrustState` (6 variants
  with `show_badge_by_default`), `ResultWarningSummary` (maps `ExtractWarning`
  from RFC-044), `ResultRecoveryAction` (6 actions), `SearchResultTrust` with
  `from_catalog` computing trust from file status + extraction warnings.
- `orbok-search` gains `orbok-extract` and `serde` as direct dependencies.

### Tests

- `orbok-search/src/tests/rfc041_filter.rs` — 12 tests: add/remove/clear,
  duplicate prevention, extension matching, label stability, copy compliance.
- `orbok-search/src/tests/rfc037_source_lifecycle.rs` — 14 tests: state labels,
  searchability, refresh eligibility, file status mapping, metadata change
  detection, missing folder, catalog string stability.
- `orbok-search/src/tests/rfc038_result_trust.rs` — 15 tests: all file status
  → trust state mappings, extraction warning → PartlyPrepared, badge display
  rules, recovery action coverage, copy compliance.
- `orbok-ui/src/tests/rfc041_search_state.rs` — 14 tests: `SearchUiState`
  operations, `AppState` message handling, results status transitions, copy
  compliance (forbidden terms, orbok/orbit naming).
- Existing `orbok-search/src/tests.rs` refactored into `tests/` subdir
  (Rust 2018+ style) with `rfc007_keyword.rs` as the first submodule.

**328 tests / 0 failures / 0 warnings.**

---

## [0.17.1] — 2026-06-21 — Crate manifest: readme attribute

### Changed

- Added `readme = "…/README.md"` to all 11 publishable crate manifests
  (`orbok-app`, `orbok-core`, `orbok-ui`, `orbok-cache`, `orbok-db`,
  `orbok-fs`, `orbok-extract`, `orbok-workers`, `orbok-embed`,
  `orbok-search`, `orbok-models`), placed immediately after `description`.
  Path is relative from each crate to the workspace root `README.md`
  (`../../README.md` for top-level crates, `../../../README.md` for
  nested crates). `orbok-bench` (`publish = false`) is unchanged.

---

## [0.17.0] — 2026-06-21 — RFC-036: Resource-Aware Indexing Scheduler and Backpressure

### Added

**RFC-036: Resource-Aware Indexing Scheduler and Backpressure.**

- **`Scheduler`** (`crates/pipeline/workers/src/scheduler/scheduler.rs`):
  production dispatch engine. `tick()` pops the next job respecting
  `ResourceMode`; `enqueue()` routes jobs to bounded queues with
  backpressure; `complete()`/`fail()` update catalog state and retry
  within `MAX_JOB_ATTEMPTS` (3); `pause()`/`resume()` persist job state
  to the catalog; `cancel_source()` removes all queued work for a removed
  folder; `drain_events()` returns `SchedulerEvent`s for the UI layer.

- **`WorkPriority`** (5 levels: `UserBlocking` → `Maintenance`): derived
  `Ord` so higher-priority jobs dispatch first; FIFO within equal priority.
  `JobKind::GenerateEmbedding` defaults to `LowBackground`;
  `JobKind::Cleanup`/`Repair` default to `Maintenance`.

- **`BoundedQueue` / `QueueSet`**: six typed bounded queues (scan, extract,
  chunk, keyword, embedding, maintenance) with capacity enforcement and
  backpressure events. `pop_next(ResourceMode::UserActive)` skips the
  embedding queue so search is never delayed (RFC-036 §9.2).

- **`ResourceMode`** (`Normal` / `UserActive` / `LowImpact` / `Paused`):
  `notify_user_active()` / `notify_user_idle()` transition modes and emit
  `SchedulerEvent`s; no duplicate events on repeated calls.

- **`SchedulerEvent`** channel: `JobQueued`, `JobStarted`, `JobCompleted`,
  `JobFailed`, `JobCancelled`, `QueueBackpressureApplied/Released`,
  `UserActivityDetected`, `ResourceModeChanged`, `PartialReadinessChanged`.

- **`SchedulerConfig`** (`SchedulerLimits` + `QueueCapacity`): conservative
  defaults (1 worker per queue; caps: scan 10k, extract 1k, embedding 2k).

- **DB migration 0003** (`0003_scheduler.sql`): adds `attempt_count`,
  `last_error_kind`, `paused_at` to `index_jobs`. Baseline updated to
  include `paused` and `waiting_for_dependency` in the `status` CHECK
  constraint.

- **`JobStatus::Paused` / `WaitingForDependency`**: new variants in
  `orbok-core::status` with stable catalog strings.

- **`OrbokError::BackpressureActive`**: typed error for full-queue rejections.

- **`IndexJobRepository::enqueue_with_priority`** / **`increment_attempt`** /
  **`count_indexed_files`**: new repo methods for RFC-036 persistence.

- **17 RFC-036 acceptance tests** in
  `crates/pipeline/workers/src/tests/rfc036_scheduler.rs`: priority
  ordering, FIFO within equal priority, capacity enforcement, embedding
  skip in `UserActive` mode, resource mode transitions, event emission,
  no-duplicate-event invariant, source cancellation across queues,
  `WorkPriority` `Ord` correctness, `JobKind` defaults, queue clear.

### Fixed

- `scripts/package.sh`: archive named `orbok-vX.Y.Z.tar.gz` (v prefix),
  writes via tmp file to avoid self-inclusion (was already fixed in v0.15.0
  but noted here for completeness).
- `chunker.rs`: removed needless `mut` on `flush` closure.
- DB baseline: `index_jobs.status` CHECK now includes all RFC-036 statuses.

**261 tests / 0 failures / 0 warnings in RFC-036 files.**

---

## [0.16.0] — 2026-06-21 — RFC-044: orbok-extract Production Hardening

### Changed

**RFC-044: `orbok-extract` Production Hardening and Boundary Cleanup.**

- **`ExtractLimits` / `ExtractContext`** (`types.rs`): new types carrying
  per-extraction resource limits (`max_file_bytes`, `max_extracted_chars`,
  `max_segments`, `max_pdf_pages`, `max_docx_xml_bytes`, `max_zip_entry_bytes`,
  `max_html_bytes`) with conservative defaults. All built-in extractors now
  implement `extract_with_context` and honor their format-specific limits.

- **`ExtractWarning`** (`types.rs`): structured warning enum added to
  `ExtractOutput.warnings` (serde `#[serde(default)]`, backward-compatible with
  cached payloads). Variants: `SomePagesUnreadable`, `PossiblyScannedPdf`,
  `SizeLimitReached`, `EncodingUnsupported`, `UnsupportedDocumentPart`,
  `ApproximateLocationOnly`, `MalformedContentRecovered`, `SomeContentSkipped`.

- **`LocationKind`** (`types.rs`): new enum on `ExtractedSegment.location_kind`
  distinguishing `Lines` / `Pages` / `Paragraphs` / `Blocks` / `Unknown`. Each
  extractor sets the correct kind: text+markdown → Lines, PDF → Pages, DOCX →
  Paragraphs, HTML → Blocks. The UI must not label pages as "line N".

- **`ErrorCategory::ParserPanic`** (`orbok-core`): new variant with stable
  catalog string `"parser_panic"` for panics caught by the isolation wrapper.

- **`ExtractorRegistry::extract_safely`** (`registry.rs`): production entry
  point. Wraps every extractor call in `std::panic::catch_unwind`; a parser
  panic returns `ErrorCategory::ParserPanic` instead of crashing the worker.
  `ExtractorRegistry::extract` now delegates to `extract_safely`. Added
  `ExtractorRegistry::new_with` constructor for test injection.

- **Crate-boundary cleanup** (RFC-044 §14 Option B): `chunker.rs` now produces
  `Vec<ExtractedChunk>` (a new DB-free type in `types.rs`) instead of
  `Vec<orbok_db::repo::ChunkSpec>`. `orbok-db` removed from `orbok-extract`
  `[dependencies]` (kept in `[dev-dependencies]` until test migration
  completes). Conversion `ExtractedChunk → ChunkSpec` lives in new
  `crates/pipeline/workers/src/chunk_adapter.rs`; `chunker.rs` propagates
  `LocationKind` through to chunks.

- **Test boundary cleanup** (RFC-044 §15): `v07_features.rs` (RFC-021/022/029
  tests covering embedding backend, PDF extraction, model integrity) moved from
  `orbok-extract` to `orbok-workers/src/tests/`; imports updated to use
  fully-qualified crate paths. `ExtractionWorker` updated to call
  `extract_safely` with `ExtractContext::default()`.

### Added

- **RFC-044 acceptance tests**: 15 new tests across two submodules:
  - `tests/rfc044_limits.rs`: file-size limits (text/HTML/DOCX), segment
    cap, char-truncation warning, clean-extraction zero-warnings invariant.
  - `tests/rfc044_isolation.rs`: panic-isolation round-trip via
    `PanickingExtractor`, missing-file → `SourceMissing`, invalid-UTF-8 →
    `EncodingError`, unsupported extension → `UnsupportedType`, `LocationKind`
    per format, chunker `LocationKind` propagation, boundary compile-proof.

**244 tests / 0 failures / 0 warnings in RFC-044 files.**

---

## [0.15.0] — 2026-06-21 — RFC pipeline: 036–045 merged; planning baseline

### Docs / Planning

- **RFC pipeline updated (RFC-036–045).** Merged ten proposed RFCs into
  `rfcs/proposed/`: the stabilization track (036–040 — scheduler, source
  lifecycle, result trust, privacy modes, diagnostics) and the foundation /
  search-UX track (041–045 — search-narrow-browse, search history,
  model-download readiness, orbok-extract hardening, search-in-folder flow).
  041–044 are the renumbered former-032–035 lineage (those numbers were already
  taken by the design-system program); 036–040 cross-references were repointed
  to 041–044 accordingly. RFC-045 arrived accepted (self-reviewed).
- **Design-system RFCs 032–035 transitioned to `rfcs/done/`** with `Implemented`
  status and release tags (v0.12.0–v0.14.0), matching what shipped; inbound
  handoff path references updated `proposed/` → `done/`.
- **`rfcs/README.md` index rebuilt** to list all 46 RFCs (35 done, 10 proposed,
  1 archived); RFC-000 integrity invariants pass. RFC-000's release tag
  corrected from the policy template's `v1.4.0` to `v0.6.0`.
- **`ROADMAP.md`** gains a current-status + forward-plan section with a
  recommended implementation order; developer handoffs added for 041–045 and
  the handoffs README updated to cover all three programs (design-system,
  stabilization, foundation/search-UX).

---

## [0.14.0] — 2026-06-21 — RFC-035: Inclusive Design

### Changed

**RFC-035: Inclusive Design.**

- **`TextScale` enum** (`Default` / `Large` / `Larger`) in `theme.rs`: uniform
  1× / 1.15× / 1.3× multiplier applied to all typography roles via `*_s`
  helper variants (`body_s`, `title_s`, `heading_s`, `meta_s`, `label_s`).
  Every view reads `state.text_scale` so the multiplier propagates with no
  per-view structural change. `theme.rs` cleaned up: duplicate `Theme` definition
  and unscaled-only helpers removed; one canonical enum and one set of helpers.
- **`reduced_motion: bool`** in `AppState`: defaults from `ORBOK_REDUCE_MOTION`
  env var (best-effort OS probe; full platform watcher is a tracked follow-up).
  Wired as a forward-compatible gate — no animations exist yet, so the flag is a
  no-op guard today that takes effect the moment motion is introduced.
- **Settings view** (RFC-035 plain-language surface):
  - Text size picker: `Default` / `Large` / `Larger` buttons.
  - Reduce motion toggle: checkbox-style button with hint text.
  - CVD note (always-on, not a toggle): "Status colors are always shown with a
    label and an icon…" in both `en` and `ja`.
- **CVD-safe status guarantee** (`components.rs`): `tone_icon(tone)` maps each
  `Tone` to a distinct lucide glyph (`CheckCircle` / `AlertTriangle` / `CircleX`
  / `Info` / `Sparkles` / `Clock`), giving every status badge three independent
  channels: text label + icon/shape + tone colour.
- **Locale-aware formatting** (`i18n.rs`): `fmt_gib`, `fmt_mib_bucket`,
  `fmt_storage_row`, `fmt_query` route all user-facing number/size/query
  display through locale-specific format strings. Views use these instead of
  ad-hoc `format!`.
- **RTL readiness audit**: no hard-coded `Alignment::Left/Right` found in view
  or component modules; `LayoutDirection` is plumbed to both navigation widgets
  in `shell.rs`. A future RTL locale requires catalog work only.
- **`OrbokSettings`**: `text_scale` and `reduced_motion` fields added; loaded
  at startup, persisted on change via `bootstrap::persist_text_scale` /
  `persist_reduced_motion`.
- **Test suite split** — `tests.rs` (681 lines) refactored into a thin router
  (118 lines) + five submodules under `tests/`:
  - `tests/i18n.rs` (87 lines): catalog, locale detection, parameterized messages.
  - `tests/state.rs` (165 lines): transitions, theme, scale, motion, notices.
  - `tests/components.rs` (109 lines): RFC-033 tone mapping, badge invariant, smokes.
  - `tests/a11y.rs` (268 lines): RFC-034 contrast guard, keyboard map, RFC-035 CVD,
    scale helpers, formatting, RTL layout proof.
  - `tests/smoke_views.rs` (74 lines): headless view renders.
- **39 tests, 0 failures, 0 warnings.** New RFC-035 tests: `cvd_icon_pairs_are_distinct`,
  `cvd_greyscale_status_distinguishable`, `text_scale_helpers_produce_correct_sizes`,
  `locale_aware_size_formatting`, `layout_direction_is_plumbed_to_navigation`,
  `set_text_scale_updates_state`, `set_reduced_motion_updates_state`,
  `text_scale_roundtrip`.

### Fixed

**Theme change now takes effect at runtime (bugfix).**

The theme picker (RFC-032) was persisting the selection and updating
`AppState::tokens` correctly, but the iced renderer always displayed the
built-in Light theme because no `.theme()` hook was wired into the iced
application builder. Without it iced ignores the snora token palette and
falls back to its own default.

Fix: `OrbokApp::iced_theme()` maps the active snora token palette to an
`iced::Theme::Custom`, bridging the six snora semantic roles (`background`,
`text_primary`, `accent`, `success`, `warning`, `danger`) to iced's
six-field `Palette`. The application builder now calls `.theme(|app|
app.iced_theme())` so every theme selection (Light / Dark / High Contrast
Light / High Contrast Dark / System) restyles the whole app immediately.

Font size change was already working via `state.text_scale` and the `*_s`
typography helpers; theme change now works correctly too.

---

## [0.13.0] — 2026-06-21 — RFC-034: Accessibility Conformance (WCAG 2.1 AA)

### Changed

**RFC-034: Accessibility Conformance.**

orbok now targets WCAG 2.1 Level AA. The following are implemented and tested:

- **`crates/ui/src/a11y.rs`** (new): contrast usage-guard module. Defines
  `RENDERED_PAIRS` — every foreground/background role pair orbok renders — and
  an `audit(tokens)` function that checks each pair against AA thresholds
  (4.5:1 normal text, 3.0:1 large/UI components) using
  `snora::design::contrast::contrast_ratio` (available since snora 0.25.1).
- **`crates/ui/src/shell.rs`**: `key_to_message(key, modifiers, text_input_focused)`
  — pure, testable keyboard-map function implementing the GUI §17.1 shortcut
  table (`Ctrl+K` → FocusSearch, `Ctrl+,` → Settings, `Escape` → DismissOverlay,
  `Enter` when focused → SubmitSearch, arrow keys → result navigation). Text
  input is never hijacked: printable keys and bare `Enter` pass through while
  typing.
- **`crates/app/src/main.rs`**: keyboard subscription via
  `iced::keyboard::listen()` wired to `key_to_message`; `FocusSearch` switches
  to the Search view (programmatic `text_input::focus` Task not available in
  iced 0.14 — documented as tracked limitation).
- **`crates/ui/src/state.rs`**: four new messages — `FocusSearch`,
  `DismissOverlay`, `SelectNextResult`, `SelectPrevResult` — with `DismissOverlay`
  closing `confirm_reset`/notices in priority order; arrow keys clamping at
  bounds.
- **`docs/src/maintainers/accessibility.md`** (new): WCAG 2.1 AA conformance
  record — full success-criteria checklist with orbok's status per criterion,
  the iced-0.14 focus-ring limitation documented as an owned tracked decision,
  and the manual a11y QA steps (keyboard walkthrough, screen reader spot check,
  high-contrast visual pass, grayscale badge distinguishability).
- **`docs/src/SUMMARY.md`**: accessibility.md added to the maintainers section.
- **`docs/src/maintainers/release_readiness.md`**: accessibility QA added as an
  M13 gate, linked to the new accessibility doc.
- **31 tests, 0 failures.** New RFC-034 tests:
  `contrast_usage_guard_all_presets`, `key_map_shortcuts`,
  `key_map_no_text_swallow`, `dismiss_overlay_closes_reset`,
  `result_navigation_bounds`, `primary_action_target_size`.

---

## [0.12.0] — 2026-06-20 — RFC-032 + RFC-033: Design Token Foundation and Component Primitive Migration; snora 0.25.1

### Changed

**RFC-032: Design Token Foundation and Theming — snora updated to 0.25.1.**

snora 0.25.1 re-exports `snora::design::contrast` on the facade (previously
only in `snora_design`). orbok bumps its snora dependency to 0.25.1.

The Snora Design token system is now the single source of truth for all visual
values in `orbok-ui`. Previously, the token bundle (`AppState::tokens`) only
drove the notice primitive; all other view code used hardcoded `.size()`,
`.padding()`, and `.spacing()` literals.

- **New `crates/ui/src/theme.rs`:** the `Theme` enum
  (`System`/`Light`/`Dark`/`HighContrastLight`/`HighContrastDark`) with preset
  mapping, setting-string round-trip, and `ORBOK_THEME` env-var resolver.
  Typography helpers (`theme::body`, `theme::meta`, `theme::label`,
  `theme::title`, `theme::heading`) wrap the snora style bridge.
- **`views.rs` and `views/wizard.rs` fully rewritten** token-driven: every
  literal font size, padding, and spacing replaced by `theme::*` and
  `tokens.spacing.*`.
- **Settings view gains a theme picker** (replaces the high-contrast toggle):
  users can choose Follow System / Light / Dark / High Contrast Light / High
  Contrast Dark; the selection persists across restarts in `OrbokSettings`.
- **`AppState`:** `high_contrast: bool` replaced by `theme: Theme`; message
  `ToggleHighContrast` replaced by `SetTheme(Theme)`.
- **`OrbokSettings`:** new `theme` field (default `"system"`).
- **`orbok-app`** resolves `System` to a concrete theme at startup via
  `Theme::from_env()` (best-effort `ORBOK_THEME` override; full platform probe
  is a tracked follow-up). Persists on `SetTheme`.
- **i18n:** `SettingsAccessibilityHeading`/`SettingsHighContrast{On,Off,Hint}`
  keys replaced by `SettingsThemeHeading`/`Theme{System,Light,Dark,...}` in
  both `en` and `ja` locales; `tests::ALL_KEYS` updated accordingly.
- **`scripts/check-design-tokens.sh`:** CI grep gate that fails if view modules
  contain literal sizes, paddings, spacings, or `iced::Color` references.

`views.rs` is 521 lines (over the 500 ELOC strong-split threshold). RFC-033's
`components.rs` will move cards/badges/buttons there, bringing it back under
the threshold.

**RFC-033: Component Primitive Migration.**

snora is now the sole gateway for UI component primitives, mirroring the
existing lucide-icons and token gateway rules.

- **New `crates/ui/src/components.rs`:** the orbok adapter layer between view
  models and snora primitives. View modules call these; they never call
  `snora::design::{button, card, chip, progress}` directly.
- **`result_card`** — search results use `card::selected` (accent border) when
  active, `card::surface` otherwise, wrapped in a ghost button for click/keyboard.
- **`source_card`**, **`health_cell`** — `card::surface` based; uniform padding
  and radius across all views.
- **`status_badge(tokens, label, tone)`** — every status badge now renders text
  + a tone-specific lucide icon + tone colour (three redundant channels; RFC-035
  CVD-safe guarantee). Badges are never colour-only.
- **`badge_tone(label)`** — stable string → `Tone` mapping (table-driven, shared
  by both the UI and the upcoming RFC-035 CVD test fixture).
- **`tone_icon(tone)`** — stable `Tone` → lucide glyph mapping: `CheckCircle`
  (success), `AlertTriangle` (warning), `CircleX` (danger), `Info` (info),
  `Sparkles` (accent/semantic), `Clock` (neutral).
- **`primary`/`secondary`/`ghost`/`danger`** — all actions route through these
  wrappers, which use `snora::design::button::*_maybe`. Every destructive action
  (Reset Catalog, Remove Source) now uses `danger`, never a neutral button.
  Every disabled action passes `None` for a true disabled state.
- **`icon_primary`/`icon_secondary`** — icon+label buttons styled via the snora
  style bridge (raw `iced::button` + `btn_style::primary/secondary`).
- **`job_progress`** — indexing view uses `progress::row` with `Tone::Accent`;
  indeterminate state passes `None`.
- **`views.rs` shrinks from 521 → 442 lines** (under the 500 ELOC threshold);
  `components.rs` is 315 lines. Both within the project's split thresholds.
- **25 tests:** 7 new RFC-033 tests (tone mapping, badge invariant, component
  smoke tests for all adapters); all 25 tests pass.

---

## [0.10.0] — 2026-06-20 — Remove lucide-icons iced feature from orbok-ui; snora 0.25 + Snora Design system

### Changed

**`lucide-icons` in `orbok-ui` no longer uses the `iced` feature.**

snora 0.18.1 fixed a latent bug: `lucide_icons::iced::icon_*()` functions
call `Icon::widget()` which returns `iced::widget::Text` typed against
lucide-icons' own `iced_core` version. When `iced_core` appears in the graph
from multiple crates, this causes type-parameter mismatches. The fix is to
call `char::from(icon)` and construct the Text widget from the glyph character
directly — which is exactly what snora's `icon_element_sized` now does.

**What changed in orbok-ui:**

`lucide-icons = { version = "1", features = ["iced"] }` → `lucide-icons = "1"`

The `iced` feature is dropped from orbok-ui's explicit request. Cargo still
compiles it (snora's `lucide-icons` feature requests it), but orbok-ui no
longer uses the `iced` module's `icon_*()` functions.

A new private `icon_text(variant, size)` helper in `views.rs` and
`views/wizard.rs` replicates snora's technique:

```rust
fn icon_text<'a>(variant: lucide_icons::Icon, size: f32) -> iced::widget::Text<'a> {
    iced::widget::text(char::from(variant).to_string())
        .font(iced::Font::with_name("lucide"))
        .size(size)
}
```

All twelve `icons::icon_*()` call sites have been replaced with
`icon_text(lucide_icons::Icon::VariantName, size)`.

`LUCIDE_FONT_BYTES` and `lucide_icons::Icon` (used in `shell.rs` for the
sidebar) are still available from the base crate without the `iced` feature.

The icon_text helper signature was also tightened. Instead of taking
`lucide_icons::Icon` by value:

```rust
// Before
fn icon_text<'a>(variant: lucide_icons::Icon, size: f32) -> iced::widget::Text<'a>
// Called as: icon_text(lucide_icons::Icon::Search, 13.0)

// After
fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a>
// Called as: icon_text(char::from(snora::lucide::Search), 13.0)
```

`snora::lucide::*` re-exports `lucide_icons::Icon::*` (all 1716 variants)
so `snora::lucide::Search` names the variant without requiring the caller
to mention `lucide_icons::Icon` at all. The `From<Icon> for char` impl is
in the base crate (no iced feature needed).

`shell.rs` similarly replaced `use lucide_icons::Icon as LucideIcon` with
`use snora::lucide` and `Icon::Lucide(lucide::Search)` etc.

After these changes, the **only** remaining direct use of `lucide_icons::` in
orbok-ui is:

```rust
// crates/ui/src/lib.rs
pub use lucide_icons::LUCIDE_FONT_BYTES;
```

This is the single reason orbok-ui still needs a direct `lucide-icons` dep.
If snora re-exported `LUCIDE_FONT_BYTES`, the dep could be dropped entirely
and snora would become the sole gateway to lucide-icons for all consumers.

**`snora` upgraded: 0.18.1 → 0.18.3** (includes 0.18.2 doc fixes)

snora 0.18.2 fixed doc examples (no API changes). snora 0.18.3 adds
`LUCIDE_FONT_BYTES` to its `lucide` re-export module. The constant is
now at `snora::lucide::LUCIDE_FONT_BYTES` alongside the icon variants.

With this, **`lucide-icons` has been removed from `orbok-ui`'s
`[dependencies]` entirely.** `snora` is now the sole gateway to the
lucide icon set for all orbok-ui consumers:

```toml
# crates/ui/Cargo.toml — after
snora = { workspace = true, features = ["lucide-icons"] }
# lucide-icons: no direct dep needed
```

`orbok-ui/src/lib.rs` re-export updated:
```rust
// before: pub use lucide_icons::LUCIDE_FONT_BYTES;
pub use snora::lucide::LUCIDE_FONT_BYTES;  // after
```

The full migration from the start of v0.9.16:

| Symbol | Before | After |
|---|---|---|
| Font bytes | `lucide_icons::LUCIDE_FONT_BYTES` | `snora::lucide::LUCIDE_FONT_BYTES` |
| Icon variants | `lucide_icons::Icon::Search` | `snora::lucide::Search` |
| Icon rendering | `lucide_icons::iced::icon_search()` | `icon_text(char::from(lucide::Search), sz)` |
| Type name | `lucide_icons::Icon` | not needed (char-based helper) |


**`crates/data/catalog/` renamed to `crates/data/db/`** so the directory name
matches the crate it contains (`orbok-db`). The other two crates in
`crates/data/` are already consistent (`cache/` → `orbok-cache`,
`fs/` → `orbok-fs`). Two references updated in root `Cargo.toml`; one line
in `architecture.md`. No source files or crate names changed.

### Added — snora 0.25 + Snora Design system

**snora upgraded: 0.18.3 → 0.25.0** (seven minor versions). All breaking
changes in 0.19–0.25 assessed against orbok's usage; the only one
(v0.24 `Palette::roles()` → `#[cfg(test)] pub(crate)`) does not affect orbok.
The version bump alone required zero source changes. iced remains `"0.14"`.

**`design` feature enabled** on orbok-ui's snora dependency, adopting the
Snora Design token system:

- **High-contrast accessibility mode.** `AppState` carries a
  `snora::design::Tokens` preset and `high_contrast: bool`. A new
  Settings → Accessibility toggle (`ToggleHighContrast`) swaps between
  `Tokens::light()` and `Tokens::high_contrast_light()`, whose contrast
  ratios are WCAG-AA-verified by snora-design's automated tests. New EN/JA
  i18n keys.
- **Notices render via `snora::design::notice::Notice`.** `friendly_notice`
  was rewritten to use the design primitive's tone-driven, contrast-verified
  colors and keyboard-reachable action/dismiss controls. The `UserNotice`
  domain enum is unchanged (still owns semantics + i18n) and gained a
  `tone()` method: Danger for hard failures, Warning for cautions, Success
  for positive confirmations, Info for neutral. This replaces orbok's
  hand-rolled notice card, cleanly separating domain meaning from accessible
  presentation.

Future incremental adoption (deferred): `chip::filter` for result badges,
`card::surface`/`selected` for result cards, `progress::row`/`card` for
download/indexing UI, `button::*` helpers. This release establishes the token
foundation and migrates the highest-value accessibility surface first.


### Tests
**205 tests / 0 failures** (189 non-GUI + 16 orbok-ui, incl. 2 new design
migration tests: tone mapping and high-contrast preset swap).

---

## [0.9.14] — 2026-06-10 — Remove lucide-icons iced feature from orbok-ui

### Changed

**`lucide-icons` in `orbok-ui` no longer uses the `iced` feature.**

snora 0.18.1 fixed a latent bug: `lucide_icons::iced::icon_*()` functions
call `Icon::widget()` which returns `iced::widget::Text` typed against
lucide-icons' own `iced_core` version. When `iced_core` appears in the graph
from multiple crates, this causes type-parameter mismatches. The fix is to
call `char::from(icon)` and construct the Text widget from the glyph character
directly — which is exactly what snora's `icon_element_sized` now does.

**What changed in orbok-ui:**

`lucide-icons = { version = "1", features = ["iced"] }` → `lucide-icons = "1"`

The `iced` feature is dropped from orbok-ui's explicit request. Cargo still
compiles it (snora's `lucide-icons` feature requests it), but orbok-ui no
longer uses the `iced` module's `icon_*()` functions.

A new private `icon_text(variant, size)` helper in `views.rs` and
`views/wizard.rs` replicates snora's technique:

```rust
fn icon_text<'a>(variant: lucide_icons::Icon, size: f32) -> iced::widget::Text<'a> {
    iced::widget::text(char::from(variant).to_string())
        .font(iced::Font::with_name("lucide"))
        .size(size)
}
```

All twelve `icons::icon_*()` call sites have been replaced with
`icon_text(lucide_icons::Icon::VariantName, size)`.

`LUCIDE_FONT_BYTES` and `lucide_icons::Icon` (used in `shell.rs` for the
sidebar) are still available from the base crate without the `iced` feature.

The icon_text helper signature was also tightened. Instead of taking
`lucide_icons::Icon` by value:

```rust
// Before
fn icon_text<'a>(variant: lucide_icons::Icon, size: f32) -> iced::widget::Text<'a>
// Called as: icon_text(lucide_icons::Icon::Search, 13.0)

// After
fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a>
// Called as: icon_text(char::from(snora::lucide::Search), 13.0)
```

`snora::lucide::*` re-exports `lucide_icons::Icon::*` (all 1716 variants)
so `snora::lucide::Search` names the variant without requiring the caller
to mention `lucide_icons::Icon` at all. The `From<Icon> for char` impl is
in the base crate (no iced feature needed).

`shell.rs` similarly replaced `use lucide_icons::Icon as LucideIcon` with
`use snora::lucide` and `Icon::Lucide(lucide::Search)` etc.

After these changes, the **only** remaining direct use of `lucide_icons::` in
orbok-ui is:

```rust
// crates/ui/src/lib.rs
pub use lucide_icons::LUCIDE_FONT_BYTES;
```

This is the single reason orbok-ui still needs a direct `lucide-icons` dep.
If snora re-exported `LUCIDE_FONT_BYTES`, the dep could be dropped entirely
and snora would become the sole gateway to lucide-icons for all consumers.

**`snora` upgraded: 0.18.1 → 0.18.3** (includes 0.18.2 doc fixes)

snora 0.18.2 fixed doc examples (no API changes). snora 0.18.3 adds
`LUCIDE_FONT_BYTES` to its `lucide` re-export module. The constant is
now at `snora::lucide::LUCIDE_FONT_BYTES` alongside the icon variants.

With this, **`lucide-icons` has been removed from `orbok-ui`'s
`[dependencies]` entirely.** `snora` is now the sole gateway to the
lucide icon set for all orbok-ui consumers:

```toml
# crates/ui/Cargo.toml — after
snora = { workspace = true, features = ["lucide-icons"] }
# lucide-icons: no direct dep needed
```

`orbok-ui/src/lib.rs` re-export updated:
```rust
// before: pub use lucide_icons::LUCIDE_FONT_BYTES;
pub use snora::lucide::LUCIDE_FONT_BYTES;  // after
```

The full migration from the start of v0.9.16:

| Symbol | Before | After |
|---|---|---|
| Font bytes | `lucide_icons::LUCIDE_FONT_BYTES` | `snora::lucide::LUCIDE_FONT_BYTES` |
| Icon variants | `lucide_icons::Icon::Search` | `snora::lucide::Search` |
| Icon rendering | `lucide_icons::iced::icon_search()` | `icon_text(char::from(lucide::Search), sz)` |
| Type name | `lucide_icons::Icon` | not needed (char-based helper) |


**`crates/data/catalog/` renamed to `crates/data/db/`** so the directory name
matches the crate it contains (`orbok-db`). The other two crates in
`crates/data/` are already consistent (`cache/` → `orbok-cache`,
`fs/` → `orbok-fs`). Two references updated in root `Cargo.toml`; one line
in `architecture.md`. No source files or crate names changed.

### Tests
**203 tests / 0 failures.**

---

## [0.9.13] — 2026-06-10 — Comprehensive audit: RFC compliance, tests, docs

Full five-point audit against RFCs, dead code, test coverage, code/test
consistency, and documentation. Three RFC compliance gaps found and closed;
documentation updated throughout.

### RFC compliance gaps closed

**RFC-003 — Sensitive directory warning wired (was untested path)**
`sensitive_warning()` existed in `orbok-fs` and was tested in isolation, but
`bootstrap::add_source` never called it. Fixed: `add_source` now checks and
returns an `Option<&'static str>` alongside the `SourceCard`. When a sensitive
path is detected, `main.rs` emits a `ShowNotice(SensitiveSourceAdded)` so
the user sees a friendly warning card in the Sources view. New `UserNotice`
variant and i18n keys in both EN and JA.

**RFC-029 — SHA-256 integrity check implemented**
The acceptance criterion "Checksum or stronger integrity check defined" was
not met: the model verifier only checked `size > 0`. Fixed with two additions:
- `ModelManifest` struct — written to `orbok-manifest.json` alongside the
  model files after every successful download. Stores SHA-256 of each file.
- `verify_embedding_model_deep()` — reads the manifest and verifies hashes.
  Returns `Valid`, `NoManifest` (manual placement), `ChecksumMismatch`, or
  `FileMissing`. Called only from the explicit Validate button, not at startup.
4 new tests cover manifest round-trip, `NoManifest`, valid checksums, and
corruption detection.

**RFC-031 — `auto` locale detects Japanese OS environment**
The acceptance criterion "`auto` locale resolves Japanese OS environments
to `ja`" was not implemented. The fallback was always `Locale::En`. Fixed:
`Locale::from_env()` checks `LANG` and `LANGUAGE` environment variables. If
either starts with `ja`, returns `Locale::Ja`. Wired into the bootstrap
locale priority chain: settings file → catalog → OS env → `En` default.
2 new tests (use `unsafe` env var mutation per Rust 2024 edition rules).

### Tests added (audit items 3 & 4)

- `safe_cleanup_preserves_sources` — RFC-001 testing requirement #1:
  all four safe `CleanupAction` variants are run in sequence; source
  registration must survive every one.
- `locale_from_env_detects_japanese` — RFC-031 §3 verified.
- `locale_from_env_english_fallback` — RFC-031 §3 negative case.
- 4 deep-verify tests in `model_verifier.rs` — RFC-029.

### Documentation fixed (audit item 5)

- `docs/src/maintainers/architecture.md` — was "nine crates"; now shows all
  twelve in the grouped `crates/` layout with correct paths.
- `docs/src/maintainers/development.md` — stale `-p orbok-app` commands
  replaced with current `cargo run` (default-members) pattern; packaging
  command added.
- `docs/src/maintainers/dep_audit.md` — date updated to 2026-06-10; snora
  corrected to 0.18.1; new deps (rfd, reqwest, futures, iced_test) added.
- `docs/src/users/quick_start.md` — install path `crates/orbok-app` →
  `crates/app`; wizard description updated to reflect HF download step.
- `README.md` — same install path fix; removed stale `(v0.1)` version tag.

### Dead code (audit item 2)
Zero dead code found across all twelve crates. No `#[allow(dead_code)]`
suppression in production code. All `TODO`/`FIXME` comments resolved in
previous releases.

### Tests
**203 tests / 0 failures** (189 non-GUI + 14 orbok-ui).

---

## [0.9.12] — 2026-06-10 — Storage wired, wizard back, result highlight, scroll

### Fixed

**Storage cleanup buttons are now actually wired** (they had no `.on_press`
and were entirely non-functional). Every action now calls the real backend:

- "Clear temporary previews" → `CleanupService::run_safe(ClearSnippetCache)` →
  shows "Temporary previews cleared" notice
- "Clear old search results" → `CleanupService::run_safe(ClearExpiredSearchCache)` →
  same notice
- "Reset saved app data…" → `AskResetCatalog` → shows a confirmation panel
  with Cancel (default focus) and a second click required on "Reset saved app
  data" → `CleanupService::run_reset(ResetCatalog, keep_settings=true)`

**Bootstrap functions added:** `clean_snippets`, `clean_search_cache`,
`reset_catalog` in `crates/app/src/bootstrap.rs`.

### Added

**Wizard Back button** — Checked and Ready pages now carry a "← Back" button
that returns to `WizardState::NotConfigured` (the initial setup screen).
Previously the only escape from a wrong-directory validation was Skip.

**Selected result highlight** — The active search result card shows
"▶  Title" prefix, replacing the `// TODO: visual highlight` stub.
Selection state was already tracked; it just was not rendered.

**Scrollable page wrapper** — Every page body is now wrapped in
`iced::widget::scrollable`. Narrow desktop windows can now scroll
instead of clipping content.

### Messages added
`CleanSnippets`, `CleanSearchCache`, `AskResetCatalog`, `ConfirmResetCatalog`,
`CancelResetCatalog`, `CleanupDone`, `WizardBack`

### State added
`AppState.confirm_reset: bool`

### Tests
**196 tests / 0 failures.**

---

## [0.9.11] — 2026-06-10 — Non-technical user UX hardening

Implements the substance of the UX architect's review for non-technical users.
The crate structure and message architecture are unchanged; the review's
proposed parallel `screens/`+`copy.rs` layout was not adopted (it would have
duplicated working code), but every user-facing recommendation was applied.

### Added — visible notices (replaces silent failures, P0)

New `orbok-ui::notice::UserNotice` — a centralized, friendly, actionable
message type covering both problems and confirmations:

- Problems: download failed, folder could not be added, search failed,
  files moved/missing — each with a plain title, explanation, and a recovery
  action ("Try again" / "Choose another folder").
- Confirmations: folder added, search ready, previews cleared.

`AppState.notice: Option<UserNotice>` with `ShowNotice` / `ClearNotice`
messages. A `friendly_notice` card renders at the top of the Search and
Sources views. Status is conveyed in words, never colour alone.

Wired so that:
- Download failure → returns to setup **and** shows "Download did not finish"
  (was silent).
- Folder-add / scan failure → shows "Folder was not added" (was logged only).
- Search failure → shows "Search did not finish" (was a no-op).
- Successful search clears any active notice.
- Folder added → shows "Folder added" confirmation.

### Changed — plain language (P0)

User-visible labels reworded for a general audience (keys unchanged):
- "Indexing" → "Preparing" / "Preparing search"
- "Index is up to date" → "Search is ready"
- "Semantic search" → "search by meaning"; "keyword search" → "basic search"
- Storage buckets: "Caches" → "Temporary previews", "Search index" →
  "Search data", "AI models" → "Search helper"
- "Reset catalog" → "Reset saved app data"

Applied to both English and Japanese catalogs.

### Changed — readability and click targets (P0)

- Core body text 13 px → 15 px; secondary metadata 11 px → 12 px (11 px no
  longer used for any readable content).
- Buttons via `icon_btn` now carry `[12, 16]` padding for a ~44 px target.
- Page padding 24/32 → 28/40 for calmer layout.

### Tests
**196 tests / 0 failures** (184 non-GUI + 12 orbok-ui, incl. 2 new notice
tests). All 17 new i18n keys are covered by the catalog-completeness test.

---

## [0.9.10] — 2026-06-10 — snora 0.8 → 0.18

### Changed

**snora upgraded: 0.8.0 → 0.18.0**

Ten minor versions. All changes between 0.8 and 0.18 were assessed against
orbok's usage. No source changes were required.

**Detailed change log (0.9 – 0.18):**

| Version | Change | Orbok impact |
|---|---|---|
| 0.9 | Doctests, migration index | None |
| 0.10 | Binary-size budget infra | None |
| 0.11 | `AppLayout` marked `#[non_exhaustive]`; toast ordering fix | None — orbok uses the builder (`AppLayout::new(body).side_bar(...)`) and does not use toasts |
| 0.12 | Render-semantics tests, workbench example, doc-test policy | None |
| 0.13 | Anchored-popover design doc, API-freeze review | None |
| 0.14 | `snora::keyboard::dismiss_on_escape` added (new public API) | None — additive only |
| 0.15 | Starter example, versioning policy, migration template | None |
| 0.16 | Alternate-engine boundary doc, performance envelope | None |
| 0.17 | `Icon` gains `PartialEq`; two RTL integration tests | None — additive only |
| 0.18 | Contributing overview, version-snippet updates, ROADMAP | None |

iced remains `"0.14"` and lucide-icons remains `"1"` in snora's workspace
dependencies — no transitive dep conflicts.

### Tests
**194 tests / 0 failures.**

---

## [0.9.9] — 2026-06-08 — Minimal view smoke tests (iced_test)

### Added

A small set of view smoke tests using `iced_test 0.14` (matches our iced
version). Deliberately minimal, per project philosophy — iced_test is young,
and orbok's real logic lives in `AppState::update`, which is already tested as
a pure function. These four tests only confirm the view builders produce a
usable interface and that key content survives refactors:

- `search_empty_state_offers_add_source` — empty search view shows its CTA
- `search_empty_cta_switches_to_sources` — clicking the CTA emits `Switch(Sources)`
- `settings_view_has_advanced_toggle` — settings exposes the advanced toggle
- `sources_view_renders_both_states` — empty and populated sources render

The tests target individual view functions (plain iced widget trees), not the
full snora shell, which keeps them stable and fast.

`iced_test` is a dev-dependency of `orbok-ui` only; it does not affect the
shipped binary.

### Tests
**194 tests / 0 failures** (184 non-GUI + 10 orbok-ui, incl. 4 new smoke tests).

---

## [0.9.8] — 2026-06-08 — Less is more: progressive disclosure

### Changed

Applied the project's core UI principle — *less is more* — by removing
technical noise from the default views and deferring it behind a single
**Advanced view** toggle (Settings → Advanced view). New users see a clean,
task-focused interface; mature users opt into detail.

**Search view**
- The Auto/Exact/Conceptual mode selector is hidden by default. Auto handles
  the common case; the switch appears only in Advanced view. New users just
  type and search.
- Result cards show only trust-relevant status badges (Stale/Missing) by
  default. Match-type badges (Keyword/Semantic/file-type) are Advanced-only.

**Indexing view (AI → Indexing)**
- "Indexed" count is always shown. Queued / Stale / Failed cells appear only
  when non-zero (or in Advanced view). A healthy idle index is now a single
  clean number instead of three zeros.

**Storage view (AI → Storage)**
- Default view groups usage into three plain-language buckets: Search index,
  AI models, Caches. The raw per-engine category breakdown
  (`keyword_index`, `vector_index`, `snippet_cache`, …) is Advanced-only.

**Settings view**
- New "Advanced view" toggle with explanatory hint. Off by default.

### State
`AppState.show_advanced: bool` (default `false`); `Message::ToggleAdvanced`.

### Tests
**184 tests / 0 failures.**

---

## [0.9.7] — 2026-06-08 — HuggingFace model download

### Added

**Model download from HuggingFace** (`crates/app/src/download.rs`, `reqwest 0.12`)

The startup wizard no longer requires users to prepare model files manually.
"Download from HuggingFace" is now the primary action on the setup screen.

**Wizard setup screen redesign**

The initial screen now has three clearly ranked actions:

1. **Download from HuggingFace** (primary) — shows model name, license, and
   size before the user commits: "multilingual-e5-small · Apache 2.0 · ~93 MB · 100+ languages"
2. **Locate existing files** (secondary) — the previous manual path flow,
   preserved for users who already have files
3. **Skip — keyword search only** (tertiary)

**Download progress screen** (`WizardState::Downloading`)

While downloading, the wizard shows:
- Current file name
- `progress_bar` widget tracking bytes received vs total
- Human-readable size counter: "84.2 MB / 95.0 MB  (88%)"
- File N-of-M indicator

When the download completes, the wizard automatically advances to `WizardState::Ready`
and the user clicks "Use model" to dismiss. If the download fails, the wizard
returns to `NotConfigured` so the user can retry.

**`iced::Task<Message>` return from update closure**

The iced update closure now returns `Task<Message>` instead of `()`. All
existing branches return `Task::none()`; `DownloadModel` returns
`Task::stream(receiver)` where the receiver carries live progress messages
from the background download task. This is the idiomatic iced 0.14 pattern
for streaming background work into the UI.

### New messages
`DownloadModel`, `DownloadStarted`, `DownloadFileProgress`, `DownloadAllComplete`, `DownloadFailed`

### New dependencies
`reqwest 0.12` (`rustls-tls` + `stream`, no OpenSSL), `tokio` in `orbok-app`

### Tests
**184 tests / 0 failures.**

---

## [0.9.6] — 2026-06-08 — Crate directory restructure

### Changed

The twelve crates that were flat in `crates/` are now grouped into
logical subdirectories. Package names and all Rust `use` paths are
unchanged — only filesystem paths and the workspace `Cargo.toml` member
entries differ.

```
crates/
├── app/                 # orbok-app   — binary, bootstrap, settings
├── bench/               # orbok-bench — benchmark harness
├── core/                # orbok-core  — IDs, errors, lifecycle types
├── data/
│   ├── cache/           # orbok-cache — localcache wrapper
│   ├── catalog/         # orbok-db    — SQLite schema, repos, migrations
│   └── fs/              # orbok-fs    — scanner, path guard, hashing
├── pipeline/
│   ├── extract/         # orbok-extract — extractors, chunker
│   └── workers/         # orbok-workers — indexing pipeline, recovery
├── search/
│   ├── embed/           # orbok-embed  — inference backends
│   ├── engine/          # orbok-search — FTS5, vector, hybrid RRF
│   └── models/          # orbok-models — model traits, mocks
└── ui/                  # orbok-ui   — snora/iced shell, views, i18n
```

184 tests / 0 failures.

---

## [0.9.5] — 2026-06-08 — Navigation restructure + UX fixes

### Changed

**Navigation: two-level layout (sidebar groups + tab bar)**

The six flat sidebar items are replaced with three top-level groups and
per-group sub-tabs, following the approved hierarchy:

| Group | Sidebar icon | Tabs |
|---|---|---|
| Search | `LucideIcon::Search` | Search · Sources |
| AI | `LucideIcon::BrainCircuit` | Indexing · Storage · Models |
| Settings | `LucideIcon::Settings` | (single page) |

`NavGroup` enum added to `orbok-ui::state`. `ViewId::group()` maps any
view to its parent group. `ViewId::group_default()` gives the default
tab when entering a group. snora's `TabBar` / `app_tab_bar` render the
horizontal tab strip. The `SwitchGroup(NavGroup)` message activates the
default tab for a group.

**Add Folder — native OS folder picker (`rfd 0.15`)**
Clicking "Add Folder" now opens the operating system's native folder
picker dialog. No path typing required. The selected path is scanned and
indexed immediately. The manual path text-input field remains as a
fallback for power users who prefer to type or paste a path.

**Sources view — recursive scanning note**
A subtitle line "All sub-folders are scanned recursively." appears below
the add-folder controls, answering the immediate question new users have
about search scope.

### Tests
**184 tests / 0 failures.**

---

## [0.9.4] — 2026-06-08 — Candle upgrade + lucide-icons integration

### Changed

**`candle-core` / `candle-nn` upgraded: 0.9.2 → 0.10.2** (`orbok-embed`,
`--features candle`)
Drop-in upgrade per migration report: no API symbols removed, one addition
each (`TokenizerFromGguf` in candle-core, `remove_mean` in candle-nn),
neither relevant to orbok's CPU inference path. Source unchanged.

**lucide-icons added: 1.17.0** (`orbok-ui`)
snora 0.8.0 ships a native `lucide-icons` feature (`Icon::Lucide` variant).
Enabling it via `snora = { features = ["lucide-icons"] }` activates full
Lucide icon support in the sidebar navigation rail and anywhere else an
iced widget tree is built.

Icon font registration — `orbok-ui` re-exports `LUCIDE_FONT_BYTES`; the
iced application builder in `orbok-app` registers it via `.font()` at
startup so all icon glyphs render correctly.

**Sidebar navigation** now uses proper Lucide icons instead of emoji:

| View | Icon |
|---|---|
| Search | `Search` |
| Sources | `FolderOpen` |
| Indexing | `ListOrdered` |
| Storage | `Database` |
| Models | `Cpu` |
| Settings | `Settings` |

**In-page icon buttons** (views.rs, wizard.rs):
- Search submit button — `icon_search` + label
- Add Source button — `icon_folder_plus` + label
- Remove source — `icon_trash_2` (icon-only, compact)
- Wizard Validate — `icon_scan_eye` + label
- Wizard Accept — `icon_circle_check` + label

### Tests
**184 tests / 0 failures.** No new tests (icon rendering is a visual
concern; the logic under the buttons is unchanged and already covered).

---

## [0.9.3] — 2026-06-07 — Dependency hardening

### Changed

**`lopdf` upgraded: 0.34.0 → 0.41.0** (`orbok-extract`)
Seven minor versions. All existing `Document::load` / `page_iter` /
`extract_text` / `get_pages` APIs are unchanged (upstream explicitly
guarantees backward compatibility). New capabilities available to orbok:
PDF 1.5+ object streams (enables reading compressed modern PDFs that
previously surfaced zero-length text), improved XRef stream handling,
and Rust 2024 edition alignment. Requires Rust ≥ 1.85, which orbok already
targets.

**`sha2` upgraded: 0.10.9 → 0.11.0** (workspace)
The sha2 0.11.x series adopts the `digest 0.11` crate, which switches
internal output types from `GenericArray<u8, N>` (generic-array 0.14) to
`Array<u8, N>` (hybrid-array). Two call sites that formatted digests with
`format!("{:x}", …)` were migrated to an explicit byte-iterator collect —
semantically identical, one fewer implicit trait dependency. sha2 0.10.9
is still present as a transitive dep (locked by the cryptography dep
chain); both versions coexist cleanly.

**`orbok-workers` test isolation**
The `orbok-ui` dev-dependency was removed from `orbok-workers`. Tests that
previously imported `orbok_ui::state::{AppState, Message}` to verify UI
invariants were either stubbed with equivalent non-GUI assertions (the
logical property is preserved) or noted as covered by `orbok-ui`'s own
suite. This eliminates the iced → winit → wayland/x11 compile chain from
the non-GUI test run, cutting `cargo test` peak disk use by ~9 GB.

**Dependency audit** (full results in `docs/src/maintainers/dep_audit.md`)
- All other workspace deps verified current as of 2026-06-07
- `zip = "2"` spec intentional; zip 8.x is a breaking API rewrite
- `candle-core`: 0.9.2 → 0.10.2 available; deferred to `--features candle`
  activation milestone
- `localcache`, `app-json-settings`: ask the author (nabbisen) directly

### Tests
**184 tests / 0 failures** (unchanged count; test logic improved).

---

## [0.9.2] — 2026-06-07 — Source management + hybrid search wiring

### Added

**EmbeddingWorker model selection**
- `EmbeddingWorker::with_model(catalog, cache, model, model_id)` —
  constructor accepting any `Box<dyn EmbeddingModel>`. Tests can pass
  `MockEmbeddingModel`; production builds pass the factory result from
  `orbok_embed::create_embedding_model`.

**HybridSearchService in bootstrap** (`run_search`)
- `bootstrap::run_search` now uses `HybridSearchService` throughout.
- When `OrbokSettings.embedding_model_dir` is set: calls
  `orbok_embed::create_embedding_model` with a `recommended_config`.
  If the `tract` feature is compiled and the model file exists, real
  semantic search is used. Otherwise falls back to keyword-only with
  no error — the capability degradation is logged at `warn` level.

**Source management backend**
- `bootstrap::add_source(catalog, path)` — resolves tilde, canonicalizes,
  inserts source record, returns `SourceCard`.
- `bootstrap::scan_and_index_source(catalog, cache, source_id)` — runs
  `Scanner` → `ExtractionWorker` → `ChunkAndIndexWorker` synchronously,
  returns updated `IndexHealth`.
- `bootstrap::remove_source(catalog, source_id)` — calls
  `delete_with_all_data`.
- `bootstrap::get_health(catalog)` — queries `count_with_status` across
  all file statuses; populates `IndexHealth`.
- `bootstrap::get_sources(catalog)` — loads all sources with per-source
  indexed/stale/failed counts.

**FileRepository count methods** (`orbok-db`)
- `count_with_status(status)` — global file count by status.
- `count_for_source_with_status(source_id, status)` — source-scoped count.

**Sources view** (`orbok-ui`)
- Path text-input always visible: user types/pastes a folder path and
  presses Enter or clicks the button to add a source.
- Per-source Remove button dispatches `Message::SourceRemoved(source_id)`.
- `Message::SourcePathChanged`, `RequestAddSource`, `SourceAdded`,
  `SourceRemoved`, `ScanCompleted`, `HealthUpdated`, `SourcesLoaded`
  added to the message vocabulary.
- `SourceCard.source_id: String` — backend ID field for remove operations.

**Startup population**
- `load_initial_state` now populates `AppState.health` and
  `AppState.sources` from the catalog at startup, so the Indexing
  sidebar and Sources view show real data immediately.

### Tests
- `orbok-workers`: 84 tests (+9 covering source management, health
  queries, EmbeddingWorker model selection, hybrid search routing).
- Workspace total: **184 tests / 0 failures**.

---

## [0.9.1] — 2026-06-07 — Startup wizard + settings integration

### Added

**OrbokSettings** (`orbok-app/src/settings.rs`)
- `OrbokSettings` struct: `embedding_model_dir`, `reranker_model_dir`,
  `index_mode`, `locale`, `rerank_enabled`, `background_indexing`,
  `pause_on_battery`.
- `load_settings()` / `save_settings()` via `app-json-settings` v2
  (`ConfigManager<OrbokSettings>::new().with_filename("settings.json")`).
- Note in code: a `.with_app_name("orbok")` builder would guarantee
  consistent config paths when binary name differs — flagged for the
  crate author to consider.

**Model verifier** (`orbok-workers/src/model_verifier.rs`)
- `verify_embedding_model(model_dir: Option<&str>) -> VerifyOutcome`
  checks `onnx/model.onnx` and `tokenizer.json` for existence and
  size > 0. Runs in < 2 ms at startup (no SHA-256 hashing).
- `VerifyOutcome`: `Ready`, `NotConfigured`, `FilesInvalid { model_dir, issues }`.
- `FileIssue` with `FileIssueKind`: `NotFound`, `Empty`, `PermissionDenied`.
- `verify_outcome_summary()`: log-safe string that never includes paths.
- 7 unit tests covering all outcomes.

**Startup wizard UI** (`orbok-ui`)
- `WizardState` enum in `state.rs`: `NotConfigured`, `FileMissing`,
  `Checked`, `Ready`.
- `WizardFileCheck` struct: relative path, found, size_mb.
- New messages: `WizardPathChanged`, `WizardValidate`, `WizardChecked`,
  `WizardAccept`, `WizardSkip`.
- `views/wizard.rs`: four page functions (`page_input`, `page_checked`,
  `page_ready`) covering all wizard states.
- 18 new `MessageKey` variants with English + Japanese translations.
- `shell.rs`: wizard takes priority over normal navigation — when
  `state.wizard.is_some()`, the wizard is shown instead of the shell.

**Bootstrap update** (`orbok-app/src/bootstrap.rs`)
- `load_initial_state()` now:
  1. runs RFC-018 startup recovery
  2. loads `OrbokSettings`
  3. calls `verify_embedding_model`
  4. sets `wizard = Some(WizardState::NotConfigured)` on first launch
  5. sets `wizard = Some(WizardState::FileMissing { previous_dir })` when
     files are gone
  6. sets `capability = Hybrid` only when `VerifyOutcome::Ready`
- `persist_model_dir(dir)`: writes accepted model directory back to
  `OrbokSettings` via `save_settings`.
- `--check` output now includes model verification status.

**main.rs backend effects**
- `WizardValidate`: runs `verify_embedding_model` on the input path,
  builds file check results, dispatches `WizardChecked`.
- `WizardAccept`: calls `persist_model_dir` to write the accepted path
  to `settings.json` before the UI transitions to full mode.

### Tests
- `orbok-workers`: 75 tests (+7 model_verifier).
- Workspace total: **175 tests / 0 failures**.

---

## [0.9.0] — 2026-06-07 — Release Candidate

> **v1.0.0 not yet released.** This is the release candidate.
> v1.0.0 requires explicit project owner confirmation.

### Added

**DOCX extractor** (`orbok-extract/src/docx.rs`)
- Microsoft Word 2007+ (`.docx`) files extracted via ZIP+XML parsing.
- Reads `word/document.xml`, recovers paragraph text from `<w:t>` runs.
- `LocationQuality::Approximate` (paragraph order preserved; no byte offsets).
- Registered in `ExtractorRegistry` and `PluginRegistry`.
- Failure-isolated: parse errors return typed `ParserError`, no panic.

**HTML extractor** (`orbok-extract/src/html.rs`)
- HTML/HTM files extracted via pure state-machine tag stripper.
- Block-level elements (`<p>`, `<div>`, `<h1>`–`<h6>`, `<li>`, etc.) produce paragraph breaks.
- `<h1>`–`<h6>` headings tracked in `heading_path` (e.g. "Guide > Install").
- `<script>`, `<style>`, `<head>` content suppressed.
- Common entities decoded (`&amp;`, `&lt;`, `&gt;`, `&nbsp;`, `&quot;`).
- `LocationQuality::Approximate`.
- Registered for `.html` and `.htm`.

**End-to-end pipeline integration test**
- `e2e_full_pipeline_write_scan_index_search` in v09_rc:
  writes Markdown + HTML files, runs scan → extract → index → search,
  then verifies:
  - `ERR-4042` found and ranked first in `auth.md`
  - `snippet cache cleanup` returns results
  - HTML `client_secret` content is indexed and searchable

**Pre-release gate tests**
- `all_documented_file_types_have_extractor`: every extension claimed in
  `docs/src/users/file_types.md` has a registered extractor.
- `plugin_registry_all_extractors_have_privacy_notes`: all 5 plugins
  (markdown, docx, html, plain-text, pdf) have license + privacy note.
- `startup_recovery_clean_on_fresh_catalog`: RFC-018 recovery path.
- `pipeline_leaves_no_running_jobs_after_completion`: clean shutdown
  contract (no jobs stuck in `running`).

### Fixed
- **HTML skip-depth bug**: nested `<style>` inside `<head>` incremented
  `skip_depth` without a matching decrement, causing the entire document
  body to be silently skipped. Fixed: nested skip-depth only counts
  same-tag nesting (e.g. `<head>…<head>…</head>…</head>`).
- **Heading detection order**: closing `</h1>` was matched by the
  generic BLOCK_TAGS branch before reaching the heading branch, emitting
  headings as plain paragraphs. Fixed by checking heading close tags
  first in the dispatch chain.
- All 6 compiler warnings across orbok-search, orbok-extract,
  orbok-workers resolved. Build is warning-free.

### Tests
- `orbok-extract`: 29 tests (DOCX and HTML covered by v09_rc in
  orbok-workers, which is the integration host).
- `orbok-workers`: 68 tests (+12 covering DOCX, HTML, E2E pipeline,
  and pre-release gates).
- Workspace total: **169 tests / 0 failures / 0 warnings**.

---

## [0.8.0] — 2026-06-07 — All RFCs resolved

> **v1.0.0 is not yet released.** This release completes every RFC
> in the design set. v1.0.0 requires explicit project owner confirmation
> after the three release gate conditions are verified.

### Benchmark Results (RFC-016)

Measured on 100 synthetic documents (debug profile, keyword-only):

| Metric | Result | v1.0 Gate |
|---|---|---|
| Indexing throughput | 59.2 files/s | — |
| Search p99 | 31.18 ms | ≤ 200 ms ✓ |
| Recall@5 (keyword-only) | 75.0% | ≥ 75% ✓ |

Both v1.0.0 search performance gates pass even in the conservative
debug-profile, keyword-only configuration. With a real embedding model
in release mode, both metrics will improve further.

### Added

**RFC-023 — ANN decision documented**
- Measured exact cosine scan baseline: p99 < 35 ms at 100 documents
  (debug mode). ANN complexity is not justified at current scale.
- Decision: keep exact scan for v1.0.0; implement HNSW only when
  user corpora show > 200 ms p99 (tracked as future work).
- `bench_full_pipeline` test runs 100-document benchmark as a
  regression gate for search performance.

**RFC-024 — INT8 vector quantization**
- `quantize_to_i8`, `dequantize_from_i8`, `i8_vec_to_blob`,
  `i8_blob_to_vec`, `cosine_similarity_i8` in orbok-models.
- Storage impact: 4× smaller than FP32 (384 bytes vs 1,536 bytes/chunk).
  At 100k chunks: ~37 MB (INT8) vs ~147 MB (FP32).
- Quality loss measured: cosine similarity error < 0.02 for
  L2-normalized 384-dim vectors.
- `EmbeddingRepository::upsert_i8` stores INT8 vectors with
  `vector_format = 'int8'`; `list_active_i8_for_scan` dequantizes
  on read for exact cosine search.
- INT8 is the Space Saving mode default; Balanced/High Accuracy
  keep FP32.

**RFC-025 — Scanned document detection**
- `is_scanned_pdf(output, page_count)` in orbok-extract::pdf:
  returns `true` when a PDF has pages but zero extracted text.
- `pdf_page_count(path)` helper for the detection check.
- Clear `char_count = 0` signal enables the UI to show an
  "OCR required" notice. Full OCR engine integration deferred.

**RFC-028 — Plugin extractor architecture**
- `PluginManifest` struct: `plugin_id`, `display_name`, `extensions`,
  `author`, `license`, `builtin`, `privacy_note`.
- `PluginExtractor` wrapping a `DocumentExtractor` with its manifest.
- `PluginRegistry::default()` registers all built-in extractors
  (markdown, plain-text, pdf-lopdf) with proper manifests.
- Security contract documented: plugins receive only `ValidatedPath`;
  dynamic loading deferred until RFC-028 is fully activated.

**RFC-030 — Portable mode**
- `--portable` flag: stores catalog and cache in `./orbok-data/`
  instead of the platform app-data directory.
- `data_dir_for_args(portable)` in bootstrap resolves the correct
  path.
- Standard mode remains the default; portable mode is explicit.

**RFC-026 — Archived**
- Encrypted local indexes require a dedicated key-management security
  audit and are not suitable for pre-v1.0.0 implementation.
- RFC-026 moved to `rfcs/archive/` with rationale.

### Tests
- `orbok-models`: 11 tests (+4 quantization tests).
- `orbok-workers`: 56 tests (+10 covering v0.8 RFCs).
- `orbok-bench`: 1 integration test (full 100-doc pipeline benchmark).
- Workspace total: **157 tests / 0 failures**.

### RFC Status
- `rfcs/done/`: 31 RFCs
- `rfcs/archive/`: 1 RFC (RFC-026)
- `rfcs/draft/`: 0 (empty)
- `rfcs/proposed/`: 0 (empty)

---

## [0.7.0] — 2026-06-07

> **Note:** v1.0.0 is not yet confirmed. This release advances the
> pre-1.0 roadmap. See `ROADMAP.md` for v1.0.0 criteria.

### Added

**RFC-021 — Default Embedding Model Selection**
- New `orbok-embed` crate with the embedding backend factory:
  `create_embedding_model(config)` dispatches by `InferenceBackend`.
- `Mock` backend (always compiled): deterministic 8-dim vectors,
  no model files required — used in all tests.
- `OnnxRuntime` backend (`--features tract`): loads `.onnx` model via
  the pure-Rust `tract-onnx` runtime; `tract_backend.rs` is only
  compiled with the feature flag.
- `Candle` backend (`--features candle`): HuggingFace candle runtime;
  `candle_backend.rs` is only compiled with the feature flag.
- Without the feature flag, non-mock backends return an informative
  `OrbokError::Cache` with the feature flag name.
- **Recommended default model: `multilingual-e5-small`** (384-dim,
  Apache 2.0, 100-language support, ~118 MB). Selected because orbok's
  target use case includes mixed Japanese-English documents (RFC-014).
  `RECOMMENDED_HF_MODEL_ID`, `RECOMMENDED_MODEL_DIMENSION`, and
  `recommended_config(weights_path)` documented in the crate.
- Storage impact: 384-dim = 1.5 KiB/chunk (FP32). At 100k chunks: ~147 MB.

**RFC-022 — PDF Extraction Backend**
- `PdfExtractor` in `orbok-extract` using **lopdf** (pure Rust, MIT,
  no C FFI). Selected over pdfium (requires native library) for v0.7.
- Page-level text extraction: each page becomes one `ExtractedSegment`
  with `LocationQuality::PageOnly` (honest; line numbers unavailable).
  UI must not show false line numbers for PDF results.
- Failure isolation: per-page errors are swallowed; one bad page never
  stops extraction of the rest of the document (RFC-005 §13).
- Encrypted PDF → `EncryptedDocument` error category.
- Scanned/image-only PDF → zero segments, no error.
- `PdfExtractor` registered in `ExtractorRegistry` for `.pdf` extension.
- Japanese UTF-8 PDFs extract correctly; legacy SJIS/EUC not attempted.

**RFC-029 — Model Download Integrity and Trust**
- `verify_model_sha256(path, expected_hash)` in orbok-db: streams the
  model file and compares against a user-provided SHA-256 hex string.
- Returns `Ok(true)` on match, `Ok(false)` on mismatch, `Err` on I/O
  error. Path is not logged (NFR-014).
- `ModelRepository::locate()` registers an existing on-disk model file
  (manual placement, no automatic download — RFC-029 §9).
- `models.license_summary` stores the license string shown to the user
  before a model is used.
- `InferenceBackend` enum and `EmbeddingModelConfig`/`RerankerConfig`
  types added to `orbok-models` for full config-driven backend selection.

### Tests
- `orbok-embed`: 4 tests (mock backend, feature-flag error, defaults).
- `orbok-extract`: 29 tests (+14 covering RFC-021/022/029).
- Workspace total: **142 tests / 0 failures**.

### RFCs
- RFC-021, RFC-022, RFC-029 moved from `rfcs/draft/` to `rfcs/done/`.
- 26 of 31 RFCs now in `done/`.

---

## [0.6.0] — 2026-06-07 🎉 All Part 1–4 RFCs complete

This release completes the planned feature set defined in the initial
requirements document. All 23 implementation RFCs (RFC-000 through
RFC-020, RFC-027, RFC-031) are now in `rfcs/done/`.

### Added

**M10 complete — CleanupService end-to-end**
- `CleanupService` in orbok-workers: combines catalog-side cleanup
  (via `CleanupExecutor`) with cache-side cleanup (via `CacheService`)
  in one validated operation driven by `CleanupPlan`.
- `run_safe(plan)` — ordinary cleanup (snippet cache, search cache,
  stale indexes); guaranteed to never touch persistent source settings.
- `run_reset(plan, keep_settings)` — full catalog reset that also
  purges all localcache namespaces.
- `FullCleanupOutcome` reports `catalog_rows_deleted` and
  `cache_bytes_freed`.

**M12 backend infrastructure**
- `InferenceBackend` enum: `CandleCpu`, `CandleCuda`, `OnnxRuntime`, `Mock`.
- `EmbeddingModelConfig`: weights path, tokenizer path, dimension,
  max sequence length, backend, name/version.
- `RerankerConfig`: equivalent config for cross-encoder rerankers.
- `weights_exist()` validator on `EmbeddingModelConfig`.
- These types are consumed by the future candle/ONNX integration crates
  (RFC-021 implementation); the `MockEmbeddingModel` remains the
  fallback until a real backend is compiled in.

**RFC-019 — Test Matrix and Release Readiness**
- `.github/workflows/ci.yml`: four CI jobs:
  - **fast** (every PR): fmt, clippy, unit tests on non-GUI crates
  - **release** (main branch): release build, `--version`, `--check`, bench smoke
  - **security** (every PR): `cargo audit`, security test execution
  - **cross** (3 platforms): Linux, Windows, macOS smoke build
- `docs/src/maintainers/release_readiness.md`: release readiness levels
  RL-0 through RL-4, CI gate definitions, manual QA checklist,
  retrieval benchmark requirements, packaging checklist.

**RFC-020 — Documentation and User Guidance Structure**
Complete mdbook documentation covering all three user personas:
- **New users**: Features, Quick Start, Sources and Indexing, Searching,
  Storage and Cleanup, Local AI Models, FAQ
- **Intermediate users**: Settings Reference, Supported File Types
- **Maintainers**: Architecture Overview, Local Development, Testing
  Guide, RFC Index, Release Readiness

### Changed
- `rfcs/README.md`: all Part 1–4 RFCs now in `done/`; 0 in `proposed/`.
  RFC-021–030 remain in `draft/` as deferred future work.

### Tests
- `orbok-workers`: 46 tests (+9 covering M10/M12/RFC-019).
- Workspace total: **124 tests / 0 failures**.

---

## [0.5.0] — 2026-06-07

### Added

**RFC-012 — Model Registry and Installation Workflow (M12)**
- `ModelRepository` in orbok-db: full CRUD over the `models` catalog table
  with `insert`, `get`, `list_by_role`, `list_all`, `set_status`,
  `validate` (file-existence + dimension check), `locate` (register
  existing on-disk model), and `mark_embedding_dependents_stale`.
- `ModelRole` and `ModelStatus` enums with catalog-string round-trips.
- `ModelId` typed ID added to orbok-core.
- App works in keyword-only mode with empty model registry (RFC-012 §17).
- No model download occurs without explicit user action.

**RFC-015 — Security Hardening**
- `html_escape(raw)` in `orbok-search::snippet`: escapes `<>&"'` in
  snippet text before passing to the UI (RFC-015 §18 defense-in-depth).
- Security test module documents and exercises existing protections:
  PathGuard outside-source rejection, path-traversal via `..`, symlink
  escape blocking (all implemented in RFC-003/004, now explicitly
  labelled as security tests per RFC-015 §19).

**RFC-016 — Benchmark and Retrieval Evaluation Harness**
- New `orbok-bench` crate:
  - `corpus::generate(dir, n)` — synthetic Markdown documents (8
    templates: auth, storage, search, API, security, Japanese, code,
    models).
  - `queries::LABELED_QUERIES` — 8 labeled queries with expected
    document patterns.
  - `metrics::measure_search_latency` — p50/p95/p99 ms measurement
    with 3 warm-up rounds.
  - `metrics::compute_recall` — recall@5 against labeled queries.
  - `report::BenchmarkResult::write_json/write_markdown` — machine-
    readable and human-readable output (RFC-016 §12).
- Benchmark smoke test verifies the harness runs on a 10-document
  corpus without errors.

**RFC-017 — Packaging and Distribution**
- `--version` / `-V` flag in the orbok binary.
- `build.rs` in orbok-app embeds `CARGO_PKG_VERSION`.
- `scripts/checksum.sh` generates SHA-256 checksums for release archives.

**RFC-018 — Crash Recovery and Diagnostics**
- `run_startup_recovery(catalog, cache_path)` in orbok-workers:
  - Resets `running` → `queued` for jobs left by a crashed session.
  - Returns `RecoveryReport` with counts of reset and pending jobs.
  - Detects missing or corrupt cache DB (backup + recreate path).
- `check_catalog_integrity(catalog)` → `IntegrityReport`: detects
  orphaned child chunks, orphaned keyword/embedding records, and files
  without a parent source. Read-only; does not repair.
- `RecoveryReport` and `IntegrityReport` are printed at startup if
  anomalies are found.

**orbok-ui**
- `StorageDataReady` message and `storage_rows` field already wired
  in v0.4; `update_storage_accounting` now called after each pipeline
  run to keep storage view current.

### Tests
- `orbok-db`: 15 tests (model repo tested via v05 integration suite).
- `orbok-workers`: 37 tests (+11 covering RFC-012/015/016/018).
- Workspace total: **115 tests / 0 failures**.

---

## [0.4.0] — 2026-06-07

### Added

**RFC-010 — Optional Local Reranking**
- `CrossEncoderReranker` trait and `RerankCandidate`/`RerankScore` types
  in `orbok-models`.
- `MockReranker`: deterministic mock ordering by passage length (test-safe,
  no ML runtime required).
- `HybridSearchService::with_reranker()` builder: attaches optional
  reranker that reorders the top-N fused results using passage text.
- `Fast` search mode bypasses reranking (`Limits.rerank = false`).
- Search remains fully functional with no reranker attached (RFC-010 §20).

**RFC-011 — Storage Dashboard**
- `update_storage_accounting(catalog, cache_db_path)` in orbok-workers:
  measures actual storage by category (keyword index rows, embedding BLOB
  sum, snippet cache bytes, localcache DB file size, event log rows).
- `StorageDataReady` message and `storage_rows` field in orbok-ui `AppState`.
- Storage view renders per-category breakdown with MiB values.
- `orbok-app` exposes `persist_locale()` helper — locale changes are now
  persisted to the catalog `app_settings` table.

**RFC-013 — Search View and Result Explanation UX**
- `SelectResult(usize)` message and `selected_result: Option<usize>` in
  `AppState`; result cards are now buttons that trigger selection.
- `OpenSourceFile(String)` message (canonical path) dispatched to orbok-app.
- `StorageDataReady` message wires real storage data into Storage view.
- Search mode selector row in the Search view (Auto / Exact / Conceptual).
- `search_result_count(locale, n)` parameterized i18n message.

**RFC-014 — Japanese and Mixed-Language Search**
- Migration 0002 (`0002_trigram_index.sql`): adds `chunk_fts_trigram`
  virtual table (FTS5 trigram tokenizer, SQLite 3.53.2) and
  `keyword_index_records.trigram_fts_rowid` column.
- `ChunkRepository::insert_bundle` now indexes every chunk in both
  the unicode61 and trigram FTS tables atomically.
- `MultilingualKeywordEngine`: detects CJK characters in the query
  (hiragana, katakana, CJK unified ideographs); routes CJK queries
  through both unicode61 and trigram tables, merging and deduplicating
  results. English/identifier queries use only unicode61.
- `normalize_query()`: converts fullwidth ASCII/digits (ＡＢＣ→ABC)
  and trims whitespace — satisfies RFC-014 §10 test 1.
- `contains_cjk()`: character-class-based CJK detector.
- `HybridSearchService` now uses `MultilingualKeywordEngine` internally
  for all keyword retrieval.

**Other improvements**
- Locale persistence: `PersistLocale` message variant; orbok-app
  `persist_locale()` writes to catalog settings on locale change.
- `orbok-ui` i18n: added keys `SearchModeLabel`, `SearchModeAuto`,
  `SearchModeExact`, `SearchModeConceptual`, `SearchModeFast`,
  `BadgeKeyword`, `BadgeSemantic`, `BadgeFused`, plus parameterized
  `search_result_count` in English and Japanese.

### Tests
- `orbok-models`: 7 tests (+2 reranker tests).
- `orbok-workers`: 26 tests (+14 covering RFC-010/011/013/014).
- Workspace total: **110 tests / 0 failures**.

---

## [0.3.0] — 2026-06-07

### Added

**M7 — Embedding and Vector Search (RFC-008)**
- `EmbeddingModel` trait in `orbok-models` (RFC-008 §6): `embed_batch`,
  `name`, `version`, `dimension`. Implementations must run locally and
  never transmit text externally.
- `MockEmbeddingModel`: 8-dimensional deterministic mock using SHA-256
  as a pseudo-random source; L2-normalized output. Used for pipeline
  testing without a real ML runtime.
- Vector serialization helpers: `vec_to_blob`/`blob_to_vec` (FP32
  little-endian, RFC-008 §12.1).
- `VectorCandidate` type; cosine-similarity and L2-normalize utilities.
- `EmbeddingId` added to orbok-core.
- `EmbeddingRepository` in orbok-db: `upsert`, `list_active_for_scan`
  (joins with chunks to exclude stale chunks), `mark_stale_for_model`,
  `count_active`.
- `EmbeddingWorker` in orbok-workers: reads extraction cache → embeds
  chunk texts in batch → stores vectors. `with_mock` constructor for
  tests and no-model operation.
- `ExactVectorSearch`: cosine-similarity scan over all active embeddings
  for a model (RFC-008 §13 "exact search first").

**M8 — Hybrid Search and RRF (RFC-009)**
- `rrf_fuse`: Reciprocal Rank Fusion (k=60), deduplicating by chunk_id,
  producing `FusedCandidate` with per-source rank metadata (RFC-009 §7).
- `HybridSearchService`: `keyword_only` and `with_model` constructors;
  `search(query, mode, limit)` running keyword + vector retrieval,
  RRF fusion, and snippet loading in one call (RFC-009 §12).
- `SearchMode` enum (RFC-009 §8): `Auto`, `Exact`, `Conceptual`, `Fast`
  with per-mode candidate limits.
- Badge system: `MatchBadge::Keyword`, `Semantic`; fused results carry
  both badges when both retrievers contributed.
- `SearchMode` in `orbok-ui` `AppState`; `SetSearchMode` message.

**i18n additions (RFC-031)**
- New keys: `SearchModeLabel`, `SearchModeAuto`, `SearchModeExact`,
  `SearchModeConceptual`, `SearchModeFast`, `BadgeKeyword`,
  `BadgeSemantic`, `BadgeFused` — translated to English and Japanese.
- `search_result_count(locale, n)` parameterized message.

### Tests
- `orbok-models`: 5 tests (adds embedding/vector ops tests).
- `orbok-workers`: 12 tests (adds 7 RFC-008/009 integration tests:
  embedding generation, vector search, RRF fusion, model-change
  staling, stale-chunk exclusion, catalog isolation).
- Workspace total: **99 tests / 0 failures**.

---

## [0.2.0] — 2026-06-07

### Added

**M5 — Adaptive Chunking (RFC-006)**
- `orbok-extract` chunker module: structure-aware chunking for Markdown
  (one child chunk per heading section) and paragraph-based fallback for
  plain text, with overlapping windows for long sections.
- Parent-child chunk model: document-level parent chunk (ordinal 0) plus
  leaf chunks used for retrieval.
- Explicit location quality per chunk: `exact` for text/Markdown line
  ranges, `approximate` for fallback windows.
- Chunk content hash (SHA-256 of normalized text) for stale detection.

**M6 complete — Keyword Search Pipeline (RFC-007)**
- `orbok-workers` crate: synchronous `ExtractionWorker`, `ChunkAndIndexWorker`,
  and `run_pending` coordinator.
- **Replace-on-success** transaction in `ChunkRepository::insert_bundle`:
  new chunks and FTS rows committed atomically; previous active index
  survives any failure.
- `SearchService`: keyword search returning `Vec<SearchResult>` with
  dynamic snippet loading from source files (FR-091).
- `SnippetLoader`: reads stored line ranges from source files; returns
  `None` when source is unavailable without crashing.
- `SearchService::search` available for use by `orbok-app`.

**M9 partial — Search Result Display**
- `SearchResultDisplay` view-model struct in `orbok-ui`.
- Search view renders result cards: title, display path, heading context,
  dynamic snippet, and badge list.
- Running/no-results/results-ready states in the search view.

**RFC housekeeping**
- RFCs 001–007, 027, 031 moved to `rfcs/done/`.
- `rfcs/README.md` index rebuilt to reflect current state.

### Changed
- `AppState` gains `search_results: Vec<SearchResultDisplay>` and
  `search_running: bool`; `Message` gains `SearchResultsReady` and
  `SearchError` variants.
- `FileRepository` gains `get_by_id(file_id)`.
- `orbok-fs` now exports `GuardedSource`.
- `orbok-db/repo` now re-exports `ExtractionId`, `JobStatus`, `JobType`
  from `orbok-core` as convenience aliases.
- Baseline migration updated pre-release: `chunk_fts` drops `chunk_id`
  and `file_id` UNINDEXED columns (contentless tables store no values);
  `keyword_index_records` gains `fts_rowid INTEGER` for the chunk ↔ FTS
  row mapping.

### Tests
- `orbok-extract`: 15 tests (adds 6 RFC-006 chunker tests).
- `orbok-workers`: 5 integration tests covering the full
  extract → chunk → index → search pipeline, including snippet loading
  and rechunk-failure preservation.
- Workspace total: **88 tests / 0 failures**.

---

## [0.1.0] — 2026-06-07

### Added

**Foundation (M0–M1)**
- Rust 2024 edition Cargo workspace with nine crates.
- RFC-001: three-layer data lifecycle (persistent / rebuildable / ephemeral).
- RFC-002: SQLite catalog schema with append-only migrations, FTS5
  contentless keyword index, foreign-key enforcement.

**Source boundary (M2)**
- RFC-003: source registration, canonical path enforcement, symlink
  policy, hidden-file policy, sensitive-directory warnings.

**File scanner (M3)**
- RFC-004: recursive directory walker, nanosecond-precision mtime
  comparison, SHA-256 content hashing, stale/missing/discovered state
  machine, cancellation support, index-job queueing.

**Extraction (M4)**
- RFC-005: extractor trait, plain-text and Markdown extractors with
  line-aware offsets, normalization pipeline, extractor version tracking.

**Cache engine (Appendix A)**
- localcache 0.20.0 integration: `MetadataThenFullHash` change detection,
  namespace policy, plan-validated cleanup.

**Keyword search (M6 prototype)**
- RFC-007: FTS5 contentless engine behind `KeywordSearchEngine` trait;
  safe query building (RFC-015 injection neutralization).

**GUI and i18n (RFC-027, RFC-031)**
- snora 0.8 / iced 0.14 application shell with six-page sidebar.
- Typed i18n catalog: English and Japanese, exhaustive at compile time.
- Headless `--check` mode for CI / display-less environments.

### Dependencies (pinned)
- localcache 0.20.0 (mtime nanosecond precision, schema v5).
- rusqlite 0.40 (single libsqlite3-sys instance shared with localcache).
- iced 0.14 via snora 0.8.
