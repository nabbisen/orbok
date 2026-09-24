# Closure Record — RFC-011: Storage Dashboard and Cleanup UX

**RFC:** [011](../done/011-storage-dashboard-and-cleanup-ux.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** Task 081 (the Storage page: categories, measured numbers,
`localcache` accounting), Task 083 (verification of the remaining criteria; it
returned the RFC to `accepted/`), Task 086 (§9a: the reset confirmation),
Task 099 (`DeleteKeywordIndex` / `DeleteVectorIndex`: executor arms, Advanced
view Storage buttons, rebuild marking), Task 099's follow-up (this record).
**Transcribed, not re-derived**, from the evidence those tasks recorded. None
of Tasks 081, 083, 086 or 099 is git-tracked (RFC-063 §5): they are
`.git-exclude/tasks/dev-team/…` and `.git-exclude/review-request/259`, `261`,
`264`, `277`, and Review 277 (`.git-exclude/reviewed/277-…`) carries the
architect's own run for criterion 5.

---

## §14 acceptance criteria

### 1. Storage Dashboard shows all required categories.

→ what was run: `never_measured_shows_the_empty_state_not_a_zero` and the
`StorageCategory::ALL`-driven render tests
(`crates/ui/src/tests/task081_storage_page.rs`).
→ what was observed: all eight categories render in Advanced view; the ordinary
view groups them into three lines (Search data, Models, Temporary previews).
→ where verified: `cargo test -p orbok-ui task081`.

### 2. Safe cleanup never deletes persistent catalog data.

→ what was run: `cleanup_service_safe_preserves_sources`
(`crates/pipeline/workers/src/tests/v06_features.rs`): a `ClearSnippetCache`
plan through `CleanupService::run_safe`.
→ what was observed: the snippet row is deleted; `SourceRepository::list()` is
non-empty afterwards. `CleanupExecutor::run_safe` refuses any plan whose
`affected_classes` includes `PersistentCatalog` before touching a row
(`plan.assert_safe_for_ordinary_cleanup()`).
→ where verified: `cargo test -p orbok-workers --lib cleanup_service_safe_preserves_sources`.

### 3. Cleanup plan is generated before cleanup execution.

→ what was run: `cleanup_is_plan_driven_and_safe`
(`crates/data/cache/src/tests.rs`).
→ what was observed: a `ResetCatalog` plan is rejected by
`CacheService::run_safe_cleanup`, and a safe plan built the same way runs.
`run_safe`, `run_reset_catalog` and `run_safe_cleanup` each take `&CleanupPlan`,
never a bare `CleanupAction`, so nothing executes without a plan object.
→ where verified: `cargo test -p orbok-cache --lib cleanup_is_plan_driven_and_safe`.

### 4. Source files are never deleted by cleanup.

→ what was run: `cleanup_service_reset_removes_sources_not_files`
(`crates/pipeline/workers/src/tests/v06_features.rs`): a real file on disk,
`CleanupService::run_reset`, the most destructive action.
→ what was observed: the file exists before and after; every catalog reference
to it is gone.
→ where verified: `cargo test -p orbok-workers --lib cleanup_service_reset_removes_sources_not_files`.

### 5. Deleting semantic index marks rebuild required.

→ what was run (test): `delete_vector_index_removes_embeddings_and_keeps_keyword_data`
(`crates/data/db/src/tests/task099_rebuild_index.rs`) and
`deleting_the_vector_index_and_draining_the_scheduler_makes_search_find_it_again`
(`crates/app/src/wired_application_tests.rs`, `#[ignore]`, needs the real model).
→ what was observed: embeddings are gone, keyword data, chunks and files are
untouched, `sources` and `app_settings` are unchanged column for column, and
one `embedding` job is queued per affected file
(`files_marked_for_rebuild == 5`).
→ what was run, by the architect, with a real model: the end-to-end test with
`RFC013_MODEL_DIR` pointing at a copy of the real `multilingual-e5-small`,
release build. Passes in 2.48 s: the baseline finds the file; right after the
delete nothing finds it (the control); after the drain it is found again.
The mutation its doc comment names, replacing the `enqueue_embedding_backfill`
call in `CleanupExecutor::delete_vector_index` with `0`, makes it fail at
`wired_application_tests.rs:2926` ("picked up the rebuild-marking"), and the
file was restored byte for byte (Review 277 §1).
→ where verified: `cargo test -p orbok-db task099_rebuild`; the ignored test
with `--ignored`.

### 6. Deleting exact index marks rebuild required.

→ what was run: `delete_keyword_index_removes_keyword_data_and_keeps_embeddings`
and `delete_keyword_index_backfill_is_idempotent` (db), and
`deleting_the_keyword_index_and_draining_the_scheduler_makes_search_find_it_again`
and `delete_keyword_index_marks_files_for_rebuild_exactly_once` (wired,
`crates/app/src/wired_application_tests.rs`).
→ what was observed: `keyword_index_records`, `chunk_fts` and
`chunk_fts_trigram` are empty afterwards; embeddings, chunks and files and the
`sources` and `app_settings` rows are untouched; one `extract` job per file is
queued, once. End to end: after the delete keyword search finds nothing, after
draining the scheduler it finds the file again. (Task 102 changes the
mechanism, not this criterion: the marking stays.)
→ where verified: `cargo test -p orbok-db task099_rebuild`,
`cargo test -p orbok --bin orbok deleting_the_keyword_index`.

### 7. Reset catalog requires strong typed confirmation.

→ amended by §9a (Task 086, 2026-09-23): the owner decided the RFC changes, not
the product. The confirmation is Task 062's dialog (title, warning, Cancel, a
danger button); nothing is typed.
→ what was run: `the_reset_confirmation_renders_cancel_and_the_danger_button`
and `escape_cancels_the_reset_confirmation_and_resets_nothing`
(`crates/ui/src/tests`), plus Task 069's
`on_its_own_view_each_confirmation_is_confirmed_by_enter`,
`switching_view_closes_every_confirmation_and_enter_cannot_confirm_it` and
`a_wizard_over_an_open_confirmation_never_confirms_it`.
→ what was observed: the dialog renders Cancel and the danger button; Escape
cancels and resets nothing; Enter confirms only while it is visible; switching
view closes it. Each was proven by breaking the code it checks (Review Request
264).
→ where verified: `cargo test -p orbok-ui task062 task069`.

