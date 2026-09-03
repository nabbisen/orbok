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
5. With the extraction cache holding entries older than the configured TTL,
   running Clear temporary extraction from the Storage view reports a non-zero
   byte reclaim and the entries are no longer retrievable.
6. Invoking Remove replaced stale indexes after a re-index reports a byte
   reclaim greater than zero and reduces the on-disk keyword-index size.
7. The README's data-lifecycle section describes the behaviour that ships,
   verified by re-running the audit's claim check against it.

---

## 11. Open questions

1. **TTL and entry-cap values for `ExtractSegments`.** No measurement exists.
   Proposal: start from a storage budget (e.g. cap the namespace at a fraction
   of the catalog size) rather than a time, because "how long ago" is not what
   the user cares about here. Needs one measurement pass on a real corpus.
2. **Does Reset clear search history unconditionally?** §5 says yes, on the
   grounds that history is derived from the user's queries. RFC-042 §13.4 has
   its own "turn off and clear" semantics; if the owner reads history as
   user-authored content rather than derived state, Reset should prompt rather
   than assume. **Owner decision.**
3. **Should Reset be confirmable and reversible?** It is not today. Out of scope
   here but worth recording: an erasure action that is correct and instant is
   more dangerous than one that is incorrect.
