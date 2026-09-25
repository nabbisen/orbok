//! Task 116 (RFC-062 pattern): migration 0011 rewrites every stored timestamp to
//! the fixed width, keeps order, leaves NULLs alone, and changes nothing the
//! second time; migration 0012 adds the scan numbers.

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
    include_str!("../../migrations/0010_source_covers_subfolders.sql"),
];
const FIXED_WIDTH_SQL: &str = include_str!("../../migrations/0011_fixed_width_timestamps.sql");

/// Old-style values, as `time`'s RFC 3339 wrote them: no fraction, a short
/// fraction, a longer one. `SHORT` is the *earlier* moment and `LONGER` the later
/// one, yet as text `SHORT` sorts after `LONGER` -- the defect this task fixes.
const NO_FRACTION: &str = "2026-09-24T15:26:29Z";
const SHORT: &str = "2026-09-24T15:26:29.1234Z";
const LONGER: &str = "2026-09-24T15:26:29.123456Z";
const FULL: &str = "2026-09-24T15:26:29.123456789Z";

/// A profile frozen at exactly version 10 with mixed-width values in several
/// columns of several tables (the same values in each, so one check covers all).
fn build_v10_catalog(path: &std::path::Path) {
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
        // Applied at old-format instants of their own.
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, 'v10-fixture', ?2)",
            rusqlite::params![(i + 1) as i64, if i % 2 == 0 { NO_FRACTION } else { SHORT }],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at, last_scanned_at) VALUES ('s','directory','persistent','/d','/d','active', \
         'balanced','exclude','ignore', ?1, ?2, NULL)",
        [NO_FRACTION, LONGER],
    )
    .unwrap();
    // `modified_at` is what change detection compares for equality.
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, modified_at, file_status, last_seen_at, last_scanned_at, \
         last_indexed_at, created_at, updated_at) VALUES ('f1','s','/d/a.md','/d/a.md','a.md', \
         1, ?1, 'indexed', ?2, NULL, NULL, ?3, ?4)",
        [SHORT, LONGER, FULL, NO_FRACTION],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, created_at, \
         updated_at, started_at, completed_at) VALUES ('j','s','f1','extract','succeeded', ?1, ?2, \
         ?3, NULL)",
        [SHORT, LONGER, NO_FRACTION],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
         extractor_version, normalization_version, status, started_at, completed_at, created_at, \
         updated_at) VALUES ('e','f1','x','1','1','succeeded', ?1, ?2, ?3, ?4)",
        [NO_FRACTION, SHORT, LONGER, FULL],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES ('k','1', ?1)",
        [SHORT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_events (event_id, event_type, severity, message, created_at) \
         VALUES ('ev','t','info','m', ?1)",
        [NO_FRACTION],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO storage_accounting (category, size_bytes, item_count, updated_at) \
         VALUES ('c', 1, 1, ?1)",
        [LONGER],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO search_history (id, search_text, filters_json, created_at, last_used_at) \
         VALUES ('h','q','[]', ?1, ?2)",
        [SHORT, NO_FRACTION],
    )
    .unwrap();
}

/// Every `*_at` column of every table, with its non-NULL values.
fn all_timestamps(conn: &rusqlite::Connection) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for table in tables {
        let columns: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .filter(|c: &String| c.ends_with("_at"))
            .collect();
        for column in columns {
            let values: Vec<Option<String>> = conn
                .prepare(&format!("SELECT {column} FROM {table} ORDER BY rowid"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            for value in values {
                out.push((table.clone(), column.clone(), value));
            }
        }
    }
    out
}

#[test]
fn migrating_a_version_10_profile_makes_every_timestamp_fixed_width() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite3");
    build_v10_catalog(&path);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let before = all_timestamps(&conn);
        assert!(
            before
                .iter()
                .any(|(_, _, v)| v.as_deref().is_some_and(|v| v.len() != 30)),
            "the fixture really holds mixed widths"
        );
        assert!(
            conn.prepare("SELECT scan_generation FROM sources").is_err(),
            "0012's column does not exist yet"
        );
    }

    let catalog = Catalog::open(&path).unwrap();
    assert_eq!(
        catalog.schema_version().unwrap(),
        migrations::latest_version()
    );

    let conn = catalog.lock();
    let after = all_timestamps(&conn);
    assert!(
        after.len() > 20,
        "the check covers many columns: {}",
        after.len()
    );
    for (table, column, value) in &after {
        if let Some(value) = value {
            assert_eq!(value.len(), 30, "{table}.{column} = {value}");
            assert!(value.ends_with('Z'), "{table}.{column} = {value}");
        }
    }
    // NULLs stay NULL.
    let null_of = |table: &str, column: &str| {
        after
            .iter()
            .filter(|(t, c, _)| t == table && c == column)
            .all(|(_, _, v)| v.is_none())
    };
    assert!(null_of("sources", "last_scanned_at"));
    assert!(null_of("files", "last_indexed_at"));
    assert!(null_of("index_jobs", "completed_at"));
    // The rewrite is the padding, nothing else.
    let file = |column: &str| -> String {
        conn.query_row(&format!("SELECT {column} FROM files"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(file("modified_at"), "2026-09-24T15:26:29.123400000Z");
    assert_eq!(file("last_seen_at"), "2026-09-24T15:26:29.123456000Z");
    assert_eq!(file("created_at"), "2026-09-24T15:26:29.123456789Z");
    assert_eq!(file("updated_at"), "2026-09-24T15:26:29.000000000Z");
    // Order is preserved -- and now correct: the moment the old text order got
    // wrong (`SHORT` is earlier than `LONGER`) sorts the right way round.
    assert!(file("modified_at") < file("last_seen_at"));
    let scan_generation: i64 = conn
        .query_row("SELECT scan_generation FROM sources", [], |r| r.get(0))
        .unwrap();
    assert_eq!(scan_generation, 0);
    let seen_generation: i64 = conn
        .query_row("SELECT seen_generation FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(seen_generation, 0);
}

/// A second run of the rewrite changes nothing: no value in any column moves.
#[test]
fn running_the_rewrite_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite3");
    build_v10_catalog(&path);
    let catalog = Catalog::open(&path).unwrap();
    let conn = catalog.lock();
    let once = all_timestamps(&conn);
    conn.execute_batch(FIXED_WIDTH_SQL).unwrap();
    assert_eq!(all_timestamps(&conn), once);
}
