//! Task 145 §7 (Review 323 §3): the sidebar-health query's regression guard
//! was `get_health_is_fast_with_many_queued_jobs`, asserting `best-of-5 <
//! 5ms` over 20,000 queued jobs. That is wall-clock time standing in for
//! "the query does not scan every row" -- a contaminated proxy, not the
//! property itself: under load, the real property can hold (the index is
//! still used) while the measured time crosses 5ms anyway, which is exactly
//! what made this test fail on CI's shared runners without the query
//! actually regressing. Replaced with a direct assertion on `EXPLAIN QUERY
//! PLAN`, in the same style as `task055_backfill_cost`'s
//! `embedding_backfill_uses_the_file_and_chunk_indexes`.

use crate::Catalog;
use crate::repo::jobs::COUNT_WITH_STATUS_SQL;

fn plan_detail(catalog: &Catalog, sql: &str) -> String {
    let conn = catalog.lock();
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let plan: Vec<String> = stmt
        .query_map(["queued"], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    plan.join("\n")
}

/// The property Task 034 §6 actually wants: `count_with_status`'s query
/// reaches its rows through `idx_index_jobs_status`, never a full scan of
/// `index_jobs` -- regardless of how busy the machine running the test is.
#[test]
fn count_with_status_query_uses_the_status_index_not_a_full_scan() {
    let catalog = Catalog::open_in_memory().unwrap();
    let plan = plan_detail(&catalog, COUNT_WITH_STATUS_SQL);
    assert!(
        plan.contains("idx_index_jobs_status"),
        "expected idx_index_jobs_status in plan:\n{plan}"
    );
    assert!(
        !plan.contains("SCAN TABLE index_jobs") && !plan.contains("SCAN index_jobs"),
        "a full scan of index_jobs makes the health query cost time in \
         proportion to the whole queue again:\n{plan}"
    );
}

/// Self-test (same shape as this project's `scripts/check-*.test.sh`
/// gutted-gate cases): proves the assertion above is not vacuous by forcing
/// the identical query to scan, with SQLite's own `NOT INDEXED` clause, and
/// confirming the plan then does say so. If this ever stopped seeing a scan
/// here, the assertions in the test above would not be testing anything.
#[test]
fn the_same_query_without_its_index_does_scan() {
    let catalog = Catalog::open_in_memory().unwrap();
    let forced_scan_sql = COUNT_WITH_STATUS_SQL.replacen(
        "FROM index_jobs WHERE",
        "FROM index_jobs NOT INDEXED WHERE",
        1,
    );
    let plan = plan_detail(&catalog, &forced_scan_sql);
    assert!(
        plan.contains("SCAN TABLE index_jobs") || plan.contains("SCAN index_jobs"),
        "NOT INDEXED was expected to force a full scan, so the test above's \
         absence check is meaningful:\n{plan}"
    );
}
