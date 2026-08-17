//! RFC-056 Slice 1: tests against [`super::run_with_context`] directly --
//! the same function `main.rs`'s subscription wires unmodified into
//! `Subscription::run_with` (see `super::subscription`/`super::run_stream`).
//! This is the same "test the core function `run` thinly wraps" precedent
//! `download.rs`'s own tests already establish for `run_with_installer`
//! (RFC-056 Handoff §4).

use super::{ResourceObservation, run_with_context};
use crate::bootstrap;
use crate::bootstrap::embedding_resolution::EmbeddingWorkerParts;
use futures::channel::mpsc::{Receiver, Sender};
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use orbok_core::{JobStatus, ModelId, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_models::{EmbeddingModel, MockEmbeddingModel};
use std::path::Path;
use std::time::{Duration, Instant};

/// A closed resource-observation channel, for every test that isn't
/// exercising RFC-057 §4.1 itself -- the sender half is dropped
/// immediately, so `try_recv` inside the loop always returns
/// `Err(Closed)`, the same as if nothing had ever signalled.
fn no_resource_signals() -> Receiver<ResourceObservation> {
    futures::channel::mpsc::channel(1).1
}

/// A resource-observation channel a test can send into while
/// `run_with_context` is running, plus the sender it uses to do so.
fn resource_signal_channel() -> (Sender<ResourceObservation>, Receiver<ResourceObservation>) {
    futures::channel::mpsc::channel(16)
}

/// A model that always fails to embed, with a stable, RFC-008 §15
/// `inference_error`-categorized error -- for RFC-036 §20.1's retry/
/// terminal split, this is a retryable category, so a job dispatched
/// through it must retry `MAX_JOB_ATTEMPTS` times before permanently
/// failing (Review 165 §5's bar: a test that makes a worker genuinely
/// fail, not one that only exercises the pre-dispatch `model_missing`
/// short-circuit).
struct AlwaysFailsEmbeddingModel;

impl EmbeddingModel for AlwaysFailsEmbeddingModel {
    fn name(&self) -> &str {
        "always-fails"
    }
    fn version(&self) -> &str {
        "v1"
    }
    fn dimension(&self) -> u32 {
        8
    }
    fn embed_batch(&self, _texts: &[&str]) -> OrbokResult<Vec<Vec<f32>>> {
        Err(OrbokError::Embedding {
            category: "inference_error",
            message: "AlwaysFailsEmbeddingModel always fails".to_string(),
        })
    }
}

/// A model whose `embed_batch` sleeps for a realistic per-document cost
/// before returning mock vectors -- creates a genuine contention window
/// deterministically, in CI, without needing `RFC048_MODEL_DIR` or the
/// real 449 MB model (Review 171 §3). `EmbeddingWorker` batches one file's
/// chunks into a single `embed_batch` call
/// (`crates/pipeline/workers/src/embedding.rs`), so sleeping once per call
/// approximates Task 011 Phase 2's measured ~144ms/document real-model
/// cost.
struct SlowEmbeddingModel;

impl EmbeddingModel for SlowEmbeddingModel {
    fn name(&self) -> &str {
        "slow"
    }
    fn version(&self) -> &str {
        "v1"
    }
    fn dimension(&self) -> u32 {
        8
    }
    fn embed_batch(&self, texts: &[&str]) -> OrbokResult<Vec<Vec<f32>>> {
        std::thread::sleep(Duration::from_millis(144));
        MockEmbeddingModel.embed_batch(texts)
    }
}

/// Register a model row for a mock-shaped embedding backend (dimension 8,
/// matching `MockEmbeddingModel`/`SlowEmbeddingModel`), returning its
/// `ModelId` -- shared by every test that wires a mock-shaped model
/// through `EmbeddingWorkerParts::for_test`, since `embeddings.model_id`
/// carries a foreign key to a real `models` row.
fn register_mock_model(catalog: &Catalog, name: &str) -> ModelId {
    use orbok_db::repo::{ModelRepository, ModelRole, ModelStatus, NewModel};
    ModelRepository::new(catalog)
        .insert(NewModel {
            role: ModelRole::Embedding,
            model_name: name.to_string(),
            model_version: "v1".to_string(),
            local_path: None,
            license_summary: None,
            size_bytes: None,
            backend: Some(name.to_string()),
            dimension: Some(8),
            status: ModelStatus::Available,
        })
        .unwrap()
        .model_id
}

fn test_context(data_dir: &Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
        data_dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(data_dir),
            standard_settings_dir: Some(data_dir),
        },
    )
    .unwrap()
}

fn seed_markdown_docs(source_dir: &Path, count: usize) {
    std::fs::create_dir_all(source_dir).unwrap();
    for i in 0..count {
        std::fs::write(
            source_dir.join(format!("doc{i}.md")),
            format!(
                "# Document {i}\n\n\
                 ## Install\n\n\
                 Run the installer and follow the on-screen prompts.\n\n\
                 ## Configure\n\n\
                 Edit the configuration file, then restart the service.\n"
            ),
        )
        .unwrap();
    }
}

