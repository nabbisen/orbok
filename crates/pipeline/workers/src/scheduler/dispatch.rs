//! Resource-aware indexing scheduler (RFC-036 §5–§16).
//!
//! The `Scheduler` is the single entry point for all background
//! indexing work. It:
//!
//! - routes jobs into typed bounded queues;
//! - enforces backpressure when queues are full;
//! - tracks resource mode (`Normal` / `UserActive` / `Paused`);
//! - dispatches one job per `tick()` call, skipping embedding in
//!   `UserActive` mode (RFC-036 §9.2);
//! - emits `SchedulerEvent`s the app layer uses to update the UI;
//! - persists job state to the catalog for crash recovery (RFC-036 §16).
//!
//! Execution model: synchronous, single-threaded. This matches the
//! existing `run_pending` model and the snora/iced GUI thread contract.
//! Async dispatch can be layered on top in a future RFC.

use super::job::{IndexJob, JobState, ResourceMode, SchedulerEvent};
use super::limits::{MAX_JOB_ATTEMPTS, SchedulerConfig};
use super::queue::QueueSet;
use orbok_core::{JobId, JobStatus, OrbokResult, SourceId, now_iso8601};
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;

/// RFC-008 §15 categories that must not retry (RFC-036 §20.1): nothing
/// about the same job changes on a subsequent attempt (a missing/invalid
/// model, a blocked backend, input that will always be too long), or a
/// retry would be actively wrong (an intentional cancellation). Every
/// other category -- including the two RFC-008 §15 categories a later
/// attempt genuinely could clear (`out_of_memory`, `inference_error`),
/// `write_error`, and non-embedding kinds like `"worker_error"` -- retries
/// by default up to `MAX_JOB_ATTEMPTS`.
fn is_terminal_category(error_kind: &str) -> bool {
    matches!(
        error_kind,
        "model_missing" | "model_invalid" | "backend_unavailable" | "input_too_long" | "canceled"
    )
}

