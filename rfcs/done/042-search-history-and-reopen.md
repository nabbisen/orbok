# RFC-042: Search History and Reopen Recent Searches

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 042  
**Title:** Search History and Reopen Recent Searches  
**Status:** Implemented (v0.21.0)
**Target milestone:** Search UX / Privacy UX refinement  
**Date:** 2026-06-18  
**Related RFC:** RFC-041 Search, Narrow Results, and Browse Around  

---

## 1. Summary

This RFC defines search history behavior for **orbok**.

The accepted decision is:

```text
Keep search history,
reopen it through Recent searches,
do not show automatic search-result tabs by default.
```

Search history should help users return to previous searches without forcing them to manage multiple result tabs or search workspaces.

The default model is:

```text
One active search screen
+ Recent searches
+ local-only storage
+ clear history
+ privacy setting
```

Future saved searches may be added through an intentional action:

```text
Keep this search
```

but automatic tabbed result sets are not part of the default UX.

---

## 2. Motivation

orbok is moving toward the workflow:

```text
Search → Narrow results → Browse around
```

Users may need to reopen a previous search, especially when:

- they are interrupted;
- they repeat a search frequently;
- they are researching across several documents;
- they forget the exact words they used;
- they return to the app after closing it;
- they used several narrowing choices and do not want to rebuild them manually.

However, showing multiple result sets as tabs by default would add unnecessary complexity.

Tabs introduce questions such as:

```text
Which tab is current?
Are these results old?
Are filters shared between tabs?
Can I close this safely?
Did I lose my search?
```

For non-technical users, this is too heavy.

Therefore, search history should be a simple list of recent searches that can be searched again.

---

## 3. Goals

- Store recent successful searches locally.
- Allow users to reopen a previous search.
- Restore search words and narrowing choices.
- Refresh results against current files when reopened.
- Avoid stale result snapshots being shown as current.
- Provide clear privacy controls.
- Allow users to clear recent searches.
- Disable history in strict privacy mode.
- Avoid tab UI by default.
- Keep all labels plain and non-technical.
- Preserve keyboard accessibility.

---

## 4. Non-Goals

This RFC does not implement:

- automatic search result tabs;
- multiple simultaneous result workspaces;
- collaborative search history;
- cloud-synced history;
- scheduled search;
- search alerts;
- chat history;
- permanent saved-search management;
- result pinboards;
- full workspace restore.

This RFC also does not store full document contents, snippets, or raw internal ranking data in history.

---

## 5. Product Decision

## 5.1. Accepted

Use:

```text
Recent searches
```

A recent search can be reopened by clicking:

```text
Search again
```

## 5.2. Rejected for Default UI

Do not use automatic result tabs such as:

```text
Search 1 | Search 2 | Search 3
```

## 5.3. Deferred Future Feature

Later, orbok may add:

```text
Keep this search
```

This would create intentional saved searches.

It should not be added until recent searches are stable and user demand is clear.

---

## 6. User-Facing Terminology

## 6.1. Required Labels

| Concept | User-facing label |
|---|---|
| search history | Recent searches |
| rerun old search | Search again |
| saved search | Kept search |
| stored filters | Narrowing choices |
| clear history | Clear recent searches |
| history setting | Remember recent searches |
| search snapshot | Previous search |
| disabled history | Recent searches are not saved |

## 6.2. Forbidden Default Labels

Do not show these in the default UI:

```text
query
snapshot
session
workspace
state
persisted
cache
database
history table
rehydrate
```

## 6.3. App Name

Use:

```text
orbok
```

Do not use the former name in new UI copy.

---

## 7. Search History Data Model

## 7.1. SearchHistoryEntry

```rust
#[derive(Debug, Clone)]
pub struct SearchHistoryEntry {
    pub id: SearchHistoryId,
    pub search_text: String,
    pub filters: Vec<StoredSearchFilter>,
    pub created_at: Timestamp,
    pub last_used_at: Timestamp,
    pub previous_result_count: Option<usize>,
    pub locale: Locale,
}
```

## 7.2. StoredSearchFilter

