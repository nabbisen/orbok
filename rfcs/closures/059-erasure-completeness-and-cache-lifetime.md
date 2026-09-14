# Closure Record — RFC-059: Erasure Completeness and Cache Lifetime

**RFC:** [059](../done/059-erasure-completeness-and-cache-lifetime.md), amended
2026-09-12 (Amendment 1, §2a: a fourth erasure site, the write-time cache cap
withdrawn, criterion 6 re-worded, criteria 8/9 added) and 2026-09-13
(Amendment 2, §2b: owner decisions ratified, criterion 5 re-worded, the size
bound moved to scheduler idle, criterion 10 added). Moved to `done/` in the
commit that adds criterion 10's evidence, per
`rfcs/handoffs/HANDOFF-059-slice6-idle-time-cache-bound.md` §5 ("with 1–10 all
evidenced, RFC-059 moves to `done/` in the same commit").
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-059-erasure-completeness-and-cache-lifetime.md`
(Slices 1-5) and `rfcs/handoffs/HANDOFF-059-slice6-idle-time-cache-bound.md`
(Slice 6); reviewed in Reviews 213, 214, 215 and 216
(`.git-exclude/reviewed/213-...` through `216-...`) -- plain text, not links:
none is git-tracked (RFC-063 §5), so a link resolves to nothing on any
checkout outside this working copy. Commits:
`5754b54` (original five slices),
`50d7763`/`d995403` (closure-record/gate follow-ups), `44ab2c9` (Review 213
§2 Critical + §3 High fixes), `5771740` (macOS test fix), `9240145` (Review
214 §2 required change), the Review 214 §4 owner-decision commit,
`58076f8` (closure-record link fix), `18f804c` (Amendment 2), and the Slice 6
commit landing this record's update. Every "where verified" line below names
the commit it ran against when it is not the current HEAD.
**Transcribed, not re-derived**, from the handoffs' own slice structure,
Reviews 213-216's own findings, and this implementation's mutation-tested
observations below.

---

## §10 acceptance criteria

### 1. With a corpus indexed containing a distinctive term, invoking Reset and then querying the trigram path for that term returns no rows -- verified against `chunk_fts_trigram` directly, not through the search API.

→ what was run: `reset_clears_the_trigram_index_queried_directly`
(`crates/pipeline/workers/src/tests/rfc059_reset_erasure.rs`) -- indexes a
real document containing the Japanese term "誤り訂正符号" through the real
extraction/chunk/index pipeline, confirms `chunk_fts_trigram MATCH` finds it,
runs `CleanupService::run_reset`, re-queries `chunk_fts_trigram` directly.
→ what was observed: PASS after `run_reset_catalog`
(`crates/data/db/src/repo/cleanup.rs`) gained
`INSERT INTO chunk_fts_trigram(chunk_fts_trigram) VALUES('delete-all')`
beside the pre-existing `chunk_fts` one. Confirmed failing before this fix:
reverting the added line left the term matching after Reset.
→ where verified: `cargo test -p orbok-workers reset_clears_the_trigram_index_queried_directly`.

### 2. With the same corpus, after Reset, no extraction-cache entry for any indexed file is retrievable through `CacheService`.

→ what was run: `reset_clears_the_extraction_cache_through_cache_service`
(same file) -- confirms `CacheService::get_fresh` finds the entry and
`engine.keys(None)` is non-empty before Reset, then confirms both are
empty/`None` after.
→ what was observed: PASS after `purge_all_cache_namespaces`
(`crates/pipeline/workers/src/cleanup_service.rs`) was rewritten to actually
enumerate (`engine.keys(None)`) and remove (`engine.remove(key)`) every
namespace's entries, then `shrink_database()`, reporting the count instead of
discarding it. Confirmed failing before this fix: the prior implementation's
three `let _ = ...` maintenance calls (`purge_stale_versions`,
`cleanup_missing_files`, `cleanup_expired` with no TTL configured) matched
nothing, and the test's post-Reset assertions failed.
→ where verified: `cargo test -p orbok-workers reset_clears_the_extraction_cache_through_cache_service`.

### 3. After Reset, `settings.json` and the installed model artifacts are byte-identical to their pre-Reset state.

→ what was run: `reset_never_touches_settings_or_model_artifacts` (same file)
-- hashes both files before and after a full reset.
→ what was observed: PASS, unchanged by this RFC's work -- neither file is
ever referenced by `CleanupExecutor` or `CleanupService` by construction, so
this is a guard against this RFC's own fixes reaching outside their scope,
not a defect this RFC found.
→ where verified: `cargo test -p orbok-workers reset_never_touches_settings_or_model_artifacts`.

### 4. Re-indexing one file twice leaves `count(chunk_fts_trigram)` unchanged, and both invariants in §6 hold after each of the four operations listed there.

→ what was run: `erasure_invariant_holds_after_all_five_operations`
(originally `..._all_four_operations`; renamed when Amendment 1 added
Remove folder as a fifth operation to the same test -- see criterion 8)
(`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`) -- inserts,
re-indexes with a new extraction (asserting `chunk_fts_count == 1` exactly,
not merely the two invariants' equality, since equality alone cannot
distinguish "cleaned up" from "both sides grew together and stayed
matched"), calls `Fts5KeywordEngine::delete` directly, calls
`CleanupExecutor::run_safe(RemoveReplacedStaleIndexes)`, calls
`run_reset_catalog` -- asserting `count(chunk_fts) == count(keyword_index_records)`
and `count(chunk_fts_trigram) == count(keyword_index_records WHERE
trigram_fts_rowid IS NOT NULL)` after each. A separate untouched control
file's chunk is asserted to survive every step but Reset, proving the fixes
are scoped, not blunt.
→ what was observed: PASS after three fixes: (a) `ChunkRepository::insert_bundle`
(`crates/data/db/src/repo/chunks.rs`) now deletes the superseded generation's
`chunk_fts`/`chunk_fts_trigram`/`keyword_index_records` rows (keyed by
`file_id`, addressed via §6's own "Prerequisite, and it is not optional" --
`Fts5KeywordEngine::index`, the RFC's originally named fix target, has no
production caller and re-indexing never reaches it, since `insert_bundle`
mints a fresh `chunk_id` on every call); (b) `Fts5KeywordEngine::delete`
(`crates/search/engine/src/fts5.rs`) now deletes the trigram row alongside
the unicode61 row before the mapping row; (c) `remove_replaced_stale_indexes`
(`crates/data/db/src/repo/cleanup.rs`) now deletes both FTS tables' rows
before letting the pre-existing `chunks` cascade run. Confirmed failing
before each fix individually (each reverted in turn, the test failed with
the expected count mismatch, then restored byte-identical). One test-only
discovery beyond the handoff's own scope: `remove_replaced_stale_indexes`'s
own fix is never independently exercised by this test, because (a) already
cleans up before it runs in every reachable production scenario -- see
criterion 6 below for the isolated test this produced.
→ where verified: `cargo test -p orbok-search erasure_invariant_holds_after_all_five_operations`.

### 5. *(Re-worded by Amendment 2.)* With the extraction cache holding entries of any age -- including one written seconds ago -- running Clear extracted text from the Storage view leaves no entry retrievable through `CacheService` and none listed by `keys(None)`. No notice reports an amount of space freed.

→ history: the original wording required entries "older than the configured
TTL" and "a non-zero byte reclaim". Review 214 §4 Q1 (owner decision
2026-09-12, ratified in Review 216) made the button an outright erase, and
Amendment 1 §2a.3 had already established that the reclaim figure is a
convention, not a measurement -- so Amendment 2 re-worded the criterion to
both. Stage 1 (commit `5754b54`) had found and fixed two production bugs in
`CleanupService::run_cache_side`'s `ClearTemporaryExtraction` branch
(neither `cleanup_expired()` nor `shrink_database()` was called); the erase
decision superseded both calls, and they were removed rather than left dead.

→ what was run (erasure half): `clear_extracted_text_leaves_no_fresh_entry_retrievable_through_cache_service`
(`crates/pipeline/workers/src/tests/rfc059_reset_erasure.rs`) -- indexes a
real file through the real pipeline (so the entry is seconds old, well
inside the 90-day TTL), confirms `CacheService::get_fresh` finds it, runs
`CleanupService::run_safe(ClearTemporaryExtraction)` -- the path
`ProfileCache::run_safe_cleanup` (`crates/app/src/runtime_storage.rs`) calls
-- then asserts `get_fresh` returns `None` **and** `keys(None)` is empty,
the criterion's two named observations exactly. The earlier
`clear_temporary_extraction_erases_the_namespace_even_when_nothing_has_expired`
(`crates/pipeline/workers/src/tests/rfc059_cache_lifetime.rs`) is kept; it
counts entries rather than going through `CacheService`.
→ what was run (notice half): `cleanup_notices_never_claim_space_was_freed`
(`crates/ui/src/tests/notice.rs`) -- for the four Safe-cleanup notices, in
both locales, asserts title and body contain none of "free", "space",
"byte", "kb", "mb", "gb", 「空き」, 「容量」, 「解放」.
→ what was observed: both PASS. Confirmed via mutation: replacing
`erase_engine_namespace(&engine)?` in the `ClearTemporaryExtraction` branch
with a no-op failed the erasure test at the `get_fresh` assertion;
restoring the pre-Q2 English body "Freed up space. Your files are
untouched." failed the notice test (`PreviewsCleared in En claims space was
freed`). Both files restored byte-identical.
→ where verified: `cargo test -p orbok-workers clear_extracted_text_leaves_no_fresh_entry_retrievable_through_cache_service`;
`cargo test -p orbok-ui cleanup_notices_never_claim_space_was_freed`.

TTL value: `OrbokCacheNamespace::default_engine_options`
(`crates/data/cache/src/namespace.rs`) sets `ExtractSegments` to a 90-day
write-time TTL, still real -- `localcache`'s `get_if_fresh` treats an entry
older than the TTL as a miss on every read regardless of any cleanup
action, forcing fresh extraction. Measurement backing the cap value:
`measure_extraction_cache_usage_against_the_rfcs_corpus`
(`crates/app/src/rfc059_cache_measurement.rs`, `#[ignore]`d, one-time) --
this project's own `rfcs/` tree (110 real markdown files) indexed through
the real hosted scheduler, measured via `CacheService::usage`: **110
entries, 595,247 payload bytes, ~5,411 bytes/entry**. The 20,000-entry cap
is enforced at scheduler idle -- criterion 10.

### 6. *(Re-worded by Amendment 1 §2a.3.)* After an ordinary re-index, both §6 invariants hold and Remove replaced stale indexes reports zero FTS rows reclaimed. After a file goes missing and returns with changed content, the same action reports a non-zero FTS-row reclaim and both invariants hold afterwards. No reported figure is presented to the user as bytes freed.

→ history: this criterion originally read "...after a re-index reports a
byte reclaim greater than zero..." The first implementation pass (commit
`5754b54`) found this literally false and disclosed it rather than forcing
a pass: §6's own "Prerequisite, and it is not optional" requires
`insert_bundle` to delete the superseded generation's
`chunk_fts`/`chunk_fts_trigram`/`keyword_index_records` rows **at replace
time**, so by the time `remove_replaced_stale_indexes` runs after an
*ordinary* re-index, those tables already hold nothing for it to find --
the reclaim happened earlier, correctly. Review 213 §5 corrected a second
claim in the same pass: the "defense-in-depth" scenario used to exercise
this action's own fix independently is not synthetic-only -- a file that
goes **missing** (`deactivate_for_missing_files` marks its chunks stale,
FTS rows kept intact on purpose, for reactivation) and then **returns with
changed content** reaches exactly that state, since `insert_bundle`'s
delete targets `chunk_status = 'active'` siblings only and never touches
the missing generation's stale row. Amendment 1 re-worded the criterion
around that real path and dropped the "reduces the on-disk keyword-index
size" clause: nothing VACUUMs the catalog, and the reported figure is rows
× 256, a dashboard convention, not a byte measurement -- it must not reach
the UI as "bytes freed."

