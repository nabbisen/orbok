# Implementation Handoff — RFC-059: Erasure Completeness and Cache Lifetime

**Project:** orbok\
**RFC:** 059\
**Lifecycle stage:** Accepted 2026-09-02; open question 2 resolved by the owner 2026-09-10. Unstarted.\
**Primary owner:** `crates/data/db/src/repo/cleanup.rs`, `.../repo/chunks.rs`, `crates/search/engine/src/{fts5,multilingual}.rs`, `crates/pipeline/workers/src/cleanup_service.rs`, `crates/data/cache/src/service.rs`\
**RFC:** [`../accepted/059-erasure-completeness-and-cache-lifetime.md`](../accepted/059-erasure-completeness-and-cache-lifetime.md)

---

## 0. Read this section before the RFC

**The RFC's two leaks are real and I re-verified both on 2026-09-10. But the RFC
mis-targets one fix, understates one leak, and does not know that the erasure
mechanism it wants already exists in the dependency.** Three corrections, all
established by reading the code rather than the RFC:

**(i) `Fts5KeywordEngine::index` has no production caller.** §6's "prerequisite,
and it is not optional" paragraph tells you to fix the replace-on-reindex delete
at `crates/search/engine/src/fts5.rs:83`. That method is called from **tests
only** — `grep -rn "\.index(" crates/` returns no non-test call site. Production
indexing goes `crates/pipeline/workers/src/chunk_and_index.rs:64` →
`ChunkRepository::insert_bundle`. Fixing `fts5.rs:83` would change nothing that
runs.

The RFC's *diagnosis* still holds, one file over: `insert_bundle` mints
`ChunkId::generate()` per chunk per call (`crates/data/db/src/repo/chunks.rs:75`),
so its `ON CONFLICT(chunk_id) DO UPDATE` at `chunks.rs:132` never fires either,
and its replace step (`chunks.rs:176-181`) only marks the previous chunks
`stale` — it deletes no FTS row in either table. **That** is the
replace-on-reindex path, and it is where §6's prerequisite belongs.

**(ii) The orphaning is not trigram-only.** §1(a) says the trigram index is
never cleared. True — `chunk_fts_trigram` has exactly one INSERT
(`chunks.rs:120`), reads at `multilingual.rs:167-171`, and zero DELETEs in the
workspace. But look at what `remove_replaced_stale_indexes` actually is
(`crates/data/db/src/repo/cleanup.rs`), in full:

```sql
DELETE FROM chunks WHERE chunk_status IN ('stale','deleted') AND file_id IN
  (SELECT file_id FROM chunks WHERE chunk_status = 'active')
```

One statement. `keyword_index_records.chunk_id` is
`PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE`
(`0001_baseline.sql:226`) and that mapping table is — per its own schema comment
at `:227-228` — **the only `chunk_id` ↔ FTS-rowid link that exists**, because
both FTS tables are contentless. So the cascade destroys the addresses before
anything uses them, and the rows in **`chunk_fts` *and* `chunk_fts_trigram`**
are orphaned: unreachable by search, unreclaimable by any cleanup, permanent.
Same for `MultilingualKeywordEngine::delete` (`multilingual.rs:93`), which drops
the mapping row while deleting only the unicode61 FTS row.

`chunk_fts` escapes the *Reset* leak because `run_reset_catalog` issues its
`'delete-all'` command (`cleanup.rs`). It does not escape this one.

**(iii) Erasure does not need file deletion and does not need upstream.**
§7 recommends (A) "delete and recreate `orbok-cache.sqlite3` on Reset" and names
its own risk — deleting an open database file — and holds the correct mechanism,
(C) a `clear_namespace()` API, for an upstream request. **localcache 0.21.1
already exposes `keys(path_like: Option<&str>)` and `remove(path)`**, and both
are namespace-scoped because `CacheService::engine` builds every engine with
`.namespace(namespace.as_namespace())` (`crates/data/cache/src/service.rs:86`).
`keys(None)` then `remove()` each, then the `shrink_database()` that
`purge_all_cache_namespaces` already calls, erases a namespace today, in-process,
with no open-handle sequencing problem and nothing to wait for.

