# RFC-036: Resource-Aware Indexing Scheduler and Backpressure

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 036  
**Title:** Resource-Aware Indexing Scheduler and Backpressure  
**Status:** Implemented (v0.17.0)
**Target milestone:** Indexing stability / weak-machine responsiveness  
**Date:** 2026-06-18  
**Related RFCs:** RFC-004 File Scanner and Change Detection, RFC-005 Document Extraction Pipeline, RFC-006 Adaptive Chunking and Location Metadata, RFC-008 Embedding Model and Vector Storage, RFC-044 `orbok-extract` Production Hardening and Boundary Cleanup  

---

## 1. Summary

This RFC defines how orbok schedules indexing work without making the app feel slow, hot, frozen, or unsafe on low-powered machines.

The accepted direction is:

```text
Keep the UI responsive.
Prioritize user-triggered search.
Run indexing in bounded background queues.
Apply backpressure instead of creating unlimited work.
Pause, resume, and recover safely.
```

orbok is a local-first document search app. Its value depends not only on result quality, but also on the feeling that the app is calm, predictable, and respectful of the user’s machine.

---

## 2. Motivation

Indexing work can be expensive: recursive scanning, document extraction, normalization, chunking, keyword indexing, embedding generation, model loading, recovery after interruption, and cleanup. On a powerful machine this may feel invisible. On a weak laptop it may make the app appear broken.

User trust is lost if typing lags, search waits behind indexing, CPU fans spin loudly for too long, battery drains unexpectedly, or one bad file blocks all work.

This RFC defines a resource-aware scheduler so orbok behaves like a polite desktop app.

---

## 3. Goals

- Keep search and UI interaction responsive.
- Avoid unbounded background work.
- Allow indexing to pause, resume, cancel, and recover.
- Separate work by cost and priority.
- Limit CPU, memory, disk, and model-inference pressure.
- Prioritize visible user actions over background indexing.
- Make progress understandable to non-technical users.
- Allow partial readiness: users can search files that are already prepared.
- Recover cleanly after crash or app restart.
- Support future worker tuning without exposing technical settings to normal users.

---

## 4. Non-Goals

This RFC does not define extraction internals, ranking, file-watcher policy in detail, model download behavior, distributed indexing, cloud indexing, GPU scheduling, or advanced user-visible scheduler settings.

It also does not require perfect automatic machine profiling for the first release.

---

## 5. Product Decision

The scheduler must use this principle:

```text
User-visible work first.
Background work only when it does not harm interaction.
```

Priority order:

1. UI responsiveness.
2. Active search.
3. Result opening and preview loading.
4. User-requested folder preparation.
5. Refresh of changed files.
6. Embedding generation.
7. Cleanup and maintenance.

Indexing should never make the search box feel delayed.

---

## 6. Work Categories

### 6.1. Discovery Work

Find files under registered folders.

Examples:

- recursive scan;
- file metadata collection;
- path validation;
- change detection;
- stale/missing marking.

Cost profile:

```text
I/O heavy, usually light CPU
```

### 6.2. Extraction Work

Read supported documents and extract text segments.

Examples:

- text extraction;
- Markdown extraction;
- PDF extraction;
- DOCX extraction;
- HTML extraction.

Cost profile:

```text
I/O + CPU, can be unpredictable
```

### 6.3. Chunking Work

Convert extraction output into chunks for indexing.

Cost profile:

```text
CPU light to medium
```

### 6.4. Keyword Index Work

Update exact-word search index.

Cost profile:

```text
I/O + CPU, must stay responsive
```

### 6.5. Embedding Work

Generate vectors for meaning-based search.

Cost profile:

```text
CPU/GPU heavy, memory heavy, model dependent
```

### 6.6. Maintenance Work

Cleanup, repair, recount storage, stale job recovery.

Cost profile:

```text
usually background, not urgent
```

---

## 7. Queue Model

Use separate bounded queues:

```text
scan_queue
extract_queue
chunk_queue
keyword_queue
embedding_queue
maintenance_queue
```

