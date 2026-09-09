//! RFC-062 §8 acceptance criteria 1 and 3.
//!
//! Criterion 1: a catalog created by orbok 0.16.0, opened by the current
//! binary, reaches the latest schema version and then accepts
//! `status='paused'` -- observed rejecting it first, against a fixture
//! built from the 0.16.0-era schema.
//!
//! Criterion 3: `git diff 0.16.0 HEAD -- crates/data/db/migrations/0001_baseline.sql`
//! is empty -- a plain shell check (`scripts/check-migration-integrity.sh`'s
//! own byte-level comparison covers it directly at the file level), so not
//! repeated here as a Rust test; what *is* worth a Rust-level assertion is
//! that restoring 0001 to its narrow CHECK didn't quietly change what a
//! fresh install ends up with, which `migrations_apply_from_empty_and_are_idempotent`
//! (`tests.rs`) already covers by reaching `latest_version()` from empty.

use crate::Catalog;

/// `0001_baseline.sql`'s own file content, included a second time here
/// rather than duplicated by hand. After RFC-062 §5 step 2 restored that
/// file to its released 0.16.0 text, this constant *is* the 0.16.0
/// baseline -- not an approximation of it -- so applying it directly to a
/// bare connection reproduces a genuine 0.16.0-era catalog: no `0007`
/// (or anything after `0001`) has ever run against it, the exact
/// condition the RFC's own repair targets.
const BASELINE_0_16_0_SQL: &str = include_str!("../../migrations/0001_baseline.sql");

/// Writes a catalog file at `path` frozen at exactly the 0.16.0 schema:
/// `schema_migrations` records only version 1, matching what a real
/// 0.16.0 install actually wrote (0.16.0 shipped nothing past baseline --
/// `0002` through `0007` were all released later).
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

/// Attempts a raw `status='paused'` insert against whatever `index_jobs`
/// currently looks like at `path`, bypassing every repository -- the
/// point is to probe the stored CHECK constraint directly, not go through
/// application code that might itself validate first.
fn try_insert_paused_job(path: &std::path::Path) -> Result<(), rusqlite::Error> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO index_jobs (job_id, job_type, status, created_at, updated_at) \
         VALUES ('probe-job', 'scan', 'paused', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .map(|_| ())
}

#[test]
fn criterion_1_a_0_16_0_catalog_rejects_paused_before_upgrade_and_accepts_it_after() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("frozen.sqlite3");
    build_0_16_0_frozen_catalog(&path);

    // Observed failing first, against the real 0.16.0 schema text --
    // confirms the fixture actually reproduces the historical bug, not
    // just that some unrelated error was raised.
    let before = try_insert_paused_job(&path);
    assert!(
        before.is_err(),
        "a genuine 0.16.0-era catalog must reject status='paused' before \
         any upgrade -- if this passes, the fixture isn't reproducing the \
         narrow CHECK the RFC describes"
    );
    let message = before.unwrap_err().to_string();
    assert!(
        message.to_lowercase().contains("check"),
        "the rejection must be the CHECK constraint specifically, not some \
         other error -- got: {message}"
    );

    // The real upgrade path: opening the same file through the production
    // `Catalog::open` runs every pending migration in order, including
    // `0007` -- the repair.
    let catalog = Catalog::open(&path).unwrap();
    assert_eq!(
        catalog.schema_version().unwrap(),
        crate::migrations::latest_version(),
        "the frozen catalog must reach the latest schema version through \
         the normal upgrade path"
    );
    drop(catalog);

    let after = try_insert_paused_job(&path);
    assert!(
        after.is_ok(),
        "after upgrading through the real migration runner, status='paused' \
         must be accepted -- got: {:?}",
        after.err()
    );
}