/// Poll `check` until it returns `true` or `timeout` elapses. Panics with
/// `what` on timeout so a hung background loop fails the test instead of
/// the test runner itself hanging forever.
async fn wait_until(timeout: Duration, what: &str, mut check: impl FnMut() -> bool) {
    let start = Instant::now();
    loop {
        if check() {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timed out after {timeout:?} waiting for: {what}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn indexed_count(catalog: &Catalog) -> u64 {
    bootstrap::get_health(catalog).indexed
}

fn job_counts_by_status(catalog: &Catalog, status: JobStatus) -> u64 {
    use orbok_db::repo::IndexJobRepository;
    IndexJobRepository::new(catalog)
        .count_by_status()
        .unwrap_or_default()
        .into_iter()
        .find(|(s, _)| *s == status)
        .map(|(_, n)| n)
        .unwrap_or(0)
}

/// Every `job_type != 'scan'` row, regardless of status. `Scan` itself is
/// excluded: since the scan-routing follow-up (Review 162 §2),
/// `scan_and_index_source` enqueues exactly one `Scan` row per call by
/// design (RFC-056 §9's `scan_and_index_source` gate is a single insert,
/// not a scan) -- RFC-004's "unchanged source, no growth" guarantee (Task
/// 013) is about what a scan *discovers* downstream
/// (`Extract`/`Chunk`/`Embedding`/`KeywordIndex`), not about the `Scan`
/// job itself, which is expected to appear once per explicit re-scan.
fn non_scan_job_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type != 'scan'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

/// RFC-056 §8.1/§8.2, Handoff Slice 1: jobs enqueued directly by
/// `Scanner::scan` (bypassing `Scheduler::enqueue` entirely) are picked up
/// by rehydration and driven to completion by the background loop, with no
/// worker invoked directly by the test.
#[tokio::test]
async fn background_loop_processes_directly_enqueued_jobs_to_indexed() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 10);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(Duration::from_secs(20), "all 10 files indexed", || {
        indexed_count(&ui_catalog) == 10
    })
    .await;

    // Every indexed file also gets a `GenerateEmbedding` job (RFC-008 §19),
    // which fails as `model_missing` here since this test passes `None`
    // for `embedding_parts` -- ten expected `failed` rows, not zero.
    // Non-embedding failures would mean extract/chunk itself broke, which
    // is what this asserts against.
    let non_embedding_failures: i64 = ui_catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE status = 'failed' AND job_type != 'embedding'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(non_embedding_failures, 0);
    handle.abort();
}

/// Task 020 / Review 179 §4: `Scheduler::drain_events()` had zero
/// production callers, so `SchedulerEvent`s accumulated in `self.events`
/// for the process's entire lifetime -- measured at ~4 events/job, so a
/// large source grows the buffer without bound. `run_with_context` now
/// discards them once per iteration; this asserts the *retained* count
/// stays small across a real run, not merely that a drain call exists
/// somewhere in the source (a test asserting `drain_events()` returns
/// empty would pass against a buffer drained once just as readily as one
/// that discards every iteration -- it proves nothing about growth).
///
/// `event_count_probe` (test-only, `None` in production) records the
/// worst retained count `run_with_context` ever observed via
/// `fetch_max`, sampled at the same point the real drain happens --
/// the peak across the whole run, not an arbitrary single reading.
#[tokio::test]
async fn event_buffer_stays_bounded_regardless_of_work_done() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let file_count = 40;
    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, file_count);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let event_count_probe = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        Some(event_count_probe.clone()),
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(Duration::from_secs(20), "all 40 files indexed", || {
        indexed_count(&ui_catalog) == file_count as u64
    })
    .await;

    // 40 files -> extract+chunk+keyword+embedding per file, ~4 events per
    // job dispatched (RFC-036 §9-§11's Queued/Started/Completed-or-Failed
    // plus one more) -- several hundred events total if never drained.
    // Bounded per-iteration draining should never retain more than a
    // small multiple of one iteration's worth.
    let peak = event_count_probe.load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        peak < 50,
        "retained event count must not scale with total work done -- \
         observed a peak of {peak} events retained while processing \
         {file_count} files"
    );

    handle.abort();
}

/// RFC-056 §9 first acceptance criterion: adding a source of a few hundred
/// files returns control to the caller in under 2 seconds.
///
/// Review 162 §2: the Slice 1 version of this test loosened its ceiling to
/// tolerate a 2.06s Windows CI measurement, attributing it to cross-platform
/// I/O variance -- wrong. `scan_and_index_source` still called `Scanner::scan`
/// inline at the time, so the number scaled with source size, not a fixed
/// per-platform constant. Scanning is now itself a `JobKind::ScanSource` job
/// this function only enqueues (one row insert), so this needs no background
/// loop at all -- it is testing exactly the synchronous cost RFC-056 exists
/// to remove, restored to the RFC's literal figure.
///
/// Review 163 §2: the timing assertion alone cannot detect a revert to
/// inline scanning on a fast-enough CI runner -- 400 files scanned inline
/// measured ~37.8ms on Linux, comfortably under 2s, so the Fast/Release
/// gates (both Linux) would not have caught Slice 1's original defect; only
/// Windows's slower runner did, by coincidence of being the slowest
/// platform rather than by the test's own design. Structural assertions
/// alongside the timing one make the property platform-independent: no
/// files discovered yet, and the `Scan` job it enqueued is still `queued` --
/// both false the instant scanning runs inline instead of being deferred.
#[tokio::test]
async fn scan_and_index_source_returns_control_in_under_two_seconds() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 400);
    let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();

    let start = Instant::now();
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    let elapsed = start.elapsed();

    println!("scan_and_index_source (enqueue-only, 400 files): {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "scan_and_index_source took {elapsed:?}, must return control in under 2s (RFC-056 §9)"
    );

    let discovered_files: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM files WHERE source_id = ?1",
            [card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        discovered_files, 0,
        "no file discovery may have run yet -- scanning must be deferred, not inline"
    );

    let scan_job_status: String = catalog
        .lock()
        .query_row(
            "SELECT status FROM index_jobs WHERE job_type = 'scan' AND source_id = ?1",
            [card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        scan_job_status, "queued",
        "the Scan job this call enqueued must still be queued -- the walk has not run"
    );
}

