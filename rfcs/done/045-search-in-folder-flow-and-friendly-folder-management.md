# RFC-045: Search-in-Folder Flow and Friendly Folder Management

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 045  
**Title:** Search-in-Folder Flow and Friendly Folder Management  
**Status:** Implemented (v0.20.0)
**Target milestone:** Search UX simplification / folder onboarding  
**Date:** 2026-06-20  

**Deferred work, recorded 2026-09-02 (RFC-000 granularity clause; RFC-063 §7).** The picker, remembered-folder handling and the flow shipped. **The selected folder never scopes the query** — §22 criterion 7 describes a UI default value, not a scoped search. Follow-up: RFC-060 §7.
**Related RFCs:** RFC-041 Search, Narrow Results, and Browse Around; RFC-037 Source Lifecycle, Refresh Policy, and Change Detection UX; RFC-038 Result Freshness, Trust Badges, and Recovery Actions; RFC-039 Privacy Modes and Local Data Visibility  
**Self-review revision:** Clarifies P0 remembered-folder behavior, search-scope semantics, and non-blocking folder-picker orchestration.  

---

> **Numbering note:** This accepted RFC is the canonical **RFC-045** package for the search-in-folder flow. Any earlier package for the same theme is superseded.

## 1. Summary

This RFC improves the first-run and everyday search flow.

The current model requires users to choose or manage a source before searching:

```text
Choose source
  ↓
prepare source
  ↓
search
```

For ordinary users, this feels like setup before value. The improved model is:

```text
Type search words
  ↓
press Search
  ↓
choose where to search only if needed
  ↓
orbok searches and prepares files in the background
```

The accepted direction is:

```text
Search first.
Ask for a folder only when no search location is selected.
Internally create or reuse a folder/source record.
Use friendly "folder" language, not "source" language.
Keep remembered folder management secondary.
```

This preserves orbok’s internal source model while making the user experience simpler.

---

## 2. Motivation

orbok is a local search app. Users naturally expect to start with:

```text
What am I looking for?
Where should the app look?
```

not:

```text
Configure a source first.
```

The current flow can create unnecessary friction:

- user must understand “source” before searching;
- search requires at least two steps;
- the empty search screen may feel unusable;
- users may not know whether they should add a folder permanently;
- simple one-folder searches are too heavy;
- source management becomes a prerequisite instead of a background capability.

This RFC makes folder selection part of the search action.

---

## 3. Goals

- Allow users to begin from the search box.
- If no folder is selected, pressing Search opens a folder picker.
- Automatically search the chosen folder after selection.
- Keep “remembered folders” available but not required as a separate first step.
- Support “this folder and subfolders” and “this folder only.”
- Make recursive search the safe default.
- Avoid exposing the word “source” in default UI.
- Allow recent/remembered folders to be selected quickly.
- Keep source lifecycle internally stable.
- Preserve privacy and local-first trust copy.
- Avoid cluttering the folder list with accidental one-off searches where possible.

---

## 4. Non-Goals

This RFC does not define:

- a new search engine;
- a new indexing architecture;
- model download behavior;
- live file watchers;
- OS-level file manager integration as P0;
- cloud folders;
- enterprise policy;
- replacement of RFC-037 source lifecycle;
- replacement of RFC-041 result filtering.

This RFC is a UX and orchestration change, not a search-core rewrite.

---

## 5. Core Concepts

### 5.1. Search Location

A **search location** is where the current search looks.

User-facing labels:

```text
Search in
Choose a folder
Documents and subfolders
Downloads only
```

Internal model may map this to a source or a transient source-like scope.

### 5.2. Remembered Folder

A **remembered folder** is a folder orbok keeps in its folder list and prepares over time.

User-facing label:

```text
Remember this folder
```

Do not say:

```text
Register source
Add source
Persist source
```

### 5.3. Search Scope

Search scope controls whether subfolders are included.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFolderScope {
    FolderAndSubfolders,
    FolderOnly,
}
```

Default:

```text
FolderAndSubfolders
```

User-facing labels:

```text
This folder and subfolders
This folder only
```

---

## 6. Product Decision

### 6.1. Pressing Search Without Location

If a user presses Search and no folder is selected:

```text
open folder picker
```

After folder selection:

```text
create or reuse folder record
set it as current search location
start preparation
run search as soon as possible
show results progressively
```

### 6.2. Remember Behavior

For P0, use this behavior:

```text
Remember selected folder by default.
Make it easy to remove from orbok later.
Do not add a Just this time prompt in the first implementation.
```

Rationale:

- remembered folders make future searches faster;
- source lifecycle already expects stable folders;
- result freshness depends on stable folder identity;
- avoiding an extra prompt keeps first search simple;
- users can remove a folder from orbok without deleting files.

P1 may add an explicit choice:

```text
Just this time
Remember
```

This is intentionally P1 because true one-time folders require transient lifecycle, cleanup, privacy, and result-history decisions.

### 6.3. Search Scope

Default scope:

```text
This folder and subfolders
```

Reason:

- most users expect folder search to include contents inside;
- it reduces surprise when files are in nested folders.

The user can switch to:

```text
This folder only
```

from the search location selector or Advanced/More options.

P0 scope semantics:

```text
Search scope is a search-time restriction, not a separate remembered folder identity.
```

This means selecting `Documents and subfolders` and then `Documents only` must not create two duplicate remembered folders. The remembered folder can still be prepared recursively; the current search applies the selected scope when choosing eligible results.

---

## 7. Revised Search Screen

### 7.1. Empty State With No Folder

```text
Search

