# Closure Record — RFC-059: Erasure Completeness and Cache Lifetime

**RFC:** [059](../accepted/059-erasure-completeness-and-cache-lifetime.md), amended
2026-09-12 (Amendment 1, §2a: a fourth erasure site, the write-time cache cap
withdrawn, criterion 6 re-worded, criteria 8/9 added). Still in `accepted/` --
Review 214 §6: "RFC-059 stays in `accepted/` until [criteria 8/9 land and this
record is updated]," now done; whether this record's completeness is enough
to move it to `done/` is a review decision, not this record's own.
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-059-erasure-completeness-and-cache-lifetime.md`;
reviewed in Review 213 (`.git-exclude/reviewed/213-rfc059-handoff-implementation-review.md`)
and Review 214 (`.git-exclude/reviewed/214-rfc059-review213-owner-decisions-review.md`)
-- plain text, not links: neither is git-tracked (RFC-063 §5), so a link
resolves to nothing on any checkout outside this working copy. Commits:
`5754b54` (original five slices),
`50d7763`/`d995403` (closure-record/gate follow-ups), `44ab2c9` (Review 213
§2 Critical + §3 High fixes), `5771740` (macOS test fix), `9240145` (Review
214 §2 required change), and the commit landing alongside this record's own
update (Review 214 §4 owner decisions Q1-Q3). Every "where verified" line
below names the commit it ran against when it is not the current HEAD.
**Transcribed, not re-derived**, from the handoff's own five-slice structure,
Reviews 213/214's own findings, and this implementation's mutation-tested
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

### 5. With the extraction cache holding entries older than the configured TTL, running Clear temporary extraction from the Storage view reports a non-zero byte reclaim and the entries are no longer retrievable.

→ what was run, in two stages: **stage 1** (commit `5754b54`)
`clear_temporary_extraction_reports_a_reclaim_once_entries_are_expired`
wrote a pseudo-random 64 KB payload through the real 90-day-TTL production
engine, backdated its `updated_at` 91 days via raw SQL, then called
`CleanupService::run_safe(ClearTemporaryExtraction)` -- the actual
production path (`ProfileCache::run_safe_cleanup` in
`crates/app/src/runtime_storage.rs`), not
`orbok_cache::CacheService::run_safe_cleanup`, which has no production
caller. **Stage 2** (Review 214 §4 Q1, owner decision 2026-09-12,
superseding stage 1): the button no longer only expires -- it erases the
whole `ExtractSegments` namespace outright, so
`clear_temporary_extraction_erases_the_namespace_even_when_nothing_has_expired`
(same file) replaced the stage-1 test, writing a **fresh** (unexpired)
entry and asserting it is gone after the action, which the pre-Q1 code
could not have passed.
→ what was observed: stage 1 found and fixed two production bugs neither
in the handoff's explicit scope: `CleanupService::run_cache_side`'s
`ClearTemporaryExtraction | RemoveTemporarySourceIndexes` branch called
neither `cleanup_expired()` nor `shrink_database()`, so the TTL would have
been configured but unenforced, and `cache_bytes_freed` would always report
0. Both fixed and confirmed via mutation. Stage 2's erase decision made
those two calls (and a since-added, since-withdrawn cleanup-time entry
cap -- see below) redundant in this branch, since nothing survives an
outright erase to expire, purge, or cap; superseded code removed rather
than left dead. Confirmed via mutation: reverting the branch to
`cleanup_expired`-only made the fresh-entry assertion fail, restored
byte-identical.
→ where verified: `cargo test -p orbok-workers clear_temporary_extraction_erases_the_namespace_even_when_nothing_has_expired`.

TTL value: `OrbokCacheNamespace::default_engine_options`
(`crates/data/cache/src/namespace.rs`) sets `ExtractSegments` to a 90-day
write-time TTL, still real -- `localcache`'s `get_if_fresh` treats an entry
older than the TTL as a miss on every read regardless of any cleanup
action, forcing fresh extraction. Measurement backing the original
proposed cap: `measure_extraction_cache_usage_against_the_rfcs_corpus`
(`crates/app/src/rfc059_cache_measurement.rs`, `#[ignore]`d, one-time) --
this project's own `rfcs/` tree (110 real markdown files) indexed through
the real hosted scheduler, measured via `CacheService::usage`: **110
entries, 595,247 payload bytes, ~5,411 bytes/entry**.

