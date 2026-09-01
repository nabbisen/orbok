# RFC-060: Search Result Integrity

**Project:** orbok\
**RFC:** 060\
**Title:** Search Result Integrity\
**Status:** Proposed\
**Target milestone:** retrieval correctness\
**Date:** 2026-09-01\
**Related RFCs:** RFC-006 Adaptive Chunking and Location Metadata (§6 persists the field it defines); RFC-010 Optional Local Reranking (§8 decides its fate); RFC-013 Search View and Result Explanation UX; RFC-038 Result Freshness and Trust Badges (§7 makes its §16.4/5/7 true); RFC-041 Search, Narrow and Browse Around (§7); RFC-045 Search-in-Folder Flow (§7); RFC-003 Source Registration and File Access Boundary (§9 closes a bypass of its boundary)

---

## 1. Summary

The search result a user sees is wrong in five independent ways, all verified at
`3e26f92`:

```text
1. PDF, DOCX and HTML snippets read the wrong bytes — a chunk labelled
   "Page 3" of a PDF yields the excerpt "1 0 obj".
2. Every result is labelled trust state "Ready", including results for files
   deleted from disk.
3. Filter chips are rendered, stored, and serialized into history, and never
   applied to a query.
4. The selected search folder never scopes the query.
5. Paused, missing and permission-denied sources still return results, and
   their files are still opened to render excerpts.
```

Plus one boundary defect on the same code path: `load_snippet` opens files with
`std::fs::File::open` and no `PathGuard`, on what is the most frequently
executed read in the product.

The application's entire search entry point is 80 lines
(`crates/app/src/bootstrap/search.rs`). It takes `(query, limit)`. Every gap
above is visible in that one file.

---

## 2. Motivation

Individually these are five medium-sized fixes. Together they are most of what
the search UI appears to offer. A filter that does not filter and a badge that
always reads Ready are worse than shipping neither, because they invite the user
to trust a signal that carries no information.

The snippet defect is the most visible: three of the seven advertised document
types show binary garbage, raw markup, or nothing where the excerpt should be,
on every result, from the first search.

---

## 3. Goals

- Snippets show text from the document, for every supported format, or show
  nothing and say so.
- A result's trust state reflects the file's and its source's real state.
- Filters filter; folder scope scopes; source status is honoured.
- Every file read on the search path goes through the RFC-003 boundary.
- Decide, rather than continue to defer, whether reranking is part of orbok.

## 4. Non-Goals

- A query language (field terms, booleans, wildcards). Deliberate; out of scope.
- ANN vector indexing. RFC-023's exact-scan deferral was re-validated by the
  2026-09-01 audit and by Owner Task 003 Part A's measurement (vector scan is
  0.8% of search cost). It stands.
- Encoding detection for non-UTF-8 documents. Real, separate, task-routed.
- Ranking quality work beyond the two defects in §10.

---

## 5. Decision 1 — persist `location_kind`

**Root cause of the snippet defect.** `chunk_locations.line_start/line_end` means
different things per extractor — file lines for text/Markdown, **page numbers**
for PDF, **paragraph indices** for DOCX, **block indices** for HTML — and the
discriminator is dropped at the database boundary. `chunk_adapter.rs`'s module
doc says so: *"The `location_kind` field is carried through the pipeline but is
not yet persisted to a dedicated DB column (that comes with later RFC work on
result trust and snippet loading)."* Both of those RFCs (038, 041) shipped
eighteen months of releases ago.

**Add a new migration** — number `0007` or later; **never** edit a released
migration file (see RFC-062, which exists because that rule was broken once) —
adding `location_kind TEXT` to `chunk_locations`, and carry the field through
`ChunkSpec` → `ChunkRecord` → `chunk_adapter`.

Backfill: existing rows get `NULL`, which §6 treats as "unknown" and therefore
as not-`Lines`. That is the safe direction: a missing snippet is honest, a
binary excerpt is not.

## 6. Decision 2 — where snippets come from