### 8. `localcache` stats appear in storage accounting.

→ what was run: `measuring_storage_reports_real_numbers_that_match_independent_reads`
and `temporary_extraction_counts_retired_namespace_rows_too`
(`crates/app/src/wired_application_tests.rs`).
→ what was observed: `CacheService::usage()` is called every time the page is
measured (before Task 081 nothing called it outside a measurement test); the
retired-namespace rows are counted too.
→ where verified: `cargo test -p orbok --bin orbok measuring_storage`.

### 9. Text-bearing caches can be deleted.

→ what was run: `the_extraction_number_drops_after_clearing_extracted_text`
(`crates/app/src/wired_application_tests.rs`): a real file through the full
pipeline, the real `TemporaryExtraction` measurement, then
`bootstrap::clean_temporary_extraction` (the function behind Storage's "Clear
extracted text" button), then the measurement again.
→ what was observed: before > 0 (so it cannot pass vacuously), after 0.
→ where verified: `cargo test -p orbok --bin orbok the_extraction_number_drops_after_clearing_extracted_text`.

### 10. Model files are shown separately from indexes.

→ what was run: the `StorageCategory::ALL`-driven render tests
(`crates/ui/src/tests/task081_storage_page.rs`).
→ what was observed: `ModelFiles` is its own category, in the ordinary view as
its own "Models" line, apart from the search-data line.
→ where verified: `cargo test -p orbok-ui task081`.

---

## Criteria not met, and why RFC-011 closes anyway

None. The RFC's own text changed for criterion 7 (§9a) by owner decision; that
is an amendment, not an unmet criterion.
