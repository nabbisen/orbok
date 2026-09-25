//! # orbok-workers
//!
//! Synchronous pipeline workers for M5/M6: pull queued jobs from the
//! catalog and execute them in dependency order.
//!
//! **Worker chain (per file):**
//! ```text
//! [Scan queues Extract]
//!   → ExtractionWorker  (extract + cache + record)
//!   → ChunkAndIndexWorker (chunk + FTS index + chunk_locations)
//! ```
//!
//! Failure isolation: one file's failure never stops the whole run
//! (RFC-004 §16, RFC-005 §13). Workers update the relevant catalog
//! records with the error category.
//!
//! RFC-036 adds the resource-aware `Scheduler` with bounded queues,
//! priority dispatch, backpressure, pause/resume/cancel, and crash
//! recovery.

mod chunk_adapter;
pub(crate) mod chunk_and_index;
pub mod cleanup_service;
mod embedding;
mod extract;
pub mod model_delivery;
pub(crate) mod model_durability;
pub mod model_lifecycle;
pub mod model_verifier;
pub mod recovery;
pub mod scheduler;
pub mod storage;

#[cfg(test)]
mod tests;

pub use chunk_and_index::ChunkAndIndexWorker;
pub use cleanup_service::{CleanupService, FullCleanupOutcome};
pub use embedding::EmbeddingWorker;
pub use extract::ExtractionWorker;
pub use model_delivery::{
    ModelDeliveryError, ModelDeliveryEvent, ModelDeliveryOutcome, install_default_model,
};
pub use model_lifecycle::{
    ManagedModelCleanupOutcome, ManagedModelStartupOutcome, ModelLifecycleError,
    cleanup_managed_model_generations, run_managed_model_startup,
};
pub use model_verifier::{
    FileIssue, FileIssueKind, VerifyOutcome, verify_embedding_model, verify_outcome_summary,
};
pub use recovery::{
    IntegrityReport, RecoveryReport, check_catalog_integrity, run_startup_recovery,
};
pub use scheduler::{
    IndexJob, JobKind, JobState, QueueCapacity, QueueKind, ResourceMode, Scheduler,
    SchedulerConfig, SchedulerEvent, SchedulerLimits, WorkPriority,
};
pub use storage::update_storage_accounting;

use orbok_core::OrbokResult;
use orbok_core::{JobStatus, JobType};
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;
use tracing::warn;

/// Task 056 (Review 232's ruling): what a chunk or embedding job that found
/// no extracted text for `file_id` should fail as, after acting on it.
///
/// - The first miss for a file re-queues its extraction, which rebuilds the
///   cache and chains chunking and embedding again. The job fails as
///   `extraction_cache_missing`; the chain creates its replacement.
/// - A second miss for a file still in `requeued` -- no successful read of
///   its text since the re-extraction -- means the cache did not keep what
///   extraction just wrote. It fails as `extraction_cache_unavailable` and
///   queues nothing, so the loop is bounded at one extra extraction per chain.
///
/// Both categories are terminal. The caller removes a file from `requeued`
/// when a chunk or embedding job for it succeeds, which proves a read, so a
/// later legitimate miss (a second "Clear extracted text", an idle trim) is
/// repaired again.
pub fn extraction_cache_miss_category(
    catalog: &Catalog,
    requeued: &mut std::collections::HashSet<orbok_core::FileId>,
    file_id: &orbok_core::FileId,
    current_job: &orbok_core::JobId,
) -> &'static str {
    if requeued.contains(file_id) {
        warn!(
            file = file_id.as_str(),
            "extracted text is missing again right after it was rebuilt; the extraction cache is not keeping it"
        );
        return "extraction_cache_unavailable";
    }
    requeued.insert(file_id.clone());
    if let Err(error) =
        IndexJobRepository::new(catalog).enqueue_extraction_unless_pending(file_id, current_job)
    {
        warn!(file = file_id.as_str(), %error, "could not queue extraction to rebuild missing extracted text");
    }
    "extraction_cache_missing"
}

/// Run all queued jobs until the queue is empty or `limit` jobs have
/// been processed. Returns the number of jobs that succeeded.
///
/// This is the legacy synchronous dispatch loop, retained for tests
/// and simple callers. Production code should use `Scheduler::tick()`
/// for resource-aware dispatch (RFC-036).
pub fn run_pending(
    catalog: &Catalog,
    extract_worker: &ExtractionWorker<'_>,
    chunk_worker: &ChunkAndIndexWorker<'_>,
    embed_worker: Option<&EmbeddingWorker<'_>>,
    limit: u32,
) -> OrbokResult<u64> {
    let jobs = IndexJobRepository::new(catalog);
    let mut succeeded = 0u64;
    // Task 056: the same once-until-a-successful-read rule the scheduler
    // host applies, held for the duration of this call.
    let mut requeued_for_cache_miss = std::collections::HashSet::new();
    let mut processed = 0u32;

    while processed < limit {
        let batch = jobs.list_queued(1)?;
        if batch.is_empty() {
            break;
        }
        let job = &batch[0];
        jobs.set_status(&job.job_id, JobStatus::Running)?;

        // RFC-008 §15: a job that did no work is not Succeeded. Handled
        // before the generic result match below (not via a generic
        // `Err`) because this is a named, terminal failure category, not
        // a runtime error from attempting the work -- there is no
        // attempt to report on. Terminal, not retried here: nothing about
        // this job changes until a model becomes available, and §18 step
        // 3 already re-queues embedding work when the active model
        // changes, which an installation is (Review 160 §5).
        if job.job_type == JobType::Embedding && embed_worker.is_none() {
            jobs.fail_with_category(&job.job_id, "model_missing", None)?;
            processed += 1;
            continue;
        }

        let result = match job.job_type {
            JobType::Extract => {
                if let Some(file_id) = &job.file_id {
                    extract_worker.run(file_id)
                } else {
                    Ok(())
                }
            }
            JobType::Chunk | JobType::KeywordIndex => {
                if let Some(file_id) = &job.file_id {
                    chunk_worker.run(file_id)
                } else {
                    Ok(())
                }
            }
            JobType::Embedding => match (&job.file_id, embed_worker) {
                (Some(file_id), Some(worker)) => worker.run(file_id),
                // embed_worker is Some here (the check above already
                // handled None); a missing file_id would be a malformed
                // job, which no current enqueue path produces.
                _ => Ok(()),
            },
            _ => Ok(()), // Other job types are no-ops in v0.2.
        };
        match result {
            Ok(()) => {
                if matches!(
                    job.job_type,
                    JobType::Chunk | JobType::KeywordIndex | JobType::Embedding
                ) && let Some(file_id) = &job.file_id
                {
                    requeued_for_cache_miss.remove(file_id);
                }
                jobs.set_status(&job.job_id, JobStatus::Succeeded)?;
                succeeded += 1;
            }
            Err(orbok_core::OrbokError::ExtractionCacheMissing) if job.file_id.is_some() => {
                let file_id = job.file_id.as_ref().expect("checked by the guard");
                let category = extraction_cache_miss_category(
                    catalog,
                    &mut requeued_for_cache_miss,
                    file_id,
                    &job.job_id,
                );
                jobs.fail_with_category(&job.job_id, category, None)?;
            }
            Err(e) => {
                warn!(job = job.job_id.as_str(), error = %e, "job failed");
                jobs.set_status(&job.job_id, JobStatus::Failed)?;
            }
        }
        processed += 1;
    }
    Ok(succeeded)
}
