# RFC-043: Model Download Readiness Check and Bounded Concurrency

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 043  
**Title:** Model Download Readiness Check and Bounded Concurrency  
**Status:** Implemented (v0.19.0)
**Amended:** 2026-09-25 (Amendment 1: the copy sections were superseded by RFC-050's wizard)
**Target milestone:** Setup Wizard / Model Management / Download Reliability  
**Date:** 2026-06-18  
**Related RFCs:** RFC-012 Model Registry and Installation Workflow, RFC-029 Model Download Integrity and Trust Policy, RFC-042 Search History and Reopen Recent Searches  

---

## 1. Summary

This RFC defines how **orbok** should check local model files, download only what is missing or invalid, and safely use bounded concurrent downloads.

The accepted product decision is:

```text
Check local files first.
Download only what is needed.
Use bounded concurrency with max_concurrent = 2.
Show one simple progress experience to the user.
Mark better search ready only after every required file is valid.
```

The current model setup requires two files:

```text
tokenizer.json
onnx/model.onnx
```

Because there are only two required files, concurrent download is reasonable and safe when bounded to two requests. However, concurrency must remain an internal implementation detail. Non-technical users should see only a simple setup progress screen.

---

## 2. Motivation

The current download behavior downloads model files one by one. This is simple but less efficient and less resilient.

A better setup flow should handle these real cases:

- the app was closed during download;
- one file completed and the other did not;
- the user retries after a network failure;
- the model files already exist locally;
- one file exists but is empty or incomplete;
- a server temporarily slows or rejects requests;
- the app restarts after a partial setup.

Without a local readiness check, orbok may waste time downloading files already present. Worse, it may treat a partially downloaded model as usable.

This RFC ensures setup is:

- faster;
- safer;
- restart-friendly;
- clearer to the user;
- respectful of remote service rate limits.

---

## 3. Goals

- Check local model files before downloading.
- Determine which required files are already valid.
- Download only missing or invalid files.
- Support safe retry after interrupted downloads.
- Use concurrent downloads only for files that still need work.
- Limit concurrency to 2 for the current model.
- Store incomplete downloads in temporary files.
- Validate files before moving them to final paths.
- Mark the model ready only when all required files pass validation.
- Show a single friendly progress experience.
- Avoid exposing technical download state to non-technical users.
- Handle rate limits and network failures gracefully.
- Keep future model-file expansion possible.

---

## 4. Non-Goals

This RFC does not define:

- model selection policy;
- model trust policy in full detail;
- automatic model update;
- multiple model download queues;
- background downloads while the app is closed;
- peer-to-peer download;
- cloud sync;
- user-visible concurrency settings;
- advanced download tuning UI.

This RFC also does not require resumable HTTP range requests for the first implementation. Safe restart of incomplete files is enough for P0.

---

## 5. Product Decision

## 5.1. Accepted

Use this behavior:

```text
Start setup
  ↓
Check local files
  ↓
Skip valid files
  ↓
Download missing or invalid files
  ↓
Validate each completed file
  ↓
Mark better search ready only when all required files are valid
```

## 5.2. Download Concurrency

Use:

```text
max_concurrent = 2
```

for the current required two-file model setup.

## 5.3. User-Facing UI

Show one progress screen:

```text
Downloading better search
████████████░░░░░░ 62%
Your files stay on this computer.
```

Do not show:

```text
2 concurrent transfers
thread pool
HTTP stream
tokenizer.json worker
```

in the default UI.

---

## 6. Required Files

For the current default model, the required files are:

| Logical file | Expected relative path | Required |
|---|---|---|
| tokenizer | `tokenizer.json` | Yes |
| model | `onnx/model.onnx` | Yes |

The model is usable only when both are present and valid.

---

## 7. Local Readiness Check

## 7.1. When to Check

orbok must check local model files:

- at app startup;
- before showing the model setup wizard;
- before starting a download;
- after a download completes;
- after the user chooses an existing model folder;
- after restart from an interrupted setup.

## 7.2. Readiness Outcomes

Internal file state:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalFileStatus {
    Ready,
    Missing,
    Partial,
    Invalid,
    CannotCheck,
}
```

Default user-facing copy must not expose these internal names directly.

Recommended UI copy:

| Internal status | User-facing copy |
|---|---|
| Ready | Ready |
| Missing | Needed |
| Partial | Needs to finish |
| Invalid | Needs to be replaced |
| CannotCheck | Could not check |

## 7.3. Model Readiness

Internal model readiness:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelReadiness {
    Ready,
    NeedsDownload,
    NeedsRepair,
    CannotCheck,
}
```

Rules:

- `Ready` only when every required file is `Ready`.
- `NeedsDownload` when at least one file is `Missing`.
- `NeedsRepair` when at least one file is `Partial` or `Invalid`.
- `CannotCheck` when file access failed unexpectedly.

---

## 8. Validation Rules

## 8.1. Minimum P0 Validation

At minimum, check:

```text
file exists
file is a regular file
file is not empty
file path matches expected layout
file can be opened for reading
```

## 8.2. Recommended P1 Validation

If known from a manifest, also check:

```text
expected byte size
checksum
```

## 8.3. Final Readiness Rule

The model must not be marked ready unless:

```text
all required files are present
all required files are non-empty
all required files pass available validation
```

Do not mark the model ready based only on folder existence.

---

## 9. Partial File Policy

## 9.1. Temporary File Names

Download to temporary files first.

Recommended pattern:

```text
tokenizer.json.part
onnx/model.onnx.part
```

or:

```text
.download/tokenizer.json.part
.download/onnx-model.onnx.part
```

## 9.2. Atomic Completion

Completion flow:

```text
download to temporary file
flush file
validate temporary file
rename temporary file to final path
re-check final path
mark file ready
```

The final path should only contain a validated completed file.

## 9.3. Handling Existing `.part` Files

P0 behavior:

```text
delete partial file and restart that file
```

P1 behavior:

```text
resume only if server and metadata make it safe
```

For P0, restart is simpler and safer.

## 9.4. Existing Final File

If the final file exists and is valid:

```text
skip download
```

If the final file exists but is invalid:

```text
move aside or replace after confirmation-free repair
```

Because this file is app-managed model data, replacing invalid model files does not require user confirmation.

User-facing copy:

```text
Some search helper files need to be repaired. orbok will download only what is needed.
```

---

## 10. Download Plan

## 10.1. DownloadPlan

```rust
#[derive(Debug, Clone)]
pub struct DownloadPlan {
    pub model_id: ModelId,
    pub files: Vec<ModelFilePlan>,
    pub max_concurrent: usize,
}
```

## 10.2. ModelFilePlan

```rust
#[derive(Debug, Clone)]
pub struct ModelFilePlan {
    pub logical_name: String,
    pub relative_path: PathBuf,
    pub remote_url: Url,
    pub expected_size: Option<u64>,
    pub expected_sha256: Option<String>,
    pub local_status: LocalFileStatus,
    pub action: DownloadAction,
}
```

## 10.3. DownloadAction

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadAction {
    Skip,
    Download,
    Replace,
    Retry,
}
```

## 10.4. Planning Rules

```text
Ready        → Skip
Missing      → Download
Partial      → Replace or Retry
Invalid      → Replace
CannotCheck  → Stop and show friendly problem
```

Concurrency applies only to files with:

```text
Download
Replace
Retry
```

---

## 11. Bounded Concurrency

## 11.1. Constants

```rust
pub const DEFAULT_MODEL_DOWNLOAD_CONCURRENCY: usize = 2;
pub const MAX_MODEL_DOWNLOAD_CONCURRENCY: usize = 2;
```

## 11.2. Rule

For the current model:

```text
run at most 2 file downloads at the same time
```

## 11.3. Future-Proofing

If future models require many files, this RFC should be revisited.

Possible future policy:

```text
default = 2
maximum = 3 or 4 only after rate-limit and reliability review
```

Do not expose this as a normal user setting.

---

## 12. Rate Limit and Server Safety

## 12.1. Rate Limit Behavior

If the server responds with a temporary rate-limit or overload response:

1. stop starting new file downloads;
2. let active safe downloads finish if possible;
3. retry with backoff;
4. if still blocked, show friendly retry message.

User-facing copy:

```text
The download is taking longer than expected. Please try again later.
```

## 12.2. Backoff

Recommended internal policy:

```text
first retry: short delay
second retry: longer delay
third retry: stop and show retry action
```

Do not show backoff details in default UI.

## 12.3. User Retry

The user should see:

```text
[Try again]
```

Retry should re-run local readiness check first, then download only what is still needed.

---

## 13. Progress Model

## 13.1. Internal Progress

Track per-file progress:

```rust
#[derive(Debug, Clone)]
pub struct FileDownloadProgress {
    pub relative_path: PathBuf,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub status: FileDownloadStatus,
}
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDownloadStatus {
    Pending,
    Checking,
    Downloading,
    Validating,
    Complete,
    Failed,
    Skipped,
}
```

## 13.2. Overall Progress

User-facing progress should be combined.

If all file sizes are known:

```text
overall = total_downloaded_bytes / total_expected_bytes
```

If some file sizes are unknown:

```text
show indeterminate progress or step progress
```

Preferred copy:

```text
Downloading better search
File 1 of 2 ready
```

## 13.3. Skipped Files

If one file is already present and valid, count it as complete in the overall progress.

Example:

```text
tokenizer.json ready
onnx/model.onnx downloading
```

Default UI should simply show:

```text
Downloading what is needed...
```

Advanced view may show per-file details.

---

## 14. User-Facing Wizard States

## 14.1. Checking

```text
Checking search helper...
```

Optional helper:

```text
orbok is checking what is already on this computer.
```

## 14.2. Already Ready

```text
Better search is ready.
```

Action:

```text
[Start searching]
```

## 14.3. Needs Download

```text
Some search helper files are needed.
orbok will download only what is missing.
```

Action:

```text
[Download]
```

## 14.4. Downloading

```text
Downloading better search
████████████░░░░░░ 62%
Your files stay on this computer.
```

## 14.5. Repairing

```text
Repairing better search
Some files need to be replaced. Your own files are not changed.
```

## 14.6. Failure

```text
Download did not finish.
Please check your connection and try again.
```

Action:

```text
[Try again]
```

---

## 15. Default and Advanced UI

## 15.1. Default UI

Default users see:

- checking;
- downloading;
- progress;
- ready;
- friendly failure;
- Try again.

They do not see:

- file paths;
- checksums;
- byte counts unless helpful;
- concurrent download count;
- HTTP status;
- internal file state.

## 15.2. Advanced View

Advanced view may show:

```text
2 files required
1 ready
1 downloading
```

Optional detail:

```text
tokenizer.json — ready
onnx/model.onnx — downloading
```

Even in Advanced view, avoid unnecessary network jargon.

---

## 16. State Model

## 16.1. ModelSetupState

```rust
#[derive(Debug, Clone)]
pub enum ModelSetupState {
    Checking,
    Ready,
    NeedsDownload(DownloadPlan),
    Downloading(DownloadSession),
    Repairing(DownloadPlan),
    Failed(FriendlyDownloadProblem),
}
```

## 16.2. DownloadSession

```rust
#[derive(Debug, Clone)]
pub struct DownloadSession {
    pub model_id: ModelId,
    pub files: Vec<FileDownloadProgress>,
    pub overall: OverallDownloadProgress,
    pub max_concurrent: usize,
}
```

## 16.3. OverallDownloadProgress

```rust
#[derive(Debug, Clone)]
pub enum OverallDownloadProgress {
    Known {
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Step {
        completed_files: usize,
        total_files: usize,
    },
    Indeterminate,
}
```

## 16.4. FriendlyDownloadProblem

```rust
#[derive(Debug, Clone)]
pub enum FriendlyDownloadProblem {
    NetworkUnavailable,
    ServerBusy,
    NotEnoughSpace,
    CannotWriteFiles,
    CannotCheckFiles,
    ValidationFailed,
    Unexpected,
}
```

---

## 17. Message Model

```rust
#[derive(Debug, Clone)]
pub enum ModelDownloadMessage {
    StartModelCheck,
    ModelCheckFinished(ModelReadinessReport),

    StartDownload,
    DownloadPlanCreated(DownloadPlan),

    FileDownloadProgress(FileDownloadProgress),
    FileDownloadFinished(PathBuf),
    FileDownloadFailed(PathBuf, FriendlyDownloadProblem),

    DownloadFinished,
    DownloadFailed(FriendlyDownloadProblem),

    RetryDownload,
    SkipBetterSearch,
}
```

Rules:

- `StartModelCheck` runs before every download attempt.
- `RetryDownload` must check local files again before downloading.
- `DownloadFinished` must trigger final validation.
- UI state must change immediately after user action.

---

## 18. Control Flow

## 18.1. Startup

```text
app starts
  ↓
check expected model folder
  ↓
if all files valid:
    setup wizard not needed
else:
    show setup wizard with readiness result
```

## 18.2. User Starts Download

```text
user clicks Download
  ↓
check local files again
  ↓
create download plan
  ↓
skip valid files
  ↓
download needed files with max_concurrent = 2
  ↓
validate all files
  ↓
mark ready
```

## 18.3. Retry After Failure

```text
user clicks Try again
  ↓
check local files again
  ↓
completed files are skipped
  ↓
only missing or invalid files are downloaded
```

## 18.4. App Restart During Download

```text
app starts
  ↓
find final files and partial files
  ↓
validate final files
  ↓
delete or ignore partial files
  ↓
download only what is missing
```

---

## 19. File Safety Rules

## 19.1. Directory Creation

Before download:

```text
create model directory
create parent directories
ensure writable
```

## 19.2. Disk Space

If expected sizes are known, check available space before download.

Friendly copy:

```text
More space is needed to finish the download.
```

Action:

```text
[Choose another location]
```

or:

```text
[Try again]
```

depending on the larger model-management design.

## 19.3. Atomic Rename

Use atomic rename where the platform supports it.

If rename fails:

- do not mark file ready;
- keep friendly failure;
- allow retry.

## 19.4. Cleanup

After successful setup:

- remove stale `.part` files;
- remove obsolete temporary files for the same model;
- keep valid completed files.

---

## 20. Error Handling

## 20.1. Network Unavailable

Copy:

```text
Download did not finish.
Please check your connection and try again.
```

## 20.2. Server Busy or Rate Limited

Copy:

```text
The download is taking longer than expected.
Please try again later.
```

## 20.3. Not Enough Space

Copy:

```text
More space is needed to finish the download.
```

## 20.4. Cannot Write Files

Copy:

```text
orbok could not save the search helper files here.
Please choose another location or check folder permissions.
```

## 20.5. Validation Failed

Copy:

```text
Some downloaded files could not be used.
orbok can download them again.
```

Action:

```text
[Try again]
```

## 20.6. Cannot Check Files

Copy:

```text
orbok could not check the search helper files.
Please choose the folder again.
```

---

## 21. Privacy and Trust

The setup wizard must keep the local-first promise visible.

Required copy during setup:

```text
Your files stay on this computer.
```

Important:

- model download contacts the model host;
- document contents must not be uploaded;
- search history and document text must not be included in download requests;
- logs must not include local document paths unless explicit diagnostics are enabled.

---

## 22. Interaction With Existing Setup Choices

The wizard still supports:

```text
Download recommended helper
Choose files already on this computer
Use basic search only
```

This RFC changes the Download path by adding:

- local readiness check;
- skip existing valid files;
- bounded concurrent download;
- partial download recovery.

The “Choose files already on this computer” path should use the same validation logic.

---

## 23. Interaction With Basic Search

If model setup fails or is skipped:

```text
Basic search remains available.
```

Copy:

```text
Basic search is ready. Search by meaning can be added later.
```

The app must not block basic search because better search setup failed.

---

## 24. Implementation Priority

## 24.1. P0

Implement:

- local readiness check;
- skip valid files;
- temporary `.part` download files;
- final validation;
- bounded concurrency of 2;
- retry only missing or invalid files;
- friendly setup copy;
- final all-files-ready gate.

## 24.2. P1

Implement:

- checksum validation from manifest;
- expected-size validation;
- disk-space precheck;
- advanced per-file detail;
- rate-limit backoff;
- cleanup of stale `.part` files.

## 24.3. P2

Implement:

- safe HTTP resume if supported;
- user-selectable model storage location;
- multiple model download queue;
- optional background download continuation;
- future concurrency review for models with many files.

---

## 25. Test Plan

## 25.1. Readiness Tests

- both files missing → NeedsDownload;
- tokenizer ready, model missing → NeedsDownload with one download action;
- tokenizer missing, model ready → NeedsDownload with one download action;
- both ready → Ready;
- empty file → Invalid;
- unreadable file → CannotCheck;
- partial file only → Partial or NeedsRepair.

## 25.2. Planning Tests

- Ready file produces Skip;
- Missing file produces Download;
- Invalid file produces Replace;
- Partial file produces Replace or Retry;
- CannotCheck stops plan with friendly problem.

## 25.3. Download Tests

- two missing files download concurrently with limit 2;
- one ready and one missing downloads only missing file;
- final file is not created until validation passes;
- `.part` file is not treated as ready;
- failed download can be retried;
- retry skips already completed file.

## 25.4. Restart Tests

- app restart after one completed file and one partial file;
- completed file is skipped;
- partial file is restarted or safely retried;
- model is not marked ready until all files pass validation.

## 25.5. Error Tests

- network failure shows friendly retry;
- rate-limit response backs off or fails gracefully;
- disk full shows friendly space message;
- validation failure causes retry path;
- cannot write final file does not mark ready.

## 25.6. UI Tests

- default UI shows one progress bar;
- default UI does not show concurrency details;
- Advanced view may show per-file status;
- Try again re-checks files first;
- Use basic search only remains available.

---

## 26. Acceptance Criteria

This RFC is accepted when:

1. orbok checks local model files before downloading.
2. Already valid files are skipped.
3. Missing or invalid files are downloaded.
4. Partial files are not treated as ready.
5. Downloads use temporary files and final rename after validation.
6. The current model download can run two file downloads concurrently.
7. Concurrency is internally limited to 2.
8. Rate-limit and network failures do not crash the app.
9. Retry re-checks local files and downloads only what is still needed.
10. Model is marked ready only after every required file passes validation.
11. Default UI shows one friendly progress experience.
12. Basic search remains available if better search setup is skipped or fails.
13. No technical download jargon appears in the default UI.

---

## 26a. Amendment 1 (2026-09-25) — the copy was superseded by RFC-050's wizard

Task 115's origin: Task 112's audit (Review Request 290 §2). This RFC's
*behaviour* shipped and is met (criteria 1–12): the local check, skipping valid
files, `.part` staging and a final rename, two concurrent transfers, retry that
re-checks. Its **user-facing copy** (§14, §15.1) was written before RFC-050
rebuilt delivery as the managed model store, and RFC-050's wizard says the same
things in other words. The seven keys that carried this RFC's wording were
never rendered and are deleted:

| This RFC's copy | Replaced by (the wizard's own key) |
|---|---|
| §14.1 "Checking…" (`ModelCheckingFiles`) | `WizardTitleValidating` "Checking model folder" |
| §14.2 "…ready" (`ModelAlreadyReady`) | `WizardTitleReady`, the identical text |
| §14.3 "only what is missing" (`ModelNeedsDownload`) | the consent screen's "Download from HuggingFace" plan |
| §14.4 "Downloading…" (`ModelDownloadInProgress`) | `WizardDownloadProgress` "Downloading model…" |
| §14.4 "Your files stay on this computer." (`ModelFilesStayLocal`) | `WizardBodyNotConfigured` "No files are uploaded — inference runs locally." |
| §14.5 "Repairing…" (`ModelRepairingFiles`) | none: a failed verification says "Try again", and Try again repairs (criterion 9) |
| §15.1 "keyword search is ready" (`ModelBasicSearchAvailable`) | `ModelsKeywordOnlyHint`, `WizardBodyNotConfigured` |

**Criterion 13** (no technical download jargon in the default UI) was only
partly met: the consent screen said "Immutable revision" and, while
downloading, "Tokenizer". They now say **Version** (「バージョン」; the value
beside it is still the exact revision identifier) and **Vocabulary**
(「語彙データ」; beside "Search model"). Criterion 13 is met.

---

## 27. Final Decision

Implement model setup as:

```text
Check local files first.
Download only what is needed.
Use max_concurrent = 2.
Validate before ready.
Show simple progress.
Recover safely after failure or restart.
```

This improves speed and reliability while keeping the setup experience simple for non-technical users.