→ what was run (ordinary-re-index half, no reclaim expected):
`remove_replaced_stale_indexes_cleans_up_the_leftover_chunk_row_after_a_reindex`
(`crates/pipeline/workers/src/tests/rfc059_reset_erasure.rs`) drives a real
re-index and calls `CleanupService::run_safe(RemoveReplacedStaleIndexes)`,
asserting `outcome.catalog_rows_deleted > 0` (the leftover `chunks` row is
genuinely removed) **and `outcome.catalog_bytes_reclaimed == 0`**,
confirmed via mutation against `insert_bundle`'s own fix.

→ what was run (missing-file-returns-changed half, reclaim expected):
`remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`
(`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`) constructs
the state that path reaches directly -- a stale chunk with its FTS rows
intact -- rather than driving `deactivate_for_missing_files` end-to-end
(disclosed: this exercises the *state* the real path produces, not the
full pipeline that produces it), and asserts `outcome.bytes_reclaimed ==
512` (one row in each FTS table, 256 bytes/row -- the dashboard's own
convention, never surfaced to a user as a byte count). Confirmed via
mutation: forcing `bytes_reclaimed: 0` made this test fail with `left: 0,
right: 512`; restored byte-identical.
→ what was observed: both halves PASS as the amended criterion now reads.
The underlying leak this criterion exists to close (pre-RFC-059, this
action deleted `chunks` rows while the FTS rows they addressed became
permanently orphaned) is closed on both paths.
→ where verified: `cargo test -p orbok-workers remove_replaced_stale_indexes_cleans_up_the_leftover_chunk_row_after_a_reindex`;
`cargo test -p orbok-search remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`.