```rust
#[derive(Debug, Clone)]
pub enum StoredSearchFilter {
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

## 7.3. SearchHistorySettings

```rust
#[derive(Debug, Clone)]
pub struct SearchHistorySettings {
    pub remember_recent_searches: bool,
    pub max_entries: usize,
    pub clear_when_privacy_strict: bool,
}
```

## 7.4. SearchUiState Additions

```rust
#[derive(Debug, Clone)]
pub struct SearchUiState {
    pub history: Vec<SearchHistoryEntry>,
    pub history_panel_open: bool,
    pub restoring_history_id: Option<SearchHistoryId>,
}
```

---

## 8. Storage Policy

## 8.1. Store

A history entry may store:

- search words;
- active narrowing choices;
- timestamp;
- previous result count;
- user-facing labels for filters;
- locale.

## 8.2. Do Not Store by Default

A history entry must not store:

- document text;
- snippets;
- embeddings;
- internal ranking scores;
- raw backend error details;
- full result list snapshot;
- model internals.

## 8.3. Maximum Entries

Default:

```text
20 recent searches
```

The implementation should keep the newest entries and remove older ones.

## 8.4. Deduplication

If the same search words and same narrowing choices are searched again:

- update the existing entry;
- move it to the top;
- update `last_used_at`;
- do not create a duplicate.

If the same search words are used with different narrowing choices, keep separate entries.

## 8.5. Empty and Failed Searches

Do not store empty searches.

Do not store failed searches unless the search was reopened from an existing history entry and the existing entry remains valid.

Zero-result searches may be stored, but they should not dominate the visible recent list.

---

## 9. Reopen Behavior

When a user selects a recent search:

1. restore search text immediately;
2. restore narrowing choices that are still valid;
3. drop invalid choices safely;
4. close the Recent searches panel;
5. show “Searching again...”;
6. search current files;
7. show current results;
8. update the entry timestamp.

Important:

> A recent search is a remembered instruction, not a frozen result list.

orbok must not show stale saved results as if they are current.

---

## 10. Invalid Restored Filters

A stored filter may become invalid.

Examples:

- folder was removed;
- folder drive is disconnected;
- kind is no longer supported;
- search style is unavailable because meaning search helper is missing.

## 10.1. Folder Removed

Show:

```text
The folder “Reports” is no longer available. Showing all folders instead.
```

## 10.2. Meaning Search Unavailable

Show:

```text
Search by meaning can be added later. Exact word search is ready now.
```

## 10.3. General Dropped Choices

Show:

```text
Some choices from this search are no longer available, so orbok searched the remaining choices.
```

---

## 11. Screen Wireframes

## 11.1. Search Screen With Recent Searches

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
│                │  Recent searches                                  │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ authentication token rotation                  │  │
│                │  │ PDFs · Documents · 10 minutes ago              │  │
│                │  │ [Search again]                                 │  │
│                │  └──────────────────────────────────────────────┘  │
│                │  ┌──────────────────────────────────────────────┐  │
│                │  │ 監査 証跡 ログ                                  │  │
│                │  │ All folders · yesterday                        │
│                │  │ [Search again]                                 │
│                │  └──────────────────────────────────────────────┘
│                │                                                    │
│                │  [Clear recent searches]                          │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

## 11.2. Search Results With Recent Searches Button

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ authentication token rotation                  │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  [Recent searches]                                │
│                │                                                    │
│                │  36 results                                       │
│                │  Narrow results                                   │
│                │  [PDFs] [This folder] [Changed recently] [More]   │
│                │                                                    │
│                │  Result card                                      │
│                │  Result card                                      │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

## 11.3. Recent Searches Drawer

```text
┌────────────────────────────────────────────────────────────────────┐
│  Search results                         │ Recent searches           │
│                                         │                            │
│  Search your files...        [Search]   │ authentication token       │
│                                         │ rotation                   │
│  36 results                             │ PDFs · Documents           │
│                                         │ 10 minutes ago             │
│  Result card                            │ [Search again]             │
│  Result card                            │                            │
│                                         │ 監査 証跡 ログ              │
│                                         │ All folders                │
│                                         │ yesterday                  │
│                                         │ [Search again]             │
│                                         │                            │
│                                         │ [Clear recent searches]    │
└────────────────────────────────────────────────────────────────────┘
```

## 11.4. Reopening a Recent Search

```text
┌────────────────────────────────────────────────────────────────────┐
│  Sidebar       │ Search                                             │
│                ├────────────────────────────────────────────────────┤
│  🔍 Search     │  ┌──────────────────────────────────────────────┐  │
│  🧠 Better     │  │ authentication token rotation                  │  │
│     search     │  └──────────────────────────────────────────────┘  │
│  ⚙ Settings   │                                      [Search]      │
│                │                                                    │
│                │  Searching again...                               │
│                │                                                    │
│                │  Narrowed by                                      │
│                │  [PDFs ×] [Documents ×] [Clear]                   │
│                │                                                    │
└────────────────────────────────────────────────────────────────────┘
```

## 11.5. Settings

```text
┌──────────────────────────────────────────────────────────┐
│  Settings                                                │
│                                                          │
│  Privacy                                                 │
│                                                          │
│  Documents are processed on this computer only.          │
│                                                          │
│  Remember recent searches                                │
│  [On]                                                    │
│  Recent searches are saved on this computer only.        │
│                                                          │
│  [Clear recent searches]                                 │
└──────────────────────────────────────────────────────────┘
```

## 11.6. Clear Confirmation

```text
┌──────────────────────────────────────────────────────────┐
│  Clear recent searches?                                  │
│                                                          │
│  This removes the list of searches shown in orbok.        │
│  Your files and search data are not deleted.              │
│                                                          │
│  [Cancel]  [Clear recent searches]                       │
└──────────────────────────────────────────────────────────┘
```

---

## 12. Message Model

```rust
#[derive(Debug, Clone)]
pub enum SearchHistoryMessage {
    OpenRecentSearches,
    CloseRecentSearches,

