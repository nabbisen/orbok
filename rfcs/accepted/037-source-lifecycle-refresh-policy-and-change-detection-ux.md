# RFC-037: Source Lifecycle, Refresh Policy, and Change Detection UX

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 037  
**Title:** Source Lifecycle, Refresh Policy, and Change Detection UX  
**Status:** Accepted
**Target milestone:** Source management / refresh stability  
**Date:** 2026-06-18  

**Returned to `accepted/` 2026-09-02 (RFC-063 §7).** Accepted 2026-06-18; carried `Implemented (v0.18.0)` while §10.1 *Startup Check* and §10.2 *Manual Refresh* — both marked **Required** — do not exist, and `orbok_fs::source_lifecycle` has zero production callers. §21 criteria 3 and 4 are unmet. **The design is settled and authorized; the wiring is dev-team Task 035.** Do not re-derive the design.
**Related RFCs:** RFC-003 Source Registration and File Access Boundary, RFC-004 File Scanner and Change Detection, RFC-041 Search, Narrow Results, and Browse Around, RFC-036 Resource-Aware Indexing Scheduler and Backpressure  

---

## 1. Summary

This RFC defines how orbok manages registered folders over time.

The accepted direction is:

```text
Use startup scan and manual refresh as the required stable baseline.
Treat live file watching as optional future work.
Represent missing, changed, removed, and preparing states clearly.
Avoid surprising background churn.
```

orbok must be honest about what it has prepared, what needs update, and what can no longer be found.

---

## 2. Motivation

Users expect local search to reflect their files, but local files are unstable: folders are renamed, external drives are disconnected, files are edited or deleted, and generated directories can change thousands of files at once.

If orbok refreshes too aggressively, it may feel noisy and heavy. If it refreshes too little, users may distrust results. This RFC defines a calm source lifecycle that favors stability and clarity.

---

## 3. Goals

- Define source states clearly.
- Define file states within a source.
- Support startup refresh.
- Support manual refresh.
- Defer live watching until the rest is stable.
- Handle missing folders gently.
- Avoid endless re-indexing from change storms.
- Keep source files safe; orbok never deletes user files.
- Provide simple user-facing copy.
- Integrate with scheduler and result trust states.

---

## 4. Non-Goals

This RFC does not define a full file watcher implementation, cloud folder sync, remote drive protocols, distributed indexing, real-time collaboration, extraction internals, or ranking behavior.

---

## 5. Product Decision

Use this baseline:

```text
Startup scan + manual refresh first.
Live watcher later.
```

Default behavior:

- when app starts, quickly check registered folders;
- mark obvious changes;
- refresh changed files in background;
- allow user to search prepared data immediately;
- allow manual refresh from Folders view;
- do not constantly watch every change in the v1 baseline.

---

## 6. User-Facing Terms

| Internal concept | User-facing label |
|---|---|
| source | Folder |
| recursive scan | Prepare folder |
| stale | Needs update |
| missing source | Folder not found |
| removed source | Removed folder |
| file watcher | Automatic refresh |
| reindex | Prepare again |
| scan | Check folder |

Avoid `source`, `watcher`, `inode`, `mtime`, `hash`, `index`, and `reindex` in default UI.

---

## 7. Source States

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Active,
    Preparing,
    NeedsUpdate,
    Paused,
    FolderNotFound,
    PermissionProblem,
    Removed,
}
```

### 7.1. Active

User copy:

```text
Ready
```

Meaning: folder exists, orbok can read it, and no known urgent refresh is pending.

### 7.2. Preparing

User copy:

```text
Preparing
```

Meaning: orbok is scanning or preparing files.

### 7.3. NeedsUpdate

User copy:

```text
Needs update
```

Meaning: folder exists and some files changed or need refresh.

### 7.4. Paused

User copy:

```text
Paused
```

Meaning: user or resource policy paused preparation.

### 7.5. FolderNotFound

User copy:

```text
Folder not found
```

Likely causes: external drive disconnected, folder renamed, or network drive unavailable.

### 7.6. PermissionProblem

User copy:

```text
Cannot open
```

Meaning: folder exists but orbok cannot read it.

### 7.7. Removed

User copy:

```text
Removed
```

Meaning: user removed it from orbok. Source files are not deleted.

---

## 8. File States

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Discovered,
    Preparing,
    Ready,
    NeedsUpdate,
    PartlyPrepared,
    CouldNotPrepare,
    FileNotFound,
    Ignored,
}
```

User-facing labels:

