# RFC-041: Search, Narrow Results, and Browse Around

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 041  
**Title:** Search, Narrow Results, and Browse Around  
**Status:** Implemented (v0.18.0)
**Target milestone:** Search UX / Filter UX refinement  
**Date:** 2026-06-18  

**Deferred work, recorded 2026-09-02 (RFC-000 granularity clause; RFC-063 §7).** The search view, narrowing chips and recovery states shipped. **§25 criterion 3's filters are stored, rendered and serialized into history, and never applied to a query.** Follow-up: RFC-060 §7.
**Source basis:** `orbit-search-filter-external-design-v0.1.md`, `orbit-filter-ui-design-addendum.md`  

---

## 1. Summary

This RFC defines the final search and filter UX model for **orbok**, formerly named **orbit**.

The accepted product direction is:

```text
Search → Narrow results → Browse around
```

The app must keep a single, simple search entry point. Users should not be asked to configure search criteria before they search. Filtering appears only after results exist or after the user explicitly asks for more control.

This RFC replaces the idea of a one-time-only search UI with a progressive search workflow:

1. the user searches normally;
2. orbok shows results;
3. orbok suggests simple ways to narrow results;
4. the user can remove any narrowing choice;
5. the user can browse around a useful result.

This gives orbok more practical power without making the UI feel like a technical search tool.

---

## 2. App Rename Note

The application name has changed from **orbit** to **orbok** because “orbit” is already used by another party.

All new user-facing copy, documentation, RFCs, code comments intended for users, and UI labels should use:

```text
orbok
```

Historical documents may still contain:

```text
orbit
```

When referencing earlier documents, treat `orbit` as the former project name.

---

## 3. Motivation

The previous search UI direction was simple and safe, but it risked becoming too limited:

```text
type query → press Search → receive one result set
```

That is easy to understand, but it does not fully support common real-world search behavior.

Users often start with an imprecise search:

- they remember a topic but not the file name;
- they remember a folder but not the exact document;
- they get too many results;
- they want only PDFs;
- they want recent files;
- they find one useful result and want nearby or similar files.

A plain one-time search forces users to repeatedly rewrite the query. A heavy criteria-builder UI, however, would overwhelm non-technical users.

This RFC defines the middle path:

```text
simple search first,
then gentle narrowing,
then optional browsing.
```

---

## 4. Goals

- Keep one integrated search box and one Search button.
- Avoid separate “AI search” and “keyword search” buttons.
- Avoid a large pre-search filter form.
- Show simple narrowing choices only after results are available.
- Make every active filter visible and removable.
- Provide a clear “Clear” action for all active filters.
- Provide no-result recovery when filters are too narrow.
- Enable browse-around actions from a selected result.
- Keep labels plain and non-technical.
- Preserve Advanced view for users who need more control.
- Support keyboard navigation and accessible interaction.
- Keep the UI responsive during filtering and result updates.

---

## 5. Non-Goals

This RFC does not define:

- ranking internals;
- database schema;
- model installation flow;
- storage cleanup flow;
- benchmark design;
- mobile layout;
- chat-style answer generation;
- cloud search;
- saved search automation.

This RFC also does not expose internal terms such as:

```text
BM25
RRF
embedding
vector
chunk
cache
catalog
source
schema
backend
```

in the default UI.

---

## 6. Core Product Decision

## 6.1. Accepted Model

Use:

```text
Search → Narrow results → Browse around
```

## 6.2. Rejected Model

Do not use:

```text
Search criteria form → search → technical filters → explorer mode
```

## 6.3. Rationale

For non-technical users, search should start from intent:

```text
I want to find something.
```

It should not start from configuration:

```text
I need to decide where, how, by which engine, and with which criteria to search.
```

The app should do the first sensible thing automatically, then offer refinement only when useful.

---

## 7. UX Principles

## 7.1. Search First

Before the first search, keep the screen focused:

```text
Search your files...
[Search]
```

Do not show filters yet.

## 7.2. Narrow Only After Results

After results appear, show a small row:

```text
Narrow results
[PDFs] [This folder] [Changed recently] [More ways]
```

## 7.3. Make Narrowing Reversible

When a narrowing choice is active, show it as a removable chip:

```text
Narrowed by:
[PDFs ×] [This folder ×] [Clear]
```

## 7.4. Do Not Say “Explorer”

Do not introduce an “Explorer” mode in the default UI.

Instead, expose browse-around actions from a selected result:

```text
[Search in this folder]
[Show nearby files]
[Show similar files]
```

## 7.5. Hide Complexity

Advanced controls are available only behind Advanced view or “More ways to narrow.”

The default UI should remain usable without documentation.

---

## 8. User-Facing Terminology

## 8.1. Required Plain Labels

| Internal concept | User-facing label |
|---|---|
| filter | Narrow results |
| source | Folder |
| source scope | Search in |
| file type | Kind |
| modified time | Changed |
| keyword search | Exact words |
| semantic search | Meaning |
| hybrid search | Best results |
| stale | Needs update |
| missing | File not found |
| indexing | Preparing search |
| cache | Temporary previews |
| catalog | Saved app data |

## 8.2. Forbidden Default Labels

Do not show these labels in the default UI:

```text
source
index
catalog
cache
embedding
vector
BM25
RRF
chunk
query
schema
engine
backend
```

## 8.3. Project Name Rule

Use:

```text
orbok
```

Do not use the former name in new user-facing UI except in migration or release notes.

---

## 9. Information Architecture

Search and filtering belong under the Search area.

```text
Search
├── Search
└── Folders
```

The Search screen owns:

- search input;
- result list;
- quick narrowing chips;
- active filter chips;
- More ways to narrow panel;
- result preview;
- browse-around actions.

The Folders screen owns:

- adding folders;
- removing folders;
- folder readiness;
- folder problems.

Filters are not a standalone top-level page.

---

## 10. Main User Flows

## 10.1. First Search

```text
Open orbok
  ↓
Search screen
  ↓
Type search words
  ↓
Press Search
  ↓
Results appear
  ↓
Narrow results row appears
```

## 10.2. Narrow Results

```text
Results visible
  ↓
Click [PDFs]
  ↓
[PDFs ×] appears immediately
  ↓
Results update
  ↓
User can remove [PDFs ×] or [Clear]
```

## 10.3. More Ways to Narrow

```text
Results visible
  ↓
Click [More ways to narrow]
  ↓
Panel opens
  ↓
Choose folder, kind, changed date, or ready status
  ↓
Click [Show results]
  ↓
Results update
```

## 10.4. Browse Around

```text
Click a useful result
  ↓
Preview opens
  ↓
Choose one:
  ├─ Open file
  ├─ Search in this folder
  ├─ Show nearby files
  └─ Show similar files
```

---

## 11. Filter Timing Rules

## 11.1. Before Search

Default:

```text
No filter row.
```

Allowed exception:

If the user has many folders and the app already has enough context, a compact selector may appear:

```text
Search in: All folders ▾
```

But the preferred first experience is still one search box only.

## 11.2. After Search With Results

Show quick narrowing chips.

Rules:

- show at most three quick chips plus More ways;
- chips must be based on the result set;
- do not show chips that produce zero results;
- do not show chips that barely change the result count;
- do not show technical categories;
- hide the row if it would not help.

## 11.3. With Active Filters

Show active filters first.

```text
Narrowed by:
[PDFs ×] [This folder ×] [Clear]
```

Then optionally show:

```text
[More ways to narrow]
```

## 11.4. After Selecting a Result

Show browse-around actions in the result preview.

---

## 12. Filter Types

## 12.1. Folder

User label:

```text
Search in
```

Options:

```text
All folders
Documents
Projects
Research papers
```

Quick chips:

```text
[This folder]
[Only Documents]
```

Rules:

- default is All folders;
- folder filter can be created by a chip, panel choice, or result action;
- if the folder is removed, reset to All folders and show a friendly notice.

Friendly notice:

```text
That folder was removed. Showing all folders instead.
```

---

## 12.2. Kind

User label:

```text
Kind
```

Default options:

```text
Documents
PDFs
Notes
Code
Spreadsheets
```

Rules:

- do not show raw file extensions in the default UI;
- Advanced view may show extensions if useful;
- file type chips should appear only when they reduce results meaningfully.

Quick chips:

```text
[PDFs]
[Notes]
[Code]
```

---

## 12.3. Changed

User label:

```text
Changed
```

Options:

```text
Any time
Today
This week
This month
This year
Choose dates...
```

Quick chips:

```text
[Changed recently]
[This month]
```

Avoid technical labels such as:

```text
mtime
modified_at
date range
```

---

## 12.4. Ready Status

User label:

```text
Ready status
```

Options:

```text
Ready
Needs update
File not found
```

Default behavior:

- prefer Ready results;
- show “Needs update” and “File not found” only when relevant;
- let Advanced view expose more status detail.

---

## 12.5. Search Style

Hidden by default.

User label:

```text
Search style
```

Options:

```text
Best results
Exact words
Meaning
```

Default:

```text
Best results
```

If the local meaning-search helper is not installed, show a friendly notice:

```text
Search by meaning can be added later. Exact word search is ready now.
```

Do not show separate main buttons for exact and meaning search.

---

## 12.6. Language

Usually hidden.

User label:

```text
Language
```

Options:

```text
Any language
English
Japanese
Mixed
```

Rules:

- show only if language detection is reliable;
- otherwise keep it in Advanced view or omit it;
- do not make language selection part of the first search.

---

## 13. Quick Suggestion Rules

orbok may suggest chips after results appear.

## 13.1. Suggest If

Show a suggestion if:

- it reduces result count meaningfully;
- it keeps enough results to be useful;
- it is easy to explain;
- it is not already active;
- it does not conflict with active filters.

## 13.2. Do Not Suggest If

Do not show a suggestion if:

- it produces zero results;
- it reduces 5 results to 4;
- it uses a technical category;
- it duplicates another suggestion;
- there are already enough chips.

## 13.3. Suggestion Priority

Suggested chip priority:

1. This folder
2. Kind, such as PDFs or Notes
3. Changed recently
4. Ready only
5. Language, if reliable
6. Search style, Advanced view only

---

## 14. Screen Wireframes

## 14.1. Search — First Empty State

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │                                                    │
│  🧠 Better     │  ┌──────────────────────────────────────────────┐  │
│     search     │  │ Search your files...                          │  │
│  ⚙ Settings   │  └──────────────────────────────────────────────┘  │
│                │                                      [Search]      │
│                │                                                    │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ Nothing to search yet                         │  │
│                │  │                                                │  │
│                │  │ Choose a folder, and orbok will prepare it     │  │
│                │  │ for search. Your files stay on this computer. │  │
│                │  │                                                │  │
│                │  │ [Choose a folder]                              │  │
│                │  └──────────────────────────────────────────────┘  │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- no filters visible;
- no technical wording;
- privacy reassurance visible;
- “Choose a folder” is preferred over “Add source.”

---

## 14.2. Search — Folder Being Prepared

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ Search your files...                          │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  Preparing “Documents” for search                  │
│                │  ████████████░░░░░░░░  62%                         │
│                │  124 files ready. You can search now.              │
│                │                                                    │
│                │  [Add another folder]                              │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- keep search box available;
- do not block the user;
- do not say “indexing” in default UI;
- show partial readiness.

---

## 14.3. Search — Results Without Filters

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ authentication token rotation                  │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  36 results                                       │
│                │                                                    │
│                │  Narrow results                                   │
│                │  [PDFs] [This folder] [Changed recently] [More]   │
│                │                                                    │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ auth.md                                       │  │
│                │  │ Documents / security                          │  │
│                │  │ ...refresh tokens should expire earlier...    │  │
│                │  └──────────────────────────────────────────────┘  │
│                │                                                    │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ idp-review.pdf                                │  │
│                │  │ Reports                                       │  │
│                │  │ ...client secret rotation policy states...    │  │
│                │  └──────────────────────────────────────────────┘  │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- filter row appears only because results exist;
- quick chips are limited;
- no advanced controls shown.

---

## 14.4. Search — Active Filters

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ authentication token rotation                  │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  8 results                                        │
│                │                                                    │
│                │  Narrowed by                                      │
│                │  [PDFs ×] [This folder ×] [Clear]                 │
│                │                                                    │
│                │  [More ways to narrow]                            │
│                │                                                    │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ idp-review.pdf                                │  │
│                │  │ Reports                                       │  │
│                │  │ ...client secret rotation policy states...    │  │
│                │  └──────────────────────────────────────────────┘  │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- active filters remain visible;
- each active filter is removable;
- Clear is visible;
- search text remains unchanged.

---

## 14.5. Search — No Results After Filtering

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ authentication token rotation                  │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  No results with these choices                    │
│                │                                                    │
│                │  Try removing one:                                │
│                │  [PDFs ×] [This folder ×]                         │
│                │                                                    │
│                │  [Clear]                                          │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- do not show only “0 results”;
- show direct recovery actions;
- keep search text and active chips visible.

---

## 14.6. More Ways to Narrow — Wide Window

