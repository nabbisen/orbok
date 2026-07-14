//! Tests for orbok-db, validating the RFC-002 §12 testing requirements
//! and the RFC-001 cleanup invariants against the design specs.

mod rfc042_history;
mod rfc050_generations;

use crate::Catalog;
use crate::migrations;
use crate::repo::{
    CleanupExecutor, EventRepository, FileRepository, IndexJobRepository, NewFile, NewSource,
    ObservedMetadata, SettingsRepository, Severity, SourceRepository, StorageAccountingRepository,
};
use orbok_core::{
    CleanupAction, CleanupPlan, FileStatus, HiddenFilePolicy, IndexMode, JobStatus, JobType,
    PersistenceMode, SourceStatus, SourceType, StorageCategory, SymlinkPolicy,
};

fn new_source(path: &str) -> NewSource {
    NewSource {
        source_type: SourceType::Directory,
        persistence_mode: PersistenceMode::Persistent,
        display_name: Some("Test".into()),
        original_path: path.into(),
        canonical_path: path.into(),
        index_mode: IndexMode::Balanced,
        include_patterns: vec!["*.md".into()],
        exclude_patterns: vec![".git".into()],
        hidden_file_policy: HiddenFilePolicy::Exclude,
        symlink_policy: SymlinkPolicy::Ignore,
        max_file_size_bytes: Some(1024 * 1024),
    }
}

fn new_file(src: &orbok_core::SourceId, path: &str) -> NewFile {
    NewFile {
        source_id: src.clone(),
        original_path: path.into(),
        canonical_path: path.into(),
        display_path: path.into(),
        extension: Some("md".into()),
        metadata: ObservedMetadata {
            file_size_bytes: 10,
            modified_at: Some("2026-01-01T00:00:00Z".into()),
            platform_file_key: None,
            content_hash: Some("abc".into()),
        },
        status: FileStatus::Discovered,
    }
}

// RFC-002 §12: "Migration from empty database."
#[test]
fn migrations_apply_from_empty_and_are_idempotent() {
    let catalog = Catalog::open_in_memory().unwrap();
    assert_eq!(
        catalog.schema_version().unwrap(),
        migrations::latest_version()
    );
    // Re-running is a no-op.
    migrations::run_pending(&catalog).unwrap();
    assert_eq!(
        catalog.schema_version().unwrap(),
        migrations::latest_version()
    );
}

// RFC-007 §8.1 depends on FTS5 in the bundled SQLite build.
#[test]
fn fts5_virtual_table_is_available() {
    let catalog = Catalog::open_in_memory().unwrap();
    let conn = catalog.lock();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'chunk_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

// RFC-002 §12: "Foreign key enforcement."
#[test]
fn foreign_keys_are_enforced() {
    let catalog = Catalog::open_in_memory().unwrap();
    let conn = catalog.lock();
    let result = conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('f1','nonexistent','/a','/a','/a',1,'discovered','t','t','t')",
        [],
    );
    assert!(result.is_err(), "insert with dangling source_id must fail");
}

// RFC-002 §12: "Unique source path constraint" is per (source, path) on
// files; sources themselves may overlap by design.
#[test]
fn duplicate_file_path_within_source_rejected() {
    let catalog = Catalog::open_in_memory().unwrap();
    let src = SourceRepository::new(&catalog)
        .insert(new_source("/docs"))
        .unwrap();
    let files = FileRepository::new(&catalog);
    files
        .insert(new_file(&src.source_id, "/docs/a.md"))
        .unwrap();
    assert!(
        files
            .insert(new_file(&src.source_id, "/docs/a.md"))
            .is_err()
    );
}

// RFC-002 §12: "File status updates" + RFC-004 §11 missing-marking.
#[test]
fn file_status_transitions_and_missing_marking() {
    let catalog = Catalog::open_in_memory().unwrap();
    let src = SourceRepository::new(&catalog)
        .insert(new_source("/docs"))
        .unwrap();
    let files = FileRepository::new(&catalog);
    let f = files
        .insert(new_file(&src.source_id, "/docs/a.md"))
        .unwrap();
    assert_eq!(f.file_status, FileStatus::Discovered);

    files.set_status(&f.file_id, FileStatus::Indexed).unwrap();
    let got = files
        .get_by_path(&src.source_id, "/docs/a.md")
        .unwrap()
        .unwrap();
    assert_eq!(got.file_status, FileStatus::Indexed);

    // Unseen since a future cutoff -> missing, never deleted.
    let cutoff = "9999-01-01T00:00:00Z";
    let n = files.mark_missing_unseen(&src.source_id, cutoff).unwrap();
    assert_eq!(n, 1);
    let got = files
        .get_by_path(&src.source_id, "/docs/a.md")
        .unwrap()
        .unwrap();
    assert_eq!(got.file_status, FileStatus::Missing);

    // Idempotent: already-missing files are not re-marked.
    let n = files.mark_missing_unseen(&src.source_id, cutoff).unwrap();
    assert_eq!(n, 0);
}

// RFC-001 §13 test 1/2: safe cleanup preserves sources, removes caches.
#[test]
fn safe_cleanup_preserves_sources_and_settings() {
    let catalog = Catalog::open_in_memory().unwrap();
    let sources = SourceRepository::new(&catalog);
    let settings = SettingsRepository::new(&catalog);
    let src = sources.insert(new_source("/docs")).unwrap();
    settings.set("ui.locale", &"ja").unwrap();

    // Seed an expired snippet row.
    {
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO snippet_cache (snippet_id, snippet_text, created_at, \
             last_accessed_at, size_bytes) VALUES ('s1','x','t','t',1)",
            [],
        )
        .unwrap();
    }

    let cleanup = CleanupExecutor::new(&catalog);
    let plan = CleanupPlan::for_action(CleanupAction::ClearSnippetCache, 1);
    let outcome = cleanup.run_safe(&plan).unwrap();
    assert_eq!(outcome.deleted_rows, 1);

    assert!(sources.get(&src.source_id).unwrap().is_some());
    assert_eq!(settings.get::<String>("ui.locale").unwrap().unwrap(), "ja");
}