Take that route. §7's option (A) stays available as a fallback and its TTL/cap
half is unaffected — see Slice 3.

---

## 1. What is actually broken, re-verified 2026-09-10

| Claim | Verified how | Holds? |
|---|---|---|
| `DELETE FROM chunk_fts_trigram` occurs zero times | `grep -rn chunk_fts_trigram crates/` — one INSERT, four read sites, nothing else | yes |
| Extraction cache opened unbounded | `EngineOptions::default()` is the derived `Default` (`cache/src/service.rs:25-32`) = `ttl: None, max_entries: None`, at all six `ExtractSegments` open sites | yes |
| "Purge all namespaces" cannot match anything | `purge_all_cache_namespaces` (`workers/src/cleanup_service.rs:147`) runs `purge_stale_versions` (matches a payload version other than the current v1), `cleanup_expired` (needs a TTL; there is none), `shrink_database` (VACUUM) | yes |
| Reset clears search history | `run_reset_catalog`'s `DELETE FROM` list includes `search_queries`; `search_result_cache` cascades (`0001_baseline.sql:254`); `PRAGMA foreign_keys` ON (`catalog.rs:65`) | **already true — no work** |

**One thing worth naming while you are in there.** All three calls in
`purge_all_cache_namespaces` are `let _ = engine.…`. A function named *purge all*
purges nothing and discards every result it would need to notice that. It is the
same shape as RFC-061 §8's discarded-`Err` finding and the same shape as the
audit's original complaint about this RFC's subject: **a name asserting more than
the body does.** Fix the discards in the same slice that fixes the body.

## 2. Slices, ordered by dependency

### Slice 1 — Reset erases (RFC §5, §6.1, §7; criteria 1, 2, 3)

Two changes, both small, and this is the slice that pays for the RFC:

1. `run_reset_catalog`: add
   `INSERT INTO chunk_fts_trigram(chunk_fts_trigram) VALUES('delete-all')`
   beside the existing `chunk_fts` one.
2. `purge_all_cache_namespaces`: replace the three no-op maintenance calls with
   a real per-namespace erase — enumerate with `keys(None)`, `remove()` each,
   then `shrink_database()` — and stop discarding the results. Report the count.

**Criterion 3 (`settings.json` and model artifacts byte-identical after Reset)
is a guard against your own fix, not against the current code.** Assert it by
hashing before and after; `models` is deliberately in `run_reset_catalog`'s
`DELETE FROM` list with a comment explaining why (`cleanup.rs`), and that comment
is about catalog rows, not the artifacts on disk. Do not "tidy" it.

### Slice 2 — stop orphaning FTS rows (RFC §6.2, §6.3 and the corrected
prerequisite; criterion 4)

Every path that drops a `keyword_index_records` row must delete both FTS rows
**first**, while the rowids are still addressable. Three sites, per §0(i) and
§0(ii):

- `Fts5KeywordEngine::delete` — add the trigram delete beside the unicode61 one,
  before the mapping-row delete.
- `remove_replaced_stale_indexes` — `SELECT fts_rowid, trigram_fts_rowid` for the
  chunks about to go, delete those FTS rows, then run the existing `DELETE FROM
  chunks` and let the cascade follow.
- `insert_bundle`'s replace step (`chunks.rs:176-181`) — delete the previous
  generation's FTS rows, addressed by `file_id`, before inserting the new chunks.
  **Not** by `chunk_id`; that is the mistake §0(i) describes.

**The invariant test is the deliverable, not the fixes.** §6's two counts, both
tables, asserted after each of: a re-index, each of the three cleanup actions,
and Reset. Write it first and watch it fail on today's code — if it passes before
you change anything, it is not testing what you think.

### Slice 3 — a finite lifetime for the extraction cache (RFC §7; criterion 5)