    SearchAgain(SearchHistoryId),
    RecentSearchRestored(SearchHistoryId),

    RemoveRecentSearch(SearchHistoryId),

    AskClearRecentSearches,
    CancelClearRecentSearches,
    ConfirmClearRecentSearches,

    ToggleRememberRecentSearches(bool),
}
```

---

## 13. Update Rules

## 13.1. After Successful Search

If history is enabled:

```text
create_or_update_history_entry(search_text, active_filters, result_count)
```

If history is disabled:

```text
do nothing
```

## 13.2. Search Again

```text
entry = load_history_entry(id)
state.search_text = entry.search_text
state.active_filters = restore_valid_filters(entry.filters)
state.history_panel_open = false
state.results_status = Searching
run_search_again()
```

## 13.3. Clear Recent Searches

```text
ask confirmation
if confirmed:
    delete all history entries
    close panel
    show friendly success notice
```

Success copy:

```text
Recent searches cleared.
```

## 13.4. Toggle Off

When turning history off:

```text
Turn off recent searches?

orbok will stop saving searches. You can also clear searches already saved.

[Cancel] [Turn off] [Turn off and clear]
```

---

## 14. Privacy Requirements

- History is stored locally only.
- User can turn it off.
- User can clear it.
- Strict privacy mode disables it.
- Diagnostics export excludes history by default.
- Logs do not include search text by default.
- If history is disabled, no new search entries are created.

Required copy:

```text
Recent searches are saved on this computer only.
```

Strict privacy copy:

```text
Recent searches are not saved while strict privacy is on.
```

---

## 15. Accessibility Requirements

- Recent search entries are keyboard focusable.
- Each entry has a clear action label.
- “Search again” must be visible text.
- Clear recent searches is keyboard reachable.
- Confirmation dialog focuses Cancel first.
- Escape closes drawer.
- Reopening a search does not unexpectedly move focus away from the search input.
- Screen reader label includes search text and summary.

Example accessible label:

```text
Search again: authentication token rotation, PDFs, Documents, 10 minutes ago
```

---

## 16. Implementation Priority

## 16.1. P0

Implement:

- local recent search storage;
- recent search list;
- Search again;
- restore search text;
- restore narrowing choices;
- rerun search against current files;
- Clear recent searches;
- Remember recent searches setting;
- no automatic result tabs.

## 16.2. P1

Implement:

- recent searches drawer;
- remove single entry;
- dropped-filter notice;
- strict privacy mode integration;
- diagnostics redaction tests.

## 16.3. P2

Implement:

- Keep this search;
- kept searches list;
- named saved searches;
- optional result-count preview;
- history search.

## 16.4. Explicitly Deferred

Do not implement by default:

- automatic tabs;
- multiple search workspaces;
- browser-like tab close/restore;
- result set pinboards;
- search workspace manager.

---

## 17. Test Plan

## 17.1. Unit Tests

- creates history entry after successful search;
- does not create entry when history disabled;
- does not store empty search;
- deduplicates same search and filters;
- stores same search with different filters separately;
- enforces max entry count;
- clears all entries.

## 17.2. Reopen Tests

- restores search text;
- restores folder filter;
- restores kind filter;
- drops missing folder filter;
- reruns search against current files;
- updates last-used timestamp.

## 17.3. Privacy Tests

- strict privacy mode disables history;
- diagnostics export excludes history by default;
- logs do not include search text by default;
- turning history off prevents new entries.

## 17.4. UI Tests

- Recent searches visible when entries exist;
- Recent searches hidden when none exist;
- Search again button works;
- Clear confirmation appears;
- Cancel keeps history;
- Clear removes history;
- no tab UI appears by default.

## 17.5. Copy Tests

- UI uses `orbok`;
- UI does not use forbidden terms;
- “Recent searches” used instead of “history” where practical;
- “Search again” used instead of “restore” or “replay.”

---

## 18. Acceptance Criteria

This RFC is accepted when:

1. Users can reopen recent searches.
2. Reopened searches are rerun against current files.
3. Search text and narrowing choices are restored.
4. Invalid old filters are safely dropped with a friendly notice.
5. Search history is local-only.
6. Users can clear recent searches.
7. Users can turn recent searches off.
8. Strict privacy mode disables history.
9. Diagnostics do not include history by default.
10. Logs do not include search text by default.
11. Default UI does not use search result tabs.
12. No technical labels appear in default history UI.
13. Keyboard and accessibility requirements pass.

---

## 19. Final Decision

Implement search history as:

```text
Recent searches
```

with:

```text
Search again
```

Do not implement automatic search-result tabs by default.

Add intentional saved searches later only if needed:

```text
Keep this search
```

This supports re-opening previous work while preserving orbok’s simple search-first experience.
