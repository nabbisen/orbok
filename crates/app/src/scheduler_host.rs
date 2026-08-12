//! RFC-056: hosts RFC-036's `Scheduler` in a long-lived background task.
//!
//! Mirrors `download.rs`'s split: a thin [`run`] resolves the active
//! profile's sealed handles and hands off to [`run_with_context`], the core
//! loop tests exercise directly -- the same function `main.rs`'s
//! subscription wires in, per RFC-056's Handoff §4 ("every test exercises
//! the shipped application's path, not a worker invoked directly").
//!
//! **Slice 2** (RFC-056 Handoff §2, RFC-036 §20.1): `GenerateEmbedding`
//! jobs dispatch through a real `EmbeddingWorker`, resolved once at
//! startup via [`crate::bootstrap::embedding_resolution`] (RFC-050 lease
//! held for the loop's whole lifetime). No model configured, or the model
//! fails to load, is not distinguished from any other unavailable-worker
//! case here -- both surface as RFC-008 §15's `model_missing`, matching
//! Slice 1's own terminal-category choice, but now routed through
//! `Scheduler::fail` like every other job kind rather than bypassing it.
//!
//! **Scan routing** (Review 162 §2): discovery is RFC-036 §6.1's own first
//! work category, with its own queue and priority (`JobKind::ScanSource`)
//! that nothing produced before this. `scan_and_index_source` now enqueues
//! a `Scan` job instead of calling `Scanner::scan` inline, so the scan/hash
//! pass -- which scales with source size, not a fixed cost Slice 1's
//! measurement mistook it for -- runs here, off the caller's thread, same
//! as everything else this module hosts.

use crate::bootstrap::embedding_resolution::EmbeddingWorkerParts;
use futures::SinkExt as _;
use futures::channel::mpsc::Sender;
use orbok::runtime_context::AllowRuntimePathProbe;
use orbok::runtime_storage::ProfileCache;
use orbok_core::{JobId, JobType, OrbokError, OrbokResult, SourceId};
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;
use orbok_fs::{ScanRequest, Scanner};
use orbok_ui::state::Message;
use orbok_workers::{
    ChunkAndIndexWorker, EmbeddingWorker, ExtractionWorker, IndexJob, JobKind, JobState, Scheduler,
};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// How long the loop sleeps after finding nothing to do before checking the
/// catalog again for newly-enqueued work. `scan_and_index_source` writes
/// new `index_jobs` rows directly, on the UI's own `Catalog` connection --
/// it does not wake this task, so idle periods are bridged by polling
/// rather than a signal.
const IDLE_POLL: Duration = Duration::from_millis(300);

/// Resolve the active profile's sealed handles and run the loop forever.
/// Wired into `main.rs`'s `Subscription::run_with`; returns only if the
/// profile's catalog or cache cannot be opened at all -- a running profile
/// never sees this return.
pub async fn run(portable: bool, output: Sender<Message>) {
    let Ok(runtime) = crate::bootstrap::resolve_runtime_context(portable) else {
        return;
    };
    let Ok(catalog) = crate::bootstrap::open_catalog(&runtime) else {
        return;
    };
    let Ok(cache) = crate::bootstrap::cache_service(&runtime) else {
        return;
    };
    // Best-effort: a settings load failure or an unresolved/unloadable
    // model falls back to `None` here, which the loop below treats as
    // RFC-008 §15 `model_missing` for any `GenerateEmbedding` job it
    // dispatches -- the same fail-closed shape `bootstrap::search`'s
    // hybrid-search fallback already uses for the same resolution.
    let embedding_parts = crate::bootstrap::load_runtime_settings(&runtime)
        .ok()
        .and_then(|settings| {
            crate::bootstrap::embedding_resolution::resolve_embedding_worker_parts(
                &runtime,
                &AllowRuntimePathProbe,
                &catalog,
                &settings,
            )
        });
    run_with_context(catalog, cache, embedding_parts, output).await
}