### 7. The README's data-lifecycle section describes the behaviour that ships, verified by re-running the audit's claim check against it.

→ what was run: re-read of `README.md`'s "Local-first by design" section
against what criteria 1-6 and 10 actually made true, and of
`docs/src/users/storage.md`'s Safe cleanup / Reset catalog sections against
Slice 4's newly-exposed actions -- the same claim-by-claim check dev-team
Task 034 §9 ran when it corrected these paragraphs downward on 2026-09-01
(no automated "audit claim check" script exists; Task 034 §9 was a manual
read against the shipped code, transcribed here as the same method applied
to what changed).
→ what was observed: three passes. First (commit `5754b54`): "Reset catalog
does not clear it" became "Reset catalog erases it," and "no expiry and no
size bound" became "a 90-day expiry and a 20,000-entry cap." Second, after
Review 214's owner decisions: the 20,000-entry cap claim was removed (at
that point nothing enforced it) and replaced with the
90-day-freshness-plus-on-demand-full-erase description;
`docs/src/users/storage.md`'s Safe-cleanup bullet changed from "Expired
extracted-text cache entries" to "All extracted-text cache entries."
Third, with Slice 6: the Ephemeral-cache bullet now states the bound and
when it runs -- "a 20,000-entry bound applied automatically when indexing
goes idle, least recently used first" -- which criterion 10 evidences. It
keeps naming that chunk bundles and previews have neither a limit nor a
bound.
→ where verified: `mdbook build` (docs/) succeeds; `git diff README.md`
read in full against criterion 10's tests, not against the paragraph's
prior wording.