Each queue must have:

- maximum length or backpressure policy;
- priority;
- cancellation behavior;
- retry behavior;
- progress reporting;
- durable job state where needed.

---

## 8. Priority Policy

### 8.1. Priority Levels

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkPriority {
    UserBlocking,
    UserVisible,
    NormalBackground,
    LowBackground,
    Maintenance,
}
```

Examples:

| Work | Priority |
|---|---|
| active search | UserBlocking |
| result preview | UserBlocking |
| user clicked “Prepare now” | UserVisible |
| newly added folder | UserVisible |
| startup refresh | NormalBackground |
| embedding generation | LowBackground |
| cleanup old previews | Maintenance |

### 8.2. Preemption

If the user starts searching while embedding is running:

```text
embedding work should yield, pause, or reduce concurrency
```

Search must not wait behind embedding work.

---

## 9. Worker Concurrency

### 9.1. Default Conservative Limits

```rust
pub struct SchedulerLimits {
    pub scan_workers: usize,
    pub extract_workers: usize,
    pub chunk_workers: usize,
    pub keyword_workers: usize,
    pub embedding_workers: usize,
    pub maintenance_workers: usize,
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            scan_workers: 1,
            extract_workers: 1,
            chunk_workers: 1,
            keyword_workers: 1,
            embedding_workers: 1,
            maintenance_workers: 1,
        }
    }
}
```

These defaults are intentionally conservative. A single calm worker is better than a fast app that feels hostile on a poor machine.

### 9.2. Embedding Rule

Embedding work must be the easiest to slow down.

Rules:

- run at low priority by default;
- pause or reduce when user is searching;
- never block keyword search;
- unload model if memory pressure policy requires it.

---

## 10. Backpressure

### 10.1. Problem

If scanning produces 100,000 files quickly, extraction and embedding cannot accept unlimited jobs.

### 10.2. Required Behavior

When downstream queues are full:

```text
pause upstream production
show calm progress
resume when capacity returns
```

Do not allocate unbounded memory, create unlimited jobs, or freeze the app.

### 10.3. Queue Capacity

Initial conceptual limits:

```rust
pub struct QueueCapacity {
    pub scan_queue_max: usize,
    pub extract_queue_max: usize,
    pub chunk_queue_max: usize,
    pub keyword_queue_max: usize,
    pub embedding_queue_max: usize,
}
```

Suggested defaults:

```text
scan_queue_max      = 10_000
extract_queue_max   = 1_000
chunk_queue_max     = 1_000
keyword_queue_max   = 1_000
embedding_queue_max = 2_000
```

These values are starting points, not final tuning.

---

## 11. Job State Model

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    WaitingForDependency,
}
```

Jobs should include:

