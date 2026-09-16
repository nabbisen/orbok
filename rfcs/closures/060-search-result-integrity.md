# Closure Record — RFC-060: Search Result Integrity

**RFC:** [060](../accepted/060-search-result-integrity.md), amended three
times: Amendment 1 (PDF extraction and `location_quality`), Amendment 2
(the vector lookup-key bug), Amendment 3 (both open questions closed, and
the slices 2–5 handoff).
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-060-slice1-pdf-extraction.md`
(Slice 1) and `rfcs/handoffs/HANDOFF-060-slices2-5-wiring-snippets-and-the-guard.md`
(Slices 2–5). Commits: `8208164`, `4aecd30` (Slice 1), `894e990`
(Amendment 2's fix, landed with RFC-061 Slice 4), `3e578fd` (Slice 2),
`67b9cc1` (Slice 3), `2d90e1c` (Slice 4a), `39875db` (Slice 4b),
`763c535` (Slice 5).
**Transcribed, not re-derived**, from what was run: every row below names
the test and what was observed, including the two criteria this
environment cannot evidence.

---

## §11 acceptance criteria

### 0. *(Amendment 1.)* A three-page PDF whose page objects are not numbered 1/2/3 is extracted with all three pages' text present and no `PossiblyScannedPdf` warning.

→ what was run: the extractor tests in
`crates/pipeline/extract/src/tests.rs` against a fixture built with font,
resources and content streams allocated first, so object ids and page
numbers deliberately disagree.
→ what was observed: PASS since Slice 1 (`8208164`). `pdf.rs` passed each
page's *object* id to `lopdf::extract_text`, which takes 1-based page
numbers, so every page came back empty and the file was reported as
possibly scanned.
→ where verified: `cargo test -p orbok-extract`.

### 0a. *(Amendment 2.)* With a real embedding model configured, a search returns at least one result carrying the `Semantic` badge.

→ **Not evidenced in this environment, and not claimed.** The assertion
exists in `two_identical_searches_return_identical_orders`
(`crates/app/src/wired_application_tests.rs`), added when Amendment 2's
`model_id` bug was fixed in `894e990`, and is `#[ignore]`d: no ONNX model
is available in CI or this sandbox, the same constraint RFC-058 §11 open
question 2 names.
→ what to run when a model is available:
`cargo test -p orbok --bin orbok --features orbok-embed/tract --release -- --ignored`.

### 1. With a three-page PDF indexed and a query matching text on page 3, the returned snippet contains words from that page and does not contain PDF object syntax.

→ what was run: `pdf_result_snippet_contains_page_text_not_raw_bytes`
(`crates/app/src/wired_application_tests.rs`), driving the real pipeline
and `bootstrap::run_search`.
→ what was observed: PASS, and **RFC-058 §6 row 6's `#[should_panic]`
wrapper is removed** — the wrapper this criterion was named as removing.
Mutations, each restored byte-identical: routing non-`Lines` kinds back to
the file read leaves the snippet empty and fails the criterion; bypassing
the quality gate as well returns `"%PDF-1.5\n1 0 obj"` and fails it with
the raw syntax.
→ where verified: `cargo test -p orbok --bin orbok pdf_result_snippet`.

### 2. With a DOCX and an HTML file indexed, their results' snippets contain document text, or are empty with the result still shown — never raw markup or binary.

→ what was run: `docx_and_html_snippets_contain_document_text_never_markup`
(same file), asserting the criterion's own disjunction.
→ what was observed: PASS, and both formats render **real document text**
today, not the permitted empty — checked by temporarily strengthening the
assertion to require the marker, then restoring the criterion's wording.
→ **A weak fixture found by mutation, and fixed:** the first HTML fixture
was a single line, which cannot distinguish fix from defect — reading
"block 2" as line 2 of a one-line file skips past the end and yields the
empty snippet the criterion allows. With a realistic multi-line fixture
the mutation fails, naming the markup it leaked (`found "</" in
"<head><title>t</title></head>\n<body>"`).
→ where verified: `cargo test -p orbok --bin orbok docx_and_html_snippets`.

