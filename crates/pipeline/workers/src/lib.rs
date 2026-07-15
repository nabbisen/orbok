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
mod chunk_and_index;
pub mod cleanup_service;
mod embedding;
mod extract;
pub mod model_delivery;
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
    let mut processed = 0u32;

    while processed < limit {
        let batch = jobs.list_queued(1)?;
        if batch.is_empty() {
            break;
        }
        let job = &batch[0];
        jobs.set_status(&job.job_id, JobStatus::Running)?;
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
            JobType::Embedding => {
                if let (Some(file_id), Some(worker)) = (&job.file_id, embed_worker) {
                    worker.run(file_id)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()), // Other job types are no-ops in v0.2.
        };
        match result {
            Ok(()) => {
                jobs.set_status(&job.job_id, JobStatus::Succeeded)?;
                succeeded += 1;
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
