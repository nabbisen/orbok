//! Task 095: a reset gives the space back. Plain `DELETE` never shrinks a
//! SQLite file (RFC-059 §10 criterion 6); after a successful reset,
//! `CleanupService::compact_after_reset` compacts both the catalog and the
//! cache file via `VACUUM`, guarded by free space, and never lets a
//! compaction failure -- or a skip -- change the reset's own
//! already-decided outcome. Task 096 split this out of `run_reset` itself
//! (that method's own doc comment says why) -- every call site below now
//! calls both in sequence, the shape `main.rs`'s post-reset task uses in
//! production, just on the same connection here rather than a fresh one
//! (this file's own tests are about the guard and the file sizes, not the
//! threading -- `catalog::tests` in `orbok-db` covers the connection
//! half, and `wired_application_tests.rs` covers the threading).

use crate::cleanup_service::{compact_if_room, compaction_margin, has_room_to_compact};
use crate::{ChunkAndIndexWorker, CleanupService, ExtractionWorker, run_pending};
use orbok_cache::{CacheService, OrbokCacheNamespace};
use orbok_core::{
    CleanupAction, CleanupPlan, FileStatus, HiddenFilePolicy, IndexMode, JobType, OrbokError,
    PersistenceMode, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata, SourceRepository,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing_subscriber::fmt::MakeWriter;

fn setup(root: &Path) -> (Catalog, CacheService) {
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(root);
    (catalog, cache)
}

fn cache_db_path(root: &Path) -> std::path::PathBuf {
    root.join("orbok-cache.sqlite3")
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn seed_indexed(catalog: &Catalog, cache: &CacheService, root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    std::fs::write(&path, content).unwrap();
    let canonical = std::fs::canonicalize(&path)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let root_str = std::fs::canonicalize(root)
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
    run_pending(catalog, &e, &c, None, 50).unwrap();
}

/// Inflate the catalog with schema-valid file rows, fast -- one source,
/// `count` files, one transaction, padded columns. Real extraction content
/// isn't the point here (RFC-059's own erasure tests already drive the
/// real pipeline elsewhere in this crate); a catalog file large enough to
/// prove `VACUUM` actually shrinks it is.
fn seed_bulk_files(catalog: &Catalog, count: usize) {
    let source_id = SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::File,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some("bulk".into()),
            original_path: "/bulk".into(),
            canonical_path: "/bulk".into(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap()
        .source_id;

    let padding = "x".repeat(600);
    let mut conn = catalog.lock();
    let tx = conn.transaction().unwrap();
    for i in 0..count {
        tx.execute(
            "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
             display_path, extension, file_size_bytes, modified_at, platform_file_key, \
             content_hash, hash_algorithm, file_status, last_seen_at, last_scanned_at, \
             created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,'md',1024,'2026-01-01T00:00:00Z',NULL,?6,'sha256', \
             'indexed','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            rusqlite::params![
                format!("file_{i}_{padding}"),
                source_id.as_str(),
                format!("/bulk/file_{i}_{padding}.md"),
                format!("/bulk/file_{i}_{padding}.md"),
                format!("file_{i}.md"),
                format!("{padding}{i}"),
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

fn pseudo_random_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 32);
    let mut block = Sha256::digest(seed.to_le_bytes()).to_vec();
    while out.len() < len {
        block = Sha256::digest(&block).to_vec();
        out.extend_from_slice(&block);
    }
    out.truncate(len);
    out
}

/// Inflate the cache with real entries -- `localcache::CacheEngine::set`
/// canonicalizes its key, so each entry needs a real file on disk, unlike
/// the catalog's synthetic rows above.
fn seed_bulk_cache_entries(catalog: &Catalog, cache: &CacheService, root: &Path, count: usize) {
    let engine = cache
        .engine::<Vec<u8>>(
            catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )
        .unwrap();
    for i in 0..count {
        let path = root.join(format!("cache_seed_{i}.bin"));
        let payload = pseudo_random_bytes(i as u64, 4096);
        std::fs::write(&path, &payload).unwrap();
        engine.set(&path, &payload).unwrap();
    }
}

// ---------------------------------------------------------------------
// Log capture -- no log-capturing test infra exists elsewhere in this
// workspace, so this is a small self-contained subscriber writing into a
// shared buffer, torn down when `f` returns.
// ---------------------------------------------------------------------

#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for SharedBuf {
    type Writer = SharedBuf;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture_logs(f: impl FnOnce()) -> String {
    let buf = SharedBuf(Arc::new(Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.clone())
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
}

fn tiny_available_space(_path: &Path) -> std::io::Result<u64> {
    // Task 095 test 3's injection: nowhere near any real file's size plus
    // margin, so `has_room_to_compact` refuses regardless of what is
    // actually free on the machine running this test.
    Ok(1024)
}

fn ample_available_space(_path: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX / 2)
}

// ---------------------------------------------------------------------
// Test 1: the catalog file shrinks.
// ---------------------------------------------------------------------

#[test]
fn reset_shrinks_the_catalog_file() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let catalog_path = dir.path().join("catalog.sqlite3");

    seed_bulk_files(&catalog, 4000);
    drop(catalog.lock()); // release before stat-ing the file below

    let before = file_len(&catalog_path);

    // Task 096: `run_reset` no longer compacts (that would still be the
    // shared connection, held across a multi-second checkpoint on the
    // real update thread) -- `compact_after_reset` is now the caller's own
    // separate step, the same as `main.rs`'s post-reset task runs it, on
    // its own connection.
    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();
    svc.compact_after_reset();

    let after = file_len(&catalog_path);
    assert!(
        before > 512 * 1024,
        "the seeded catalog must be meaningfully bigger than empty before \
         reset, or this test proves nothing (was {before} bytes)"
    );
    assert!(
        after < before / 4,
        "reset must compact the catalog file, not just delete its rows: \
         before={before} after={after}"
    );
}

// ---------------------------------------------------------------------
// Test 2: the cache file shrinks, same shape.
// ---------------------------------------------------------------------

#[test]
fn reset_shrinks_the_cache_file() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());

    seed_bulk_cache_entries(&catalog, &cache, dir.path(), 400);

    let before = file_len(&cache_path);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();
    svc.compact_after_reset();

    let after = file_len(&cache_path);
    assert!(
        before > 512 * 1024,
        "the seeded cache must be meaningfully bigger than empty before \
         reset, or this test proves nothing (was {before} bytes)"
    );
    assert!(
        after < before / 4,
        "reset must compact the cache file, not just erase its entries: \
         before={before} after={after}"
    );
}

// ---------------------------------------------------------------------
// Test 3: a reset with no room still succeeds.
// ---------------------------------------------------------------------

#[test]
fn a_reset_with_no_room_still_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let catalog_path = dir.path().join("catalog.sqlite3");

    seed_bulk_files(&catalog, 4000);
    drop(catalog.lock());
    seed_bulk_cache_entries(&catalog, &cache, dir.path(), 400);

    let catalog_before = file_len(&catalog_path);
    let cache_before = file_len(&cache_path);

    let svc = CleanupService::new(&catalog, &cache, &cache_path)
        .with_available_space_reader(tiny_available_space);
    let outcome = svc
        .run_reset(
            &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
            true,
        )
        .expect("a reset must succeed even when there is no room to compact");
    svc.compact_after_reset();

    assert!(
        outcome.catalog_rows_deleted > 0,
        "the seeded rows must actually have been deleted"
    );
    let files_left: i64 = catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        files_left, 0,
        "the data must be gone regardless of compaction"
    );

    assert_eq!(
        file_len(&catalog_path),
        catalog_before,
        "the catalog file must be left alone when there is no room to compact"
    );
    assert_eq!(
        file_len(&cache_path),
        cache_before,
        "the cache file must be left alone when there is no room to compact"
    );
}

