//! Task 114 (RFC-062 pattern): migration 0010 adds `sources.covers_subfolders`
//! with every existing folder set to cover its subfolders.

use crate::repo::SourceRepository;
use crate::{Catalog, migrations};

const MIGRATION_SQL: &[&str] = &[
    include_str!("../../migrations/0001_baseline.sql"),
    include_str!("../../migrations/0002_trigram_index.sql"),
    include_str!("../../migrations/0003_scheduler.sql"),
    include_str!("../../migrations/0004_search_history.sql"),
    include_str!("../../migrations/0005_keyword_rowid_indexes.sql"),
    include_str!("../../migrations/0006_managed_model_generations.sql"),
    include_str!("../../migrations/0007_index_jobs_status_check.sql"),
    include_str!("../../migrations/0008_chunk_location_kind.sql"),
    include_str!("../../migrations/0009_files_no_text_found_check.sql"),
];

/// A profile frozen at exactly version 9, with two folders and a file.
fn build_v9_catalog(path: &std::path::Path) {
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
             VALUES (?1, 'v9-fixture', '2026-01-01T00:00:00Z')",
            [(i + 1) as i64],
        )
        .unwrap();
    }
    for id in ["s1", "s2"] {
        conn.execute(
            "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
             canonical_path, status, index_mode, hidden_file_policy, symlink_policy, \
             created_at, updated_at) VALUES (?1,'directory','persistent','/d','/d','active', \
             'balanced','exclude','ignore','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('f1','s1','/d/a.md','/d/a.md','a.md',1,'indexed', \
         '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
}

/// An existing profile opens with every folder covering its subfolders, its
/// data untouched, and the column refuses anything but 0 or 1.
#[test]
fn an_existing_profile_opens_with_every_folder_covering_its_subfolders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite3");
    build_v9_catalog(&path);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert!(
            conn.prepare("SELECT covers_subfolders FROM sources")
                .is_err(),
            "the column does not exist before the migration"
        );
    }

    let catalog = Catalog::open(&path).unwrap();

    assert_eq!(
        catalog.schema_version().unwrap(),
        migrations::latest_version()
    );
    let sources = SourceRepository::new(&catalog).list().unwrap();
    assert_eq!(sources.len(), 2);
    assert!(
        sources.iter().all(|s| s.covers_subfolders),
        "every existing folder covers its subfolders"
    );
    let files: i64 = catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(files, 1, "nothing else changed");
    let bad = catalog.lock().execute(
        "UPDATE sources SET covers_subfolders = 2 WHERE source_id = 's1'",
        [],
    );
    assert!(bad.is_err(), "the CHECK admits only 0 and 1");
}