/// RFC-008 §15, carried into the hosting loop: with no model configured
/// (`embedding_parts: None`), `GenerateEmbedding` jobs fail as
/// `model_missing`, terminally -- not retried (RFC-036 §20.1's terminal
/// category set) -- and re-scanning the same unchanged source does not
/// grow the job table (RFC-004 change detection at the `Scanner` level),
/// exactly matching the guarantee Task 013/Review 161 established for the
/// legacy `run_pending` path.
#[tokio::test]
async fn no_model_configured_embedding_jobs_fail_as_model_missing_without_unbounded_growth() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 5);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(Duration::from_secs(20), "all 5 files indexed", || {
        indexed_count(&ui_catalog) == 5
    })
    .await;
    wait_until(
        Duration::from_secs(20),
        "no jobs left queued (embedding jobs resolved)",
        || job_counts_by_status(&ui_catalog, JobStatus::Queued) == 0,
    )
    .await;

    let embedding_failures: i64 = ui_catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = 'embedding' \
             AND status = 'failed' AND error_category = 'model_missing'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        embedding_failures, 5,
        "one model_missing failure per file, matching run_pending's existing behaviour"
    );

    let before = non_scan_job_count(&ui_catalog);
    // Re-scan the same, unchanged source -- the Scan job itself is a new
    // row by design (Review 162 §2), but must not discover anything new
    // downstream.
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();
    wait_until(
        Duration::from_secs(20),
        "the re-scan's Scan job resolves",
        || job_counts_by_status(&ui_catalog, JobStatus::Queued) == 0,
    )
    .await;
    let after = non_scan_job_count(&ui_catalog);
    assert_eq!(
        before, after,
        "re-scanning an unchanged source must not grow the non-Scan job table"
    );

    handle.abort();
}

/// RFC-036 §12 / RFC-019, RFC-056 Slice 3: `background_indexing = false`
/// pauses the loop before any job dispatches -- no file gets indexed, and
/// a `Scan` job already `queued` when the loop starts is marked `paused`
/// in the catalog (RFC-036 §12.2's "persist progress") rather than left
/// `queued` forever with nothing ever picking it up.
#[tokio::test]
async fn background_indexing_disabled_pauses_before_any_job_runs() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 5);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        false,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    // Several idle cycles (IDLE_POLL is 300ms), to prove the loop stays
    // paused rather than "hasn't gotten to it yet."
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        indexed_count(&ui_catalog),
        0,
        "no file may be indexed while background_indexing is disabled"
    );
    let scan_status: String = ui_catalog
        .lock()
        .query_row(
            "SELECT status FROM index_jobs WHERE job_type = 'scan' AND source_id = ?1",
            [card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        scan_status, "paused",
        "the queued Scan job must be paused, not left queued forever"
    );

    handle.abort();
}

/// RFC-056 §9 criterion 4 (Review 174 §3, required): *"turning it on
/// resumes it"* -- the second half of the criterion, which the previous
/// test alone leaves untested and which was previously false:
/// `Scheduler::resume` had zero callers anywhere. The round trip, not
/// resume in isolation: a test that only asserted "resume un-pauses"
/// would pass against a `resume` nothing calls, the exact shape §1.3's
/// self-caught gap in the review request had. Two separate
/// `run_with_context` spawns, matching how
/// `interrupted_running_job_is_recovered_and_completes_after_restart`
/// already simulates a restart -- a real restart always constructs a
/// fresh `Scheduler`, so this is the scenario that actually matters, not
/// pause/resume on one live instance.
#[tokio::test]
async fn background_indexing_off_then_on_pauses_then_resumes() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 5);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    // Off: the queued Scan job is paused, nothing indexes.
    {
        let loop_catalog = bootstrap::open_catalog(&context).unwrap();
        let loop_cache = bootstrap::cache_service(&context).unwrap();
        let (tx, rx) = futures::channel::mpsc::channel(64);
        let handle = tokio::spawn(run_with_context(
            loop_catalog,
            loop_cache,
            None,
            false,
            true,
            no_resource_signals(),
            tx,
            None,
        ));
        drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)
        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.abort();
    }
    assert_eq!(
        indexed_count(&ui_catalog),
        0,
        "nothing may index while paused"
    );
    let scan_status: String = ui_catalog
        .lock()
        .query_row(
            "SELECT status FROM index_jobs WHERE job_type = 'scan' AND source_id = ?1",
            [card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(scan_status, "paused");

    // On, via a fresh `Scheduler` (a real restart constructs one): the
    // rows the previous run paused must be resumed, not left stranded.
    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(
        Duration::from_secs(20),
        "all 5 files indexed after resuming",
        || indexed_count(&ui_catalog) == 5,
    )
    .await;

    handle.abort();
}

/// RFC-057 §4.2, HANDOFF-057 §3.1: the scheduling consequence, not the
/// mode field. A live `UserActive` signal must make `queue.rs:222`'s
/// existing skip actually fire -- an embedding job must not dispatch
/// while the signal keeps arriving -- and going idle must let it resume.
/// Asserted against `embeddings` table rows, not `scheduler.resource_mode()`
/// (which this test has no access to anyway, running through the real
/// `run_with_context` per RFC-056 §8.8, not a bare `Scheduler`).
#[tokio::test]
async fn user_active_signal_defers_embedding_and_idle_resumes_it() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 1);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let model_id = register_mock_model(&loop_catalog, "mock");
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(MockEmbeddingModel),
        model_id.clone(),
    ));
    let (mut signal_tx, signal_rx) = resource_signal_channel();

    // `tokio::spawn` only schedules: the signaller may not be polled
    // before the loop's first iteration. Queue one observation
    // synchronously, on this thread, so the channel is non-empty before
    // the loop can possibly drain it -- spawn order alone does not
    // guarantee this (Task 026; Review 185 §5).
    signal_tx.try_send(ResourceObservation::UserActive).unwrap();

    // Keep the scheduler continuously "user active" -- well inside
    // `USER_IDLE_TIMEOUT` -- for the rest of the sustained window below.
    let keep_active = tokio::spawn(async move {
        loop {
            let _ = signal_tx.try_send(ResourceObservation::UserActive);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        true,
        signal_rx,
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    let embedding_row_count = |catalog: &Catalog, model_id: &ModelId| -> i64 {
        catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE model_id = ?1",
                [model_id.as_str()],
                |row| row.get(0),
            )
            .unwrap()
    };

    wait_until(
        Duration::from_secs(20),
        "the file is indexed (extract+chunk unaffected by UserActive)",
        || indexed_count(&ui_catalog) == 1,
    )
    .await;

    // Sustained window while still signalling active: embedding must not
    // have run.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        embedding_row_count(&ui_catalog, &model_id),
        0,
        "embedding must defer to a live UserActive signal (RFC-036 §9.2, queue.rs:222)"
    );

    // Stop signalling; once idle, embedding resumes.
    keep_active.abort();
    wait_until(
        Duration::from_secs(20),
        "embedding resumes once the user goes idle",
        || embedding_row_count(&ui_catalog, &model_id) > 0,
    )
    .await;

    handle.abort();
}

