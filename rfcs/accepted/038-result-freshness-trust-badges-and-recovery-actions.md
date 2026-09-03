# RFC-038: Result Freshness, Trust Badges, and Recovery Actions

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 038  
**Title:** Result Freshness, Trust Badges, and Recovery Actions  
**Status:** Accepted
**Target milestone:** Search result trust / UX honesty  
**Date:** 2026-06-18  

**Returned to `accepted/` 2026-09-02 (RFC-063 §7).** Accepted 2026-06-18; carried `Implemented (v0.18.0)` while §16 criteria **4, 5 and 7** are false — `bootstrap/search.rs` hardcodes `ResultTrustDisplay::default()` = `Ready` with no recovery actions, so a deleted file's result reads clean. **Wiring is RFC-060 §7.**
**Related RFCs:** RFC-041 Search, Narrow Results, and Browse Around, RFC-044 `orbok-extract` Production Hardening and Boundary Cleanup, RFC-037 Source Lifecycle, Refresh Policy, and Change Detection UX  

---

## 1. Summary

This RFC defines how orbok communicates whether search results are current, incomplete, missing, or partially prepared.

The accepted direction is:

```text
Every result should be understandable and trustworthy.
If a result may be stale or incomplete, say so plainly.
Always offer a recovery action where possible.
```

Default UI must avoid technical terms such as `stale`, `index`, `extract warning`, and `cache`.

Use plain trust labels:

```text
Ready
Needs update
File not found
Still being prepared
Partly prepared
Cannot open
```

---

## 2. Motivation

Search results are not all equal. A result may appear because the file is current, changed after indexing, missing, partially extracted, still being prepared, or inaccessible.

If orbok hides these facts, users may lose trust when a result fails to open or appears outdated. If orbok over-explains with technical language, non-technical users may feel confused.

This RFC defines clear trust badges and actions.

---

## 3. Goals

- Define result freshness states.
- Map extraction warnings to simple result messages.
- Show trust badges only when useful.
- Avoid color-only status.
- Provide recovery actions.
- Keep normal ready results visually clean.
- Support Advanced view for details.
- Integrate with search filters and source lifecycle.
- Prevent stale or missing results from appearing as fully trustworthy.

---

## 4. Non-Goals

This RFC does not define ranking algorithms, extraction internals, source refresh scheduling, diagnostics bundles, model download behavior, or full preview rendering.

---

## 5. Result Trust States

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTrustState {
    Ready,
    NeedsUpdate,
    FileNotFound,
    StillBeingPrepared,
    PartlyPrepared,
    CannotOpen,
}
```

### 5.1. Ready

Copy:

```text
Ready
```

Default display:

```text
No badge needed
```

A clean result should not be cluttered.

### 5.2. NeedsUpdate

Copy:

```text
Needs update
```

Meaning: file changed since it was prepared. The result may still be useful but not guaranteed current.

Action:

```text
[Prepare again]
```

### 5.3. FileNotFound

Copy:

```text
File not found
```

Meaning: file was moved, deleted, or drive disconnected.

Actions:

```text
[Check folder]
[Remove from results]
```

### 5.4. StillBeingPrepared

Copy:

```text
Still being prepared
```

Meaning: file exists but full indexing is not complete.

### 5.5. PartlyPrepared

Copy:

```text
Partly prepared
```

Meaning: some content was extracted, but warnings exist.

Action:

```text
[View details]
```

### 5.6. CannotOpen

Copy:

```text
Cannot open
```

Meaning: permission or OS open issue.

Action:

```text
[Show in folder]
```

if possible.

---

## 6. Badge Display Rules

### 6.1. Default View

Show badges only when trust-relevant:

```text
Needs update
File not found
Still being prepared
Partly prepared
Cannot open
```

Do not show `Ready`, `Exact`, `Meaning`, `PDF`, or `Markdown` unless Advanced view is on or the badge is directly useful.

### 6.2. Advanced View

Advanced view may show:

```text
Exact words
Meaning
PDF
Location approximate
Partly prepared
```

Still avoid algorithm names.

### 6.3. Color Rule

Badges must use text, not color alone. Color may reinforce, but not replace text.

---

## 7. Warning Mapping

RFC-044 adds structured extraction warnings.

Map them to trust states:

| ExtractWarning | ResultTrustState | Default copy |
|---|---|---|
| SomePagesUnreadable | PartlyPrepared | Partly prepared |
| PossiblyScannedPdf | PartlyPrepared | Partly prepared |
| SizeLimitReached | PartlyPrepared | Partly prepared |
| UnsupportedDocumentPart | PartlyPrepared | Partly prepared |
| ApproximateLocationOnly | Ready or PartlyPrepared | hidden default or Advanced detail |
| EncodingUnsupported | CannotOpen or PartlyPrepared | Could not prepare fully |
| MalformedContentRecovered | PartlyPrepared | Partly prepared |

---

## 8. Result Card Wireframes

### 8.1. Ready Result

```text
┌──────────────────────────────────────────────┐
│ auth.md                                      │
│ Documents / security                         │
│ ...refresh tokens should expire earlier...   │
└──────────────────────────────────────────────┘
```

No badge shown.

### 8.2. Needs Update

```text
┌──────────────────────────────────────────────┐
│ auth.md                                      │
│ Documents / security                         │
│ ...refresh tokens should expire earlier...   │
│ [Needs update]                               │
└──────────────────────────────────────────────┘
```

### 8.3. File Not Found

```text
┌──────────────────────────────────────────────┐
│ idp-review.pdf                               │
│ Reports                                      │
│ ...client secret rotation policy states...   │
│ [File not found]                             │
└──────────────────────────────────────────────┘
```

### 8.4. Partly Prepared

```text
┌──────────────────────────────────────────────┐
│ annual-report.pdf                            │
│ Research papers                              │
│ ...governance policy...                      │
│ [Partly prepared]                            │
└──────────────────────────────────────────────┘
```

---

## 9. Result Preview Actions

### 9.1. Needs Update Preview

```text
auth.md
Documents / security