```rust
pub struct IndexJob {
    pub id: JobId,
    pub file_id: Option<FileId>,
    pub source_id: SourceId,
    pub kind: JobKind,
    pub priority: WorkPriority,
    pub state: JobState,
    pub attempt_count: u32,
    pub last_error_kind: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Job kinds:

```rust
pub enum JobKind {
    ScanSource,
    ExtractFile,
    ChunkFile,
    UpdateKeywordIndex,
    GenerateEmbedding,
    Cleanup,
    Repair,
}
```

---

## 12. Pause, Resume, Cancel

### 12.1. User-Facing Pause

Default copy:

```text
Pause preparing
Resume preparing
```

Do not say “pause indexing workers.”

### 12.2. Safe Pause

Pause means:

```text
finish the current small unit
stop taking new work
persist progress
```

### 12.3. Cancel Source Preparation

If user removes a folder or cancels preparation:

```text
stop new jobs for that folder
let safe current jobs finish or abort if supported
mark queued jobs cancelled
do not delete user files
```

### 12.4. App Close

On app close:

```text
persist in-progress job states
finish or stop safely
recover on next startup
```

---

## 13. Resource Awareness

### 13.1. User Activity

When user is actively typing, searching, or browsing results:

```text
reduce background work
```

Signal examples:

- search input changed recently;
- search submitted;
- result list is updating;
- preview loading;
- window is focused.

### 13.2. Battery and Thermal Policy

P0 may not implement battery/thermal detection, but scheduler should allow future policy:

```text
on battery → reduce background work
low battery → pause heavy work
thermal warning → pause embedding
```

### 13.3. Memory Pressure

If model loading or embedding causes memory pressure:

```text
pause embedding
continue keyword search
show friendly notice only if user action is affected
```

User copy:

```text
orbok paused better search preparation to keep this computer responsive.
```

---

## 14. UI Requirements

### 14.1. Preparing Status

Default user copy:

```text
Preparing “Documents” for search
124 files ready. You can search now.
```

Do not say:

```text
Indexing queue depth: 412
```

### 14.2. Partial Readiness

Search must work for prepared files while background work continues.

Copy:

```text
Some files are still being prepared.
Results will improve as preparation finishes.
```

### 14.3. Pause/Resume

Show:

```text
[Pause preparing]
```

After pause:

```text
Preparation paused.
You can search files already prepared.
[Resume preparing]
```

### 14.4. Failure Summary

If some files fail:

```text
Some files could not be prepared.
Other files are still searchable.
```

Action:

```text
[Review files]
```

Advanced view may show counts and error categories.

---

## 15. Events

```rust
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    JobQueued(JobId),
    JobStarted(JobId),
    JobPaused(JobId),
    JobResumed(JobId),
    JobCompleted(JobId),
    JobFailed(JobId),
    JobCancelled(JobId),
    QueueBackpressureApplied(QueueKind),
    QueueBackpressureReleased(QueueKind),
    UserActivityDetected,
    ResourceModeChanged(ResourceMode),
}
```

Resource mode:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceMode {
    Normal,
    UserActive,
    LowImpact,
    Paused,
}
```

---

## 16. Data Persistence and Recovery

Persist enough state to recover:

- source jobs;
- file jobs;
- job state;
- attempt count;
- dependency state;
- last failure kind;
- timestamps.

Do not persist transient thread/task handles.

On startup:

```text
Running → Pending or FailedRecoverable
Paused → Paused
Pending → Pending
Completed → Completed
```

User copy during recovery:

```text
orbok is checking unfinished work from last time.
```

---

## 17. Testing

### 17.1. Unit Tests

- queue priority ordering;
- queue capacity enforcement;
- backpressure applied and released;
- pause stops new jobs;
- resume restarts pending jobs;
- cancel source cancels queued jobs;
- failed job retry limit.

### 17.2. Integration Tests

- add folder with many files;
- search while indexing continues;
- remove folder during indexing;
- crash/restart recovery;
- embedding yields to active search;
- failed extraction does not block other files.

### 17.3. Performance Tests

- UI remains responsive during indexing;
- search latency does not regress under background load;
- memory stays bounded for large source trees;
- weak-machine profile remains usable.

---

## 18. Acceptance Criteria

This RFC is accepted when:

1. Indexing work uses bounded queues.
2. Background work cannot grow unbounded.
3. Search and UI interactions have priority.
4. Embedding work yields to active search.
5. Preparation can pause and resume.
6. Removing a folder cancels queued work safely.
7. Crash/restart recovery does not corrupt job state.
8. Partial readiness is visible and searchable.
9. User-facing copy avoids technical scheduling terms.
10. Tests cover queue limits, pause/resume, cancellation, and recovery.

---

## 19. Final Decision

Implement a conservative, resource-aware scheduler:

```text
bounded queues
low default concurrency
search-first priority
safe pause/resume
crash recovery
clear non-technical progress
```

This is essential for orbok’s poor-machine performance promise.

---

## 20. Amendment (2026-08-12): two gaps found on first execution