/// RFC-057 §4.3c / §6.2 item 6, through the real application path: the
/// interleaving that motivated deriving the mode instead of mutating it
/// per source. A per-source-mutation design (Slice 1's shape, before
/// Amendment 1) would pass right up to the point the user goes idle and
/// fail there -- `notify_user_idle` returned unconditionally to `Normal`,
/// dropping the still-true battery observation and letting embedding
/// resume while genuinely still on battery. The `OnBattery(true)`
/// observation below is sent exactly once, before the loop even spawns
/// (same race-avoidance as the deferral test above) -- it is
/// level-triggered state, not an edge like `UserActive`, so nothing needs
/// to repeat it for the derivation to keep honoring it.
#[tokio::test]
async fn low_impact_survives_a_user_activity_interleaving_through_the_app() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 1);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let model_id = register_mock_model(&loop_catalog, "mock");
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(MockEmbeddingModel),
        model_id.clone(),
    ));
    let (mut signal_tx, signal_rx) = resource_signal_channel();
    signal_tx
        .try_send(ResourceObservation::OnBattery(true))
        .unwrap();

    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        true,
        signal_rx,
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    let embedding_row_count = |catalog: &Catalog, model_id: &ModelId| -> i64 {
        catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE model_id = ?1",
                [model_id.as_str()],
                |row| row.get(0),
            )
            .unwrap()
    };

    wait_until(
        Duration::from_secs(20),
        "the file is indexed (extract+chunk unaffected by LowImpact)",
        || indexed_count(&ui_catalog) == 1,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        embedding_row_count(&ui_catalog, &model_id),
        0,
        "embedding must defer on battery (RFC-057 §4.3a, queue.rs:222)"
    );

    // User types for a while, still on battery -- embedding stays deferred
    // either way (UserActive and LowImpact impose the identical
    // restriction, §4.3c).
    for _ in 0..10 {
        let _ = signal_tx.try_send(ResourceObservation::UserActive);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        embedding_row_count(&ui_catalog, &model_id),
        0,
        "embedding must still defer while the user is active"
    );

    // User goes idle (no further UserActive signals, and the battery
    // observation is never repeated). Hold well past USER_IDLE_TIMEOUT: if
    // the derivation dropped the battery state on the idle transition and
    // fell back to Normal, embedding would run here.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        embedding_row_count(&ui_catalog, &model_id),
        0,
        "embedding must still defer once idle -- still on battery (RFC-057 §4.3c)"
    );

    // Off battery: only now does embedding run.
    let _ = signal_tx.try_send(ResourceObservation::OnBattery(false));
    wait_until(
        Duration::from_secs(20),
        "embedding resumes once off battery and idle",
        || embedding_row_count(&ui_catalog, &model_id) > 0,
    )
    .await;

    handle.abort();
}

/// RFC-057 §4.4: `pause_embedding_on_battery_enabled = false` means the
/// user opted out of the reduction -- an `OnBattery(true)` observation
/// must not defer embedding at all, proving the gate suppresses the
/// *effect*, not merely a differently-labeled mode that still restricts
/// scheduling the same way.
#[tokio::test]
async fn on_battery_does_not_defer_embedding_when_the_setting_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 1);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let model_id = register_mock_model(&loop_catalog, "mock");
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(MockEmbeddingModel),
        model_id.clone(),
    ));
    let (mut signal_tx, signal_rx) = resource_signal_channel();
    signal_tx
        .try_send(ResourceObservation::OnBattery(true))
        .unwrap();

    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        false, // pause_embedding_on_battery_enabled: disabled
        signal_rx,
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    let embedding_row_count = |catalog: &Catalog, model_id: &ModelId| -> i64 {
        catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE model_id = ?1",
                [model_id.as_str()],
                |row| row.get(0),
            )
            .unwrap()
    };

    wait_until(
        Duration::from_secs(20),
        "embedding completes despite being on battery, since the setting is disabled",
        || embedding_row_count(&ui_catalog, &model_id) > 0,
    )
    .await;

    handle.abort();
}

