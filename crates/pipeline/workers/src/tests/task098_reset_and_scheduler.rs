//! Task 098: does a reset that commits while the background scheduler has
//! a job mid-flight ever leave an orphan, or stop the scheduler? Report-
//! first investigation (Review 275 §4) -- these two tests are the
//! deterministic reproduction named in the task's own §1 ("hold a job at
//! a known point, commit the reset, then let it proceed"), kept because
//! they would still mean something after any future fix: they pin the
//! two claims the review request's answer rests on.

use crate::{ChunkAndIndexWorker, CleanupService, ExtractionWorker, run_pending};
use orbok_cache::CacheService;
use orbok_core::{
    CleanupAction, CleanupPlan, ExtractionId, FileStatus, HiddenFilePolicy, IndexMode, JobType,
    PersistenceMode, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    ChunkRepository, ChunkSpec, FileRepository, IndexJobRepository, NewFile, NewSource,
    ObservedMetadata, SourceRepository,
};
use std::fs;
use std::path::Path;

fn setup(root: &Path) -> (Catalog, CacheService) {
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(root);
    (catalog, cache)
}

fn cache_db_path(root: &Path) -> std::path::PathBuf {
    root.join("orbok-cache.sqlite3")
}

/// Same shape as `rfc059_reset_erasure.rs`'s own `seed_indexed`, but stops
/// after exactly the Extract job (`limit: 1`) rather than running the
/// whole pipeline -- so the Chunk job it enqueues is left queued, and the
/// extraction it wrote is left as the one thing downstream work depends
/// on, both still present when the caller runs a reset next.
fn seed_and_extract_only(
    catalog: &Catalog,
    cache: &CacheService,
    root: &Path,
    name: &str,
    content: &str,
) -> orbok_core::FileId {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    let canonical = fs::canonicalize(&path)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let root_str = fs::canonicalize(root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let src = SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::File,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some(name.into()),
            original_path: canonical.clone(),
            canonical_path: root_str,
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();
    let file = FileRepository::new(catalog)
        .insert(NewFile {
            source_id: src.source_id.clone(),
            original_path: canonical.clone(),
            canonical_path: canonical.clone(),
            display_path: name.into(),
            extension: Some("md".into()),
            metadata: ObservedMetadata {
                file_size_bytes: content.len() as u64,
                modified_at: Some("2026-01-01T00:00:00Z".into()),
                platform_file_key: None,
                content_hash: Some("abc".into()),
            },
            status: FileStatus::Discovered,
        })
        .unwrap();
    IndexJobRepository::new(catalog)
        .enqueue(JobType::Extract, Some(&src.source_id), Some(&file.file_id))
        .unwrap();
    let e = ExtractionWorker::new(catalog, cache);
    let c = ChunkAndIndexWorker::new(catalog, cache);
    let ran = run_pending(catalog, &e, &c, None, 1).unwrap();
    assert_eq!(ran, 1, "control: exactly the Extract job must have run");
    file.file_id
}

fn table_count(catalog: &Catalog, table: &str) -> i64 {
    catalog
        .lock()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// Task 098 §1.1/§1.2, the "merely queued" case: once Extract has run, the
/// Chunk job it enqueued sits in `index_jobs` referencing the file and
/// extraction Reset is about to delete. Reset deletes `index_jobs` in the
/// very same transaction as everything else (`CleanupExecutor::run_reset_catalog`),
/// so the queued job cannot survive to race with anything -- it simply
/// stops existing alongside its own target rows. Confirms the scheduler's
/// next poll finds nothing, does no work, raises no error, and leaves
/// every downstream table empty.
#[test]
fn a_queued_chunk_job_cannot_outlive_the_reset_that_deletes_its_own_row() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());

    let file_id = seed_and_extract_only(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "content about zephyrgraph for task098 testing.",
    );

    let queued_chunk_jobs: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = 'chunk' AND status = 'queued' \
             AND file_id = ?1",
            [file_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        queued_chunk_jobs, 1,
        "control: the Chunk job Extract enqueues must still be queued"
    );
    assert_eq!(table_count(&catalog, "extraction_records"), 1);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();

    // The scheduler's own loop would poll for queued work next -- nothing
    // is left to find, and this must not error.
    let e = ExtractionWorker::new(&catalog, &cache);
    let c = ChunkAndIndexWorker::new(&catalog, &cache);
    let ran = run_pending(&catalog, &e, &c, None, 50).unwrap();
    assert_eq!(
        ran, 0,
        "no queued job survives a reset for the scheduler to run"
    );

    for table in [
        "sources",
        "files",
        "extraction_records",
        "chunks",
        "embeddings",
        "keyword_index_records",
        "index_jobs",
    ] {
        assert_eq!(
            table_count(&catalog, table),
            0,
            "{table} must be empty after reset"
        );
    }
}