This file changed after orbok prepared it.

[Prepare again]
[Open file anyway]
```

### 9.2. File Not Found Preview

```text
idp-review.pdf
Reports

orbok cannot find this file. It may have been moved, deleted,
or the drive may be disconnected.

[Check folder]
[Remove from results]
```

### 9.3. Partly Prepared Preview

```text
annual-report.pdf
Research papers

Only part of this file was prepared.

[Open file]
[View details]
```

Advanced details may say:

```text
Some pages could not be prepared.
This PDF may contain images instead of selectable text.
```

---

## 10. Interaction With Filters

Add optional filter in “More ways to narrow”:

```text
Ready status
(✓) Ready
( ) Needs update
( ) File not found
( ) Partly prepared
```

Default behavior:

- show Ready results normally;
- include Needs update only when useful;
- suppress File not found from default ranking unless relevant;
- never hide user-relevant warnings if a result is shown.

---

## 11. Ranking Guidance

Trust state may affect display but should not silently erase useful results.

| State | Default ranking |
|---|---|
| Ready | normal |
| NeedsUpdate | slightly lower |
| StillBeingPrepared | lower or shown in preparing section |
| PartlyPrepared | normal or slightly lower |
| FileNotFound | hidden by default unless relevant |
| CannotOpen | lower |

This is implementation guidance, not a ranking formula.

---

## 12. Data Model

```rust
pub struct SearchResultTrust {
    pub state: ResultTrustState,
    pub warnings: Vec<ResultWarningSummary>,
    pub recovery_actions: Vec<ResultRecoveryAction>,
}
```

```rust
pub enum ResultRecoveryAction {
    PrepareAgain,
    CheckFolder,
    RemoveFromResults,
    OpenAnyway,
    ShowInFolder,
    ViewDetails,
}
```

```rust
pub enum ResultWarningSummary {
    SomePagesUnreadable,
    PossiblyScannedPdf,
    SizeLimitReached,
    UnsupportedDocumentPart,
    ApproximateLocation,
}
```

---

## 13. Event Model

```rust
pub enum ResultTrustEvent {
    TrustStateComputed(FileId, ResultTrustState),
    RecoveryActionRequested(ResultRecoveryAction),
    ResultHiddenBecauseFileMissing(FileId),
    ResultWarningDisplayed(FileId),
}
```

---

## 14. UX Copy

| Situation | Copy |
|---|---|
| File changed | This file changed after orbok prepared it. |
| Missing file | orbok cannot find this file. |
| Missing folder | The folder may be disconnected or moved. |
| Partial extraction | Only part of this file was prepared. |
| Scanned PDF | This PDF may contain images instead of selectable text. |
| Unreadable pages | Some pages could not be prepared. |
| Size limit | Only part of this large file was prepared. |
| Permission | orbok cannot open this file. |

---

## 15. Testing

### 15.1. Unit Tests

- file ready → Ready;
- changed file → NeedsUpdate;
- missing file → FileNotFound;
- extraction warning → PartlyPrepared;
- permission issue → CannotOpen;
- preparing job → StillBeingPrepared.

### 15.2. UI Tests

- Ready result has no clutter badge;
- Needs update badge appears;
- File not found badge appears;
- Partly prepared badge appears;
- badges are visible without color;
- recovery actions match state.

### 15.3. Integration Tests

- edit file after indexing;
- delete file after indexing;
- disconnect source folder;
- PDF emits warning;
- large file limit emits warning;
- search results remain understandable.

---

## 16. Acceptance Criteria

This RFC is accepted when:

1. Result trust states are explicit.
2. Default UI shows trust badges only when useful.
3. Warnings from extraction are represented in result trust.
4. Missing files do not appear as normal ready results.
5. Changed files show Needs update.
6. Partly prepared files are honest but still searchable.
7. Recovery actions are available.
8. Status is not communicated by color alone.
9. Advanced view can show more detail.
10. Copy avoids technical terms.

---

## 17. Final Decision

Implement result trust as a first-class search result property:

```text
Ready
Needs update
File not found
Still being prepared
Partly prepared
Cannot open
```

Use badges sparingly, and always give the user a safe next action.
