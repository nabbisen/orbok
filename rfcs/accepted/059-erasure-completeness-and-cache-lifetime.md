# RFC-059: Erasure Completeness and Cache Lifetime

**Project:** orbok\
**RFC:** 059\
**Title:** Erasure Completeness and Cache Lifetime\
**Status:** Accepted\
**Accepted:** 2026-09-02 by the project owner\
**Target milestone:** privacy correctness\
**Date:** 2026-09-01\
**Related RFCs:** RFC-001 Local Data Classification and Lifecycle (this makes its "Ephemeral" class true); RFC-011 Storage Dashboard and Cleanup UX (adds two actions it defines but never exposed); RFC-039 Privacy Modes and Local Data Visibility; RFC-018 Crash Recovery, Diagnostics and Repair Tools (§8 borrows its repair framing)

---

## 1. Summary

orbok's "Reset catalog" action does not erase what orbok indexed. Two
independent leaks, both verified:

```text
(a) The trigram keyword index is never cleared. `DELETE FROM
    chunk_fts_trigram` occurs zero times in the workspace. After a full
    reset the index still matches terms from the reset corpus.

(b) The extraction cache — which holds the complete extracted text of every
    document — survives every cleanup orbok offers. It is opened with
    ttl: None, max_entries: None, and "purge all namespaces" calls only
    expiry and stale-version sweeps, neither of which can match anything.
```

The README states *"Full extracted text is not stored permanently by default."*
That is false today.

This RFC decides what erasure must guarantee, how the extraction cache is
bounded, and what the product may claim while the answer is being built.

---

## 2. Motivation

orbok's first promise is privacy. A destructive action named "Reset catalog"
that leaves both a token index over the user's documents and the documents' full
extracted text in place is the most serious non-functional defect the product
can carry, because it is the one a user cannot detect and cannot work around.

The trigram half is not a partial leak. A character-trigram index over Japanese
prose is a substantial reconstruction of the source: it preserves every
three-character sequence in the corpus. And because `keyword_index_records` is
cascade-deleted by the reset, those rows are also permanently unreachable by
orbok itself — the leak is simultaneously a privacy failure and unreclaimable
disk.

---

## 2a. Amendment 1 (2026-09-12) — what implementation found, and what review found after it

Review Request 213 implemented all five handoff slices; the code that landed
is correct and stays. Reviewing it by execution found that §6 named three
erasure sites and there are four, that §7's cap breaks the pipeline it was
meant to bound, and that criterion 6 as worded cannot hold once §6 is done
right. This amendment records all three so the RFC describes what must be
true, not what was first thought.

### 2a.1 The fourth site — "Remove folder"

`bootstrap::remove_source` → `SourceRepository::delete_with_all_data` →
`DELETE FROM sources`, whose own doc comment says the cascade runs "through
files → extraction → chunks → indexes". It cannot reach the indexes: both FTS
tables are contentless and `keyword_index_records` — the only chunk→rowid link
— is cascaded away first. Observed directly (probe against
`insert_bundle` + `delete_with_all_data`, 2026-09-12):

```text
before:               fts=1  trigram=1  kir=1
after remove folder:  fts=1  trigram=1  kir=0  chunks=0
trigram MATCH for the removed folder's term: 1 row
```

