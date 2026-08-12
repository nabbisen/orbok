//! RFC-056 Slice 1: tests against [`super::run_with_context`] directly --
//! the same function `main.rs`'s subscription wires unmodified into
//! `Subscription::run_with` (see `super::subscription`/`super::run_stream`).
//! This is the same "test the core function `run` thinly wraps" precedent
//! `download.rs`'s own tests already establish for `run_with_installer`
//! (RFC-056 Handoff §4).

use super::run_with_context;
use crate::bootstrap;
use crate::bootstrap::embedding_resolution::EmbeddingWorkerParts;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use orbok_core::{JobStatus, ModelId, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_models::{EmbeddingModel, MockEmbeddingModel};
use std::path::Path;
use std::time::{Duration, Instant};

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
    let handle = tokio::spawn(run_with_context(loop_catalog, loop_cache, None, tx));
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
    let handle = tokio::spawn(run_with_context(loop_catalog, loop_cache, None, tx));
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
        tx,
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

/// The success-path complement to the test above: a real `EmbeddingWorker`
/// (the mock model, standing in for a loaded backend) actually embeds and
/// persists vectors through the hosting loop, not just terminates jobs.
#[tokio::test]
async fn embedding_worker_with_a_real_model_indexes_files_and_writes_embeddings() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let ui_catalog = bootstrap::open_catalog(&context).unwrap();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, 3);
    let (card, _) = bootstrap::add_source(&ui_catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&ui_catalog, &card.source_id).unwrap();

    let loop_catalog = bootstrap::open_catalog(&context).unwrap();
    let loop_cache = bootstrap::cache_service(&context).unwrap();

    let model_id = {
        use orbok_db::repo::{ModelRepository, ModelRole, ModelStatus, NewModel};
        ModelRepository::new(&loop_catalog)
            .insert(NewModel {
                role: ModelRole::Embedding,
                model_name: "mock".to_string(),
                model_version: "v1".to_string(),
                local_path: None,
                license_summary: None,
                size_bytes: None,
                backend: Some("mock".to_string()),
                dimension: Some(8),
                status: ModelStatus::Available,
            })
            .unwrap()
            .model_id
    };
    let embedding_parts = Some(EmbeddingWorkerParts::for_test(
        Box::new(MockEmbeddingModel),
        model_id.clone(),
    ));
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(
        loop_catalog,
        loop_cache,
        embedding_parts,
        tx,
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
    assert!(
        embedding_rows > 0,
        "a real EmbeddingWorker run must persist at least one embedding row"
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
    let handle = tokio::spawn(run_with_context(loop_catalog, loop_cache, None, tx));
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

async fn index_via_background_loop(
    context: &RuntimeContext,
    ui_catalog: &Catalog,
    file_count: u64,
) {
    let loop_catalog = bootstrap::open_catalog(context).unwrap();
    let loop_cache = bootstrap::cache_service(context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(loop_catalog, loop_cache, None, tx));
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
#[tokio::test]
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
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(run_with_context(loop_catalog, loop_cache, None, tx));
    drop(rx); // never drained in tests: sends must fail-fast, not block (see report_health's comment)

    let search_catalog = bootstrap::open_catalog(&context).unwrap();
    let mut latencies = Vec::new();
    let overall_start = Instant::now();
    let sampling = tokio::time::timeout(Duration::from_secs(90), async {
        while indexed_count(&ui_catalog) < 300 {
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
        "300 files indexed with concurrent search sampling every 10ms: total {overall:?} \
         ({} search samples, avg {avg:?}, max {max:?} per search)",
        latencies.len()
    );
    assert!(
        sampling.is_ok(),
        "300 files did not finish indexing within 90s while a concurrent search ran every 10ms -- \
         see the no-concurrency baseline for comparison; report this per HANDOFF §3.2"
    );
}