### 8. *(Added by Amendment 1.)* With a folder indexed containing a distinctive term, invoking Remove folder and then querying `chunk_fts_trigram` and `chunk_fts` directly for that term returns no rows, and both §6 invariants hold. Remove folder is the fifth operation in criterion 4's invariant test.

→ what was run: `erasure_invariant_holds_after_all_five_operations`
(renamed from `..._all_four_operations`,
`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`) gained a
fifth operation -- seeds a fresh source/file with a distinctive Japanese
term after Reset (Operation 4) has emptied every table, confirms a
`chunk_fts_trigram MATCH` finds it, calls
`SourceRepository::delete_with_all_data` ("Remove folder"), re-asserts the
erasure invariant, then asserts zero trigram matches and an empty
`chunk_fts` table.
→ what was observed: PASS after `delete_with_all_data`
(`crates/data/db/src/repo/sources.rs`) was rewritten, in one transaction,
to delete `chunk_fts`/`chunk_fts_trigram` rows addressed via
`files -> chunks -> keyword_index_records` for the source **before** the
pre-existing `DELETE FROM sources` cascade. Confirmed failing before this
fix -- the exact probe Review 213 §2 ran by execution: reverting the fix
reproduced `count(chunk_fts)=1 must equal count(keyword_index_records)=0`
at the "Remove folder" checkpoint; restored byte-identical.
→ where verified: `cargo test -p orbok-search erasure_invariant_holds_after_all_five_operations`,
commit `44ab2c9`.

