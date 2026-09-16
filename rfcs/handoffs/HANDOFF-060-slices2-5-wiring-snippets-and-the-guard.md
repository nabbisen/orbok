# Implementation Handoff — RFC-060 Slices 2–5: source status, real snippets, the request struct, and the path guard

**Project:** orbok\
**RFC:** 060 — §5, §6, §7, §9, §10 tail; Amendment 3 closes both open questions\
**Lifecycle stage:** Accepted; Slice 1 shipped (`8208164`, `4aecd30`), Amendment 2's fix shipped (`894e990`). Everything below is unstarted.\
**Primary owner:** `crates/search/engine/src/{snippet,hybrid,service,fts5,multilingual,vector}.rs`, `crates/app/src/bootstrap/search.rs`, `crates/data/db/`\
**RFC:** [`../done/060-search-result-integrity.md`](../done/060-search-result-integrity.md)

---

## 0. What is already done — do not redo it

Read Amendment 3 §4c.3 first. In short: §10's two ranking defects are fixed
(the CJK merge fuses via `rrf_fuse_keyword_lists`, `rrf_fuse` has a `chunk_id`
tie-break), §6's robustness fixes are in (64 KiB cap, saturating arithmetic),
and **§8 needs no work**: the reranker claim is gone, the seam remains, and
RFC-010 is parked in `proposed/`.

**Both open questions are closed.** The one that shapes your work: **the
snippet path never extracts.** It reads the cached segments or returns `None`
with the result still shown.

## 1. Slice 2 — source status at the query layer, and the path guard

These two go together because they close the same hole from two directions:
**a paused source's files are currently opened from disk to render snippets.**

**(a) `SourceState::is_searchable()` has no caller.** Four retrieval sites need
a `sources` join — `fts5.rs`, `multilingual.rs`, `vector.rs`, and
`snippet.rs::chunk_records_for` — so that a non-active source contributes no
candidates. Do it in SQL, not by filtering afterwards: post-filtering shrinks
the result set below `limit` and makes "no results" ambiguous, which RFC-041
§25.5 forbids.

**(b) `snippet.rs` calls `std::fs::File::open` directly** (`:37`), with no
`PathGuard`/`ValidatedPath` anywhere in the module, while `path_guard.rs`'s own
doc says *"Before any backend code reads a file it must obtain a
`ValidatedPath`"* and the README says the backend "never reads arbitrary
filesystem paths". Route it through the guard.

Task 045 split `load_snippet` into a path wrapper and `load_snippet_from(record,
impl Read)`. The guard belongs in the wrapper; the reading half stays unchanged.

**Record, do not fix, the TOCTOU limitation:** the guard canonicalises and
checks membership, then opens by path later, so a path swapped for a symlink in
between still escapes. RFC-060 §9 already says this is accepted and separate.
"The boundary is TOCTOU" is defensible; "the boundary is not called" is not.

**Criterion 6** is the test: with a source paused, a query matching its files
returns no results **and no file under it is opened** — observable through the
guard, not by inspection. Assert the non-opening, or the test only checks half
of what it claims.

## 2. Slice 3 — persist `location_kind`, then render snippets by kind

**Depends on Slice 2 only for review order, not for code.**

`location_kind` exists in the pipeline (`extract/src/types.rs:218,257`) and is
dropped at the database boundary. Add **migration `0008`** — never edit a
released migration, which is what RFC-062 exists about — adding `location_kind
TEXT` to `chunk_locations`, and carry the field `ChunkSpec` → `ChunkRecord` →
`chunk_adapter`. Existing rows get `NULL`, read as "unknown", which is
not-`Lines` and therefore yields no snippet: honest, per Amendment 1 §2a.3's
disposition of the backfill question.

Then §6: **only `LocationKind::Lines` reads the raw file.** Pages, Paragraphs
and Blocks render from the cached `ExtractOutput` segments — the same source
`embedding.rs` already reconstructs chunk text from. If the cache has no entry,
return `None`. Do not extract.

Criteria 1 and 2. Criterion 2's wording matters: a DOCX or HTML result's
snippet contains document text **or is empty with the result still shown** —
never markup or binary.

## 3. Slice 4 — the request struct: trust, filters, folder scope

`run_search(catalog, model, query, limit)` (`bootstrap/search.rs:28`) has no
parameter surface for any of these, which is why RFC-058's rows 3 and 4 could
not be written. Grow it into a request struct and thread it through.

| Capability | What exists | What is missing |
|---|---|---|
| Trust | `SearchResultTrust::from_catalog` | `bootstrap/search.rs` hardcodes `ResultTrustDisplay::default()` = `Ready` |
| Filters | `ActiveFilter` in UI state; `extension_matches_kind` | nothing passes filters into the query |
| Folder scope | `SearchFolderScope`; RFC-045's picker | the scope never reaches `run_search` |

Filters apply **at the query**, for the same reason as §1(a).

Criteria 3, 4, 5. When this lands, RFC-058 §6's rows 3 and 4 become writable —
say so in the submission so that work is not forgotten.

## 4. Slice 5 — document-chunk duplication (lowest priority)

The parent `"document"` chunk carries the whole file's text and is indexed into
both FTS tables; nothing dedupes by `file_id`, so one file can occupy two of
twenty slots with a document-level blur plus a section-level match. Either
exclude `chunk_kind = 'document'` from retrieval or cap results per file.

**Measure before choosing.** On a real corpus, count how often a file appears
twice in the top 20 today. If it is rare, this is not worth the retrieval
change, and saying so with the number is the right outcome.

## 5. Definition of done

- Criteria 1–6 have tests that were **observed failing** before their fixes;
  criterion 7 (determinism over 20 repetitions) already has a test from
  RFC-058's work — confirm it still passes rather than rewriting it.
- No released migration edited; `scripts/check-migration-integrity.sh` green.
- The snippet path contains no extraction call. Grep for it in the submission.
- A closure record per RFC-063 when the last slice lands, one row per
  criterion including 0 and 0a.
- CHANGELOG entries per slice.

## 6. Stop conditions

- **The cached `ExtractOutput` turns out not to be addressable from the search
  path** without re-reading the file (e.g. the cache key needs a
  `ValidatedPath` the search path does not hold). Report it: that changes §6's
  design, and re-extracting is *not* the fallback — Amendment 3 §4c.1 rules it
  out.
- A `sources` join measurably worsens p99 on the RFC-048 benchmark. Report the
  numbers rather than dropping the join.
- Slice 3's migration needs to change an existing column rather than add one.
  Stop; that is a different conversation.
