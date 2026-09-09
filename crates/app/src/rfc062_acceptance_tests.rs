//! RFC-062 §8 acceptance criterion 2: on a catalog upgraded from 0.16.0,
//! toggling background indexing off actually pauses indexing, observable
//! as jobs ceasing to be dispatched -- not merely that `Scheduler::pause`
//! returns `Ok`, but that a real queued job's status actually moves out of
//! `queued`, the state `run_with_context`'s dispatch loop selects from.
//!
//! Criterion 1 (a 0.16.0 catalog rejects `status='paused'` before the
//! upgrade and accepts it after) lives in `orbok-db`'s own test suite
//! (`crates/data/db/src/tests/rfc062_migration_integrity.rs`), next to the
//! migration it verifies. This criterion needs `Scheduler::pause` too
//! (`orbok-workers`), which is why it lives here instead.

use orbok_core::{JobStatus, JobType};
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;
use orbok_workers::Scheduler;

/// `crates/data/db/migrations/0001_baseline.sql`'s own file content --
/// after RFC-062 §5 step 2 restored it to its released 0.16.0 text, this
/// constant *is* that text, the same reuse `orbok-db`'s own criterion-1
/// test makes of the identical file.
const BASELINE_0_16_0_SQL: &str = include_str!("../../data/db/migrations/0001_baseline.sql");

fn build_0_16_0_frozen_catalog(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
    )
    .unwrap();
    conn.execute_batch(BASELINE_0_16_0_SQL).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, applied_at) \
         VALUES (1, 'baseline', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
}

#[test]
fn criterion_2_pausing_background_indexing_actually_pauses_an_upgraded_0_16_0_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("frozen.sqlite3");
    build_0_16_0_frozen_catalog(&path);

    // The real upgrade path -- opening through `Catalog::open` runs every
    // pending migration, including `0007`'s repair.
    let catalog = Catalog::open(&path).unwrap();

    let job_id = IndexJobRepository::new(&catalog)
        .enqueue(JobType::Scan, None, None)
        .unwrap();
    assert_eq!(
        IndexJobRepository::new(&catalog)
            .count_with_status(JobStatus::Queued)
            .unwrap(),
        1,
        "the job must start out queued -- otherwise pausing it proves nothing"
    );

    let mut scheduler = Scheduler::with_defaults();
    scheduler.pause(&catalog).expect(
        "pause must succeed on an upgraded 0.16.0 catalog -- before RFC-062's \
             repair, this failed on the stored narrow CHECK and the failure was \
             silently swallowed by `scheduler_host.rs`'s own `let _ =` (fixed \
             separately, RFC-061 §8(a))",
    );

    assert_eq!(
        IndexJobRepository::new(&catalog)
            .count_with_status(JobStatus::Queued)
            .unwrap(),
        0,
        "the job must no longer be queued after pause -- the dispatch loop \
         (`run_with_context`) only ever selects 'queued' jobs, so this is what \
         'jobs ceasing to be dispatched' actually means"
    );
    assert_eq!(
        IndexJobRepository::new(&catalog)
            .count_with_status(JobStatus::Paused)
            .unwrap(),
        1,
        "the job must have actually moved to 'paused', not just left 'queued'"
    );
    let _ = job_id;
}