**Only `LocationKind::Lines` reads the raw file.** For every other kind, render
from the cached `ExtractOutput` segments, which `orbok-cache` already stores and
which `embedding.rs::chunk_text` already reconstructs from. The data is present;
the snippet path simply does not use it.

**Interim guard, shippable before the migration:** return `None` unless
`location_quality == "exact"`. It is one condition and it removes the binary
garbage immediately, at the cost of no snippet for PDF/DOCX/HTML until §5 lands.
Recommended as part of Task 034 so that the visible defect stops now.

Two robustness fixes on the same function, from the audit:
- `BufRead::lines()` allocates one `String` per line with no cap, so a file with
  no newline materialises entirely to produce an 8-line snippet. Read through
  `Read::take(64 KiB)`.
- `(end - start + 1)` underflows when a stored `line_end < line_start` — a panic
  in debug. Use `saturating_sub` / `saturating_add`.

## 7. Decision 3 — wire trust, filters, folder scope, and source status

All four are the same shape: the capability exists and the entry point does not
carry the parameter. `run_search_with(context, probe, catalog, query, limit)`
grows a request struct.

| Capability | What exists | What is missing |
|---|---|---|
| **Trust** | `SearchResultTrust::from_catalog` — zero production callers | `bootstrap/search.rs:79` hardcodes `ResultTrustDisplay::default()` = `Ready`, empty actions |
| **Filters** | `ActiveFilter` in UI state, serialized into history; `extension_matches_kind` in `orbok-search` | Nothing passes filters to the query or applies them to results |
| **Folder scope** | `SearchFolderScope` in UI state; RFC-045's picker works | The scope never reaches `run_search` |
| **Source status** | `SourceState::is_searchable()` — never called | No retrieval query joins `sources`; four query sites need it (`fts5.rs`, `multilingual.rs`, `vector.rs`, `snippet.rs::chunk_records_for`) |

**Source status is the one to do first**, because it is also a correctness fix
for §9: a paused source's files are currently opened from disk to render
excerpts. Requiring `s.status = 'active'` closes that at the query layer as well
as at the guard layer.

**Filter application: at the query, not after.** Applying filters post-fusion
silently shrinks the result count below `limit` and makes "no results" ambiguous
between "nothing matched" and "everything was filtered out" — which RFC-041 §25.5
requires be distinguishable.

## 8. Decision 4 — the reranker: implement or remove

**This is the decision this RFC exists to force.** RFC-010 is marked
`Implemented (v0.4.0)`. The only `CrossEncoderReranker` is `MockReranker`, which
scores by passage **length**. `with_reranker` has no production caller. The
README advertises *"with optional local reranking"*.

Three options.

**(A) Remove the claim, keep the seam.** Delete the README claim, delete
`MockReranker` from anything but tests, mark RFC-010 honestly (RFC-063 decides
the mechanism), and leave the `CrossEncoderReranker` trait in place as a
documented extension point. *Recommended.* Reranking needs a second model —
another download, another 100+ MB, another forward pass per query — and this
project has an unmet p99 gate with **one** model. Adding a second before the
first is fast is the wrong order.

**(B) Implement it.** A real cross-encoder on top of the current architecture
would multiply per-query inference cost. Not before RFC-061 §6 makes model
construction a once-per-process cost, and not before RFC-048's p99 gate is met.

**(C) Leave it as it is.** Rejected. The status quo advertises a capability the
product does not have; that is the class of defect this whole review is about.

Either way, one small fix applies now: `enrich_many(&fused, limit)` truncates to
`limit` **before** reranking, so even a real reranker could only reorder what is
already visible and could never promote a candidate from fusion rank 21–50 —
which is exactly what `Limits::fusion_n = 50` exists for. Enrich `fusion_n`,
rerank, then truncate.

## 9. Decision 5 — the search path respects the RFC-003 boundary