| FileState | Copy |
|---|---|
| Discovered | Waiting |
| Preparing | Preparing |
| Ready | Ready |
| NeedsUpdate | Needs update |
| PartlyPrepared | Partly prepared |
| CouldNotPrepare | Could not prepare |
| FileNotFound | File not found |
| Ignored | Skipped |

---

## 9. Source Lifecycle

```text
Added
  ↓
Preparing
  ↓
Active
  ├─ file changes detected → Needs update
  ├─ folder missing → Folder not found
  ├─ permission lost → Cannot open
  ├─ user pauses → Paused
  └─ user removes → Removed
```

---

## 10. Refresh Types

### 10.1. Startup Check

Required.

On app start:

```text
check registered folder exists
check permission lightly
detect obvious changed/missing files
queue safe refresh work
```

User copy if visible:

```text
Checking folders...
```

### 10.2. Manual Refresh

Required.

User action:

```text
[Check again]
```

or:

```text
[Prepare again]
```

For missing folders, prefer `Check again`. For active folders, prefer `Prepare again`.

### 10.3. Automatic Refresh

Deferred.

Automatic refresh means live filesystem watching. This should not be required for initial stabilization. If implemented later, it must be debounced, pausable, source-scoped, optional, resource-aware, and clear in settings.

---

## 11. Change Detection

### 11.1. Baseline Detection

For each file, store enough metadata:

```rust
pub struct FileFingerprint {
    pub size_bytes: u64,
    pub modified_at: Option<Timestamp>,
    pub content_hash: Option<String>,
}
```

Default policy:

```text
metadata check first
content hash only when needed
```

### 11.2. Change Outcomes

| Condition | File state |
|---|---|
| unchanged metadata | Ready |
| size/mtime changed | NeedsUpdate |
| file missing | FileNotFound |
| new file | Discovered |
| unsupported file | Ignored |
| read error | CouldNotPrepare |

### 11.3. Hash Policy

Do not hash every file on every startup by default. It is too expensive.

Use hash after extraction, when metadata is suspicious, when exact confirmation is required, or when data integrity requires it.

---

## 12. Missing Folders

When a folder is missing:

```text
Folder not found
```

Friendly detail:

```text
This can happen if a drive is disconnected or the folder was moved.
```

Actions:

```text
[Check again]
[Choose folder again]
[Remove from orbok]
```

Rules:

- do not immediately delete indexed data;
- mark results from this folder as file/folder not found where relevant;
- let the user recover by reconnecting drive and checking again.

---

## 13. External Drives and Network Folders

Treat unstable paths gently.

If a folder disappears:

```text
do not assume deletion
```

Keep it in the folder list as:

```text
Folder not found
```

Only remove it when user chooses:

```text
Remove from orbok
```

User copy:

```text
Your files were not deleted. orbok just cannot find this folder right now.
```

---

## 14. Change Storms

A change storm occurs when many changes arrive quickly, such as build output, dependency installation, sync client update, or git checkout.

Rules:

- batch changes;
- debounce automatic refresh;
- avoid many individual notices;
- use one summary.

User copy:

```text
Many files changed. orbok will prepare them gradually.
```

---

## 15. Ignore Policy

Default ignored examples may include:

```text
.git
target
node_modules
dist
build
.cache
```

This is sensitive because some users may want those folders. Keep ignored defaults conservative, show in Advanced view, and allow override later.

---

## 16. Symlink Policy

Default recommended policy:

```text
Do not follow symlinks outside selected folder.
```

If a symlink points outside, skip it, record an Advanced-view warning, and avoid scary default errors.

User copy if needed:

```text
Some linked folders were skipped because they point outside the folder you chose.
```

---

## 17. UI Wireframes

### 17.1. Folder List — Healthy

```text
Folders

[Choose a folder]

┌──────────────────────────────────────────────┐
│ Documents                                    │
│ Ready · 812 files                            │
│ [Prepare again] [Remove from orbok]          │
└──────────────────────────────────────────────┘
```

### 17.2. Folder Preparing

```text
┌──────────────────────────────────────────────┐
│ Documents                                    │
│ Preparing · 124 files ready                  │
│ ███████████░░░░░░ 62%                         │
│ [Pause preparing]                            │
└──────────────────────────────────────────────┘
```

### 17.3. Folder Missing

```text
┌──────────────────────────────────────────────┐
│ Reports                                      │
│ Folder not found                             │
│ This can happen if a drive is disconnected   │
│ or the folder was moved.                     │
│ [Check again] [Choose folder again]          │
│ [Remove from orbok]                          │
└──────────────────────────────────────────────┘
```

