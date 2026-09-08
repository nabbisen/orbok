# Implementation Handoff — RFC-060 Slice 1: PDF extraction, and the quality the chunker never reads

**Project:** orbok\
**RFC:** 060 (Amendment 1 §4a only — §5/§6/§7 are a later slice)\
**Lifecycle stage:** Accepted 2026-09-02; Amendment 1 added 2026-09-08. This slice is unstarted.\
**Primary owner:** `crates/pipeline/extract/src/pdf.rs`, `.../chunker.rs`\
**RFC:** [`../accepted/060-search-result-integrity.md`](../accepted/060-search-result-integrity.md)

> **Scope rule:** This slice is **two production lines and the tests that prove
> them**. It does not touch `bootstrap/search.rs`, the `chunk_locations` schema,
> or snippet rendering — those are §5/§6/§7 and one of them is still waiting on
> an owner decision. If you find yourself writing a migration, stop.

---

## 1. Why this is its own slice, and why it goes first

RFC-060 §6 plans to render non-`Lines` snippets from the cached `ExtractOutput`
segments. **For real PDFs those segments are empty** (§4a.1). The plan rests on a
cache holding no content for the format it exists to fix, so §6 cannot be
evaluated — let alone measured — until extraction produces text.

Two lines close that. They are worth doing before the decision blocking §6
arrives.

## 2. Defect A — `pdf.rs:116` passes an object ID where lopdf wants a page number

**The evidence is in RFC-060 §4a.1; do not re-derive it.** The short form:
`extract_text(page_numbers: &[u32])` resolves through `get_pages()`'s
`page_number → object_id` map, and orbok passes the object ID. On a PDF whose
page objects land at 5/7/9 — which is what happens when font, resources and
content streams are allocated first, as any real writer does — every page
extracts empty and the file is reported `PossiblyScannedPdf`.

**The fix is one line.** `page_num` is already computed at `pdf.rs:106` and used
for `line_start`, `line_end`, `heading_path` and the `unreadable_pages` warning
— everywhere except the extraction call itself:

```rust
-            match doc.extract_text(&[*obj_id]) {
+            match doc.extract_text(&[page_num]) {
```

`obj_id` then becomes unused in the loop body; let clippy tell you and remove the
binding cleanly rather than prefixing an underscore.

### 2.1 The fixture is the actual work

**A fixture whose page objects happen to be numbered 1/2/3 cannot detect this.**
RFC-058's row 6 uses one — it reserves page IDs first, documented there as a
workaround — and I confirmed that applying the one-line fix does not change row
6's behaviour at all.

So build the opposite: allocate font, resources and content streams **first**, so
the page objects land wherever a real writer would put them. Assert:

1. All three pages' text is present in `ExtractOutput.segments`.
2. **No `PossiblyScannedPdf` warning**, and no `SomePagesUnreadable`.
3. Each segment's `line_start`/`line_end` is its **page number** (1, 2, 3) — not
   its object ID. The current code sets these correctly already; assert it so the
   fix cannot be "corrected" later by passing object IDs consistently everywhere.

**Observe it failing first.** Against unfixed code the fixture yields
`segments: []`, `char_count: 0`, `warnings: [SomePagesUnreadable{pages:[1,2,3]}, PossiblyScannedPdf]`.
Report that output.

This becomes RFC-060 acceptance criterion 0.

### 2.2 Replace the test that could not see this

`orbok-workers::v07_features::pdf_extractor_extracts_text_from_valid_pdf` carries
its own comment: *"May or may not extract text from this minimal PDF depending on
lopdf version… doesn't panic, returns Ok"* — it accepts either outcome.

**A test that tolerates an unknown cannot report one**, and this is what let a
total extraction failure look identical to a benign version difference for as
long as it has existed. Tighten it to assert extracted text, or delete it in
favour of §2.1's fixture and say which you did. Do not leave both, with one of
them still accepting failure.

## 3. Defect B — the chunker never reads the quality the extractors set

**RFC-060 §4a.2 describes this as the document chunk hardcoding
`location_quality`. That understates it, and the amendment is being corrected in
the same commit as this handoff.** What is actually true:

**Five extractors write `LocationQuality`** — `markdown.rs` and `text.rs` set
`Exact`, `html.rs` and `docx.rs` set `Approximate`, `pdf.rs` sets `PageOnly`.

**The chunker reads it zero times.** `grep -c '\.location_quality' chunker.rs`
against segment values returns **0**. Every chunk's quality is a bare literal
chosen from *how the chunker chunked*, not from *what the source supports*:

| `chunker.rs` | function | literal |
|---|---|---|
| `:65` | `chunk()` — the whole-file document chunk | `"exact"` |
| `:164` | `append_markdown_sections()` | `"exact"` |
| `:202` | `append_paragraph_chunks()` | `"exact"` |
| `:257` | `append_text_windows()` | `"approximate"` |
| `:279` | `empty_document_chunk()` | `"unknown"` |

`:164` and `:202` both derive `location_kind` from the segment and then hardcode
the quality beside it — the same asymmetry as `:65`, in two more places.

**PDF, DOCX and HTML all take `append_paragraph_chunks`.** So every chunk they
produce claims `"exact"`, whatever the extractor observed.

### 3.1 The consequence, which is why this is not cosmetic

Task 034 §5 shipped an interim guard: `load_snippet` returns `None` unless
`location_quality == "exact"`. It was reviewed, mutation-tested, and believed to
suppress snippets for PDF/DOCX/HTML until §5/§6 land.

**It has never suppressed anything for those formats**, because nothing in the
pipeline ever gives them a non-`"exact"` quality.

Its test (`non_exact_location_quality_yields_no_snippet`) constructs a
`ChunkRecord` with `"approximate"` **directly** and asserts `load_snippet`
honours it. That is true and it is all it proves. Nothing asserted that the field
ever *receives* a non-exact value in a real pipeline — so a guard with an inert
precondition passed as a working guard. **This is the recurring class, and it is
why RFC-058's row 6 fails with `"%PDF-1.5\n1 0 obj"` rather than with an absent
snippet.**

### 3.2 What to build

Derive each chunk's `location_quality` from the segments it spans, the same way
`location_kind` already is. The conservative rule — a chunk is only as good as
its worst segment:

```
all segments Exact                    -> "exact"
any segment Approximate or PageOnly   -> "approximate"
no segments / any Unknown             -> "unknown"
```

`ExtractedChunk.location_quality` is `&'static str` while
`ExtractedSegment.location_quality` is the `LocationQuality` enum, and **there is
no conversion function anywhere** — write one, in `chunker.rs` or `types.rs`, so
the mapping exists in one place rather than five.

`append_text_windows` (`:257`) keeps `"approximate"` regardless: the windower's
own boundaries are approximate even over exact segments. Say so in a comment.

**Observe it failing first**: assert a PDF's chunks come out `"approximate"`, and
watch that fail on today's `"exact"`.

### 3.3 Expect this to change what row 6 does

Once B lands, Task 034's guard starts firing for PDF, and RFC-058 row 6's failure
message will change from *"snippet must not contain raw PDF object syntax"* to a
missing snippet. **Row 6 is `#[should_panic(expected = "…raw PDF object syntax")]`
— it will break.**

That is correct behaviour, not a regression, and it is yours to handle: update row
6's expected message to the new failure, keeping it `#[should_panic]` until §6
renders real page text. **Do not delete the row and do not remove
`#[should_panic]`** — the assertion it makes (a PDF result's snippet contains
document text) is still unmet after this slice.

## 4. Not in scope

`chunk_locations`'s `location_kind` column and its migration (§5); rendering
snippets from the extraction cache (§6, and blocked on RFC-060 §12's open
question); anything in `bootstrap/search.rs` (§7).

## 5. Definition of done

1. `pdf.rs` extracts by page number; a fixture with non-coincidental page object
   IDs proves it, and was observed failing first.
2. `pdf_extractor_extracts_text_from_valid_pdf` no longer accepts both outcomes —
   tightened or deleted, stated which.
3. Chunk `location_quality` derives from spanned segments through one conversion
   function; a PDF's chunks come out `"approximate"`; observed failing first.
4. RFC-058 row 6 still runs, still `#[should_panic]`, with its expected message
   updated to the post-B failure.
5. Full suite green on all three `cross` legs.

## 6. Stop conditions

Stop and report if:

- Fixing A changes any **existing** extraction test's outcome. Nothing should
  regress; if something does, that test was asserting the broken behaviour.
- Deriving B's quality makes a chunk `"unknown"` that is currently `"exact"` for
  **text or Markdown**. Those extractors set `Exact`, so their chunks should stay
  exact; if they do not, the derivation is wrong.
- Row 6 cannot be kept `#[should_panic]` after B (§3.3).
- Either fix appears to need the `chunk_locations` schema. It does not — both are
  upstream of the DB boundary — and if it looks otherwise you have crossed into §5.