/// The resource-aware scheduler (RFC-036 §5–§16).
pub struct Scheduler {
    #[allow(dead_code)] // reserved for future per-queue tuning
    config: SchedulerConfig,
    queues: QueueSet,
    resource_mode: ResourceMode,
    /// Events accumulated since the last `drain_events` call.
    events: Vec<SchedulerEvent>,
    /// Number of jobs that completed successfully in this session.
    completed_count: u64,
    /// Number of jobs that failed in this session.
    failed_count: u64,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            queues: QueueSet::new(&config.capacity),
            config,
            resource_mode: ResourceMode::default(),
            events: Vec::new(),
            completed_count: 0,
            failed_count: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(SchedulerConfig::default())
    }

    // ── Resource mode ───────────────────────────────────────────────────

    /// Notify the scheduler that the user is actively searching or
    /// typing. Background embedding will yield (RFC-036 §13.1).
    pub fn notify_user_active(&mut self) {
        if self.resource_mode != ResourceMode::Paused {
            let changed = self.resource_mode != ResourceMode::UserActive;
            self.resource_mode = ResourceMode::UserActive;
            if changed {
                self.emit(SchedulerEvent::UserActivityDetected);
                self.emit(SchedulerEvent::ResourceModeChanged(
                    ResourceMode::UserActive,
                ));
            }
        }
    }

    /// Notify the scheduler that user activity has subsided.
    pub fn notify_user_idle(&mut self) {
        if self.resource_mode == ResourceMode::UserActive {
            self.resource_mode = ResourceMode::Normal;
            self.emit(SchedulerEvent::ResourceModeChanged(ResourceMode::Normal));
        }
    }

    pub fn resource_mode(&self) -> ResourceMode {
        self.resource_mode
    }

    // ── Pause / Resume / Cancel ─────────────────────────────────────────

    /// Pause all background work (RFC-036 §12.1–§12.2).
    /// In-flight jobs finish their current small unit; no new jobs start.
    pub fn pause(&mut self, catalog: &Catalog) -> OrbokResult<()> {
        if self.resource_mode == ResourceMode::Paused {
            return Ok(());
        }
        self.resource_mode = ResourceMode::Paused;
        self.emit(SchedulerEvent::ResourceModeChanged(ResourceMode::Paused));
        // Persist paused state for recovery (RFC-036 §16).
        let conn = catalog.lock();
        conn.execute(
            "UPDATE index_jobs SET status = 'paused', updated_at = ?1 WHERE status = 'queued'",
            rusqlite::params![now_iso8601()],
        )
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        Ok(())
    }

    /// Resume background work after a pause (RFC-036 §12.2).
    pub fn resume(&mut self, catalog: &Catalog) -> OrbokResult<()> {
        if self.resource_mode != ResourceMode::Paused {
            return Ok(());
        }
        self.resource_mode = ResourceMode::Normal;
        self.emit(SchedulerEvent::ResourceModeChanged(ResourceMode::Normal));
        // Restore paused jobs to queued so they are picked up again.
        let conn = catalog.lock();
        conn.execute(
            "UPDATE index_jobs SET status = 'queued', updated_at = ?1 WHERE status = 'paused'",
            rusqlite::params![now_iso8601()],
        )
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        Ok(())
    }

    /// Cancel all queued work for a source (RFC-036 §12.3).
    /// Called when the user removes a folder.
    pub fn cancel_source(&mut self, source_id: &SourceId, catalog: &Catalog) -> OrbokResult<usize> {
        let cancelled = self.queues.cancel_source(source_id);
        // Persist cancellation to catalog.
        let conn = catalog.lock();
        conn.execute(
            "UPDATE index_jobs SET status = 'canceled', updated_at = ?1 \
             WHERE source_id = ?2 AND status IN ('queued','paused')",
            rusqlite::params![now_iso8601(), source_id.as_str()],
        )
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        Ok(cancelled)
    }

    // ── Enqueue ─────────────────────────────────────────────────────────

    /// Submit a job to the appropriate bounded queue (RFC-036 §7).
    ///
    /// Returns `Err` with a `BackpressureActive` variant if the target
    /// queue is full — the caller must wait and retry rather than
    /// allocating unbounded memory (RFC-036 §10.2).
    pub fn enqueue(&mut self, job: IndexJob, catalog: &Catalog) -> OrbokResult<()> {
        let queue = self.queues.queue_for(job.kind);
        if queue.is_full() {
            let kind = queue.kind();
            if !queue.backpressure_active {
                queue.backpressure_active = true;
                self.events
                    .push(SchedulerEvent::QueueBackpressureApplied(kind));
            }
            return Err(orbok_core::OrbokError::BackpressureActive {
                queue: format!("{kind:?}"),
            });
        }
        // If backpressure was active and the queue now has room, release it.
        if queue.backpressure_active && !queue.is_full() {
            queue.backpressure_active = false;
            let kind = queue.kind();
            self.events
                .push(SchedulerEvent::QueueBackpressureReleased(kind));
        }
        // Persist job to catalog for crash recovery, under this same job's
        // own id (RFC-036 §16) -- `complete`/`fail` later update the
        // catalog row by this id, so the two must never diverge.
        let jobs = IndexJobRepository::new(catalog);
        jobs.enqueue_with_priority(
            &job.id,
            job.kind.as_job_type(),
            Some(&job.source_id),
            job.file_id.as_ref(),
            job.priority.as_i64(),
        )?;
        let id = job.id.clone();
        self.queues.queue_for(job.kind).push(job);
        self.emit(SchedulerEvent::JobQueued(id));
        Ok(())
    }

    // ── Rehydration (RFC-056 §4.5) ──────────────────────────────────────

    /// Load a job that is already persisted in the catalog — e.g. one
    /// enqueued directly via `IndexJobRepository::enqueue` by the scanner
    /// or a worker (bypassing `Scheduler::enqueue`), or one recovered from
    /// a previous session — into the in-memory queue without re-inserting
    /// it into the catalog.
    ///
    /// The catalog, not `QueueSet`, is the durable source of truth: a
    /// freshly constructed `Scheduler` starts with empty queues and must be
    /// told about every `queued` row before `tick()` can dispatch it. This
    /// is that telling. Silently drops the job if its queue is full —
    /// callers periodically re-rehydrate, so a job dropped this pass is
    /// picked up on the next once room exists, and the catalog row is
    /// untouched either way.
    ///
    /// Returns whether the job was actually pushed (RFC-036 §20.2):
    /// `scheduler_host::rehydrate` needs this to know whether a `blocked`
    /// row's catalog status may be corrected back to `queued` — it must
    /// not, if this call also dropped it for lack of room.
    pub fn load_persisted(&mut self, job: IndexJob) -> bool {
        let queue = self.queues.queue_for(job.kind);
        if queue.is_full() {
            return false;
        }
        queue.push(job);
        true
    }

    // ── Dispatch ─────────────────────────────────────────────────────────

    /// Dispatch one job from the queues (RFC-036 §8, §9).
    ///
    /// Returns the job to run, or `None` when paused or all queues are
    /// empty. The caller executes the job and reports back via
    /// `complete` or `fail`.
    pub fn tick(&mut self) -> Option<IndexJob> {
        if self.resource_mode == ResourceMode::Paused {
            return None;
        }
        let job = self.queues.pop_next(self.resource_mode)?;

        // Release backpressure on the queue that just yielded a slot.
        let queue = self.queues.queue_for(job.kind);
        if queue.backpressure_active && !queue.is_full() {
            queue.backpressure_active = false;
            let kind = queue.kind();
            self.events
                .push(SchedulerEvent::QueueBackpressureReleased(kind));
        }

        self.emit(SchedulerEvent::JobStarted(job.id.clone()));
        Some(job)
    }

    /// Mark a job as successfully completed (RFC-036 §11).
    pub fn complete(&mut self, job_id: &JobId, catalog: &Catalog) -> OrbokResult<()> {
        let jobs = IndexJobRepository::new(catalog);
        jobs.set_status(job_id, JobStatus::Succeeded)?;
        self.completed_count += 1;
        self.emit(SchedulerEvent::JobCompleted(job_id.clone()));
        self.emit_readiness(catalog);
        Ok(())
    }

    /// Mark a job as failed; retry if under the attempt limit and the
    /// category itself is retryable (RFC-036 §11, §20.1).
    ///
    /// `error_message` is recorded alongside a terminal failure's category
    /// (the diagnostics-contract `error_category`/`error_message` columns,
    /// RFC-002) -- it is discarded on a retry, where only `error_kind`
    /// (`last_error_kind`) is tracked, matching `increment_attempt`'s
    /// existing shape.
    pub fn fail(
        &mut self,
        mut job: IndexJob,
        error_kind: &str,
        error_message: Option<&str>,
        catalog: &Catalog,
    ) -> OrbokResult<()> {
        job.attempt_count += 1;
        job.last_error_kind = Some(error_kind.to_string());
        if job.attempt_count < MAX_JOB_ATTEMPTS && !is_terminal_category(error_kind) {
            // Re-queue for retry (RFC-036 §17.1 retry limit test).
            tracing::debug!(
                job = job.id.as_str(),
                attempt = job.attempt_count,
                error = error_kind,
                "job failed — will retry"
            );
            job.state = JobState::Pending;
            let id = job.id.clone();
            let queue = self.queues.queue_for(job.kind);
            let pushed = !queue.is_full();
            if pushed {
                queue.push(job);
            }
            let jobs = IndexJobRepository::new(catalog);
            // RFC-036 §20.2: `queued` must not be written when the push was
            // skipped for lack of room — that claims an in-memory copy
            // exists when none does, and a rehydration pass that has
            // already seen this id (every retried job has, by definition)
            // will never reload it. `blocked` records the true state;
            // `scheduler_host::rehydrate` re-discovers `blocked` rows
            // separately, bypassing the `known` gate that only makes sense
            // for a row that might still be correctly tracked in memory.
            jobs.set_status(
                &id,
                if pushed {
                    JobStatus::Queued
                } else {
                    JobStatus::Blocked
                },
            )?;
            jobs.increment_attempt(&id, error_kind)?;
        } else {
            tracing::warn!(
                job = job.id.as_str(),
                attempts = job.attempt_count,
                error = error_kind,
                terminal_category = is_terminal_category(error_kind),
                "job permanently failed"
            );
            let jobs = IndexJobRepository::new(catalog);
            jobs.fail_with_category(&job.id, error_kind, error_message)?;
            self.failed_count += 1;
            self.emit(SchedulerEvent::JobFailed {
                id: job.id.clone(),
                error_kind: error_kind.to_string(),
            });
        }
        Ok(())
    }

    // ── Progress ─────────────────────────────────────────────────────────

    pub fn pending_count(&self) -> usize {
        self.queues.total_pending()
    }

    pub fn completed_count(&self) -> u64 {
        self.completed_count
    }

    pub fn failed_count(&self) -> u64 {
        self.failed_count
    }

    pub fn is_idle(&self) -> bool {
        self.queues.total_pending() == 0
    }

    // ── Events ────────────────────────────────────────────────────────────

    /// Take all accumulated events since the last call.
    /// The UI layer calls this on each frame to update progress copy.
    pub fn drain_events(&mut self) -> Vec<SchedulerEvent> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self, event: SchedulerEvent) {
        self.events.push(event);
    }

    fn emit_readiness(&mut self, catalog: &Catalog) {
        // Count indexed files for partial readiness notice (RFC-036 §14.2).
        let conn = catalog.lock();
        let ready: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE file_status = 'indexed'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let pending = self.pending_count() as u64;
        self.events.push(SchedulerEvent::PartialReadinessChanged {
            ready_count: ready as u64,
            pending_count: pending,
        });
    }
}
