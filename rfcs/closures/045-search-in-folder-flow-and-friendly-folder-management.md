# Closure Record — RFC-045: Search-in-Folder Flow and Friendly Folder Management

**RFC:** [045](../done/045-search-in-folder-flow-and-friendly-folder-management.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** the search-in-folder flow work that shipped it in
v0.20.0. Verified for this record by Task 038, decisions in Review 201
§4 — neither Task 038 nor Review 201 is git-tracked (RFC-063 §5):
Task 038 is `.git-exclude/tasks/dev-team/038-rfc063-closure-records.md`,
Review 201 is `.git-exclude/reviewed/201-task038-rfc041-tier-question-review.md`.
**Not reconstructed from the RFC text**: every criterion below was checked
against the running code (function/message names, call sites, and any
test that exercises it), per RFC-058 §5's own rule that a criterion with
nothing to put in *what was observed* is not a criterion.

---

## §22 acceptance criteria

### 1. User can type a query before choosing a folder.

→ what was run: code inspection of `Message::QueryChanged`
(`crates/ui/src/state.rs`).
→ what was observed: it updates `self.query` unconditionally, with no
gating on whether `search_location` has a selection.
→ where verified: `query_changed_message_updates_both_query_and_search_ui_text`
(`crates/ui/src/tests/rfc041_search_state.rs`) — a general query-update
test, not RFC-045-specific, but it exercises the exact code path this
criterion depends on.

### 2. Pressing Search without a location opens a folder picker.

→ what was run: code inspection of `Message::SubmitSearch`
(`crates/app/src/main.rs:290-311`).
→ what was observed: it checks `!app.state.search_location.has_selected()`,
then dispatches `Message::ChooseFolderRequested` and an `iced::Task` that
opens `rfd::AsyncFileDialog`.
→ where verified: structural only — no automated test drives
`SubmitSearch` with no location selected. This logic lives in `main.rs`'s
untested `iced` update loop, not `AppState::update`.

### 3. Cancelling folder picker keeps the query and shows no error.

→ what was run: code inspection of `Message::FolderPickerCancelled`
(`crates/ui/src/state.rs`).
→ what was observed: the handler only sets `picker_in_progress = false`;
it does not touch `query` and does not set `notice`.
→ where verified: structural only — no test in `crates/ui/src/tests/*`
exercises `FolderPickerCancelled`.

### 4. Selecting a folder starts search flow automatically.

→ what was run: code inspection of `Message::FolderPicked`
(`crates/app/src/main.rs:346-421`) and `Message::SearchLocationSelected`
(`crates/ui/src/state.rs:988-997`).
→ what was observed: `FolderPicked` resolves or creates the source, then
calls `bootstrap::run_search` immediately using `app.state.last_query`;
`SearchLocationSelected` sets `results_status = Searching` when a query
is pending.
→ where verified: structural only — no test drives `FolderPicked`/
`SearchLocationSelected` through the full path; only isolated state-layer
tests exist in `crates/ui/src/tests/rfc045_location.rs`.

### 5. The selected folder is created or reused internally as a remembered folder for P0.

→ what was run: code inspection of `Message::FolderPicked`
(`crates/app/src/main.rs:349-353`) and
`bootstrap::find_source_by_canonical_path`
(`crates/app/src/bootstrap/sources.rs`).
→ what was observed: `FolderPicked` calls `find_source_by_canonical_path`
before falling back to `bootstrap::add_source`, reusing an existing
`SourceCard` when the canonical path already exists.
→ where verified: structural only — `find_source_by_canonical_path` has
no dedicated test; its only exercise is this one call site.

### 6. Existing remembered folders are not duplicated.

→ what was run: same code path as criterion 5.
→ what was observed: the same canonical-path dedup covers this criterion
— a folder already registered as a source is reused, not re-added.
→ where verified: structural only, same evidence as criterion 5.
→ note, not a criterion failure: a *separate* feature —
`search_location.recent_locations` (the "recent folder chips" row,
rendered at `views.rs:275-291`, driven by `Message::RecentFolderSelected`)
— is never populated anywhere in `crates/app`. That chip list is
permanently empty in the shipped app, but this criterion is about
source-record deduplication, which the canonical-path mechanism above
satisfies independently of the chip feature.

### 7. Default search scope is "This folder and subfolders."

→ what was run: code inspection of `SearchFolderScope::default()`
(`crates/ui/src/state/location.rs:33-40`); a full-tree grep for
`SearchFolderScope` across `crates/app`, `crates/search/engine`,
`crates/data`.
→ what was observed: `default()` correctly returns
`FolderAndSubfolders` — but `SearchFolderScope` occurs nowhere outside
`crates/ui`. `bootstrap::run_search`/`run_search_with`
(`crates/app/src/bootstrap/search.rs:14-29`) take only
`(context, catalog, query, limit)` — no scope, no location, no source id
parameter exists in the signature at all, at any of their call sites
(`main.rs:314,398,436`; `wired_application_tests.rs`). This is a UI
default label, not a query restriction — the folder scope never reaches
the search path by any route.
→ where verified: `default_scope_is_folder_and_subfolders`
(`crates/ui/src/tests/rfc045_location.rs`) proves the default *value*;
nothing proves it restricts a query, because nothing does.

### 8. User can choose "This folder only."

→ what was run: same grep/signature inspection as criterion 7; code
inspection of `Message::SearchScopeChanged`
(`crates/ui/src/state.rs:992-994`) and `SearchLocationState::set_scope`
(`crates/ui/src/state/location.rs:159-163`).
→ what was observed: choosing "This folder only" calls `set_scope`, which
mutates UI state only (the rendered chip label, via
`search_location_chip` in `crates/ui/src/i18n.rs`) — the same missing
mechanism as criterion 7 means this choice has no effect on which files
are searched. One root cause, named here separately from criterion 7 so
an audit of this specific criterion finds it addressed directly rather
than needing to infer it from criterion 7's entry (Review 201 §4).
→ where verified: `changing_scope_preserves_folder_identity`
(`crates/ui/src/tests/rfc045_location.rs`) proves `set_scope` preserves
`source_id`; nothing proves — because nothing does — that the resulting
scope narrows a search.

### 9. Changing folder scope does not create duplicate remembered folders.

→ what was run: code inspection of `SearchLocation::with_scope`
(`crates/ui/src/state/location.rs:107-112`).
→ what was observed: `with_scope` preserves `source_id` across a scope
change — no new source record is created or looked up.
→ where verified: `changing_scope_preserves_folder_identity`
(`crates/ui/src/tests/rfc045_location.rs:111-126`) explicitly asserts
`source_id` is unchanged after `set_scope`.

### 10. Search results can appear before full preparation completes.

→ what was run: code inspection of `Message::FolderPicked`
(`crates/app/src/main.rs:378-421`) and
`bootstrap::scan_and_index_source`'s own doc comment
(`crates/app/src/bootstrap/sources.rs:78-88`).
→ what was observed: `scan_and_index_source` only *enqueues* a background
`Scan` job and returns promptly (RFC-056 §9); `FolderPicked` calls it,
then immediately calls `run_search` against whatever is already indexed
— it does not wait for the scan.
→ where verified: structural only. The closest adjacent automated
coverage is `search_view_shows_partial_readiness_banner_only_while_queued`
(`crates/ui/src/tests/smoke_views.rs`), which is UI-only and not
RFC-045-specific — no test exercises the real "pick folder → search
before scan completes" sequence end to end.

### 11. Folders screen is not required before first search.

→ what was run: code inspection of `Message::SubmitSearch`
(`crates/app/src/main.rs:288-311`) and `AppState::default()`
(`crates/ui/src/state.rs`).
→ what was observed: `SubmitSearch` opens the folder picker directly from
the Search view; the app starts on `ViewId::Search` by default, with no
requirement to visit `ViewId::Sources` first.
→ where verified: `search_empty_state_offers_add_source`
(`crates/ui/src/tests/smoke_views.rs`) confirms the Search view renders
and functions fully with zero registered sources.

### 12. Default UI says "folder," not "source."

→ what was run: `grep -n 'NavSources\|SourcesTitle\|SourcesAddFolder\|SearchInLabel\|SearchChooseFolder\|SearchAddSource\|SearchNoSourcesBody\|SearchSnippetUnavailable' crates/ui/src/i18n/en.rs crates/ui/src/i18n/ja.rs crates/ui/src/views.rs`.
→ what was observed: the primary navigation/label keys are clean —
`NavSources`/`SourcesTitle` → "Folders", `SourcesAddFolder` → "Add
Folder", `SearchInLabel` → "Search in", `SearchChooseFolder` → "Choose a
folder" — and tested. But the *same* Search view this RFC's flow lives on
renders `SearchAddSource` → "Add Source" (the empty-state CTA button,
`views.rs:351`) and `SearchNoSourcesBody` → "...so orbok can build a
local search **index**." (`views.rs:346`, an explicitly forbidden term).
The Japanese catalog has the matching leak (`"ソースを追加"`). **Not met**
for the UI as actually rendered, despite the primary folder-vocabulary
labels being correct.
→ where verified: `chip_label_never_says_source_or_recursive`
(`crates/ui/src/tests/rfc045_location.rs`) covers the chip label only;
no test covers `SearchAddSource`/`SearchNoSourcesBody` — see "Criteria
not met" below for why the passing copy tests did not catch this.

### 13. The user can remove a remembered folder from orbok without deleting files.

→ what was run: code inspection of `Message::SourceRemoved`
(`crates/app/src/main.rs:222-226`) →
`bootstrap::remove_source` → `SourceRepository::delete_with_all_data`
(`crates/data/db/src/repo/sources.rs:166-177`).
→ what was observed: a single `DELETE FROM sources ...` SQL statement.
The function takes only `&Catalog`; it has no filesystem access of any
kind, so it cannot touch a file on disk by construction, not merely by
having not been observed to.
→ where verified: `source_delete_cascades_to_files`
(`crates/data/db/src/tests.rs:209-224`) proves the catalog-side cascade
(source → files); the filesystem-inertness claim is a structural property
of the function's signature, not a separate test result.

---

## Criteria not met, and why RFC-045 closes anyway

- **§22 criteria 7 and 8, folder scope, are not wired to the search
  path at all.** `SearchFolderScope` is UI-only; `run_search`'s signature
  has no scope parameter. One missing mechanism produces both unmet
  criteria — recorded separately (Review 201 §4) so an audit of criterion
  8 specifically finds it addressed here, not only inferable from
  criterion 7's entry. Follow-up: RFC-060 §7, which is already the
  planned instrument for wiring source/scope information into search
  results and queries.

- **§22 criterion 12, default UI avoids "source," is violated by copy
  actually rendered** on the Search view (`SearchAddSource`,
  `SearchNoSourcesBody`), in both locales. This is a copy fix, not a
  missing mechanism, and not part of Task 038's scope — recorded here so
  the gap is written down rather than silently passing because the
  existing tests (`default_ui_copy_avoids_forbidden_terms`,
  `default_ui_copy_avoids_forbidden_technical_terms` — both defined
  against RFC-041, exercised by RFC-045's shared Search view) check a
  curated key list that happens to omit the three keys actually
  rendered, rather than checking every `MessageKey` exhaustively. That
  test-coverage gap is itself a finding, reported in Task 038's own
  submission per Review 201 §5(b).

**Everything else — the picker flow, automatic search on folder
selection, remembered-folder reuse and deduplication, and folder-first
removal without touching disk — shipped and is either directly tested or
structurally verifiable by inspection with no contradicting evidence.**
That is RFC-045's main design decision (RFC-000's granularity clause),
which is why it stays in `done/` while RFC-041 — where two of three named
subjects did not ship at all — was returned to `accepted/` (Review 201
§2, this task).
