//! Task 034 §6 (audit P-02): `get_health` materialized and sorted the
//! entire queued-job table just to call `.len()` on it, once after every
//! completed job. Asserted by relative timing, at a fixed 20,000-row scale:
//! a `COUNT(*)` over `idx_index_jobs_status` is still O(matching rows) in
//! SQLite (there is no O(1) row-count fast path for a filtered count), so
//! the correct claim is not "constant regardless of scale" -- it is
//! "substantially cheaper than materializing and sorting every matching
//! row into a `JobRecord`", which is what `list_queued(..).len()` did.

use crate::bootstrap::get_health;
use orbok_core::JobStatus;
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;
use std::time::Instant;

fn seed_queued_jobs(catalog: &Catalog, n: usize) {
    let mut conn = catalog.lock();
    let tx = conn.transaction().unwrap();
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO index_jobs \
                 (job_id, job_type, status, created_at, updated_at) \
                 VALUES (?1, 'extract', 'queued', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            )
            .unwrap();
        for i in 0..n {
            stmt.execute([format!("job-{i}")]).unwrap();
        }
    }
    tx.commit().unwrap();
}

fn best_of<F: FnMut() -> R, R>(mut f: F, n: u32) -> std::time::Duration {
    let _ = f(); // warm up (query-plan/page-cache costs unrelated to this)
    let mut best = std::time::Duration::MAX;
    for _ in 0..n {
        let start = Instant::now();
        let _ = f();
        best = best.min(start.elapsed());
    }
    best
}

/// `get_health` (via `count_with_status`, a `COUNT(*)` over
/// `idx_index_jobs_status`) must be substantially cheaper than the old
/// `list_queued(u32::MAX).len()` shape -- materializing and sorting every
/// matching row into a `JobRecord` -- at the same 20,000-row scale.
/// Confirmed failing before landing `count_with_status`: `get_health`
/// itself (then backed by `list_queued(..).len()`) measured ~20ms at this
/// scale, comparable to `list_queued` measured directly below.
#[test]
fn count_with_status_is_substantially_faster_than_materializing_every_row() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed_queued_jobs(&catalog, 20_000);
    let jobs = IndexJobRepository::new(&catalog);

    let count_elapsed = best_of(|| jobs.count_with_status(JobStatus::Queued).unwrap(), 5);
    let materialize_elapsed = best_of(|| jobs.list_queued(u32::MAX).unwrap().len(), 5);

    assert!(
        count_elapsed.as_micros() * 3 < materialize_elapsed.as_micros(),
        "count_with_status ({count_elapsed:?}) must be at least 3x faster \
         than list_queued(u32::MAX).len() ({materialize_elapsed:?}) at the \
         same 20,000-row scale -- get_health must use the former"
    );
}

/// `get_health` itself must actually be fast at this scale, not just the
/// underlying query in isolation -- confirms the wiring, not only the
/// repository method.
#[test]
fn get_health_is_fast_with_many_queued_jobs() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed_queued_jobs(&catalog, 20_000);
    let elapsed = best_of(|| get_health(&catalog), 5);
    assert!(
        elapsed.as_millis() < 5,
        "get_health with 20,000 queued jobs took {elapsed:?}, expected a \
         COUNT(*)-backed query to stay well under 5ms"
    );
}

/// Sanity check that the query is still counting the right thing, not just
/// fast because it counts nothing: `queued` reflects exactly what was
/// seeded, and only `queued` rows (a `succeeded` row must not be counted).
#[test]
fn get_health_queued_count_is_correct() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed_queued_jobs(&catalog, 7);
    {
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO index_jobs (job_id, job_type, status, created_at, updated_at) \
             VALUES ('done-1', 'extract', 'succeeded', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    let health = get_health(&catalog);
    assert_eq!(health.queued, 7);
}