This is the erasure action a user actually reaches for — Reset is the nuclear
one — and it leaves the folder's full trigram index behind, permanently. §6's
list becomes **four** sites, and the invariant test's four operations become
**five** (criterion 8 below). The fix is the same shape as the other three:
delete both FTS tables' rows addressed via `files → chunks →
keyword_index_records` before the `sources` delete, in the same transaction.

A fifth case is adjacent and unowned: a file that goes **missing and never
returns** keeps its chunks `stale` (`deactivate_for_missing_files`) with FTS
rows intact, and `remove_replaced_stale_indexes` skips it by design (no active
sibling). Search is gated on `chunk_status`, so the content does not surface —
but it stays on disk until Reset. No retention policy exists for missing files
(RFC-037 has none). **Owner decision, routed, not decided here.**

### 2a.2 The cap is not a bound; it is a failure mode

§7 said "the TTL and cap are not optional" on the premise that the cache is by
definition rebuildable. The *data* is; the *pipeline* treats the cache as its
only source of text. `chunk_and_index.rs` hard-fails on a miss
(`"extraction cache miss: run extraction first"`), and `embedding.rs` returns
`Ok(None)` on a miss, which `EmbeddingWorker::run` turns into `Ok(())` — **the
job succeeds with no vectors written**, and nothing retries.

localcache enforces `max_entries` on every `set`, LRU by `last_accessed_at`.
Extract and Chunk are both `NormalBackground` (FIFO within a priority), so a
scan's *N* extractions all run before any chunk job; Embedding is
`LowBackground` and runs after all of those. So for any corpus larger than the
cap, the first files' entries are evicted before their chunk and embedding jobs
run: those chunk jobs fail, those embedding jobs silently succeed empty. The
implementation's 20,000 was measured on 110 files and could not see this; the
handoff's third stop condition ("a cap that would evict during a normal
indexing run") was exactly this and was not checked.

**Disposition: write-time `max_entries` is withdrawn from §7.** The TTL stays
(90 days cannot fire inside an indexing run). Bounding the namespace's size
moves to the cleanup action — a cleanup-time cap is still a cap, and it cannot
evict under a running pipeline. The value itself remains open question 1. The
deeper fix — downstream jobs that do not depend on a cache — is RFC-060 §6's
snippet question wearing different clothes, and is routed there.

### 2a.3 Criterion 6, re-worded

Criterion 6 asked that `Remove replaced stale indexes` "after a re-index
reports a byte reclaim greater than zero". Once `insert_bundle` deletes the
superseded generation's FTS rows at replace time — which §6 requires — an
ordinary re-index leaves that action nothing to reclaim, and it correctly
reports 0. The reclaim is real but happens earlier. The action does have work
on one production path the implementation believed did not exist: a file that
goes missing (chunks marked `stale`, FTS rows kept for reactivation) and then
**returns changed** — `insert_bundle`'s delete targets `chunk_status =
'active'` only, so the missing generation's rows survive until this action
runs. Criterion 6 is re-worded below to name that path, and its "reduces the
on-disk keyword-index size" clause is dropped: nothing VACUUMs the catalog, and
the reported figure is rows × 256, a convention shared with the dashboard, not
a measurement. It must not be shown to a user as bytes.

---

## 2b. Amendment 2 (2026-09-13) — owner decisions, and where the bound actually runs

Review 214 put four questions to the owner; Review 216 records the answers.
Recorded here so the RFC says what the program does.

**"Clear extracted text" erases.** The Storage-view action erases the whole
`ExtractSegments` namespace on press — every entry, regardless of age — not
only entries past the TTL. The label says *clear*; §7's own argument is that
the cache is rebuildable; and a user pressing it for privacy must not have to
wait out ninety days. Criterion 5 is re-worded to that. The four Safe-cleanup
actions each show their own done-notice title with one shared body, and no
notice claims an amount of space freed (Amendment 1 §2a.3: the figure is a
convention, not a measurement).

**The size bound runs at scheduler idle, not in the cleanup action.**
Amendment 1 §2a.2 moved the bound from write time to "the cleanup action". Once
that action erases outright, a trim inside it runs against an empty namespace
and is dead — Review 215 §3. The owner chose to keep the bound (option A) and
give it the one home that is safe: **when the hosting loop's queue and the
catalog's `index_jobs` have both drained** — no Extract, Chunk or Embedding job
queued or running — trim `ExtractSegments` to `cleanup_time_entry_cap()`,
least-recently-accessed first, via localcache's public `list_entries()` /
`remove()` / `shrink_database()`. That is the only moment a trim provably
cannot evict an entry a job still needs; it runs once per transition to idle,
not once per poll. Criterion 10 guards it; `HANDOFF-059-slice6` builds it. The
cap's *value* remains open question 1.

**A file that goes missing and never returns** (Amendment 1 §2a.1's fifth
case) is routed to an RFC-037 amendment as a retention rule; it is not this
RFC's.

---

## 3. Goals

- Define what "erase" guarantees, in terms a user can check.
- Make the extraction cache's lifetime finite and stated.
- Expose the two cleanup actions that already exist and reclaim real space.
- Bring the README's data-lifecycle section into agreement with the code.
- Leave a verification path: a user (or a test) can confirm erasure happened.

## 4. Non-Goals

- Encryption at rest. RFC-026 is withdrawn pending a dedicated security audit;
  that decision stands and this RFC does not reopen it.
- Secure deletion / overwrite-in-place guarantees against forensic recovery.
  SQLite, the filesystem and the SSD's own controller all defeat this; claiming
  it would be the same defect in a new place.
- Storage accounting accuracy. That is a separate finding (the dashboard
  double-counts and understates the keyword index by ~1.8×) and belongs to a
  task, not here.
- The `localcache` engine's internals. §7 states what we need from it; its
  design is the upstream project's.

---

## 5. Decision 1 — what "Reset" means

**Reset erases every artifact orbok derived from the user's documents. It never
touches the user's documents.**

Concretely, after Reset returns success:

| Class | Must hold |
|---|---|
| `chunks`, `chunk_locations`, `embeddings` | empty |
| `chunk_fts` | empty |
| **`chunk_fts_trigram`** | **empty** — the current gap |
| `keyword_index_records` | empty |
| `files`, `sources` | empty (registration is derived state; the folders on disk are not) |
| **extraction cache** | **no entry retrievable** — the current gap |
| chunk-bundle cache, preview cache | no entry retrievable |
| search history | governed by RFC-042 §13.4's existing "turn off and clear"; Reset clears it |
| settings, model artifacts | **untouched** — they are not derived from documents |

The last row is deliberate and is worth stating because it is the one a naive
"delete the data directory" implementation gets wrong: forcing a 490 MB model
re-download is not privacy, it is damage.

## 6. Decision 2 — the trigram index gets a deletion path

There is no path to repair; there is a path to write. `chunk_fts_trigram` is
inserted at `chunks.rs:120` and read at `multilingual.rs:175-179`, and that is
the complete set of operations against it. `keyword_index_records.trigram_fts_rowid`
is a handle nothing has ever released.

Three call sites need it, and they must delete FTS rows **before** dropping the
mapping row that addresses them:

1. `run_reset_catalog` — add the `'delete-all'` command for the trigram table
   beside the existing one.
2. `Fts5KeywordEngine::delete` — delete the trigram row alongside the unicode61
   row, then the mapping row.
3. `remove_replaced_stale_indexes` — collect `fts_rowid` and `trigram_fts_rowid`
   for the chunks about to be deleted, delete those FTS rows, then let the
   cascade run.

**Prerequisite, and it is not optional.** The replace-on-reindex delete in
`Fts5KeywordEngine::index` is keyed on `chunk_id`, and `insert_bundle` mints a
fresh UUID `chunk_id` on every call — so that delete has never matched anything.
Adding trigram deletes to a path keyed on a chunk id that never repeats fixes
nothing. Re-indexing must delete the **previous** chunks' FTS rows, addressed by
`file_id`, before the new chunks are inserted.

**Invariant test, both tables:**

```text
count(chunk_fts) == count(keyword_index_records)
count(chunk_fts_trigram) == count(keyword_index_records WHERE trigram_fts_rowid IS NOT NULL)
```
asserted after: a re-index, each of the three cleanup actions, and Reset.

## 7. Decision 3 — the extraction cache gets a finite lifetime

The extraction cache exists to avoid re-parsing a PDF when only the embedding
model changed. That is a real benefit and this RFC does not remove it. It makes
the cost bounded and the lifetime stated.

**Three options. Recommendation: (A) now, (C) when upstream allows.**

**(A) Delete and recreate `orbok-cache.sqlite3` on Reset. Bound the extraction
namespace with a TTL and an entry cap.** *Recommended.*
The cache is by definition rebuildable, so deleting the file is safe and is the
only mechanism available today that actually erases. Independently, give
`ExtractSegments` a TTL and `max_entries` so steady-state growth is bounded
between resets. Both are local changes; neither waits on anyone.
Risk: deleting an open database file needs the handle closed first, and the
cleanup service currently holds engines open across the sweep. Sequencing is the
work here, not the deletion.

**(B) Stop caching full extracted text; cache only what the embedding step
needs.** Honest and eliminates the class, but it makes a model change re-parse
every PDF, which is a large regression on the exact operation RFC-008's model
lifecycle makes routine. Rejected unless (A) proves impossible.

**(C) A `clear_namespace()` API in `localcache`.** The correct long-term
mechanism: erasure becomes a supported operation instead of file deletion. This
project owns that dependency and has a channel for the ask
(`.git-exclude/upstream-requests/`, filenames prefixed `orbok-`). **Open the
request as part of this RFC's implementation, not after** — if it lands quickly,
(A)'s file deletion becomes a fallback for old versions rather than the design.

Whichever lands, the TTL and cap are not optional: an unbounded cache with no
expiry is what made "purge expired" a no-op in the first place.

> **Amended 2026-09-12 (§2a.2), corrected 2026-09-13 (§2b).** "Cap" here means
> a bound enforced **when the indexing pipeline is idle** — not a write-time
> `max_entries` on the engine, and not inside the cleanup action either. A
> write-time LRU evicts under a running pipeline whose chunk and embedding jobs
> read this namespace as their only source of text; the cleanup action now
> erases outright, so a trim there has nothing to trim. The 20,000-entry
> write-time cap is withdrawn; the value survives as `cleanup_time_entry_cap()`
> and is applied at scheduler idle (criterion 10).

## 8. Decision 4 — expose the two cleanup actions that already work

`ClearTemporaryExtraction` and `RemoveReplacedStaleIndexes` are implemented in
`CleanupService` and reachable from no UI. The Storage view offers only Clear
snippets, Clear search cache, and Reset catalog.

Add both to the Safe Cleanup row — **after** §6 lands, so that
`RemoveReplacedStaleIndexes` actually frees bytes instead of reporting rows
deleted while reclaiming nothing.

## 9. Decision 5 — what the README says in the meantime

The claims *"Full extracted text is not stored permanently by default"* and
*"Ephemeral cache — recent snippets, search result cache. LRU-evicted"* are
false today and become true only when §7 ships.

**They are corrected in text immediately** (dev-team Task 034), not held until
the code catches up. The corrected text states what is true — extracted text is
cached locally and is removed by Reset once §7 lands; today it is removed by
deleting the data directory — and this RFC restores the stronger claim as an
acceptance criterion.

A privacy claim that does not hold is the one documentation defect this project
cannot carry, and a month of under-promising is recoverable.

---

## 10. Acceptance criteria

Phrased per RFC-058 §5.

1. With a corpus indexed containing a distinctive term, invoking Reset and then
   querying the trigram path for that term returns no rows — verified against
   `chunk_fts_trigram` directly, not through the search API.
2. With the same corpus, after Reset, no extraction-cache entry for any indexed
   file is retrievable through `CacheService`.
3. After Reset, `settings.json` and the installed model artifacts are byte-identical
   to their pre-Reset state.
4. Re-indexing one file twice leaves `count(chunk_fts_trigram)` unchanged, and
   both invariants in §6 hold after each of the four operations listed there.
5. *(Re-worded by Amendment 2.)* With the extraction cache holding entries of
   any age — including one written seconds ago — running Clear extracted text
   from the Storage view leaves no entry retrievable through `CacheService`
   and none listed by `keys(None)`. No notice reports an amount of space freed.
6. *(Re-worded by Amendment 1.)* After an ordinary re-index, both §6
   invariants hold and Remove replaced stale indexes reports zero FTS rows
   reclaimed — the reclaim happened at replace time. After a file goes missing
   and returns with changed content, the same action reports a non-zero FTS-row
   reclaim and both invariants hold afterwards. No reported figure is
   presented to the user as bytes freed.
7. The README's data-lifecycle section describes the behaviour that ships,
   verified by re-running the audit's claim check against it.
8. *(Added by Amendment 1.)* With a folder indexed containing a distinctive
   term, invoking Remove folder and then querying `chunk_fts_trigram` and
   `chunk_fts` directly for that term returns no rows, and both §6 invariants
   hold. Remove folder is the fifth operation in criterion 4's invariant test.
9. *(Added by Amendment 1.)* Indexing a corpus larger than any configured
   extraction-cache bound leaves every file with active chunks and, when a
   model is configured, embeddings — no chunk job fails on a cache miss and no
   embedding job completes empty.
10. *(Added by Amendment 2.)* With more than `cleanup_time_entry_cap()` entries
    in the extraction cache and the scheduler idle (no queued or running index
    jobs, in memory or in `index_jobs`), the namespace is trimmed to the cap,
    least-recently-accessed first, exactly once per transition to idle. With
    any index job queued, no entry is evicted, however many are present — and
    that half is the one to break deliberately and watch fail.

---

## 11. Open questions

1. **TTL and entry-cap values for `ExtractSegments`.** No measurement exists.
   Proposal: start from a storage budget (e.g. cap the namespace at a fraction
   of the catalog size) rather than a time, because "how long ago" is not what
   the user cares about here. Needs one measurement pass on a real corpus.
2. ~~**Does Reset clear search history unconditionally?**~~ **Resolved
   2026-09-10 — owner decision: keep it unconditional.** §5's row stands.

   Note for whoever implements this: **it is already the shipped behaviour, not
   a proposal.** `run_reset_catalog` (`crates/data/db/src/repo/cleanup.rs:65`)
   has `search_queries` in its unconditional `DELETE FROM` list, and
   `search_result_cache.query_id` is `ON DELETE CASCADE`
   (`0001_baseline.sql:254`) with `PRAGMA foreign_keys` ON
   (`catalog.rs:65`, asserted at `crates/data/db/src/tests.rs:85`). So the
   decision is to *keep* code that exists; there is no work item here, and the
   confirmation dialog's own text already promises it ("This removes registered
   folders and all search data").
3. **Should Reset be confirmable and reversible?** It is not today. Out of scope
   here but worth recording: an erasure action that is correct and instant is
   more dangerous than one that is incorrect.