/// The hosting loop's core logic (RFC-056 §4.1): pulls jobs from
/// `Scheduler::tick()`, executes them through the existing extract/chunk
/// workers, and reports progress. Owns its `Catalog`/`ProfileCache` --
/// both are `Send + 'static` sealed handles (§4.2) -- and never returns.
pub(crate) async fn run_with_context(
    catalog: Catalog,
    cache: ProfileCache,
    embedding_parts: Option<EmbeddingWorkerParts>,
    mut output: Sender<Message>,
) {
    let mut scheduler = Scheduler::with_defaults();
    let mut known: HashSet<JobId> = HashSet::new();
    let cache_service = cache.service();
    let extract = ExtractionWorker::new(&catalog, cache_service);
    let chunk = ChunkAndIndexWorker::new(&catalog, cache_service);
    // `embedding_parts`' RFC-050 lease guard (if any) is held inside this
    // `EmbeddingWorker` for the loop's entire lifetime -- the loop never
    // returns, so nothing needs to `drop` it explicitly (the same pattern
    // `bootstrap/tests/embedding_blocking_measurement.rs` established for
    // a shorter-lived scan/index pass).
    let embed = embedding_parts.map(|parts| {
        EmbeddingWorker::with_model(&catalog, cache_service, parts.model, parts.model_id)
    });

    loop {
        // `rehydrate` is a full `queued`-rows scan (RFC-036's queues have
        // no cheap "anything new?" signal) -- calling it every iteration
        // turns this into an O(n^2) crawl over a few hundred files (traced
        // during Slice 1 development: fine at 10 files, never finished 300
        // files). Only rehydrate when the in-memory queue has genuinely run
        // dry, so its cost is paid a handful of times per run (once per
        // queue-kind transition), not once per job.
        let job = match scheduler.tick() {
            Some(job) => job,
            None => {
                rehydrate(&mut scheduler, &catalog, &mut known);
                match scheduler.tick() {
                    Some(job) => job,
                    None => {
                        tokio::time::sleep(IDLE_POLL).await;
                        continue;
                    }
                }
            }
        };

        let file_id = job.file_id.clone();
        let result = match (job.kind, &file_id) {
            (JobKind::ScanSource, _) => run_scan(&catalog, job.source_id.clone()),
            (JobKind::ExtractFile, Some(file_id)) => extract.run(file_id),
            (JobKind::ChunkFile, Some(file_id)) | (JobKind::UpdateKeywordIndex, Some(file_id)) => {
                chunk.run(file_id)
            }
            (JobKind::GenerateEmbedding, Some(file_id)) => match &embed {
                Some(embed) => embed.run(file_id),
                // No model resolved at startup (RFC-008 §15) -- terminal,
                // via the same `OrbokError::Embedding` categorization path
                // every other embedding failure now goes through.
                None => Err(OrbokError::Embedding {
                    category: "model_missing",
                    message: "no embedding model configured".to_string(),
                }),
            },
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                let _ = scheduler.complete(&job.id, &catalog);
                known.remove(&job.id);
            }
            Err(error) => {
                tracing::warn!(job = job.id.as_str(), error = %error, "indexing job failed");
                // Not removed from `known`: `Scheduler::fail` either
                // re-queues the job in-memory (retry -- still correctly
                // tracked there) or permanently fails it in the catalog,
                // which then never reappears in `list_queued`. Removing it
                // here would risk `rehydrate` loading a second in-memory
                // copy of a job `fail`'s own retry path already re-queued.
                let error_kind = error_kind_for(&error);
                let _ = scheduler.fail(job, error_kind, Some(&error.to_string()), &catalog);
            }
        }
        report_health(&catalog, &mut output).await;
    }
}