```text
┌────────────────────────────────────────────────────────────────────┐
│  Search results                         │ More ways to narrow       │
│                                         │                            │
│  Search your files...        [Search]   │ Search in                  │
│                                         │ (•) All folders            │
│  36 results                             │ ( ) Documents              │
│                                         │ ( ) Projects               │
│  Result card                            │ ( ) Research papers        │
│  Result card                            │                            │
│  Result card                            │ Kind                       │
│                                         │ [✓] Documents              │
│                                         │ [✓] PDFs                   │
│                                         │ [ ] Notes                  │
│                                         │ [ ] Code                   │
│                                         │                            │
│                                         │ Changed                    │
│                                         │ (•) Any time               │
│                                         │ ( ) This week              │
│                                         │ ( ) This month             │
│                                         │                            │
│                                         │ [Show results] [Clear]     │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- keep results visible;
- panel must be closable;
- Escape closes the panel;
- Show results is visible even if changes apply immediately.

---

## 14.7. More Ways to Narrow — Narrow Window

```text
┌─────────────────────────────────────────────────────────────┐
│ Search your files...                              [Search]  │
├─────────────────────────────────────────────────────────────┤
│ 36 results                                                  │
│ [PDFs] [This folder] [Changed recently]                     │
│                                                             │
│ More ways to narrow                                         │
│ ─────────────────────────────────────────────────────────── │
│ Search in                                                   │
│ (•) All folders                                             │
│ ( ) Documents                                               │
│ ( ) Projects                                                │
│                                                             │
│ Kind                                                        │
│ [✓] Documents  [✓] PDFs  [ ] Notes  [ ] Code                │
│                                                             │
│ Changed                                                     │
│ (•) Any time  ( ) This week  ( ) This month                 │
│                                                             │
│ [Show results] [Clear]                                      │
├─────────────────────────────────────────────────────────────┤
│ Result card                                                 │
│ Result card                                                 │
└─────────────────────────────────────────────────────────────┘
```

Requirements:

- use inline expansion instead of cramped side panel;
- page may scroll;
- controls remain large.

---

## 14.8. Result Preview With Browse-Around Actions

```text
┌────────────────────────────────────────────────────────────────────┐
│  Results                                │ Preview                   │
│                                         │                           │
│  ┌──────────────────────────────────┐   │ auth.md                   │
│  │ auth.md                           │   │ Documents / security      │
│  │ Documents / security              │   │                           │
│  │ ...refresh tokens should...       │   │ ...refresh tokens should  │
│  └──────────────────────────────────┘   │ expire earlier than...     │
│                                         │                           │
│                                         │ Why this result appeared   │
│                                         │ Exact words matched        │
│                                         │ Meaning was similar        │
│                                         │                           │
│                                         │ [Open file]               │
│                                         │ [Search in this folder]   │
│                                         │ [Show nearby files]       │
│                                         │ [Show similar files]      │
└────────────────────────────────────────────────────────────────────┘
```

Requirements:

- no top-level Explorer mode;
- browse-around actions appear only after result selection;
- “Why this result appeared” uses plain language.

---

## 15. Interaction Rules

## 15.1. Applying a Quick Chip

When the user clicks a chip:

1. the chip becomes active immediately;
2. it appears in “Narrowed by”;
3. results update;
4. if update takes time, show “Updating results...”.

## 15.2. Removing One Filter

Clicking:

```text
[PDFs ×]
```

removes only that filter.

## 15.3. Clearing All Filters

Clicking:

```text
[Clear]
```

removes all filters.

No confirmation is needed because this action is harmless and reversible.

## 15.4. Search Text Changes

When the user changes search text:

- keep active filters by default;
- update results when Search is submitted;
- if filters now produce no results, show no-result recovery.

## 15.5. More Ways Panel

The panel:

- opens from “More ways to narrow”;
- can be closed;
- does not clear current filters;
- provides Show results and Clear;
- must not trap users.

## 15.6. Browse-Around Actions

### Search in this folder

Adds a folder filter based on the selected result.

### Show nearby files

Shows files in the same folder or nearby path context.

### Show similar files

Uses current search capabilities to find similar results.

If “Show similar files” depends on meaning search and the helper is unavailable, show:

```text
Search by meaning can be added later. Exact word search is ready now.
```

---

## 16. State Model

## 16.1. SearchUiState

```rust
#[derive(Debug, Clone)]
pub struct SearchUiState {
    pub text: String,
    pub active_filters: Vec<ActiveFilter>,
    pub suggested_filters: Vec<SuggestedFilter>,
    pub more_panel_open: bool,
    pub results_status: ResultsStatus,
    pub selected_result_id: Option<ResultId>,
}
```

## 16.2. ActiveFilter

```rust
#[derive(Debug, Clone)]
pub enum ActiveFilter {
    Folder {
        id: FolderId,
        label: String,
    },
    Kind {
        id: KindId,
        label: String,
    },
    Changed {
        value: ChangedFilter,
        label: String,
    },
    ReadyStatus {
        value: ReadyFilter,
        label: String,
    },
    SearchStyle {
        value: SearchStyle,
        label: String,
    },
    Language {
        value: LanguageFilter,
        label: String,
    },
}
```

## 16.3. SuggestedFilter

```rust
#[derive(Debug, Clone)]
pub struct SuggestedFilter {
    pub filter: ActiveFilter,
    pub label: String,
    pub estimated_result_count: usize,
}
```

`estimated_result_count` is implementation-facing and should not be shown by default.

## 16.4. ResultsStatus

```rust
#[derive(Debug, Clone)]
pub enum ResultsStatus {
    NotSearchedYet,
    Preparing,
    Searching,
    Updating,
    Ready {
        total_count: usize,
    },
    EmptyBeforeAnyFolder,
    EmptyAfterSearch,
    EmptyAfterFiltering,
    Problem {
        friendly_message: String,
    },
}
```

---

## 17. Message Model

```rust
#[derive(Debug, Clone)]
pub enum SearchMessage {
    SearchTextChanged(String),
    SubmitSearch,

