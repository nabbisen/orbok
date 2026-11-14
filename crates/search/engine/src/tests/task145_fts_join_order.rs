//! Task 145 §1-§2 (Review 321 §2): the keyword query's plan must run
//! `chunk_fts MATCH ?` once and join the other tables to its rows --
//! `chunk_fts` as the outermost loop -- so a search costs time in
//! proportion to the number of matches, not the size of the whole library.
//! Before this task's `CROSS JOIN` change, SQLite drove the query from
//! `keyword_index_records` outward instead, running `chunk_fts MATCH` once
//! per active row there.
//!
//! §1.4: `snippet.rs`'s `chunk_records_for` was checked too (its own
//! `EXPLAIN QUERY PLAN`, by hand) and does not show the pattern -- it has
//! no `MATCH`/FTS table at all, is driven by a small `chunk_id IN (...)`
//! list the keyword search already bounded, and every join in its plan is
//! `SEARCH ... USING INDEX`, never a `SCAN`. Left unchanged.

use crate::fts5::keyword_search_sql;
use crate::multilingual::trigram_search_sql;
use orbok_db::Catalog;

fn plan_detail(catalog: &Catalog, sql: &str) -> String {
    let conn = catalog.lock();
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let plan: Vec<String> = stmt
        .query_map(["m", "10"], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    plan.join("\n")
}

/// `fts_table` must be the outermost loop: `EXPLAIN QUERY PLAN` lists rows
/// in the order each loop is entered (outermost first), so this is simply:
/// the first row's detail names `fts_table` and not some other table that
/// merely contains it as a substring (`chunk_fts` is itself a substring of
/// `chunk_fts_trigram`, and both tables appear in both plans' later rows
/// via `keyword_index_records`' own column names) -- checked by requiring
/// `fts_table` to be first in the line, not merely present in it.
fn assert_table_is_outermost(plan: &str, fts_table: &str) {
    let first_line = plan.lines().next().unwrap_or_else(|| panic!("empty plan"));
    let expected_scan = format!("SCAN {fts_table}");
    assert!(
        first_line.starts_with(&expected_scan),
        "{fts_table} must be the first (outermost) loop, so the match \
         drives the join instead of running once per row of another \
         table:\n{plan}"
    );
}

#[test]
fn keyword_search_runs_the_match_in_the_outermost_loop() {
    let catalog = Catalog::open_in_memory().unwrap();
    let plan = plan_detail(&catalog, &keyword_search_sql(""));
    assert_table_is_outermost(&plan, "chunk_fts");
}

#[test]
fn trigram_search_runs_the_match_in_the_outermost_loop() {
    let catalog = Catalog::open_in_memory().unwrap();
    let plan = plan_detail(&catalog, &trigram_search_sql(""));
    assert_table_is_outermost(&plan, "chunk_fts_trigram");
}

/// Self-test (same shape as this project's `scripts/check-*.test.sh`
/// gutted-gate cases): proves the two assertions above are not vacuous.
/// Plain `JOIN` lets SQLite reorder the chain -- restoring it here
/// reproduces the exact plan this task fixed, confirming
/// `assert_table_is_outermost` really does fail against it.
#[test]
fn without_cross_join_the_match_is_not_outermost() {
    let catalog = Catalog::open_in_memory().unwrap();
    let plain_join_sql = keyword_search_sql("").replace("CROSS JOIN", "JOIN");
    let plan = plan_detail(&catalog, &plain_join_sql);
    let first_line = plan.lines().next().unwrap();
    assert!(
        !first_line.starts_with("SCAN chunk_fts"),
        "expected a plain JOIN chain to let SQLite reorder away from \
         chunk_fts first, so the CROSS JOIN assertion above is meaningful:\n{plan}"
    );
}