[ What are you looking for?                         ]

Search in: [Choose a folder]

[Search]

You can choose a folder when you search.
Your files stay on this computer.
```

Behavior:

- typing is allowed before folder selection;
- pressing Search opens folder picker if no folder is selected.

### 7.2. User Presses Search

```text
Choose where to search
```

Open OS folder picker through `rfd`.

After user selects a folder:

```text
Searching in Documents
Preparing files in the background...
```

### 7.3. Selected Folder

```text
Search

[ renewal policy                                 ]

Search in: [Documents and subfolders ×] [Change]

[Search]
```

### 7.4. Recent Folders

```text
Recent folders:
[Documents] [Downloads] [Project notes]
```

Clicking one sets it as the current search location.

### 7.5. Remembered Folder Status

If selected folder is being prepared:

```text
Documents is still being prepared.
124 files ready. Results will improve as preparation finishes.
```

---

## 8. Folder Picker Flow

### 8.1. Basic Flow

```text
Search submitted
  ↓
no selected location
  ↓
open folder picker
  ↓
folder selected
  ↓
create/reuse folder record
  ↓
set current location
  ↓
start preparation
  ↓
execute search
```

### 8.2. Cancel Flow

If user cancels folder picker:

```text
return to search screen
keep search words
show no error
```

Copy:

```text
Choose a folder to search.
```

Do not show a red error.

### 8.3. Folder Already Remembered

If selected folder already exists in remembered folders:

```text
reuse existing folder record
set as current search location
refresh if needed
search ready files immediately
```

### 8.4. Folder Not Yet Remembered

P0:

```text
create remembered folder
start preparation
search progressively
```

User copy:

```text
orbok will remember this folder for faster search next time.
```

Do not show a `Do not remember` choice in P0. That choice implies transient-folder behavior, which is deferred to P1. Instead, make the normal folder-management action clear and safe:

```text
Remove from orbok
Your files will not be deleted.
```

---

## 9. Optional P1: Just This Time

P1 may add this lightweight confirmation after selection:

```text
Remember this folder?

Remembering makes future searches faster.

[Just this time] [Remember]
```

### 9.1. Just This Time Behavior

If user chooses “Just this time”:

- create a transient search location;
- prepare only enough for the current session/search;
- do not add it to remembered Folders list;
- do not persist recent-folder entry unless privacy settings permit;
- clean transient search data according to privacy/storage policy.

### 9.2. Risk

Just-this-time introduces lifecycle complexity. It is useful but not required for P0.

P0 can safely default to remembered folders and make removal easy.

---

## 10. Drag-and-Drop Folder Search

P1 feature.

If user drags a folder onto orbok:

```text
Search in this folder?

[Search here] [Remember folder]
```

If search text already exists, run search after folder is accepted.

If no search text exists, set the folder as search location and focus search input.

---

## 11. Search Location Selector

### 11.1. Default Selector

```text
Search in: [Choose a folder]
```

After selection:

```text
Search in: [Documents and subfolders ×] [Change]
```

### 11.2. Selector Menu

```text
Search in
✓ Documents and subfolders
  Documents only
  Downloads and subfolders
  Choose another folder...