// ---------------------------------------------------------------------
// Test 3 (guard + log), and test 4 (a failed compaction is never a failed
// reset): both exercise `compact_if_room` directly -- the exact function
// `compact_after_reset` calls, on both files -- with injected closures, so
// neither needs a real disk near-full or a real corrupted file.
// ---------------------------------------------------------------------

#[test]
fn compact_if_room_skips_and_logs_when_space_is_not_enough() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.sqlite3");
    std::fs::write(&path, vec![0u8; 4096]).unwrap();

    let vacuum_called = Arc::new(Mutex::new(false));
    let vacuum_called_inner = Arc::clone(&vacuum_called);

    let log = capture_logs(|| {
        compact_if_room(
            "catalog",
            &path,
            |_p| tiny_available_space(&path),
            move || {
                *vacuum_called_inner.lock().unwrap() = true;
                Ok(())
            },
        );
    });

    assert!(
        !*vacuum_called.lock().unwrap(),
        "vacuum must not run when there is not enough free space"
    );
    assert!(
        log.contains("not enough free space"),
        "the skip must be logged; got: {log}"
    );
}

#[test]
fn compact_if_room_logs_a_warning_but_never_errors_when_vacuum_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.sqlite3");
    std::fs::write(&path, vec![0u8; 4096]).unwrap();

    let log = capture_logs(|| {
        // `()` return, not `Result` -- structurally cannot propagate this
        // failure to a caller, which is the property Task 095 §1.4 asks
        // for ("a failed compaction is never a failed reset").
        compact_if_room(
            "catalog",
            &path,
            |_p| ample_available_space(&path),
            || Err(OrbokError::Database("forced test failure".into())),
        );
    });

    assert!(
        log.contains("compaction failed"),
        "a failed compaction must be logged at warn!; got: {log}"
    );
}