/// Discover files under `source_id`, hash them, and enqueue the resulting
/// `Extract`/`Chunk`/`Embedding` jobs (RFC-004 change detection makes
/// re-scanning an unchanged source a cheap no-op, verified by Task 013's
/// tests and carried through unchanged here). A fresh, unshared cancel
/// token: cancelling a scan mid-flight is RFC-036 §12.3/§18.6, Slice 3's
/// scope, not wired here.
fn run_scan(catalog: &Catalog, source_id: SourceId) -> OrbokResult<()> {
    Scanner::new(catalog)
        .scan(
            &ScanRequest {
                source_id,
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &AtomicBool::new(false),
        )
        .map(|_summary| ())
}

async fn report_health(catalog: &Catalog, output: &mut Sender<Message>) {
    let health = crate::bootstrap::get_health(catalog);
    // A dropped UI receiver (window closed) must not corrupt job state or
    // stop indexing -- it only silences progress reporting, matching
    // `download.rs`'s `ui_open` idiom.
    let _ = output.send(Message::HealthUpdated(health)).await;
}

/// Load every catalog `queued` row the in-memory scheduler doesn't already
/// know about. Catches jobs written directly by `IndexJobRepository::enqueue`
/// (the scanner, extraction, chunking -- all bypass `Scheduler::enqueue`),
/// and jobs recovered from a previous session: RFC-018's
/// `reset_interrupted_jobs` already runs at startup and puts any
/// interrupted `running` job back to `queued` before this loop ever starts,
/// so rehydration is the only piece still needed to complete RFC-036 §16
/// for the in-memory queue.
fn rehydrate(scheduler: &mut Scheduler, catalog: &Catalog, known: &mut HashSet<JobId>) {
    let Ok(records) = IndexJobRepository::new(catalog).list_queued(u32::MAX) else {
        return;
    };
    for record in records {
        if known.contains(&record.job_id) {
            continue;
        }
        let Some(source_id) = record.source_id else {
            continue; // No current enqueue path produces this; defensive.
        };
        let kind = job_kind_for(record.job_type);
        known.insert(record.job_id.clone());
        scheduler.load_persisted(IndexJob {
            id: record.job_id,
            file_id: record.file_id,
            source_id,
            kind,
            priority: kind.default_priority(),
            state: JobState::Pending,
            attempt_count: 0,
            last_error_kind: None,
        });
    }
}

/// The category `Scheduler::fail` (RFC-036 §20.1) matches on to decide
/// retry vs. terminal. An `OrbokError::Embedding` carries its own RFC-008
/// §15 category directly; every other error kind -- scan, extract, chunk
/// failures -- keeps the pre-Slice-2 `"worker_error"` label, which is not
/// in `is_terminal_category`'s set, so those jobs retry exactly as they
/// always have.
fn error_kind_for(error: &OrbokError) -> &'static str {
    match error {
        OrbokError::Embedding { category, .. } => category,
        _ => "worker_error",
    }
}

fn job_kind_for(job_type: JobType) -> JobKind {
    match job_type {
        JobType::Scan => JobKind::ScanSource,
        JobType::Extract => JobKind::ExtractFile,
        JobType::Chunk => JobKind::ChunkFile,
        JobType::KeywordIndex => JobKind::UpdateKeywordIndex,
        JobType::Embedding => JobKind::GenerateEmbedding,
        JobType::DeleteStale => JobKind::Cleanup,
        JobType::Rebuild => JobKind::Repair,
    }
}

/// The app-wide subscription that hosts the scheduler for the process's
/// lifetime. `Subscription::run_with` deduplicates by `(portable, builder)`
/// identity (`portable` never changes after startup), so `main.rs` calling
/// this on every `.subscription()` re-evaluation still spawns [`run`]
/// exactly once, the same long-lived pattern the iced websocket example
/// uses for a perpetual connection -- unlike `download.rs`'s
/// `Task::stream`, which drives a one-shot action to completion.
pub fn subscription(portable: bool) -> iced::Subscription<Message> {
    iced::Subscription::run_with(portable, run_stream)
}

// `+ use<>` (edition 2024 precise capturing): the returned stream must not
// depend on `portable`'s elided lifetime, only on the `bool` value copied
// out of it immediately below -- `Subscription::run_with` requires a plain
// `fn(&D) -> S`, not one whose `S` is tied to the borrow's lifetime.
fn run_stream(portable: &bool) -> impl futures::Stream<Item = Message> + use<> {
    let portable = *portable;
    iced::stream::channel(64, async move |output| {
        run(portable, output).await;
    })
}

#[cfg(test)]
mod tests;