// RFC-001 §14: cleanup cannot run from a plan touching persistent data.
#[test]
fn safe_executor_rejects_reset_plan() {
    let catalog = Catalog::open_in_memory().unwrap();
    let cleanup = CleanupExecutor::new(&catalog);
    let plan = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    assert!(cleanup.run_safe(&plan).is_err());
}

// RFC-001 §13 test 4: reset catalog clears catalog rows (never source
// files — nothing here touches the filesystem at all). Settings may be
// preserved.
#[test]
fn reset_catalog_clears_rows_optionally_keeping_settings() {
    let catalog = Catalog::open_in_memory().unwrap();
    let sources = SourceRepository::new(&catalog);
    let settings = SettingsRepository::new(&catalog);
    sources.insert(new_source("/docs")).unwrap();
    settings.set("ui.locale", &"en").unwrap();

    let plan = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    CleanupExecutor::new(&catalog)
        .run_reset_catalog(&plan, true)
        .unwrap();

    assert!(sources.list().unwrap().is_empty());
    assert_eq!(settings.get::<String>("ui.locale").unwrap().unwrap(), "en");
}

// RFC-002 §12: source deletion cascades only where intended.
#[test]
fn source_delete_cascades_to_files() {
    let catalog = Catalog::open_in_memory().unwrap();
    let sources = SourceRepository::new(&catalog);
    let files = FileRepository::new(&catalog);
    let src = sources.insert(new_source("/docs")).unwrap();
    files
        .insert(new_file(&src.source_id, "/docs/a.md"))
        .unwrap();

    sources.delete_with_all_data(&src.source_id).unwrap();
    assert!(
        files
            .get_by_path(&src.source_id, "/docs/a.md")
            .unwrap()
            .is_none()
    );
}

#[test]
fn source_status_and_scan_touch() {
    let catalog = Catalog::open_in_memory().unwrap();
    let sources = SourceRepository::new(&catalog);
    let src = sources.insert(new_source("/docs")).unwrap();
    sources
        .set_status(&src.source_id, SourceStatus::Paused)
        .unwrap();
    assert_eq!(
        sources.get(&src.source_id).unwrap().unwrap().status,
        SourceStatus::Paused
    );
    assert!(sources.list_active().unwrap().is_empty());
    sources.touch_scanned(&src.source_id).unwrap();
    assert!(
        sources
            .get(&src.source_id)
            .unwrap()
            .unwrap()
            .last_scanned_at
            .is_some()
    );
}

#[test]
fn job_queue_round_trip() {
    let catalog = Catalog::open_in_memory().unwrap();
    let src = SourceRepository::new(&catalog)
        .insert(new_source("/docs"))
        .unwrap();
    let jobs = IndexJobRepository::new(&catalog);
    let id = jobs
        .enqueue(JobType::Extract, Some(&src.source_id), None)
        .unwrap();
    assert_eq!(jobs.list_queued(10).unwrap().len(), 1);
    jobs.set_status(&id, JobStatus::Running).unwrap();
    jobs.set_status(&id, JobStatus::Succeeded).unwrap();
    assert!(jobs.list_queued(10).unwrap().is_empty());
    let counts = jobs.count_by_status().unwrap();
    assert!(counts.contains(&(JobStatus::Succeeded, 1)));
}

// RFC-001 §10: storage accounting reports by category.
#[test]
fn storage_accounting_round_trip() {
    let catalog = Catalog::open_in_memory().unwrap();
    let storage = StorageAccountingRepository::new(&catalog);
    storage
        .upsert(StorageCategory::KeywordIndex, 2048, 12)
        .unwrap();
    storage
        .upsert(StorageCategory::KeywordIndex, 4096, 24)
        .unwrap();
    let rows = storage.all().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].size_bytes, 4096);
    assert_eq!(rows[0].item_count, 24);
}

#[test]
fn events_append_and_read() {
    let catalog = Catalog::open_in_memory().unwrap();
    let events = EventRepository::new(&catalog);
    events
        .append("scan_completed", Severity::Info, "scan ok", None)
        .unwrap();
    let recent = events.recent(5).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].0, "scan_completed");
}

// Settings privacy contract: typed values round-trip.
#[test]
fn settings_typed_round_trip() {
    let catalog = Catalog::open_in_memory().unwrap();
    let settings = SettingsRepository::new(&catalog);
    assert!(
        settings
            .get::<u64>("storage.cache_limit_bytes")
            .unwrap()
            .is_none()
    );
    settings
        .set("storage.cache_limit_bytes", &(8u64 * 1024 * 1024 * 1024))
        .unwrap();
    assert_eq!(
        settings
            .get::<u64>("storage.cache_limit_bytes")
            .unwrap()
            .unwrap(),
        8 * 1024 * 1024 * 1024
    );
}

// Persistence to disk (not just :memory:).
#[test]
fn catalog_persists_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("orbok-catalog.sqlite3");
    {
        let catalog = Catalog::open(&path).unwrap();
        SourceRepository::new(&catalog)
            .insert(new_source("/docs"))
            .unwrap();
    }
    let catalog = Catalog::open(&path).unwrap();
    assert_eq!(SourceRepository::new(&catalog).list().unwrap().len(), 1);
}