    ApplySuggestedFilter(usize),
    RemoveFilter(usize),
    ClearFilters,

    OpenMoreWays,
    CloseMoreWays,
    ApplyPanelChanges,

    SelectResult(ResultId),
    OpenSelectedResult(ResultId),

    SearchInResultFolder(ResultId),
    ShowNearbyFiles(ResultId),
    ShowSimilarFiles(ResultId),
}
```

Rules:

- state changes must render immediately;
- background search/filter work must not freeze the UI;
- user-facing messages must be friendly before reaching the view layer.

---

## 18. Implementation Notes for iced/snora

## 18.1. Search Page Composition

Recommended page structure:

```text
page_container
└── column
    ├── search_input_row
    ├── status_or_notice
    ├── quick_filter_row_or_active_filter_row
    ├── result_area
    └── optional_more_ways_panel
```

## 18.2. Quick Filter Chip

Conceptual component:

```rust
pub fn filter_chip<'a>(
    label: &'a str,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let visible_label = if selected {
        format!("{label} ×")
    } else {
        label.to_string()
    };

    button(text(visible_label).size(TEXT_BUTTON))
        .padding([10, 14])
        .on_press(on_press)
        .into()
}
```

Production code should avoid comments containing technical jargon in user-facing modules.

## 18.3. Result Update Behavior

Filtering should update the UI in two steps:

1. immediately update chip state;
2. asynchronously update results.

This prevents the app from feeling stuck.

---

## 19. Accessibility Requirements

- Filter chips must be keyboard focusable.
- Active chips must include visible text and `×`.
- Clear must be keyboard accessible.
- Color must not be the only selected-state indicator.
- Search input remains focusable after applying filters.
- Escape closes the More ways panel.
- Tab order must be predictable:
  1. search input;
  2. Search button;
  3. quick chips or active chips;
  4. More ways button;
  5. result list;
  6. preview actions.
- No-result recovery actions must be reachable without mouse.
- Focus must not jump unexpectedly after a chip is clicked.
- Minimum chip height should be 36 px.
- Preferred button/input height should be 44 px.

---

## 20. Error and Recovery Rules

## 20.1. No Results After Filtering

Show:

```text
No results with these choices.

Try removing one:
[PDFs ×] [This folder ×]