Give `ExtractSegments` a TTL and `max_entries` at its open sites. **RFC-059 open
question 1 has no answer and you are not expected to invent one:** take one
measurement pass on a real corpus (entry count and payload bytes per indexed
document, from `CacheService::usage` and `entry_count`), propose a cap derived
from it, and put the number in the submission for the architect to route. A cap
expressed as a fraction of catalog size is the RFC's proposal, not a decision.

### Slice 4 — expose the two working cleanup actions (RFC §8; criterion 6)

`ClearTemporaryExtraction` and `RemoveReplacedStaleIndexes` are implemented and
reachable from no UI. Add both to the Storage view's Safe cleanup row —
**after Slice 2**, per §8, so `RemoveReplacedStaleIndexes` frees bytes instead of
reporting rows deleted while reclaiming nothing. New user-visible strings go
through `crates/ui/src/i18n.rs` in both locales; `crates/ui/src/tests/rfc041_search.rs`'s
exhaustive key guard will fail the build if you add an English key without its
Japanese pair.

### Slice 5 — the README (RFC §9; criterion 7)

Task 034 already corrected the text downward. This slice restores the stronger
claim **only** for what Slices 1 and 3 make true, and criterion 7 is verified by
re-running the audit's claim check, not by reading the paragraph.

## 3. Definition of done

- The §6 invariant test exists, covers both tables, covers all four operations,
  and was observed failing before Slices 1–2 landed.
- Reset leaves no retrievable extraction-cache entry, verified through
  `CacheService`, not by inspecting the file.
- A trigram query for a term from the reset corpus returns zero rows, queried
  against `chunk_fts_trigram` directly — criterion 1 is explicit that going
  through the search API does not count.
- `purge_all_cache_namespaces` returns a count and no call site discards it.
- The extraction cache's TTL and cap are set, and the measurement behind the
  numbers is in the submission.
- A closure record per RFC-063, one row per acceptance criterion, naming what was
  run and what was observed.

## 4. Stop conditions

- **`keys()` + `remove()` turns out not to be namespace-scoped in practice.**
  I verified the builder sets `.namespace()` and that localcache documents
  `entry_count` as "entries in the current namespace", but I did not run it. If
  an erase of one namespace removes another's entries, stop — that is an upstream
  report, and §7's option (A) becomes the route.
- **The Slice 2 invariant collides with chunk reactivation.** There are two
  stale-marking paths and they are not the same, so do not treat them alike.
  `insert_bundle:176` marks a *superseded* extraction stale; the missing-file
  cascade below it marks a file's chunks stale because the file vanished, and
  its own comment (`chunks.rs:204-209`) says why that one must stay
  restorable — *"unlike a genuinely superseded extraction, a missing file can
  return with byte-identical content … `reactivate_last_stale_generation` needs
  the chunks intact"*. Superseded chunks are never reactivated, so deleting
  their FTS rows is safe; a missing file's are, and deleting theirs would leave
  reactivation restoring chunks that no index can find. If your reading of
  `reactivate_last_stale_generation:246` disagrees with that split, stop —
  it is a design question, not an implementation one.
- **The measurement in Slice 3 suggests a cap that would evict during a normal
  indexing run.** A cache that thrashes is worse than one that is large. Report
  the number rather than picking a compromise.

## 5. Not in scope

- Encryption at rest (RFC-026 is withdrawn; §4).
- Secure-deletion / anti-forensic guarantees (§4 — and claiming them would be
  this RFC's own defect in a new place).
- Storage-accounting accuracy. The dashboard double-counts and understates the
  keyword index by ~1.8×; that is a separate task.
- Making Reset reversible or adding a second confirmation (§11 q3, out of scope
  there and here).
- **Search history.** The owner decided 2026-09-10 that Reset clears it
  unconditionally, and the code already does. Change nothing.

## 6. Upstream

RFC §7 option (C) asks localcache for `clear_namespace()`. **Still worth sending,
but the ask is now smaller and must say so:** we can erase a namespace today with
`keys()` + `remove()`; what an API would add is atomicity and one statement
instead of O(n) round-trips. The architect sends this; do not open it from the
implementation.