/// RFC-057 §3.2 / HANDOFF-057 §3.2: a live, repeated `UserActive` signal
/// must not resume a scheduler the user paused via `background_indexing`
/// -- unlike `background_indexing_disabled_pauses_before_any_job_runs`,
/// which never sends anything, this actually exercises the new channel
/// path while paused.
///
/// **What this does not isolate:** in this exact scenario -- `pause()`
/// runs at startup, before anything is ever rehydrated -- the in-memory
/// queue is empty the whole time, so `rehydrate`'s own refusal to
/// discover `paused` catalog rows would produce the same passing result
/// even with `notify_user_active`'s `Paused` guard removed (confirmed:
/// breaking the guard here did not fail this test). This is a real,
/// valid end-to-end regression guard for the startup-pause path, but it
/// is not proof of the guard specifically. Two complementary tests cover
/// what this one cannot (Review 177 §3):
/// `notify_user_active_does_not_override_paused_with_a_job_already_queued`
/// in `rfc036_scheduler.rs` isolates the guard alone at the `Scheduler`
/// level (a job already sitting in memory when `pause()` runs, which the
/// guard is the only thing standing between it and dispatch), and
/// `user_active_does_not_resume_paused_with_work_enqueued_after_pause`
/// below isolates it through the real application path -- work enqueued
/// *after* the pause is written `queued`, not `paused`, so `rehydrate`
/// can find it the moment a broken guard lets the mode leave `Paused`,
/// which is exactly the scenario a user actually reaches (turn indexing
/// off, keep using the app, add a folder).
#[tokio::test]
async fn user_active_signal_does_not_override_paused() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 3);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (mut signal_tx, signal_rx) = resource_signal_channel();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        false, // background_indexing off -> Paused before the loop starts
        true,
        signal_rx,
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    for _ in 0..10 {
        let _ = signal_tx.try_send(ResourceObservation::UserActive);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert_eq!(
        indexed_count(&ui_catalog),
        0,
        "a UserActive signal must not un-pause a scheduler the user paused (background_indexing=false)"
    );

    handle.abort();
}

/// RFC-057 §3.2 / Review 177 §3: the scenario a user actually reaches --
/// turn `background_indexing` off, keep using the app, add a folder --
/// isolated through the real application path. `user_active_signal_does_not_override_paused`
/// (above) cannot catch a broken guard here: `pause()` runs before
/// anything exists to enqueue, so every catalog row it touches is already
/// `paused`, and `rehydrate` never discovers `paused` rows regardless of
/// mode. Enqueuing *after* the pause is settled writes fresh rows as
/// `queued`, giving a broken guard something real to dispatch: if a
/// `UserActive` signal wrongly leaves `Paused`, `rehydrate` finds these
/// rows and they run.
#[tokio::test]
async fn user_active_does_not_resume_paused_with_work_enqueued_after_pause() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (mut signal_tx, signal_rx) = resource_signal_channel();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        false, // paused at startup
        true,
        signal_rx,
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    // Let the loop reach its paused steady state first.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // NOW enqueue fresh work: these rows are written `queued`, not
    // `paused`, so `rehydrate` can find them the moment the mode leaves
    // `Paused` for any reason.
    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 3);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    for _ in 0..10 {
        let _ = signal_tx.try_send(ResourceObservation::UserActive);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert_eq!(
        indexed_count(&ui_catalog),
        0,
        "user activity must not resume a paused scheduler, even for work enqueued after the pause"
    );
    handle.abort();
}

/// RFC-036 §20.1 / Review 165 §5: a worker that genuinely fails -- not the
/// pre-dispatch `model_missing` short-circuit every test above exercises --
/// must retry up to the attempt limit, then permanently fail with its own
/// RFC-008 §15 category recorded in the catalog. This is the concrete bar
/// Review 165 §5 set for Slice 2: the seven fail mutants that survived
/// mutation testing did so because nothing before this exercised
/// `Scheduler::fail`/`scheduler_host::run` against a worker that actually
/// errors mid-embed.
#[tokio::test]
async fn embedding_worker_that_always_fails_is_retried_then_permanently_failed() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 2);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(AlwaysFailsEmbeddingModel),
        ModelId::from_string("always-fails_v1".to_string()),
    ));
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(
        Duration::from_secs(20),
        "both embedding jobs permanently fail",
        || job_counts_by_status(&ui_catalog, JobStatus::Failed) == 2,
    )
    .await;

    let categories: Vec<String> = {
        let conn = ui_catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT error_category FROM index_jobs \
                 WHERE job_type = 'embedding' AND status = 'failed'",
            )
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(categories.len(), 2);
    assert!(
        categories.iter().all(|c| c == "inference_error"),
        "must carry the worker's real RFC-008 §15 category, not model_missing: {categories:?}"
    );

    handle.abort();
}

/// The success-path complement to the test above: `EmbeddingWorker` (the
/// mock model standing in for a loaded backend -- the *dispatch path* is
/// real, the model is not, see the name) actually embeds and persists
/// vectors through the hosting loop, not just terminates jobs.
///
/// Review 171 §4: asserts RFC-056 §9 criterion 2 literally --
/// `embeddings` non-zero *and equal to the chunk count* -- rather than
/// merely non-zero, which would still pass if only one of the three
/// seeded files had embedded.
#[tokio::test]
async fn embedding_worker_persists_embeddings_through_the_real_dispatch_path() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 3);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let model_id = register_mock_model(&loop_catalog, "mock");
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(MockEmbeddingModel),
        model_id.clone(),
    ));
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(Duration::from_secs(20), "all 3 files indexed", || {
        indexed_count(&ui_catalog) == 3
    })
    .await;
    wait_until(
        Duration::from_secs(20),
        "no jobs left queued (embedding jobs resolved)",
        || job_counts_by_status(&ui_catalog, JobStatus::Queued) == 0,
    )
    .await;

    let embedding_rows: i64 = ui_catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM embeddings WHERE model_id = ?1",
            [model_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let active_chunks: i64 = ui_catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunks c JOIN files f ON c.file_id = f.file_id \
             WHERE f.source_id = ?1 AND c.chunk_status = 'active'",
            [card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        active_chunks > 0,
        "the seeded documents must have produced at least one chunk"
    );
    assert_eq!(
        embedding_rows, active_chunks,
        "RFC-056 §9 criterion 2: embeddings must be non-zero and equal the chunk count"
    );
    assert_eq!(
        job_counts_by_status(&ui_catalog, JobStatus::Failed),
        0,
        "a working model must not produce any failed jobs"
    );

    handle.abort();
}