[Clear]
```

## 20.2. Folder Removed While Active

Show:

```text
That folder was removed. Showing all folders instead.
```

## 20.3. Search Failure

Show:

```text
Search did not finish. Please try again.
```

Action:

```text
[Try again]
```

## 20.4. Meaning Search Unavailable

Show:

```text
Search by meaning can be added later. Exact word search is ready now.
```

## 20.5. File Not Found

Show:

```text
File not found. It may have been moved or the drive may be disconnected.
```

Actions:

```text
[Search all folders]
[Remove from results]
```

---

## 21. Advanced View Rules

When Advanced view is off:

- show integrated search;
- show quick chips;
- show active chips;
- show More ways to narrow;
- hide search style by default;
- hide raw extensions by default;
- hide internal result details.

When Advanced view is on:

- allow Search style:
  - Best results;
  - Exact words;
  - Meaning;
- allow raw file extensions where useful;
- allow detailed ready status;
- allow result explanation badges;
- still avoid algorithm names unless explicitly required.

Advanced view must not become a developer console.

---

## 22. Copy Specification

## 22.1. Main Copy

| Location | Copy |
|---|---|
| Search placeholder | Search your files... |
| Search button | Search |
| Empty heading | Nothing to search yet |
| Empty body | Choose a folder, and orbok will prepare it for search. Your files stay on this computer. |
| Empty CTA | Choose a folder |
| Preparing heading | Preparing “{folder}” for search |
| Preparing detail | {ready_count} files ready. You can search now. |
| Narrow row | Narrow results |
| Active row | Narrowed by |
| More button | More ways to narrow |
| Clear button | Clear |
| No filtered results | No results with these choices |
| No filtered results body | Try removing one. |
| Folder action | Search in this folder |
| Nearby action | Show nearby files |
| Similar action | Show similar files |

## 22.2. Filter Copy

| Filter | Copy |
|---|---|
| Folder | Search in |
| Kind | Kind |
| Changed | Changed |
| Ready status | Ready status |
| Search style | Search style |
| Language | Language |

## 22.3. Status Copy

| Internal | Copy |
|---|---|
| current | Ready |
| stale | Needs update |
| missing | File not found |

---

## 23. Implementation Priority

## 23.1. P0

Implement first:

- integrated search button;
- no pre-search filter row;
- quick chips after results;
- active filter chips;
- remove one filter;
- clear all filters;
- no-results-after-filtering recovery;
- Folder filter;
- Kind filter;
- plain labels.

## 23.2. P1

Implement next:

- More ways to narrow panel;
- Changed filter;
- Ready status filter;
- result-level Search in this folder;
- smart suggestions based on current results;
- accessible focus behavior.

## 23.3. P2

Implement later:

- Language filter;
- Search style in Advanced view;
- Show nearby files;
- Show similar files;
- saved filter sets;
- privacy-aware search history.

---

## 24. Test Plan

## 24.1. Unit Tests

- active filter can be added;
- active filter can be removed;
- Clear removes all filters;
- search text is preserved when filters change;
- suggested filters exclude already active filters;
- suggested filters exclude zero-result choices.

## 24.2. UI Behavior Tests

- no filters shown before first search;
- quick chips shown after results;
- active chips shown after filter selection;
- no-result recovery appears after over-filtering;
- More ways panel opens and closes;
- Escape closes panel;
- focus remains stable after chip click.

## 24.3. Accessibility Tests

- all chips reachable by keyboard;
- Clear reachable by keyboard;
- active chip selected state is visible without color;
- no-result recovery is screen-reader understandable;
- tab order is predictable.

## 24.4. Copy Tests

- default UI does not show forbidden terms;
- project name is `orbok`;
- former name `orbit` is not shown in new user-facing copy;
- error messages are friendly.

---

## 25. Acceptance Criteria

This RFC is accepted when:

1. The default Search screen has no filter form before search.
2. Results show quick narrowing suggestions only when useful.
3. Active filters are visible and individually removable.
4. Clear removes all filters without clearing search text.
5. No-result-after-filtering state provides recovery actions.
6. More ways to narrow exists for deeper control.
7. Browse-around actions are available from selected results.
8. Default labels avoid technical terms.
9. Advanced view provides additional control without exposing raw internals unnecessarily.
10. Keyboard and accessibility requirements are met.
11. UI uses `orbok` consistently as the product name.
12. Search/filter updates never show a blank screen.

---

## 26. Migration Notes from Earlier Designs

Earlier documents may refer to:

```text
orbit
```

This RFC uses:

```text
orbok
```

Earlier designs may use the word:

```text
filter
```

This RFC prefers:

```text
Narrow results
```

Earlier designs may use:

```text
Explorer
```

This RFC replaces that with result-level actions:

```text
Search in this folder
Show nearby files
Show similar files
```

---

## 27. Final Decision

Implement the orbok search UX as:

```text
Search → Narrow results → Browse around
```

Do not make filter selection a prerequisite for search.

Do not expose Explorer as a separate mode in the default UI.

Do provide guided, reversible narrowing after results appear.