// ---------------------------------------------------------------------
// Test 5: nothing is lost by compacting -- a fresh start after a reset
// that did compact still indexes and is still searchable.
// ---------------------------------------------------------------------

#[test]
fn a_reset_that_compacted_leaves_a_usable_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let catalog_path = dir.path().join("catalog.sqlite3");

    seed_bulk_files(&catalog, 4000);
    drop(catalog.lock());
    let before = file_len(&catalog_path);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();
    svc.compact_after_reset();

    assert!(
        file_len(&catalog_path) < before,
        "this test needs compaction to have actually happened, or it \
         proves nothing about a rebuilt file's usability"
    );

    seed_indexed(
        &catalog,
        &cache,
        dir.path(),
        "after-reset.md",
        "a rebuilt catalog can still find marlinquartz in fresh content.",
    );

    let hits: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunk_fts_trigram WHERE chunk_fts_trigram MATCH ?1",
            ["marlinquartz"],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        hits > 0,
        "indexing and search must both work against the VACUUMed file"
    );
}

// ---------------------------------------------------------------------
// The guard formula itself, directly.
// ---------------------------------------------------------------------

#[test]
fn has_room_to_compact_requires_the_file_size_plus_its_margin() {
    let file_size = 100_000_000u64; // 100 MB -> margin is 10% = 10 MB, clear of the 1 MiB floor
    let margin = compaction_margin(file_size);
    assert_eq!(margin, file_size / 10);

    assert!(
        !has_room_to_compact(file_size, file_size),
        "exactly the file size alone is not enough"
    );
    assert!(
        !has_room_to_compact(file_size + margin - 1, file_size),
        "one byte short of the margin must still refuse"
    );
    assert!(
        has_room_to_compact(file_size + margin, file_size),
        "file size plus the exact margin must be enough"
    );
}

#[test]
fn compaction_margin_has_a_floor_for_tiny_files() {
    assert_eq!(compaction_margin(0), 1024 * 1024);
    assert_eq!(compaction_margin(100), 1024 * 1024);
}

// ---------------------------------------------------------------------
// Definition-of-done measurement, not a gate -- `#[ignore]`d so CI never
// times it. Same scale as Task 092/Review 270 §5's own catalog VACUUM
// research (100,000 files), extended to also measure the cache file and
// the compaction this task adds. Run:
// `cargo test -p orbok-workers --release --lib task095_measure_real_reset_compaction -- --ignored --nocapture`
// ---------------------------------------------------------------------

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn task095_measure_real_reset_compaction() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let catalog_path = dir.path().join("catalog.sqlite3");

    seed_bulk_files(&catalog, 100_000);
    seed_bulk_cache_entries(&catalog, &cache, dir.path(), 5_000);
    drop(catalog.lock());

    let catalog_before = file_len(&catalog_path);
    let cache_before = file_len(&cache_path);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    // Task 096 §3: reported separately now that they are separate calls --
    // `run_reset` (the delete work, still synchronous on the real update
    // thread in production) versus `compact_after_reset` (moved off it).
    let delete_start = Instant::now();
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();
    let delete_elapsed = delete_start.elapsed();

    let compact_start = Instant::now();
    svc.compact_after_reset();
    let compact_elapsed = compact_start.elapsed();

    let catalog_after = file_len(&catalog_path);
    let cache_after = file_len(&cache_path);

    println!(
        "catalog: {catalog_before} -> {catalog_after} bytes\n\
         cache:   {cache_before} -> {cache_after} bytes\n\
         run_reset (deletes only, stays on the update thread) took {delete_elapsed:?}\n\
         compact_after_reset (both files, now off it) took {compact_elapsed:?}"
    );
}