/// RFC-036 §16 / RFC-056 §9: a job left `running` by an abrupt exit is
/// reset to `queued` by the existing RFC-018 startup recovery (unchanged
/// by this slice, already running at real app startup) and then picked up
/// by a freshly constructed `Scheduler`'s rehydration pass -- exactly what
/// happens on an actual restart, since a new process always starts with an
/// empty in-memory queue. `check_catalog_integrity` (RFC-018 §16 test 7)
/// confirms no orphaned/partial state resulted from the interruption.
#[tokio::test]
async fn interrupted_running_job_is_recovered_and_completes_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 6);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    // Simulate a crash mid-job: force one queued job to `running`, as if
    // the (now-dead) previous session's loop had popped it via `tick()`
    // but never reached `complete`/`fail` before the process died.
    {
        use orbok_db::repo::IndexJobRepository;
        let jobs = IndexJobRepository::new(&ui_catalog);
        let queued = jobs.list_queued(1).unwrap();
        let stuck = queued.first().expect("at least one job was enqueued");
        jobs.set_status(&stuck.job_id, JobStatus::Running).unwrap();
    }
    assert_eq!(job_counts_by_status(&ui_catalog, JobStatus::Running), 1);

    // "Restart": the same recovery step `bootstrap::load_initial_state`
    // runs before any UI or hosting loop exists, against a fresh process.
    let cache_db_path = temp.path().join(orbok_db::CACHE_FILE_NAME);
    let report = orbok_workers::run_startup_recovery(&ui_catalog, &cache_db_path).unwrap();
    assert_eq!(report.jobs_reset, 1);
    assert_eq!(job_counts_by_status(&ui_catalog, JobStatus::Running), 0);

    // A fresh process constructs a fresh `Scheduler` with empty queues --
    // rehydration is what makes the reset-to-queued job reachable again.
    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    wait_until(
        Duration::from_secs(20),
        "all 6 files indexed after recovery",
        || indexed_count(&ui_catalog) == 6,
    )
    .await;

    let integrity = orbok_workers::check_catalog_integrity(&ui_catalog).unwrap();
    assert!(
        integrity.is_clean(),
        "interrupted job must leave no orphaned/partial state: {integrity:?}"
    );

    handle.abort();
}

