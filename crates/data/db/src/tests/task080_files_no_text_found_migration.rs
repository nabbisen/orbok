//! Task 080 (RFC-062 §5 pattern): migration 0009 widens `files.file_status`'s
//! CHECK to admit `'no_text_found'`, the same table-rewrite migration 0007
//! used for `index_jobs.status`.

use crate::migrations;

const MIGRATION_SQL: &[&str] = &[
    include_str!("../../migrations/0001_baseline.sql"),
    include_str!("../../migrations/0002_trigram_index.sql"),
    include_str!("../../migrations/0003_scheduler.sql"),
    include_str!("../../migrations/0004_search_history.sql"),
    include_str!("../../migrations/0005_keyword_rowid_indexes.sql"),
    include_str!("../../migrations/0006_managed_model_generations.sql"),
    include_str!("../../migrations/0007_index_jobs_status_check.sql"),
    include_str!("../../migrations/0008_chunk_location_kind.sql"),
];

/// A catalog frozen at exactly version 8 -- every migration through
/// `chunk_location_kind` applied and recorded, `no_text_found` migration
/// (9) not yet run. Reproduces the state every profile upgraded from a
/// pre-Task-080 release will actually be in.
fn build_v8_catalog(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
    )
    .unwrap();
    for (i, sql) in MIGRATION_SQL.iter().enumerate() {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) \
             VALUES (?1, 'v8-fixture', '2026-01-01T00:00:00Z')",
            [(i + 1) as i64],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, \
         created_at, updated_at) VALUES ('s','directory','persistent','/d','/d','active', \
         'balanced','exclude','ignore','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
}

fn try_insert_no_text_found(path: &std::path::Path) -> Result<(), rusqlite::Error> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('probe-file','s','/d/f.pdf','/d/f.pdf','f.pdf',1,'no_text_found', \
         '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .map(|_| ())
}

/// Observed failing first: a catalog at exactly version 8 rejects
/// `file_status = 'no_text_found'` -- the CHECK really was narrow before
/// this migration, so accepting it afterward is migration 9's doing, not
/// something that was already true.
#[test]
fn a_v8_catalog_rejects_no_text_found_before_migrating() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite3");
    build_v8_catalog(&path);

    let before = try_insert_no_text_found(&path);
    assert!(
        before.is_err(),
        "a v8 catalog must reject file_status='no_text_found' before migration 9 runs"
    );
    let message = before.unwrap_err().to_string();
    assert!(
        message.to_lowercase().contains("check"),
        "the rejection must be the CHECK constraint, got: {message}"
    );
}

/// The same catalog, migrated to the latest version, accepts the new
/// status -- and every earlier status is unaffected (a plain
/// INSERT...SELECT, no column added or removed).
#[test]
fn after_migrating_the_same_catalog_accepts_no_text_found() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite3");
    build_v8_catalog(&path);

    let catalog = crate::Catalog::open(path.clone()).unwrap();
    assert_eq!(
        catalog.schema_version().unwrap(),
        migrations::latest_version()
    );
    drop(catalog);

    let after = try_insert_no_text_found(&path);
    assert!(
        after.is_ok(),
        "after migrating, file_status='no_text_found' must be accepted: {after:?}"
    );

    // An ordinary status must still be accepted too -- the rewrite did not
    // narrow anything.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('probe-file-2','s','/d/g.md','/d/g.md','g.md',1,'indexed', \
         '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
}