### 3. With a file indexed and then deleted from disk, its result carries a non-`Ready` trust state and at least one recovery action.

→ what was run: `a_result_for_a_file_deleted_from_disk_is_not_labelled_ready`
(same file), which deliberately does **not** refresh the source first.
→ what was observed: PASS, and **RFC-058 §6 row 2's `#[should_panic]`
wrapper is removed**. The catalog alone could not close this: a deleted
file still reads `indexed` until a rescan, which is the window the row
occupies, so `trust_for` reports a file that is no longer on disk as not
found regardless of the row. Mutations: restoring
`ResultTrustDisplay::default()` fails with `state: Ready,
recovery_actions: []`; deriving trust from the catalog row alone fails the
same way, proving the on-disk check is load-bearing.
→ where verified: `cargo test -p orbok --bin orbok a_result_for_a_file_deleted`.

### 4. With a `.pdf` and a `.md` both matching, applying the Documents filter returns the `.pdf` and not the `.md`; removing the filter returns both.

→ what was run: `a_kind_filter_returns_only_that_kind` (same file), through
`bootstrap::scope_from_ui` — the same conversion `main.rs` uses, not a
hand-built scope.
→ **Vocabulary deviation, deliberate and recorded in the test:** this
codebase's `KindFilter` has a dedicated `Pdfs` kind, and `Documents` means
Office documents (`docx`, `doc`, `odt`, `rtf`). The filter that selects a
PDF here is `Pdfs`; the assertion is the criterion's.
→ what was observed: PASS. Mutation: dropping the kind predicate returns
the `.md` alongside the `.pdf`.
→ where verified: `cargo test -p orbok --bin orbok a_kind_filter_returns_only`.

### 5. With two sources registered and a query matching a file in each, a search scoped to source A returns only A's file.

→ what was run: `a_search_scoped_to_one_folder_excludes_the_other` (same
file), also through `scope_from_ui`.
→ what was observed: PASS. Mutation: dropping the folder predicate returns
folder B's file in a search scoped to folder A.
→ where verified: `cargo test -p orbok --bin orbok a_search_scoped_to_one_folder`.

### 6. With a source set to paused, a query matching its files returns no results, and no file under that source is opened during the search (observable via the guard, not by inspection).

→ what was run: `a_paused_source_yields_no_results_and_no_file_is_opened`
(`crates/search/engine/src/tests/rfc060_source_status.rs`), asserting
**each of the four retrieval sites separately** — unicode61 keyword,
trigram keyword, vector scan, enrichment lookup — plus the search service,
plus that the guard refuses the paused source's file.
→ **The first version of this test was too weak, and mutation is what
showed it.** It asserted only through `SearchService`, and stayed green
with the keyword join removed: the enrichment join dropped the record
instead, which is exactly the post-filtering RFC-041 §25.5 forbids passing
itself off as a fix. Rewritten per-site, the four mutations now fail in
their own position: `(1,0,0,0)`, `(0,1,0,0)`, `(0,0,1,0)`, `(0,0,0,1)`.
→ RFC-058 §6 row 5's `#[should_panic]` wrapper is removed.
→ where verified: `cargo test -p orbok-search rfc060`; `cargo test -p orbok --bin orbok a_paused_source`.

### 7. Two identical searches issued in one process return identical result orders, over 20 repetitions.

→ **The end-to-end assertion is not evidenced here, and is not claimed.**
`two_identical_searches_return_identical_orders` is `#[ignore]`d for the
reason its own doc comment gives: the tie `rrf_fuse`'s `chunk_id`
tie-break exists to break only occurs when a keyword rank and a vector
rank cross, which needs real vector candidates from a real model. A
keyword-only fusion cannot produce it.
→ what *was* run, and passes without a model: the mechanism's own unit
tests, `rrf_fuse_is_deterministic_under_structural_ties` and
`rrf_fuse_keyword_lists_is_deterministic_under_structural_ties`
(`crates/search/engine/src/tests/task034_ranking_fusion.rs`), which
construct the structural tie directly.
→ Slice 5's per-file cap does not affect this: it walks fused candidates
in rank order, so it is deterministic by construction.

