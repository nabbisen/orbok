# RFC-040: Safe Diagnostics and Redacted Support Bundle

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 040  
**Title:** Safe Diagnostics and Redacted Support Bundle  
**Status:** Accepted
**Target milestone:** Supportability / privacy-safe debugging  
**Date:** 2026-06-18  
**Related RFCs:** RFC-018 Crash Recovery, Diagnostics, and Repair Tools, RFC-044 `orbok-extract` Production Hardening and Boundary Cleanup, RFC-039 Privacy Modes and Local Data Visibility  

**Returned to `accepted/` 2026-09-17 (RFC-063 §7, Review Request 236 §5.2 follow-up).** Carried
`Implemented (v0.19.0)` while §22 criterion 1 ("User can create a local
support file manually") was false: no view renders a Create support file
control, nothing in `crates/app` handles `Message::DiagnosticsCreateBundle`,
and nothing writes a support file. `crates/app/src/diagnostics.rs` holds the
manifest, a text-redaction helper and the preview text under
`#![allow(dead_code)]`, and its module comment has said since Review Request
132 §2c that none of it is wired — the status was never corrected. With no
file and no preview, criteria 1–7, 9 and 11 are unmet or unobservable;
criterion 10 is unmet (`redact_text` has no test); 8 and 12 hold only because
nothing exists to violate them.
The flow is built by Task 067, which also records the design amendments it
makes (a single readable file built from typed counts rather than redacted
text; no opt-ins).

---

## 1. Summary

This RFC defines a privacy-safe diagnostics bundle for orbok.

The accepted direction is:

```text
Diagnostics must help developers debug problems
without exposing document contents, search words, or raw local paths by default.
```

Users should be able to create a support file from the UI. Default support files are redacted. Sensitive details require explicit opt-in.

---

## 2. Motivation

A local document search app can fail in complex ways: parser failure, missing folders, database migration issue, model readiness problem, storage cleanup problem, indexing recovery issue, platform-specific permission issue, or slow search under load.

Developers need diagnostics to fix issues. But orbok’s trust promise means diagnostics must not leak private documents or search intent.

---

## 3. Goals

- Create a user-visible diagnostics export flow.
- Redact sensitive data by default.
- Exclude document contents by default.
- Exclude search text by default.
- Exclude raw local paths by default.
- Include enough technical state to debug.
- Provide explicit opt-ins for additional details.
- Show a preview summary before export.
- Respect privacy modes.
- Make support files easy to attach to bug reports.

---

## 4. Non-Goals

This RFC does not define automatic telemetry, crash upload service, remote logging, document upload, full database export, embedding export, model file export, or cloud support integration.

orbok must not silently upload diagnostics.

---

## 5. Product Decision

Use a manual export flow:

```text
Settings
  ↓
Create support file
  ↓
Preview what will be included
  ↓
Export local .zip or .tar.gz
```

Default copy:

```text
Create a support file

This file helps diagnose problems.
It does not include your documents or search words by default.
```

---

## 6. Bundle Contents

### 6.1. Included by Default

Default bundle may include:

- app version;
- build profile;
- OS/platform;
- architecture;
- enabled features;
- data directory type, redacted;
- settings summary, safe subset;
- database schema version;
- migration status;
- source counts, redacted;
- file state counts;
- extraction warning counts;
- indexing job counts;
- scheduler queue summary;
- model readiness status;
- model file presence/size only;
- storage category summary;
- recent error categories;
- redacted logs;
- diagnostics manifest.

### 6.2. Excluded by Default

Must exclude:

- document contents;
- extracted text;
- snippets;
- embeddings;
- search text;
- recent searches;
- raw local paths;
- full index contents;
- full database copy;
- model files;
- authentication secrets;
- environment variables unless allowlisted;
- OS username if avoidable.

---

## 7. Optional Opt-Ins

Optional checkboxes:

```text
[ ] Include folder names
[ ] Include recent search words
[ ] Include detailed logs
```

Do not include document contents in normal UI. That should remain unsupported unless a future explicit secure support process exists.

Each opt-in must explain risk.

Example:

```text
Include recent search words
This may reveal what you were looking for.
```

Default:

```text
Off
```

Strict privacy mode hides or disables sensitive opt-ins.

---

## 8. Redaction Policy

### 8.1. Paths

Raw path:

```text
/home/user/Documents/contracts/acme.pdf
```

Redacted path:

```text
<folder:1>/contracts/acme.pdf
```

or stricter:

```text
<file:pdf>
```

Default should not expose full home directory or username.

### 8.2. Search Text

Default:

```text
<redacted search text>
```

### 8.3. Source Names

Default:

```text
Folder 1
Folder 2
```

Optional folder-name opt-in may include display names but not full absolute paths.

### 8.4. Logs

Logs must pass through redaction before export.

Redact:

- paths;
- search text;
- snippets;
- environment variables;
- URLs with query tokens;
- model download tokens if any;
- user names where practical.

---

## 9. Bundle Format

Recommended:

```text
orbok-support-YYYYMMDD-HHMMSS.zip
```

or:

```text
orbok-support-YYYYMMDD-HHMMSS.tar.gz
```

Inside:

```text
manifest.json
summary.txt
app.json
platform.json
settings-redacted.json
storage-summary.json
sources-summary.json
indexing-summary.json
extraction-summary.json
models-summary.json
recent-errors.json
logs-redacted.txt
```

No database file by default.

---

## 10. Manifest

```json
{
  "app": "orbok",
  "bundle_version": 1,
  "created_at": "2026-06-18T00:00:00Z",
  "privacy_mode": "standard",
  "redacted": true,
  "includes_document_contents": false,
  "includes_search_text": false,
  "includes_raw_paths": false
}
```

---

## 11. UI Flow

### 11.1. Settings Entry

```text
Diagnostics

Create a support file if something is not working.
The file does not include your documents or search words by default.

[Create support file]
```

### 11.2. Preview

```text
Create support file

Included:
✓ App version
✓ Platform summary
✓ Folder status counts
✓ Search preparation status
✓ Model readiness
✓ Redacted logs

Not included:
× Documents
× Search words
× Raw folder paths

[Cancel] [Create file]
```

### 11.3. Optional Details

Advanced:

```text
More details

[ ] Include folder names
[ ] Include recent search words
[ ] Include detailed logs
```

### 11.4. Done

```text
Support file created.

[Show file]
[Done]
```

---

## 12. Data Sources

Diagnostics collectors:

```rust
pub trait DiagnosticsCollector {
    fn collect(&self, policy: &DiagnosticsPolicy) -> DiagnosticsSection;
}
```

Sections:

```rust
pub enum DiagnosticsSectionKind {
    App,
    Platform,
    Settings,
    Storage,
    Sources,
    Indexing,
    Extraction,
    Models,
    Scheduler,
    Errors,
    Logs,
}
```

Policy:

```rust
pub struct DiagnosticsPolicy {
    pub include_raw_paths: bool,
    pub include_folder_names: bool,
    pub include_recent_searches: bool,
    pub include_detailed_logs: bool,
    pub privacy_mode: PrivacyMode,
}
```

---

## 13. Extraction Diagnostics

From RFC-044, include aggregate warnings:

```json
{
  "files_prepared": 812,
  "files_partly_prepared": 12,
  "files_failed": 3,
  "warnings": {
    "possibly_scanned_pdf": 4,
    "some_pages_unreadable": 3,
    "size_limit_reached": 2
  }
}
```

Do not include extracted text.

---

## 14. Scheduler Diagnostics

From RFC-036, include:

```json
{
  "queues": {
    "scan": { "pending": 0, "running": 0 },
    "extract": { "pending": 12, "running": 1 },
    "keyword": { "pending": 3, "running": 1 },
    "embedding": { "pending": 120, "running": 0 }
  },
  "resource_mode": "normal"
}
```

No file paths by default.

---

## 15. Source Diagnostics

Default:

```json
{
  "sources_total": 3,
  "sources_ready": 2,
  "sources_missing": 1,
  "files_ready": 812,
  "files_needs_update": 12,
  "files_failed": 3
}
```

With folder-name opt-in:

```json
{
  "sources": [
    {
      "display_name": "Documents",
      "state": "ready",
      "files_ready": 812
    }
  ]
}
```

No raw full paths unless a future explicit raw-path opt-in exists.

---

## 16. Model Diagnostics

Include:

- model role;
- readiness;
- required files present;
- expected size known;
- validation status;
- backend enabled.

Do not include model files.

Example:

```json
{
  "embedding_model": {
    "status": "ready",
    "required_files_present": true,
    "validation": "passed"
  }
}
```

---

## 17. Log Redaction

Before export:

```text
raw log
  ↓
redaction pipeline
  ↓
logs-redacted.txt
```

Redaction rules must be tested.

Patterns:

- absolute paths;
- search text markers;
- source names if not opted in;
- URL query tokens;
- environment-like secrets.

---

## 18. Security and Privacy Rules

- Diagnostics export is manual only.
- No automatic upload.
- Export path is chosen by user or saved to a visible location.
- Bundle preview appears before creation.
- Sensitive opt-ins default off.
- Strict privacy mode disables sensitive opt-ins by default.
- Bundle manifest records what was included.
- User can cancel.

---

## 19. Events

```rust
pub enum DiagnosticsEvent {
    CreateSupportFileRequested,
    DiagnosticsPreviewGenerated,
    DiagnosticsExportStarted,
    DiagnosticsExportFinished,
    DiagnosticsExportFailed,
    DiagnosticsOptionChanged,
}
```

---

## 20. Error Handling

If export fails:

```text
Support file was not created.
Please choose another location or try again.
```

If logs cannot be read:

```text
Support file was created without logs.
```

If a section fails:

```text
Some details could not be included.
```

Do not fail the entire export for one non-critical section unless manifest cannot be written.

---

## 21. Testing

### 21.1. Unit Tests

- redacts absolute paths;
- redacts search text;
- redacts URL tokens;
- excludes document contents;
- excludes recent searches by default;
- manifest flags match policy.

### 21.2. Integration Tests

- create default support file;
- create support file in strict privacy mode;
- include folder names opt-in;
- export with missing log file;
- export with collector failure;
- verify no raw paths by default.

### 21.3. Manual QA

- inspect bundle manually;
- verify no source documents;
- verify no search words;
- verify preview copy;
- verify Cancel works;
- verify exported archive opens on Linux/Windows/macOS.

---

## 22. Acceptance Criteria

This RFC is accepted when:

1. User can create a local support file manually.
2. Default support file is redacted.
3. Document contents are excluded.
4. Search text is excluded.
5. Raw paths are excluded by default.
6. Bundle manifest states inclusion flags.
7. Preview shows what will and will not be included.
8. Sensitive opt-ins are off by default.
9. Strict privacy mode restricts diagnostics.
10. Redaction tests exist.
11. Export failure is friendly and recoverable.
12. No diagnostics are uploaded automatically.

---

## 23. Final Decision

Implement privacy-safe diagnostics:

```text
manual export
redacted by default
preview before creation
explicit opt-ins
no documents
no search text
no raw paths by default
```

This supports developers without breaking orbok’s local-first trust promise.