/// RFC-036 §20.2: a `blocked` row -- the honest state `Scheduler::fail`'s
/// retry branch records when its in-memory push is skipped for lack of
/// room, rather than falsely `queued` -- is recovered by `rehydrate` once
/// room exists, its catalog status corrected back to `queued`. Calls
/// `super::rehydrate` directly (a private fn, reachable from this child
/// module): it is the real function `run_with_context`'s loop calls, not a
/// reimplementation of its logic, matching how `dispatch.rs`'s own unit
/// tests exercise `Scheduler::fail` without going through this module at
/// all -- triggering genuine backpressure through the full application
/// path would need thousands of files against `Scheduler::with_defaults`'s
/// real capacities.
#[test]
fn rehydrate_recovers_a_blocked_job_once_queue_room_exists() {
    use orbok_core::{
        HiddenFilePolicy, IndexMode, JobType, PersistenceMode, SourceType, SymlinkPolicy,
    };
    use orbok_db::repo::{IndexJobRepository, NewSource, SourceRepository};
    use orbok_workers::{IndexJob, JobKind, JobState, QueueCapacity, Scheduler, SchedulerConfig};
    use std::collections::HashSet;

    let catalog = Catalog::open_in_memory().unwrap();
    let source = SourceRepository::new(&catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: None,
            original_path: "/tmp/rehydrate-blocked-test".to_string(),
            canonical_path: "/tmp/rehydrate-blocked-test".to_string(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();

    let capacity = QueueCapacity {
        extract_queue_max: 1,
        ..QueueCapacity::default()
    };
    let mut sched = Scheduler::new(SchedulerConfig {
        capacity,
        ..SchedulerConfig::default()
    });
    let mut known = HashSet::new();
    let jobs = IndexJobRepository::new(&catalog);

    // job_a fills the single extract slot, seeded directly the way a real
    // rehydrate() pass would (catalog row + in-memory copy + `known`).
    let job_a_id = orbok_core::JobId::generate();
    jobs.enqueue_with_priority(
        &job_a_id,
        JobType::Extract,
        Some(&source.source_id),
        None,
        0,
    )
    .unwrap();
    known.insert(job_a_id.clone());
    assert!(sched.load_persisted(IndexJob {
        id: job_a_id.clone(),
        file_id: None,
        source_id: source.source_id.clone(),
        kind: JobKind::ExtractFile,
        priority: JobKind::ExtractFile.default_priority(),
        state: JobState::Pending,
        attempt_count: 0,
        last_error_kind: None,
    }));

    // job_b's retry has nowhere to go while job_a still occupies the
    // queue's one slot -- `sched.fail` records it `blocked` (RFC-036
    // §20.2, verified directly against `Scheduler::fail` in
    // `rfc036_scheduler.rs`).
    let job_b_id = orbok_core::JobId::generate();
    jobs.enqueue_with_priority(
        &job_b_id,
        JobType::Extract,
        Some(&source.source_id),
        None,
        0,
    )
    .unwrap();
    let job_b = IndexJob {
        id: job_b_id.clone(),
        file_id: None,
        source_id: source.source_id.clone(),
        kind: JobKind::ExtractFile,
        priority: JobKind::ExtractFile.default_priority(),
        state: JobState::Pending,
        attempt_count: 0,
        last_error_kind: None,
    };
    sched.fail(job_b, "worker_error", None, &catalog).unwrap();
    assert_eq!(
        jobs.status_of(&job_b_id).unwrap(),
        Some(JobStatus::Blocked),
        "precondition: job_b must be blocked before recovery is attempted"
    );

    // Room now exists: job_a is popped (simulating dispatch), freeing the
    // one extract slot.
    let popped = sched.tick().expect("job_a must still be dispatchable");
    assert_eq!(popped.id, job_a_id);

    super::rehydrate(&mut sched, &catalog, &mut known);

    assert_eq!(
        jobs.status_of(&job_b_id).unwrap(),
        Some(JobStatus::Queued),
        "a recovered blocked row's catalog status must be corrected back to queued"
    );
    assert!(
        known.contains(&job_b_id),
        "a recovered blocked row must be tracked in `known` like any other in-memory job"
    );
    let recovered = sched
        .tick()
        .expect("job_b must be dispatchable again after recovery");
    assert_eq!(recovered.id, job_b_id);
}

/// `job_is_still_queued` directly -- the exact check the dispatch loop
/// makes before running a popped job (RFC-036 §12.3). Proven at the
/// function level because the integration test below cannot distinguish
/// this fix from the pre-existing graceful degradation a stale job
/// already gets via `FileNotFound`/`SourceNotFound` errors and retry
/// exhaustion: both converge on "no crash, catalog ends up clean" without
/// this check, so that test alone would pass identically with or without
/// it (verified while writing it -- see the review request).
#[test]
fn job_is_still_queued_reflects_the_catalog_row_honestly() {
    use orbok_core::{
        HiddenFilePolicy, IndexMode, JobType, PersistenceMode, SourceType, SymlinkPolicy,
    };
    use orbok_db::repo::{IndexJobRepository, NewSource, SourceRepository};

    let catalog = Catalog::open_in_memory().unwrap();
    let source = SourceRepository::new(&catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: None,
            original_path: "/tmp/job-is-still-queued-test".to_string(),
            canonical_path: "/tmp/job-is-still-queued-test".to_string(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();
    let jobs = IndexJobRepository::new(&catalog);

    let id = orbok_core::JobId::generate();
    jobs.enqueue_with_priority(&id, JobType::Extract, Some(&source.source_id), None, 0)
        .unwrap();
    assert!(
        super::job_is_still_queued(&catalog, &id),
        "a freshly enqueued row is queued"
    );

    jobs.set_status(&id, JobStatus::Canceled).unwrap();
    assert!(
        !super::job_is_still_queued(&catalog, &id),
        "a canceled row is not queued"
    );

    // Deleted entirely -- RFC-036 §12.3's actual production path, source
    // removal cascade-deleting via the FK on `sources`, not a status
    // update.
    SourceRepository::new(&catalog)
        .delete_with_all_data(&source.source_id)
        .unwrap();
    assert!(
        !super::job_is_still_queued(&catalog, &id),
        "a row deleted out from under it is not queued"
    );
}

/// RFC-036 §12.3, RFC-056 Slice 3: removing a source mid-flight -- its
/// `index_jobs` rows cascade-deleted by the FK on `sources`
/// (`bootstrap::remove_source`) -- does not crash or wedge the hosting
/// loop. The dispatch-time freshness check in `scheduler_host.rs` skips
/// any popped job whose catalog row is no longer `queued`, so the loop
/// keeps processing an unrelated source added afterward. (This test alone
/// cannot prove the freshness check specifically did the work -- see
/// `job_is_still_queued_reflects_the_catalog_row_honestly` above for that;
/// this one guards the surrounding system against a real crash/wedge
/// regardless of cause.)
#[tokio::test]
async fn removing_a_source_mid_flight_does_not_crash_or_wedge_the_loop() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let doomed_dir = temp.path().join("doomed");
    seed_markdown_docs(&doomed_dir, 50);
    let (doomed_card, _) =
        bootstrap::add_source(&ui_catalog, &doomed_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &doomed_card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    // Let the loop actually start discovering/dispatching this source's
    // jobs before removing it out from under it -- a genuine race, not a
    // removal before anything existed to race with.
    wait_until(
        Duration::from_secs(20),
        "at least one job exists for the doomed source",
        || non_scan_job_count(&ui_catalog) > 0,
    )
    .await;

    bootstrap::remove_source(&ui_catalog, doomed_card.source_id.as_str()).unwrap();

    // Prove the loop is still alive and functioning: a second, unrelated
    // source added afterward is indexed normally. Robust to how far the
    // doomed source got before removal -- the cascade delete removes any
    // of its already-`indexed` `files` rows too, so they cannot inflate
    // this count.
    let survivor_dir = temp.path().join("survivor");
    seed_markdown_docs(&survivor_dir, 3);
    let (survivor_card, _) =
        bootstrap::add_source(&ui_catalog, &survivor_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &survivor_card.source_id).unwrap();

    wait_until(
        Duration::from_secs(20),
        "the survivor source's 3 files are indexed",
        || indexed_count(&ui_catalog) == 3,
    )
    .await;

    let doomed_rows: i64 = ui_catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM sources WHERE source_id = ?1",
            [doomed_card.source_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(doomed_rows, 0, "the removed source's row must be gone");

    let integrity = orbok_workers::check_catalog_integrity(&ui_catalog).unwrap();
    assert!(
        integrity.is_clean(),
        "a source removed mid-flight must leave no orphaned/partial state: {integrity:?}"
    );

    handle.abort();
}

async fn index_via_background_loop(
    context: &RuntimeContext,
    ui_catalog: &Catalog,
    file_count: u64,
) {
    let loop_catalog = bootstrap::open_catalog(context).unwrap();
    let loop_cache = bootstrap::cache_service(context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        None,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)
    wait_until(
        Duration::from_secs(90),
        &format!("all {file_count} files indexed"),
        || indexed_count(ui_catalog) == file_count,
    )
    .await;
    handle.abort();
}

/// Baseline: how long the background loop alone takes to index 300 files
/// with no concurrent catalog access, for comparison against the
/// concurrent-search measurement below (HANDOFF §3.2).
#[tokio::test]
async fn background_indexing_baseline_with_no_concurrent_access() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 300);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let start = Instant::now();
    index_via_background_loop(&context, &ui_catalog, 300).await;
    println!(
        "background indexing, 300 files, no concurrent access: {:?}",
        start.elapsed()
    );
}

/// HANDOFF §3.2: measure whether a concurrent UI search is stalled by the
/// hosting loop's own catalog operations while it is actively indexing --
/// and, since both connections share one `Catalog` with no `busy_timeout`
/// pragma set (`catalog.rs`), whether sustained concurrent access instead
/// slows *indexing* down (a `SQLITE_BUSY` returns as an immediate `Err`
/// that this slice's `let _ =` call sites silently drop, not a retry).
/// Reported, not gated on a strict threshold -- per the handoff, a
/// measurable regression is a finding to report, not something to work
/// around speculatively in this slice.
///
/// Review 171 §3: Slice 1's version of this test passed `None` for
/// `embedding_parts`, so the loop's own work was extract/chunk only -- too
/// fast (whole 300-file run in ~1.1s) to create a meaningful contention
/// window, leaving HANDOFF §3.2's question unanswered through two more
/// slices ("re-run that measurement in Slice 2 with embedding's ~144ms
/// per document in the loop", Review 164 §3). `SlowEmbeddingModel` puts
/// that realistic per-document cost in the loop so the question is
/// finally asked against something slow enough to matter.
/// `flavor = "multi_thread"`: the background loop and the search-sampling
/// loop must run on genuinely separate OS threads for
/// `SlowEmbeddingModel`'s `std::thread::sleep` to block only the loop's
/// own work rather than starving the search task outright on a
/// single-threaded executor -- the same pattern `model_delivery.rs`'s
/// tests already use where genuine concurrency matters.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn search_latency_while_background_indexing_is_running() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 300);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();
    let model_id = register_mock_model(&loop_catalog, "slow");
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(SlowEmbeddingModel),
        model_id,
    ));
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        true,
        true,
        no_resource_signals(),
        tx,
        None,
    ));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    let search_catalog = bootstrap::open_catalog(&context).unwrap();
    let mut latencies = Vec::new();
    let overall_start = Instant::now();
    // Loop until every job -- Scan, Extract, Chunk, and (this test's whole
    // point) Embedding -- has reached a terminal state, not merely until
    // `indexed_count` hits 300: `indexed` is set at the chunk stage,
    // before embedding even starts (RFC-008 §19's job is a separate,
    // later queue entry), so gating on it alone would let this loop exit
    // before `SlowEmbeddingModel`'s sleeps ever ran -- exactly the gap
    // that made the first cut of this fix measure ~1.1s, identical to the
    // no-embedding baseline, despite the slow model being wired in.
    //
    // 300 files * ~144ms/doc of simulated embedding cost alone is ~43s,
    // serialized through the loop's one-job-at-a-time dispatch; measured
    // ~45s locally, in isolation. 120s (comfortable headroom over that in
    // isolation) still timed out on Windows CI, where this test's worker
    // threads share the runner with every other test in the same binary
    // running concurrently (`cargo test` doesn't serialize test
    // functions) -- the same cross-platform I/O/scheduling variance
    // Review 162 §2.2 already found for scanning. 300s absorbs that
    // without shrinking the file count or the per-document cost, both of
    // which are what makes the measurement below realistic.
    let sampling = tokio::time::timeout(Duration::from_secs(300), async {
        while job_counts_by_status(&ui_catalog, JobStatus::Queued) > 0 {
            let start = Instant::now();
            let _ = bootstrap::run_search(&context, &search_catalog, "install", 20);
            latencies.push(start.elapsed());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let overall = overall_start.elapsed();
    handle.abort();

    let max = latencies.iter().max().copied().unwrap_or_default();
    let sum: Duration = latencies.iter().sum();
    let avg = sum
        .checked_div(latencies.len().max(1) as u32)
        .unwrap_or_default();
    println!(
        "300 files indexed with a ~144ms/doc simulated embedding cost and concurrent \
         search sampling every 10ms: total {overall:?} ({} search samples, avg {avg:?}, \
         max {max:?} per search) -- HANDOFF §3.2",
        latencies.len()
    );
    assert!(
        sampling.is_ok(),
        "300 files did not finish indexing within 300s while a concurrent search ran every 10ms -- \
         see the no-concurrency baseline for comparison; report this per HANDOFF §3.2"
    );

    // §3.2b (HANDOFF-056, Review 172 §3): settle what the during-indexing
    // average actually is. The loop above only exits once every job is
    // terminal, and the background task is now stopped, so this samples
    // the identical search against a full catalog with no concurrent
    // writes. If this stays near the during-indexing average, the cost is
    // catalog size, not contention; if it falls back toward Slice 1's
    // ~270µs no-embedding baseline, the concurrent writes during indexing
    // are the real cost.
    let post_indexing_latencies: Vec<Duration> = (0..20)
        .map(|_| {
            let start = Instant::now();
            let _ = bootstrap::run_search(&context, &search_catalog, "install", 20);
            start.elapsed()
        })
        .collect();
    let post_max = post_indexing_latencies
        .iter()
        .max()
        .copied()
        .unwrap_or_default();
    let post_sum: Duration = post_indexing_latencies.iter().sum();
    let post_avg = post_sum
        .checked_div(post_indexing_latencies.len().max(1) as u32)
        .unwrap_or_default();
    println!(
        "post-indexing (full catalog, no concurrent writes), {} search samples: \
         avg {post_avg:?}, max {post_max:?} -- compare against the during-indexing \
         figure above to settle §3.2b",
        post_indexing_latencies.len()
    );
}