### 8. With a short, highly relevant Japanese chunk and a long, weakly relevant one, the relevant chunk ranks first.

→ what was run: `cjk_merge_ranks_the_dense_relevant_chunk_first` (same
file), unchanged by this handoff.
→ what was observed: PASS. Fixed by Task 034 (the CJK merge fuses via
`rrf_fuse_keyword_lists` rather than sorting incomparable bm25 scales).
→ where verified: `cargo test -p orbok-search task034_ranking`.

### 9. `load_snippet` called with a path outside every registered source returns an error rather than file contents.

→ what was run: `load_snippet_refuses_a_path_outside_every_registered_source`
(`crates/search/engine/src/tests/rfc060_source_status.rs`). The entry point
is now `SnippetSource::load`, `load_snippet`'s successor.
→ what was observed: PASS. Mutation: bypassing the guard returns the
stray file's contents and fails the test with them in the message
(`got Ok(Some("secret text outside every source"))`).
→ **TOCTOU is recorded, not closed** (RFC-060 §9 accepts it as separate):
the guard canonicalises and checks membership, then the file is opened by
path, so a path swapped for a symlink in between still escapes. "The
boundary is TOCTOU" is defensible; "the boundary is not called" was not.
→ where verified: `cargo test -p orbok-search rfc060`.

---

## §10's own subject: document-chunk duplication

The handoff (§4) offered two fixes and asked for a measurement first.
Measured against this repository's own `rfcs/` tree — 113 files, 15
queries, top 20 each — **before** any change:

| | |
|---|---|
| Duplicate slots (a file already shown) | **174 of 300** |
| Queries repeating at least one file | **14 of 15** |
| Worst single file | **18 of 20 slots** |
| Results that were the whole-file `"document"` chunk | **11 of 300** |

So the handoff's first option (exclude `chunk_kind = 'document'`) would
have reclaimed 11 slots of 300 and left the rest: the duplication is
mostly *section* chunks of one file competing with each other. The
measurement chose the second option. After capping to one result per file,
and widening the keyword candidate pool because the old sizing assumed one
candidate becomes one result: **0 duplicate slots, and 209 distinct-file
results where there were 126.** Benchmark p99 at 1,000 documents is
unchanged (139.9 / 139.9 / 143.1 ms against a 200 ms budget) and Recall@5
stays at 88%.

The measurement is kept as an `#[ignore]`d test
(`crates/app/src/rfc060_duplication_measurement.rs`) so the numbers can be
re-derived rather than trusted.

## The `sources` join's cost (handoff §6 stop condition 2)

Not triggered, measured rather than assumed. Alternating A/B at 1,000
documents, same binary pair, no builds in between: p99 median **145.1 ms
with the joins against 141.0 ms without**, versus a 200 ms budget. Both
arms produce occasional ~40 ms outliers on this machine, so single samples
do not decide it — an early 194 ms reading on the with-join arm was
matched by a 182 ms outlier on the baseline arm, which has no joins at all.

## Stop conditions

- **§6 first condition — the cached `ExtractOutput` not addressable from
  the search path — did not apply.** Slice 2 gives the snippet path a
  `ValidatedPath`, which is exactly the key `CacheService::get_fresh`
  needs; the search service gained the cache handle `main.rs` already had.
- **§6 second condition (benchmark p99)** — measured above, not triggered.
- **§6 third condition (a migration changing an existing column)** — did
  not apply: `0008` adds a nullable column.

## Full gate suite, this implementation

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
(0 failures), every `scripts/check-*.sh` gate, `mdbook build`, and
`git diff --check` — green for each of the five commits.
