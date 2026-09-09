//! RFC-061 §10: acceptance-criteria verification for the failure-visibility
//! work (§8/Slice 3). Each test names the criterion it covers and fails
//! under the exact mutation the criterion describes ("deliberately failing
//! X..."), not merely under an unrelated error.
//!
//! Criteria 1, 4, 5, 6, and 9 are not covered here: 1 (no double-processing)
//! and 5 (window stays responsive during a large scan) are properties of
//! the scheduler's existing dispatch/enqueue design predating this handoff
//! (`scan_and_index_source` only ever enqueues a `Scan` job -- the walk and
//! hashing already run inside the hosted scheduler's own background task,
//! never on the update thread, so criterion 5 was never this handoff's to
//! satisfy); 4 (unreadable data directory at startup) fires before `iced`'s
//! event loop exists, so there is no UI to show a notice in yet -- only the
//! `error!` log half applies, and every early bootstrap failure already
//! propagates as `Err` out of `main` rather than being swallowed; 6 and 9
//! are the RFC-048 p99 measurement itself, covered by
//! `scheduler_host/tests.rs`'s own measurement test, not an acceptance
//! assertion.

use crate::bootstrap;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use orbok_core::{JobStatus, JobType};
use orbok_db::repo::IndexJobRepository;
use orbok_workers::Scheduler;
use std::path::Path;

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

/// Criterion 3: "With a catalog file made read-only, invoking Reset catalog
/// produces a user-visible notice; today it silently does nothing."
///
/// A real read-only-*file* reproduction is unreliable here: `catalog` and
/// `blocker` below open the same file at different points, but a
/// permission bit set via `chmod` after a connection is already open does
/// not retroactively block writes through that connection's existing file
/// descriptor on Linux/macOS (permissions are checked at `open()`, not on
/// each `write()`). A held write lock reproduces the same class of failure
/// -- a write that cannot complete -- deterministically and portably: this
/// is the same `SQLITE_BUSY`-after-`busy_timeout` path RFC-061 Slice 1's
/// `busy_timeout` targets, and it is what an actually-read-only file would
/// also eventually surface as (`SQLITE_READONLY`), just via a different
/// SQLite error code reaching the same `OrbokResult::Err`.
#[test]
fn criterion_3_reset_catalog_surfaces_a_write_failure_instead_of_silently_doing_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();

    let catalog_path = temp.path().join(orbok_db::CATALOG_FILE_NAME);
    let blocker = orbok_db::Catalog::open(&catalog_path).unwrap();
    blocker.lock().execute_batch("BEGIN IMMEDIATE;").unwrap();

    let result = bootstrap::reset_catalog(&catalog, &cache);
    drop(blocker); // release the write lock regardless of the assertion below

    assert!(
        result.is_err(),
        "reset_catalog must surface a write failure (here: sustained catalog \
         contention past busy_timeout) rather than silently doing nothing -- \
         `main.rs`'s ConfirmResetCatalog handler maps this Err onto \
         UserNotice::CatalogResetFailed"
    );
}

/// Criterion 7: "Deliberately failing a `scheduler.complete` (test hook)
/// leaves the job in `known`, does not re-run the work, and retries the
/// write on the next tick."
///
/// `known` itself is `scheduler_host::run_with_context`'s own in-memory
/// bookkeeping, not reachable from a unit test without driving the whole
/// hosted loop under a precisely-timed lock (race-prone, not deterministic
/// -- rejected for that reason). What this test verifies directly, without
/// that race, is the contract `run_with_context`'s `Ok(()) => match
/// scheduler.complete(...) { ... }` arm depends on for "leaves the job in
/// known and retries on the next tick" to be a safe thing to do at all:
/// that a failed `complete` genuinely does not record success (so a
/// same-job retry is not a silent no-op), and that retrying the identical
/// call once the catalog is available again succeeds and correctly
/// persists completion -- "the next tick" is exactly a second call to
/// `complete` with the same `job_id`.
#[test]
fn criterion_7_failed_scheduler_complete_does_not_lose_the_job_and_retries_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let job_id = IndexJobRepository::new(&catalog)
        .enqueue(JobType::Scan, None, None)
        .unwrap();
    let mut scheduler = Scheduler::with_defaults();

    let catalog_path = temp.path().join(orbok_db::CATALOG_FILE_NAME);
    let blocker = orbok_db::Catalog::open(&catalog_path).unwrap();
    blocker.lock().execute_batch("BEGIN IMMEDIATE;").unwrap();

    let first_attempt = scheduler.complete(&job_id, &catalog);
    assert!(
        first_attempt.is_err(),
        "a contended completion write must surface as Err, not silently succeed \
         -- the live-lock RFC-061 §8(a) fixed happened precisely because this \
         used to be swallowed by a `let _ =`"
    );
    assert_eq!(
        IndexJobRepository::new(&catalog)
            .count_with_status(JobStatus::Succeeded)
            .unwrap(),
        0,
        "a failed completion write must not have recorded success anyway"
    );

    drop(blocker); // "the next tick": the catalog is available again

    let retry = scheduler.complete(&job_id, &catalog);
    assert!(
        retry.is_ok(),
        "retrying the identical completion once the catalog is available again \
         must succeed"
    );
    assert_eq!(
        IndexJobRepository::new(&catalog)
            .count_with_status(JobStatus::Succeeded)
            .unwrap(),
        1,
        "the retry must actually persist completion, not just return Ok with no effect"
    );
}

/// Criterion 8: "Clicking Clear snippets with the cache path unavailable
/// produces a notice and the process survives."
///
/// `ProfileCache::new` never touches the filesystem (the localcache
/// database opens lazily, inside `run_safe_cleanup`), so occupying the
/// cache database's path with a directory before that first real access
/// reproduces "the cache path is unavailable" deterministically: SQLite
/// cannot open a directory as a database file.
#[test]
fn criterion_8_clean_snippets_surfaces_an_error_when_the_cache_path_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();

    let cache_db_path = temp.path().join(orbok_db::CACHE_FILE_NAME);
    std::fs::create_dir_all(&cache_db_path).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();

    let result = bootstrap::clean_snippets(&catalog, &cache);

    assert!(
        result.is_err(),
        "clean_snippets must surface an error, not silently no-op, when the cache \
         path is unavailable -- `main.rs`'s CleanSnippets handler maps this Err \
         onto UserNotice::StorageUnavailable, and (by the absence of any \
         `.unwrap()`/`.expect()` on that path) the process itself does not panic"
    );
}