**The 20,000-entry cap itself moved and is not currently enforced
anywhere** (Review 214 §3, Amendment 1 §2a.2): a write-time `max_entries`
evicted the earliest files' cached text before their own chunk jobs could
read it back, hard-failing those jobs above the cap (criterion 9 below).
Withdrawn. The value (`OrbokCacheNamespace::cleanup_time_entry_cap`,
`crates/data/cache/src/namespace.rs`) is kept as the decided number for a
future cleanup-time enforcement point; the enforcement code itself
(`enforce_extract_segments_cleanup_cap`, briefly present in commit
`9240145`) was removed once Q1's erase decision made it dead in its only
call site. Review 214 §3/§4 Q4 recommends a scheduler-idle hook as the
real home for it -- **not built as part of this closure**; tracked as
follow-up work below.

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
against what criteria 1-6 above actually made true, and of
`docs/src/users/storage.md`'s Safe cleanup / Reset catalog sections against
Slice 4's newly-exposed actions -- the same claim-by-claim check dev-team
Task 034 §9 ran when it corrected these paragraphs downward on 2026-09-01
(no automated "audit claim check" script exists; Task 034 §9 was a manual
read against the shipped code, transcribed here as the same method applied
to what changed).
→ what was observed: two passes. First (commit `5754b54`): "Reset catalog
does not clear it" became "Reset catalog erases it," and "no expiry and no
size bound" became "a 90-day expiry and a 20,000-entry cap." Second, after
Review 214's owner decisions changed what is actually true: the
20,000-entry cap claim was removed (it is decided but not enforced
anywhere yet -- criterion 5 above) and replaced with the accurate
90-day-freshness-plus-on-demand-full-erase description; `docs/src/users/storage.md`'s
Safe-cleanup bullet changed from "Expired extracted-text cache entries" to
"All extracted-text cache entries," matching the erase decision. The
`Ephemeral cache` bullet keeps naming that chunk bundles and previews have
neither an expiry nor a size bound, honestly stated rather than implied
fixed.
→ where verified: `mdbook build` (docs/) succeeds; `git diff README.md
docs/src/users/storage.md` read in full against criteria 1-6 and Review
214's own decisions, not against either paragraph's prior wording.

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
Together they are enough: the corpus test proves the pipeline behaves
correctly today, the registration test proves it stays that way. Confirmed
via mutation (Review 213's own execution, cited in Amendment 1 §2a.2):
temporarily restoring `max_entries: Some(3)` in
`OrbokCacheNamespace::default_engine_options` and re-running the corpus
test at 5 files reproduced the failure Review 213 found (a chunk job
hard-failed on a cache miss); restored byte-identical.
→ where verified: `cargo test -p orbok-workers indexing_above_any_cache_bound_leaves_every_file_with_active_chunks
extract_segments_namespace_is_registered_with_a_ttl_but_no_write_time_cap`,
commit `44ab2c9`.

---

## Criteria not met, and why RFC-059 closes anyway

None outstanding as numbered §10 criteria. Criterion 6 was re-worded by
Amendment 1 to match what is actually true (§2a.3) rather than left
unmet; the version of this section that argued for the pre-Amendment
wording no longer applies, since that wording no longer exists.

One decided-but-unenforced item, disclosed rather than hidden: the
extraction cache's 20,000-entry size bound (criterion 5) has a value but no
current runner anywhere in the codebase, since the write-time enforcement
that existed briefly was withdrawn (it broke indexing above the cap,
criterion 9) and the owner's Q1 decision to make "Clear temporary
extraction" an outright erase removed the cleanup-time enforcement that
briefly replaced it. Review 214 §3/§4 Q4 recommends a scheduler-idle hook
as the real home for this bound -- **routed as follow-up work, not built
as part of this closure.**

## Slice 4 and 5 (not named as numbered §10 criteria, done as part of this handoff)

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
- **Per-action done-notices** (Review 214 §4 Q2, owner decision
  2026-09-12, not a numbered criterion but part of this closure's scope):
  the four Safe-cleanup buttons used to share one `UserNotice::PreviewsCleared`
  ("Temporary previews cleared / Freed up space. Your files are
  untouched."), including for actions whose actual reclaim can be zero.
  Each action now shows its own notice title
  (`crates/ui/src/notice.rs`: `PreviewsCleared`, `SearchCacheCleared`,
  `ExtractedTextCleared`, `ReplacedDataRemoved`); all four share one body
  ("Your files are untouched.") with "Freed up space" dropped, since that
  figure is a dashboard convention, not a measurement.
  `user_notice_text_never_relies_on_colour_alone`
  (`crates/ui/src/tests/notice.rs`) confirms the four notices are still
  pairwise distinguishable by `(title, body)` despite the shared body.

---

## Follow-up work (not part of this closure)

- **A scheduler-idle hook to enforce the extraction cache's entry-cap
  bound** (Review 214 §3/§4 Q4). The value is decided
  (`OrbokCacheNamespace::cleanup_time_entry_cap`); no code runs it. The
  right trigger point is when the hosting loop drains to no queued
  Extract/Chunk/Embedding jobs -- the only moment a trim provably cannot
  evict an entry a job still needs.
- **A retention policy for a file that goes missing and never returns**
  (RFC-059 Amendment 1 §2a.1, Review 214 §4 Q5). Its stale chunks and
  intact FTS rows stay on disk indefinitely (search is gated on
  `chunk_status`, so nothing surfaces, but nothing reclaims the space
  either). Review 214 recommends an RFC-037 amendment naming a retention
  rule, not a new RFC. No tracking home exists yet.
- **The upstream `localcache` request** (RFC-059 §7 option (C)) now has two
  concrete asks -- `clear_namespace()` and a public maintenance-time LRU
  trim -- both already composable from the public API used in this closure,
  both better sent as one statement. Per the handoff's own instruction,
  this implementation does not open it; Review 214 §7: the architect
  drafts it on the owner's word.

## Full gate suite, this implementation

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
(every crate, 0 failures), `mdbook build` (docs/), and every
`scripts/check-*.sh` gate (design tokens, i18n literals -- four new
`tracing::error!` log-message literals added to
`scripts/i18n-literal-allowlist.txt`, migration integrity, RFC lifecycle) --
all green as of this record.