This RFC was marked Implemented at v0.17.0. The scheduler was built and unit
tested; **the application never ran it** until RFC-056 wired it in
(`7599ee9`), so until then no code path exercised `Scheduler::fail` against a
real failure. Both findings below came from that first integration, were
reported rather than absorbed by the implementer, and are recorded here because
neither is a design change this RFC should have to rediscover.

### 20.1 `fail()` has no concept of a terminal failure category

`dispatch.rs`'s `fail(job, error_kind, catalog)` takes an arbitrary
`error_kind: &str`, increments `attempt_count`, and retries until
`MAX_JOB_ATTEMPTS` (3). Two consequences:

1. **Categories that cannot succeed on retry are retried anyway.** RFC-008 §15
   names `model_missing`, and Review 160 §5 established it is terminal: nothing
   about the job changes until a model appears, and RFC-008 §18 step 3 already
   re-queues embedding work when the active model changes — which an
   installation is. Two attempts are wasted for every such job.
2. **The permanent-failure branch never writes `error_category`.** It calls
   `set_status(&job.id, JobStatus::Failed)`; `error_kind` reaches only a
   `tracing::warn!` and a `SchedulerEvent::JobFailed`. The
   `error_category` column — which Task 013 (`d198713`) began writing via
   `fail_with_category`, and which RFC-008 §15's diagnostics contract depends on
   — is left null.

RFC-056 Slice 1 worked around this by calling
`IndexJobRepository::fail_with_category` directly for `GenerateEmbedding` jobs,
bypassing `Scheduler::fail` for that one category. That replicates
`run_pending`'s existing behaviour rather than inventing policy, and is correct
as a stopgap.

**It stops being sufficient at RFC-056 Slice 2.** Embedding introduces
`out_of_memory` and `inference_error` (RFC-008 §15), which *should* retry. An
all-or-nothing bypass cannot distinguish them from `model_missing`. The
resolution — a retryable/terminal distinction on the category, with the category
persisted on permanent failure — belongs in this RFC's model, not in a caller's
special case.

### 20.2 A retried job can be silently dropped under backpressure

Also in `fail()`'s retry branch:

```rust
job.state = JobState::Pending;
let id = job.id.clone();
let queue = self.queues.queue_for(job.kind);
if !queue.is_full() {
    queue.push(job);
}
let jobs = IndexJobRepository::new(catalog);
jobs.set_status(&id, JobStatus::Queued)?;
```

If the queue is full the push is skipped, but `set_status(Queued)` runs
regardless. The catalog then says `queued` while no in-memory copy exists. A
rehydration pass that tracks already-seen ids will not reload it, because the id
was seen before the retry — so the job is durably recorded as queued and never
runs again.

Unlikely to fire at current capacities (`limits.rs`), and pre-existing rather
than introduced by RFC-056. Recorded because "a job vanished" is expensive to
diagnose from scratch, and because §10's backpressure model is where the answer
belongs: either the push must not be skipped, or the skip must be reflected in
the persisted state.

### Status

Neither finding changes this RFC's design. Both are gaps between what §10 and
§11 describe and what `dispatch.rs` does, surfaced the first time the scheduler
ran against real work. §20.1 needs resolving as part of RFC-056 Slice 2; §20.2
is recorded for whoever next touches the backpressure path.

**Update (2026-08-12): both resolved.** §20.1 landed in RFC-056 Slice 2:
`is_terminal_category` gives `fail()` the terminal/retryable distinction RFC-008
§15 requires, and the permanent-failure branch now calls
`IndexJobRepository::fail_with_category` instead of a bare `set_status`.

§20.2 landed in RFC-056 Slice 3, which turned out to be "whoever next touches
the backpressure path" — pause/resume, wired the same slice, shares the queue-
state machinery. `fail()`'s retry branch now records `JobStatus::Blocked`
(rather than `Queued`) when the in-memory push is skipped for lack of room, and
`scheduler_host::rehydrate` re-discovers `blocked` rows separately from
`queued` ones, bypassing the `known`-id gate that only makes sense for a row
that might still be correctly tracked in memory. A job dropped under
backpressure is recovered once room exists, rather than durably lost.