```

### 11.3. Removing Current Location

Clicking `×` clears the current search location but keeps search text.

If user presses Search again:

```text
open folder picker
```

---

## 12. Folders Screen Role

The Folders screen remains important, but it is no longer required before first search.

Purpose:

- see remembered folders;
- pause/resume preparation;
- check missing folders;
- remove folder from orbok;
- prepare again;
- view folder status.

User-facing title:

```text
Folders
```

not:

```text
Sources
```

---

## 13. Interaction With RFC-037 Source Lifecycle

When folder is chosen from search:

- create or reuse `SourceRecord`;
- initial state becomes `Preparing`;
- scheduler starts scan/preparation;
- search can run against ready files;
- if folder becomes missing, show RFC-037 missing-folder recovery.

No special source lifecycle is needed for P0.

---

## 14. Interaction With RFC-036 Scheduler

Folder selected from search creates user-visible jobs.

Priority:

```text
UserVisible
```

The initial search should not block until all preparation is complete.

Behavior:

```text
prepare gradually
show partial results
update results as more files become ready
```

---

## 15. Interaction With RFC-038 Result Trust

If files are still being prepared, results may show:

```text
Still being prepared
```

If selected folder is only partly prepared:

```text
Some files are still being prepared.
Results will improve as preparation finishes.
```

---

## 16. Interaction With RFC-039 Privacy Modes

Recent folders and remembered folders are different.

Strict privacy may affect:

- recent folders;
- recent searches;
- transient search locations.

Strict mode should not automatically remove remembered folders unless user asks.

If strict mode is enabled, copy:

```text
Recent folders are not saved while Strict privacy is on.
```

---

## 17. State Model

```rust
#[derive(Debug, Clone)]
pub struct SearchLocationState {
    pub selected: Option<SearchLocation>,
    pub recent_locations: Vec<SearchLocationSummary>,
    pub picker_in_progress: bool,
}
```

```rust
#[derive(Debug, Clone)]
pub enum SearchLocation {
    Remembered {
        source_id: SourceId,
        display_name: String,
        scope: SearchFolderScope,
    },
    Transient {
        path: PathBuf,
        display_name: String,
        scope: SearchFolderScope,
    },
}
```

For P0, `Transient` may remain unused.

---

## 18. Message Model

```rust
#[derive(Debug, Clone)]
pub enum SearchLocationMessage {
    ChooseFolderRequested,
    FolderPickerCancelled,
    FolderPicked(PathBuf),
    SearchLocationSelected(SearchLocation),
    SearchLocationCleared,
    SearchScopeChanged(SearchFolderScope),
    RememberFolderRequested(PathBuf),
    RemoveRememberedFolderRequested(SourceId),
    RecentFolderSelected(SourceId),
}
```

Search submit integration:

```rust
#[derive(Debug, Clone)]
pub enum SearchMessage {
    SearchTextChanged(String),
    SubmitSearch,
    LocationNeededBeforeSearch,
    LocationReadyThenSearch(SourceId),
}
```

---

## 19. Implementation Rules

## 19.0. Folder Picker Orchestration

The folder picker must be launched from an app command/task path, not from pure view rendering. The UI should guard against duplicate picker requests while `picker_in_progress` is true.

Required behavior:

```text
Search submitted
  ↓
state records pending query
  ↓
folder picker task starts
  ↓
result message returns selected path or cancellation
  ↓
state transition continues search flow
```

This keeps iced state updates predictable and avoids repeated dialogs during re-render.

### 19.1. No Search Text Loss

If folder picker opens after search submit, the typed query must remain.

### 19.2. No Error on Cancel

Cancelling folder picker is not an error.

### 19.3. Reuse Existing Folder

Do not duplicate folder records when the selected path is already remembered.

### 19.4. Friendly Copy

Never show:

```text
source id
registration
index job
recursive flag
```

in default UI.

### 19.5. Progressive Search

Do not wait for full preparation if partial results are available.

---

## 20. Accessibility Requirements

- Search input remains focused after location changes where appropriate.
- Folder picker button has accessible label: `Choose folder to search`.
- Folder chip can be removed by keyboard.
- Search scope control is keyboard accessible.
- Status text is not color-only.
- Error/cancel states are announced as text.

---

## 21. Testing

### 21.1. Unit Tests

- submit search with no location requests folder picker;
- cancel picker preserves search text;
- folder picked creates/reuses source;
- existing folder is not duplicated;
- selected scope defaults to subfolders;
- clearing location keeps search text;
- strict privacy disables recent folder persistence if configured.

### 21.2. Integration Tests

- first-run user searches without folder;
- user selects folder and search starts;
- preparation begins in background;
- search results update as files become ready;
- missing selected folder shows recovery;
- user removes remembered folder without deleting files.

### 21.3. UI Tests

- no source-management screen required before search;
- Search button opens folder picker if needed;
- recent folder chips work;
- folder chip shows readable label;
- recursive/only scope is understandable.

---

## 22. Acceptance Criteria

This RFC is accepted when:

1. User can type a query before choosing a folder.
2. Pressing Search without a location opens a folder picker.
3. Cancelling folder picker keeps the query and shows no error.
4. Selecting a folder starts search flow automatically.
5. The selected folder is created or reused internally as a remembered folder for P0.
6. Existing remembered folders are not duplicated.
7. Default search scope is “This folder and subfolders.”
8. User can choose “This folder only.”
9. Changing folder scope does not create duplicate remembered folders.
10. Search results can appear before full preparation completes.
11. Folders screen is not required before first search.
12. Default UI says “folder,” not “source.”
13. The user can remove a remembered folder from orbok without deleting files.

---

## 23. Final Decision

Implement the search-in-folder flow:

```text
type query
press Search
choose folder if needed
search immediately
prepare in background
remember folder by default
make removal easy
```

This keeps orbok’s internal architecture stable while making search feel direct and friendly.