`snippet.rs` contains zero references to `PathGuard` or `ValidatedPath` and
calls `std::fs::File::open` on `files.canonical_path`. `path_guard.rs`'s own doc
states: *"Before any backend code reads a file it must obtain a
`ValidatedPath`."* The README states the backend *"never reads arbitrary
filesystem paths."*

Route `load_snippet` through the guard. Note what this does **not** fix: the
guard is a time-of-check/time-of-use design (canonicalize, check membership,
open later by path), so a path replaced by a symlink in between still escapes.
That is a known, separate, lower-severity limitation and is recorded rather than
solved here — but "the boundary is TOCTOU" is a defensible position and "the
boundary is not called" is not.

## 10. Decision 6 — the two ranking defects on this path

Both are small and both are routed to Task 034 rather than waiting for this RFC.
They are recorded here because they change what a "correct result" is:

1. **CJK ranking is inverted.** `multilingual.rs:117` and `:143` sort descending
   on a score the project's own doc (`lib.rs:63`) defines as lower-is-better.
   Reversing the comparator is necessary and **not sufficient**: the merge
   compares `bm25(chunk_fts)` against `bm25(chunk_fts_trigram)`, which are not
   on a common scale. Fuse the two candidate lists with `rrf_fuse` instead.
2. **Fusion is non-deterministic.** `rrf.rs:61` collects from a `HashMap` and
   sorts on score alone; ties keep map iteration order. Ties are structural
   (keyword rank 1 / vector rank 5 scores identically to 5 / 1). Add a
   `chunk_id` tie-break to `rrf_fuse` and to both merges.

One further retrieval-quality item, lower priority: the parent `"document"`
chunk carries the whole file's text, is indexed into both FTS tables, and
nothing dedupes by `file_id` — so a matching file often consumes two of twenty
result slots with a document-level blur and a section-level match. Exclude
`chunk_kind = 'document'` from retrieval or cap results per file.

---

## 11. Acceptance criteria

Phrased per RFC-058 §5. These are the same assertions RFC-058 §6 requires, and
they must be observed failing before their fixes land.

1. With a three-page PDF indexed and a query matching text on page 3, the
   returned snippet contains words from that page and does not contain PDF
   object syntax.
2. With a DOCX and an HTML file indexed, their results' snippets contain
   document text, or are empty with the result still shown — never raw markup or
   binary.
3. With a file indexed and then deleted from disk, its result carries a
   non-`Ready` trust state and at least one recovery action.
4. With a `.pdf` and a `.md` both matching, applying the Documents filter
   returns the `.pdf` and not the `.md`; removing the filter returns both.
5. With two sources registered and a query matching a file in each, a search
   scoped to source A returns only A's file.
6. With a source set to paused, a query matching its files returns no results,
   and no file under that source is opened during the search (observable via the
   guard, not by inspection).
7. Two identical searches issued in one process return identical result orders,
   over 20 repetitions.
8. With a short, highly relevant Japanese chunk and a long, weakly relevant one,
   the relevant chunk ranks first.
9. `load_snippet` called with a path outside every registered source returns an
   error rather than file contents.

---

## 12. Open questions

1. **§8 — implement or remove the reranker.** Owner decision. My recommendation
   is (A): remove the claim, keep the seam, revisit after the p99 gate is met.
2. **Snippets for `Pages`/`Paragraphs`/`Blocks` when the extraction cache has
   been cleared.** §6 renders from the cache; RFC-059 gives the cache a finite
   lifetime. So a snippet can become unavailable for an old result. Options:
   fall back to no snippet (honest, simple), or re-extract on demand (correct,
   expensive, and re-reads a user file at query time — which has its own privacy
   shape). Proposal: no snippet, with the result still shown. Needs a decision
   before §6 is implemented.
3. **Does a `NULL` `location_kind` on a backfilled row mean "unknown" or "lines"?**
   §5 says unknown. That is safe but it means every chunk indexed before the
   migration loses its snippet until re-indexed — which, once Task 035 lands,
   happens on the next rescan. Acceptable, but it should be a stated consequence
   rather than a discovered one.