### 17.4. Needs Update

```text
┌──────────────────────────────────────────────┐
│ Documents                                    │
│ Needs update · 12 files changed              │
│ Search still works with prepared files.      │
│ [Prepare again]                              │
└──────────────────────────────────────────────┘
```

---

## 18. Events

```rust
pub enum SourceEvent {
    SourceAdded(SourceId),
    SourceCheckStarted(SourceId),
    SourceCheckFinished(SourceId),
    SourceMissing(SourceId),
    SourcePermissionProblem(SourceId),
    SourceRefreshRequested(SourceId),
    SourceRefreshPaused(SourceId),
    SourceRefreshResumed(SourceId),
    SourceRemoved(SourceId),
    FileDiscovered(FileId),
    FileChanged(FileId),
    FileMissing(FileId),
    FileIgnored(FileId),
}
```

---

## 19. Data Model (Amendment 1)

> **Amendment 1 (2026-09-03), after Task 035 landed.** This section specifies
> `SourceRecord.state` as a **persisted** field carrying the full 7-variant
> `SourceState`. The implementation persists 5 variants instead —
> `orbok_core::SourceStatus`, matching the `sources.status` catalog CHECK
> constraint — and derives `Preparing`/`NeedsUpdate` at render time from
> queued-job and stale-file counts the catalog already holds, rather than
> persisting them.
>
> This is a deliberate deviation, confirmed correct by review (Review 200,
> `.git-exclude/reviewed/200-task035-wire-rfc037-source-refresh-review.md`
> §2.2 — not git-tracked, see RFC-063 §5), not an oversight this section
> failed to anticipate: `Preparing` and
> `NeedsUpdate` are derived facts, computable from state the catalog already
> holds. Persisting them as their own column would be denormalization that
> can go stale — a worse failure mode than deriving them fresh on every
> render. `check_source_path` (`orbok_fs::source_lifecycle`) only ever
> returns `Active`/`FolderNotFound`/`PermissionProblem` in practice, so the
> 5-variant `SourceStatus` never needed to represent the other two anyway.
>
> The `SourceRecord` shape below is retained as originally specified — it
> remains the correct target for a future migration that widens
> `sources.status`, should one ever be needed — with this note recording
> that the shipped implementation does not match it in full. See the
> [closure record](../closures/037-source-lifecycle-refresh-policy-and-change-detection-ux.md)
> for how this was verified.

```rust
pub struct SourceRecord {
    pub id: SourceId,
    pub display_name: String,
    pub root_path: PathBuf,
    pub state: SourceState,
    pub last_checked_at: Option<Timestamp>,
    pub last_prepared_at: Option<Timestamp>,
    pub file_count_ready: u64,
    pub file_count_needs_update: u64,
    pub file_count_failed: u64,
}
```

```rust
pub struct FileRecord {
    pub id: FileId,
    pub source_id: SourceId,
    pub path: PathBuf,
    pub state: FileState,
    pub fingerprint: FileFingerprint,
    pub last_seen_at: Option<Timestamp>,
    pub last_prepared_at: Option<Timestamp>,
    pub last_problem_kind: Option<String>,
}
```

---

## 20. Testing

### 20.1. Unit Tests

- source state transitions;
- file state transitions;
- metadata change detection;
- missing folder detection;
- manual refresh creates jobs;
- removed source cancels jobs.

### 20.2. Integration Tests

- add folder;
- edit file;
- delete file;
- rename folder;
- disconnect simulated source;
- permission failure where platform supports;
- re-add moved folder;
- refresh after app restart.

### 20.3. UX Tests

- missing folder copy is understandable;
- remove from orbok does not imply deleting files;
- needs-update state does not block existing search;
- progress remains calm for many changes.

---

## 21. Acceptance Criteria

This RFC is accepted when:

1. Source states are explicit.
2. File states are explicit.
3. Startup check exists.
4. Manual refresh exists.
5. Missing folders are recoverable.
6. Removed folders do not delete source files.
7. Change detection marks files as needing update.
8. Search can still use prepared data while refresh is pending.
9. Live watcher is not required for initial stability.
10. User-facing labels avoid technical terms.

---

## 22. Final Decision

Implement a stable source lifecycle based on:

```text
startup check
manual refresh
gentle missing-folder recovery
bounded refresh jobs
deferred live watching
```

This keeps orbok reliable without surprising the user.