### 9. *(Added by Amendment 1.)* Indexing a corpus larger than any configured extraction-cache bound leaves every file with active chunks and, when a model is configured, embeddings -- no chunk job fails on a cache miss and no embedding job completes empty.

→ what was run: `indexing_above_any_cache_bound_leaves_every_file_with_active_chunks`
(`crates/pipeline/workers/src/tests/rfc059_cache_lifetime.rs`) indexes 5
real files through the real `ExtractionWorker`/`ChunkAndIndexWorker`
pipeline via `run_pending`, asserting no Extract/Chunk job fails with a
non-`model_missing` category and every file ends with at least one active
chunk. No embedding model is available in this sandbox (disclosed, not
fabricated -- the same constraint every other model-dependent test in this
project names, e.g. RFC-058 Review Request 209 §3's row 7), so the
embedding half of this criterion is not exercised end-to-end here.
→ what was observed: the test alone, at 5 files against the real 20,000
cap, would pass regardless of whether the cap were ever reinstated at
write time -- it does not by itself prove the mechanism. **The actual
guard is structural, per Review 214 §1's own instruction not to overstate
this test's reach**: `extract_segments_namespace_is_registered_with_a_ttl_but_no_write_time_cap`
asserts `max_entries` registers `NULL` for `ExtractSegments`, so a write-time
cap cannot silently be reintroduced without that assertion catching it.
Criterion 10's pending-job test adds the idle-time half: the trim never
runs while a job could still need an entry. Confirmed via mutation (Review
213's own execution, cited in Amendment 1 §2a.2): temporarily restoring
`max_entries: Some(3)` in `OrbokCacheNamespace::default_engine_options` and
re-running the corpus test at 5 files reproduced the failure Review 213
found (a chunk job hard-failed on a cache miss); restored byte-identical.
→ where verified: `cargo test -p orbok-workers indexing_above_any_cache_bound_leaves_every_file_with_active_chunks
extract_segments_namespace_is_registered_with_a_ttl_but_no_write_time_cap`,
commit `44ab2c9`.

### 10. *(Added by Amendment 2.)* With more than `cleanup_time_entry_cap()` entries in the extraction cache and the scheduler idle (no queued or running index jobs, in memory or in `index_jobs`), the namespace is trimmed to the cap, least-recently-accessed first, exactly once per transition to idle. With any index job queued, no entry is evicted, however many are present.

→ what was built: `CleanupService::trim_extraction_cache_to` and
`trim_engine_namespace_to` (`crates/pipeline/workers/src/cleanup_service.rs`)
-- `list_entries()`, sort by `(last_accessed_at, updated_at)`, `remove()` the
oldest excess, `shrink_database()` only if something was removed; no raw
SQL, no second connection. Called from the scheduler host's idle branch
(`crates/app/src/scheduler_host.rs`, via
`ProfileCache::trim_extraction_cache_to`) behind a `trimmed_since_idle` flag
that resets whenever a job is dispatched, and behind `indexing_is_idle`:
`Scheduler::is_idle()` **and** zero `index_jobs` rows in any of Queued,
Running, Paused, Blocked or WaitingForDependency, failing closed on a
catalog error. `cleanup_time_entry_cap()` has one production reader, that
idle branch.

→ deviation from the handoff, disclosed: HANDOFF-059-slice6 §2 treats the
loop's second consecutive `tick() == None` as idle. It is not always: in
`ResourceMode::Paused` `tick()` returns `None` with jobs merely paused, and
in UserActive/LowImpact the embedding queue is skipped, so `None` can mean
"deferred", not "drained". The implementation uses the criterion's own
definition (nothing pending in memory or in `index_jobs`) instead, which is
why the guard reads the catalog. Ruled correct in Review 217 §2.

→ paused behaviour, intended (Review 217 §7.1): a profile paused with jobs
still pending never trims. That is safe: only the Extract job writes
`ExtractSegments` (Chunk and Embedding only read it), and nothing extracts
while indexing is paused, so a paused profile's cache cannot grow -- the
bound can lapse only by what was already written before the pause.

→ what was run (trim half): `idle_loop_trims_extraction_cache_to_cap_once_per_transition`
(`crates/app/src/scheduler_host/tests.rs`) -- seeds 5 entries with distinct
`last_accessed_at`, runs the real `run_with_context` loop with the cap
overridden to 3 (a test-only `tokio::task_local!`, not a ninth parameter),
waits for the namespace to reach 3 and asserts the survivors are the three
most recently accessed; then adds a sixth entry, waits four idle polls, and
asserts 4 remain -- no second trim without a transition.
→ what was run (pending-job half): `pending_index_job_blocks_the_idle_trim_until_it_leaves_the_queue`
(same file) -- seeds 5 entries, enqueues an Extract job, starts the loop with
background indexing off (so the job sits `paused` and `tick()` returns
`None` -- exactly the §2 premise hole above), waits five idle polls and
asserts all 5 remain; then cancels the job and waits for the trim to 3.
→ what was observed: both PASS. Mutations, each restored byte-identical:
**M1** removing the `indexing_is_idle` guard failed the pending-job test
(`left: 3, right: 5`); **M2** moving an unguarded trim above `rehydrate`
failed it the same way; **M3** removing `trimmed_since_idle = true` failed
the trim test on the no-second-trim assertion (`left: 3, right: 4`).
→ measurement (handoff §6 stop condition 1):
`measure_idle_trim_cost_at_the_cap`
(`crates/pipeline/workers/src/tests/rfc059_cache_lifetime.rs`, `#[ignore]`d),
release build, 20,000 synthetic entries: `list_entries()` 8.95–10.35 ms over
five samples; a trim at exactly the cap (no removal) 9.90 ms; a trim of
1,000 entries over the cap 29.28 ms. Well under the "few hundred
milliseconds" stop threshold; no paging needed.
→ where verified: `cargo test -p orbok --bin orbok idle_loop_trims_extraction_cache_to_cap_once_per_transition
pending_index_job_blocks_the_idle_trim_until_it_leaves_the_queue`;
`cargo test --release -p orbok-workers measure_idle_trim_cost_at_the_cap -- --ignored`.

---

## Criteria not met, and why RFC-059 closes anyway

None outstanding. Criterion 6 was re-worded by Amendment 1 and criterion 5 by
Amendment 2 to match owner decisions and what is actually true, rather than
left unmet. The 20,000-entry bound that the previous version of this section
disclosed as decided-but-unenforced is now enforced (criterion 10). The cap's
*value* stays RFC-059 §11 open question 1: it is backed by one 110-file
measurement, not by a real large corpus.

## Slice 4, 5 and 6 (not named as numbered §10 criteria, done as part of the handoffs)

- **Slice 4** ("expose the two cleanup actions that already work," RFC-059
  §8): `ClearTemporaryExtraction` and `RemoveReplacedStaleIndexes` are
  reachable from the Storage view's Safe cleanup row (`crates/ui/src/views.rs`),
  added after Slice 2 per §8's own explicit ordering. `Message` variants
  and reducer arms (`crates/ui/src/state.rs`), handlers
  (`crates/app/src/main.rs`) mirror the pre-existing `CleanSnippets`/
  `CleanSearchCache` pattern, including the RFC-061 §8(d)
  panic-on-bad-cache-path fix those two already carry. Copy went through
  two corrections: the initial draft ("Clear extracted text cache" /
  "Remove replaced index entries") failed
  `default_ui_copy_avoids_forbidden_terms`
  (`crates/ui/src/tests/rfc041_search.rs`) for using "cache"/"index";
  the first passing draft ("Clear temporary extracted text" / "Remove
  outdated search data") shipped in `5754b54` but Review 214 §4(c) found
  it too close to "Clear old search results" to tell apart, especially in
  Japanese (「古い検索結果を削除」 vs 「古い検索データを削除」); the final
  copy ("Clear extracted text" / "Remove old data from updated files," Q3,
  owner decision 2026-09-12) resolves both.
- **Slice 5**: README/storage.md corrections, folded into criterion 7 above.
- **Slice 6**: the idle-time size bound, criterion 10 above.
- **Per-action done-notices** (Review 214 §4 Q2, owner decision
  2026-09-12): the four Safe-cleanup buttons used to share one
  `UserNotice::PreviewsCleared` ("Temporary previews cleared / Freed up
  space. Your files are untouched."), including for actions whose actual
  reclaim can be zero. Each action now shows its own notice title
  (`crates/ui/src/notice.rs`: `PreviewsCleared`, `SearchCacheCleared`,
  `ExtractedTextCleared`, `ReplacedDataRemoved`); all four share one body
  ("Your files are untouched."). `user_notice_text_never_relies_on_colour_alone`
  (`crates/ui/src/tests/notice.rs`) confirms the four notices are still
  pairwise distinguishable by `(title, body)` despite the shared body;
  `cleanup_notices_never_claim_space_was_freed` (criterion 5) keeps "freed"
  from coming back.

---

## Follow-up work (not part of this closure)

- **A retention policy for a file that goes missing and never returns**
  (RFC-059 Amendment 1 §2a.1, Amendment 2 §2b). Its stale chunks and intact
  FTS rows stay on disk indefinitely (search is gated on `chunk_status`, so
  nothing surfaces, but nothing reclaims the space either). Routed to an
  RFC-037 amendment as a retention rule.
- **The upstream `localcache` request** (RFC-059 §7 option (C)) has two
  concrete asks -- `clear_namespace()` and a public maintenance-time LRU
  trim -- both composed from the public API in this closure
  (`erase_engine_namespace`, `trim_engine_namespace_to`), both better sent
  as one statement. Review 214 §7: the architect drafts it on the owner's
  word.
- **The cap's value** (RFC-059 §11 open question 1): revisit against a real
  large mixed corpus (PDF, code) if one is measured.

## Full gate suite, this implementation

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
(every crate, 0 failures), `mdbook build` (docs/), and every
`scripts/check-*.sh` gate (design tokens, i18n literals, migration
integrity, RFC lifecycle) -- all green as of this record.
