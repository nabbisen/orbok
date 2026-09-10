# Closure Record — RFC-059: Erasure Completeness and Cache Lifetime

**RFC:** [059](../accepted/059-erasure-completeness-and-cache-lifetime.md) (still in `accepted/` at the
time of this record -- criterion 6 has a disclosed nuance below; whether that
still permits moving to `done/` is a review decision, not this record's own.)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-059-erasure-completeness-and-cache-lifetime.md`,
commit `5754b54`, not yet reviewed at the time of this writing -- neither the
review nor its request is git-tracked (RFC-063 §5). Every "where verified"
line below runs against that same commit unless it names a different one.
**Transcribed, not re-derived**, from the handoff's own five-slice structure
and this implementation's own mutation-tested observations below.

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

→ what was run: `erasure_invariant_holds_after_all_four_operations`
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
→ where verified: `cargo test -p orbok-search rfc059_erasure_invariant`.

### 5. With the extraction cache holding entries older than the configured TTL, running Clear temporary extraction from the Storage view reports a non-zero byte reclaim and the entries are no longer retrievable.

→ what was run: `clear_temporary_extraction_reports_a_reclaim_once_entries_are_expired`
(`crates/pipeline/workers/src/tests/rfc059_cache_lifetime.rs`) -- writes a
pseudo-random (not uniform-byte, which compresses to nothing under
`.compress()` and always reports 0 bytes freed regardless of correctness)
64 KB payload through the real 90-day-TTL production engine, backdates its
stored `updated_at` 91 days via raw SQL against the cache database file
(not by opening a mismatched short-TTL test engine, which would evaluate
expiry against its own TTL rather than the real one), then calls
`CleanupService::run_safe(ClearTemporaryExtraction)` -- the actual
production path (`ProfileCache::run_safe_cleanup` in
`crates/app/src/runtime_storage.rs`), not
`orbok_cache::CacheService::run_safe_cleanup`, which has no production
caller.
→ what was observed: PASS, after finding and fixing two production bugs
this criterion's own test exposed, neither in the handoff's explicit scope:
`CleanupService::run_cache_side`'s `ClearTemporaryExtraction |
RemoveTemporarySourceIndexes` branch called neither `cleanup_expired()` nor
`shrink_database()` before this fix -- meaning the new TTL (below) would
have been configured but silently unenforced by the one UI action meant to
enforce it, and `cache_bytes_freed` would always report 0 regardless of how
many entries were actually removed. Both calls added; both confirmed
individually via mutation (reverted, test failed with the exact expected
message, restored byte-identical).
→ where verified: `cargo test -p orbok-workers clear_temporary_extraction_reports_a_reclaim_once_entries_are_expired`.
TTL/cap values: `OrbokCacheNamespace::default_engine_options`
(`crates/data/cache/src/namespace.rs`) sets `ExtractSegments` to a 90-day
TTL and a 20,000-entry cap. Measurement backing that number:
`measure_extraction_cache_usage_against_the_rfcs_corpus`
(`crates/app/src/rfc059_cache_measurement.rs`, `#[ignore]`d, one-time) --
this project's own `rfcs/` tree (110 real markdown files) indexed through
the real hosted scheduler, measured via `CacheService::usage`: **110
entries, 595,247 payload bytes, ~5,411 bytes/entry**. At 20,000 entries and
that average the namespace's worst-case size is roughly 100 MB; real corpora
with larger documents (PDFs, code) will average higher per entry. **This
number is proposed, not decided** -- RFC-059 §11 open question 1 explicitly
routes the decision to the architect, and this implementation does not
invent one beyond what the measurement supports.

### 6. Invoking Remove replaced stale indexes after a re-index reports a byte reclaim greater than zero and reduces the on-disk keyword-index size.

→ what was run, and **what did not hold as literally worded, disclosed
rather than forced to pass**:
`remove_replaced_stale_indexes_cleans_up_the_leftover_chunk_row_after_a_reindex`
(`crates/pipeline/workers/src/tests/rfc059_reset_erasure.rs`) drives a real
re-index and calls `CleanupService::run_safe(RemoveReplacedStaleIndexes)`.
It observes `outcome.catalog_rows_deleted > 0` (the leftover `chunks` row for
the superseded generation is genuinely removed) **and
`outcome.catalog_bytes_reclaimed == 0`** -- not greater than zero. This is a
direct, correct consequence of criterion 4's own fix: §6's "Prerequisite,
and it is not optional" requires `insert_bundle` to delete the superseded
generation's `chunk_fts`/`chunk_fts_trigram`/`keyword_index_records` rows
**at replace time**, so by the time `remove_replaced_stale_indexes` runs
after an ordinary re-index, those tables already hold nothing for it to
find. Confirmed directly, not assumed: `stale_chunks_before` (a real stale
`chunks` row) is asserted present going in, and the same scenario with
`insert_bundle`'s fix reverted (mutation, restored after) would have left
this test observing a non-zero reclaim instead -- doing the RFC's own
required fix (criterion 4) is what makes criterion 6's literal setup
("after a re-index") no longer produce anything for this specific action to
reclaim.

The byte-reclaim mechanism this criterion actually asks for **is** real and
independently verified: `outcome.bytes_reclaimed` is a new field on
`CleanupOutcome` (`crates/data/db/src/repo/cleanup.rs`), 256 bytes (the same
per-record convention `orbok_workers::storage::update_storage_accounting`
already uses for its `KeywordIndex` dashboard row) times the number of
`chunk_fts`/`chunk_fts_trigram` rows `remove_replaced_stale_indexes` itself
deletes. `remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`
(`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`) constructs a
stale chunk with its FTS rows manually left intact (bypassing
`insert_bundle`, since no production path leaves one today -- the
disclosed, defense-in-depth scenario the handoff's own §6 item 3 asks this
function to guard regardless) and asserts `outcome.bytes_reclaimed == 512`
(one row in each FTS table). Confirmed via mutation: forcing
`bytes_reclaimed: 0` in the implementation made this test fail with
`left: 0, right: 512`; the fix restored byte-identical.
→ what was observed: the underlying leak this criterion exists to close
(pre-RFC-059, this action deleted `chunks` rows while the FTS rows they
addressed became permanently orphaned -- "reporting rows deleted while
reclaiming nothing," RFC-059 §8's own words) is closed, and its own
byte-reclaim reporting is real. The literal "after a re-index" framing is
not the scenario where that reclaim is observed, because criterion 4's own
required fix moved the reclaim earlier in the pipeline.
→ where verified: `cargo test -p orbok-workers remove_replaced_stale_indexes_cleans_up_the_leftover_chunk_row_after_a_reindex`;
`cargo test -p orbok-search remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`.
See "Criteria not met, and why this closes anyway" below.

### 7. The README's data-lifecycle section describes the behaviour that ships, verified by re-running the audit's claim check against it.

→ what was run: re-read of `README.md`'s "Local-first by design" section
against what criteria 1-6 above actually made true, and of
`docs/src/users/storage.md`'s Safe cleanup / Reset catalog sections against
Slice 4's newly-exposed actions -- the same claim-by-claim check dev-team
Task 034 §9 ran when it corrected these paragraphs downward on 2026-09-01
(no automated "audit claim check" script exists; Task 034 §9 was a manual
read against the shipped code, transcribed here as the same method applied
to what changed).
→ what was observed: two claims restored to what Slices 1 and 3 now make
true -- "Reset catalog does not clear it" became "Reset catalog erases it,"
and "no expiry and no size bound" became "a 90-day expiry and a
20,000-entry cap." The `Ephemeral cache` bullet was corrected to name that
the bound applies only to the extracted-text namespace (chunk bundles and
previews remain unbounded, honestly stated rather than implied to be fixed
too). `docs/src/users/storage.md` gained the newly-exposed "Expired
extracted-text cache entries" bullet under Safe cleanup and a corrected
Reset catalog description naming the extraction cache explicitly.
→ where verified: `mdbook build` (docs/) succeeds; `git diff README.md
docs/src/users/storage.md` read in full against criteria 1-6's own evidence
above, not against the paragraph's prior wording.

---

## Criteria not met, and why RFC-059 closes anyway

- **Criterion 6, taken at maximal literalness ("after a re-index... reports
  a byte reclaim greater than zero"), does not hold**, for the reason
  detailed under criterion 6 above: criterion 4's own required fix
  (`insert_bundle` deleting the superseded generation's FTS/keyword-index
  rows at replace time, per §6's "Prerequisite, and it is not optional")
  moves the byte-reclaim earlier in the pipeline, so `remove_replaced_stale_indexes`
  usually has nothing left to reclaim by the time it runs after an ordinary
  re-index. The byte-reclaim *mechanism* criterion 6 asks for is real,
  implemented, and independently verified in the one scenario where this
  action's own fix has genuine work to do (a stale chunk whose FTS rows
  were never pre-deleted -- defense-in-depth, not the common path). This is
  judged a correct, disclosed consequence of doing criterion 4 right, not a
  gap to route around with a differently-constructed test.

## Slice 4 and 5 (not named as numbered §10 criteria, done as part of this handoff)

- **Slice 4** ("expose the two cleanup actions that already work," RFC-059
  §8): `ClearTemporaryExtraction` and `RemoveReplacedStaleIndexes` are now
  reachable from the Storage view's Safe cleanup row (`crates/ui/src/views.rs`),
  added after Slice 2 per §8's own explicit ordering. New `Message`
  variants and reducer arms (`crates/ui/src/state.rs`), handlers
  (`crates/app/src/main.rs`) mirror the pre-existing `CleanSnippets`/
  `CleanSearchCache` pattern exactly, including the RFC-061 §8(d)
  panic-on-bad-cache-path fix those two already carry. New copy
  ("Clear temporary extracted text" / "Remove outdated search data," EN;
  matching JA) passes `default_ui_copy_avoids_forbidden_terms`
  (`crates/ui/src/tests/rfc041_search.rs`) without an exemption -- confirmed
  failing first with the initial, more literal copy ("Clear extracted text
  cache" / "Remove replaced index entries"), which the gate correctly
  rejected for using the forbidden terms "cache" and "index."
- **Slice 5**: README/storage.md corrections, folded into criterion 7 above.

---

## Full gate suite, this implementation

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
(every crate, 0 failures), `mdbook build` (docs/), and every
`scripts/check-*.sh` gate (design tokens, i18n literals -- four new
`tracing::error!` log-message literals added to
`scripts/i18n-literal-allowlist.txt`, migration integrity, RFC lifecycle) --
all green as of this record.