/// Task 098 §1.1/§1.2/§1.5, the real race: a worker that already read the
/// file/extraction it needs -- the one gap the investigation found, since
/// no worker's read and write share a transaction -- attempting its write
/// *after* a reset has committed. Reproduced deterministically by holding
/// exactly what `ChunkAndIndexWorker::run` would hold at that point
/// (`file_id`, `extraction_id`) from before the reset, then calling the
/// same repository method its write step calls
/// (`ChunkRepository::insert_bundle`) after the reset, with those now-stale
/// IDs. `chunks.file_id`/`chunks.extraction_id` are both `ON DELETE
/// CASCADE` (`0001_baseline.sql`), so this must fail on the foreign-key
/// check, not silently succeed -- and RFC-006 §12's own transaction
/// guarantee (`insert_bundle`'s own doc comment: "a failure leaves the
/// previous active chunks untouched") means it must roll back cleanly,
/// leaving no partial row anywhere.
#[test]
fn a_write_attempted_after_the_reset_fails_clean_and_creates_no_orphan() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());

    let file_id = seed_and_extract_only(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "content about marlinquartz for task098 testing.",
    );
    let extraction_id: String = catalog
        .lock()
        .query_row(
            "SELECT extraction_id FROM extraction_records WHERE file_id = ?1",
            [file_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    let extraction_id = ExtractionId::from_string(extraction_id);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();
    assert_eq!(
        table_count(&catalog, "files"),
        0,
        "control: the reset must have already deleted the file this worker read earlier"
    );

    // The exact write `ChunkAndIndexWorker::run` would have issued next,
    // called directly with the stale, pre-reset IDs -- simulating a
    // worker thread that read before the reset and writes after it.
    let specs = vec![ChunkSpec {
        chunk_kind: "paragraph",
        chunk_ordinal: 0,
        heading_path: None,
        title: None,
        normalized_text: "content about marlinquartz for task098 testing.".into(),
        line_start: 1,
        line_end: 1,
        byte_start: Some(0),
        byte_end: Some(48),
        location_quality: "exact",
        location_kind: "lines",
        parent_idx: None,
    }];
    let result = ChunkRepository::new(&catalog).insert_bundle(&file_id, &extraction_id, &specs);

    // Confirmed directly, not assumed: `Err(Database("FOREIGN KEY
    // constraint failed"))` -- the same generic `OrbokError::Database`
    // shape any other DB error takes, which is exactly why
    // `scheduler_host.rs`'s catch-all handles it the same as any other
    // failed job (Task 098's own investigation, §1 below).
    match &result {
        Err(orbok_core::OrbokError::Database(message)) => {
            assert!(
                message.contains("FOREIGN KEY"),
                "expected a foreign-key failure, got a different Database error: {message}"
            );
        }
        other => panic!(
            "a write against a file the reset already deleted must fail with a \
             foreign-key error, not succeed silently or fail some other way -- got {other:?}"
        ),
    }
    for table in ["chunks", "chunk_locations", "keyword_index_records"] {
        assert_eq!(
            table_count(&catalog, table),
            0,
            "{table} must still be empty -- insert_bundle's own transaction must have \
             rolled back cleanly, not left a partial write"
        );
    }
}
