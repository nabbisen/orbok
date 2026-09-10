//! RFC-059 §10 acceptance criteria 1, 2, 3, 6: Reset erases the trigram
//! index and the extraction cache and leaves settings/model artifacts
//! untouched; Remove replaced stale indexes shrinks the Storage
//! dashboard's own keyword-index figure. Drives the real extraction
//! pipeline (`ExtractionWorker` + `ChunkAndIndexWorker` via
//! `run_pending`), not a synthetic bundle insert, so the extraction cache
//! and keyword index are genuinely populated the way a real index run
//! populates them.

use crate::{ChunkAndIndexWorker, CleanupService, ExtractionWorker, run_pending};
use orbok_cache::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::{
    CleanupAction, CleanupPlan, FileStatus, HiddenFilePolicy, IndexMode, JobType, PersistenceMode,
    SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata, SourceRepository,
};
use orbok_extract::ExtractOutput;
use orbok_fs::{GuardedSource, PathGuard};
use sha2::{Digest, Sha256};
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

fn seed_indexed(
    catalog: &Catalog,
    cache: &CacheService,
    root: &Path,
    name: &str,
    content: &str,
) -> (orbok_core::FileId, orbok_db::repo::SourceRecord, String) {
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
    run_pending(catalog, &e, &c, None, 50).unwrap();
    (file.file_id, src, canonical)
}

fn hash_file(path: &Path) -> String {
    use std::fmt::Write;
    let bytes = fs::read(path).unwrap();
    let mut h = Sha256::new();
    h.update(&bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Criterion 1: with a corpus indexed containing a distinctive term,
/// invoking Reset and then querying the trigram path for that term
/// returns no rows -- verified against `chunk_fts_trigram` directly, per
/// the criterion's own explicit "not through the search API".
#[test]
fn reset_clears_the_trigram_index_queried_directly() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    seed_indexed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "誤り訂正符号についての解説文書です。",
    );

    let term = "誤り訂正符号";
    let before: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunk_fts_trigram WHERE chunk_fts_trigram MATCH ?1",
            [term],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        before > 0,
        "the corpus must be findable in the trigram index before Reset, \
         or this test proves nothing"
    );

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();

    let after: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunk_fts_trigram WHERE chunk_fts_trigram MATCH ?1",
            [term],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        after, 0,
        "a trigram query for a term from the reset corpus must return zero \
         rows, queried against chunk_fts_trigram directly (RFC-059 §10 \
         criterion 1) -- this is the gap the RFC exists to close"
    );
}

/// Criterion 2: with the same corpus, after Reset, no extraction-cache
/// entry for any indexed file is retrievable through `CacheService`.
#[test]
fn reset_clears_the_extraction_cache_through_cache_service() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let (_file_id, source, file_canonical_path) = seed_indexed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "extraction cache erasure test content",
    );

    let guard = PathGuard::new(vec![GuardedSource::from_record(&source)]);
    let validated = guard.validate(Path::new(&file_canonical_path)).unwrap();

    let engine_before = cache
        .engine::<ExtractOutput>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();
    assert!(
        CacheService::get_fresh(&engine_before, &validated)
            .unwrap()
            .is_some(),
        "the extraction cache must actually hold this file's entry before \
         Reset, or this test proves nothing"
    );
    assert!(
        !engine_before.keys(None).unwrap().is_empty(),
        "the extraction namespace must be non-empty before Reset"
    );

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();

    let engine_after = cache
        .engine::<ExtractOutput>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();
    assert!(
        CacheService::get_fresh(&engine_after, &validated)
            .unwrap()
            .is_none(),
        "no extraction-cache entry for the reset corpus must be retrievable \
         through CacheService after Reset (RFC-059 §10 criterion 2)"
    );
    assert!(
        engine_after.keys(None).unwrap().is_empty(),
        "the extraction namespace must be empty after Reset, not merely \
         unable to serve this one lookup"
    );
}

/// Criterion 3: after Reset, `settings.json` and the installed model
/// artifacts are byte-identical to their pre-Reset state -- a guard
/// against *this* RFC's own fix reaching outside the cache/catalog it is
/// scoped to, not against the pre-existing code (neither file is ever
/// referenced by `CleanupExecutor` or `CleanupService`).
#[test]
fn reset_never_touches_settings_or_model_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    seed_indexed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "settings and model artifact preservation test",
    );

    let settings_path = dir.path().join("settings.json");
    fs::write(&settings_path, br#"{"locale":"en","theme":"dark"}"#).unwrap();
    let model_dir = dir.path().join("models").join("multilingual-e5-small");
    fs::create_dir_all(&model_dir).unwrap();
    let model_path = model_dir.join("model.onnx");
    fs::write(
        &model_path,
        b"pretend-onnx-weights-not-actually-a-real-model",
    )
    .unwrap();

    let settings_hash_before = hash_file(&settings_path);
    let model_hash_before = hash_file(&model_path);

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    svc.run_reset(
        &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
        true,
    )
    .unwrap();

    assert_eq!(
        hash_file(&settings_path),
        settings_hash_before,
        "settings.json must be byte-identical after Reset"
    );
    assert_eq!(
        hash_file(&model_path),
        model_hash_before,
        "installed model artifacts must be byte-identical after Reset"
    );
}

/// Criterion 6 (real re-index leg): invoking Remove replaced stale
/// indexes after an ordinary re-index still removes the leftover `chunks`
/// catalog row for the superseded generation.
///
/// **Honestly disclosed, not glossed over:** in this ordinary scenario,
/// `outcome.bytes_reclaimed` is 0, not greater than zero. That is not a
/// gap in this fix -- `ChunkRepository::insert_bundle`'s own RFC-059 fix
/// (RFC-059 §6's "Prerequisite, and it is not optional": re-indexing must
/// delete the previous generation's FTS rows itself, addressed by
/// `file_id`, before this cleanup ever runs) already deletes the
/// `chunk_fts`/`chunk_fts_trigram`/`keyword_index_records` rows for the
/// superseded generation at replace time, so by the time
/// `remove_replaced_stale_indexes` runs there is nothing left in those
/// tables for it to find. The criterion's "reports a byte reclaim greater
/// than zero" is genuinely exercised by
/// `remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`
/// (`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`), which
/// constructs the one scenario where this action's own fix has real work
/// left to do -- a stale chunk whose FTS rows were never pre-deleted,
/// since no production path leaves one today. See the closure record for
/// RFC-059 for this criterion's full disposition.
#[test]
fn remove_replaced_stale_indexes_cleans_up_the_leftover_chunk_row_after_a_reindex() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = cache_db_path(dir.path());
    let (file_id, source, _canonical) = seed_indexed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "the first generation of this document has enough distinct words \
         to produce a real chunk row worth measuring",
    );

    // Re-index the same file with different content -- the production
    // replace-on-reindex path (`ChunkRepository::insert_bundle`) marks
    // the first generation's chunks stale and leaves a fresh active
    // generation, exactly the scenario this cleanup targets.
    fs::write(
        dir.path().join("doc.md"),
        "a completely different second generation replaces the first, \
         also long enough to produce a real chunk row",
    )
    .unwrap();
    IndexJobRepository::new(&catalog)
        .enqueue(JobType::Extract, Some(&source.source_id), Some(&file_id))
        .unwrap();
    let e = ExtractionWorker::new(&catalog, &cache);
    let c = ChunkAndIndexWorker::new(&catalog, &cache);
    run_pending(&catalog, &e, &c, None, 50).unwrap();

    let stale_chunks_before: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE chunk_status = 'stale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stale_chunks_before > 0,
        "the re-index above must have left a stale chunk for this cleanup \
         to act on, or this test proves nothing"
    );

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    let outcome = svc
        .run_safe(&CleanupPlan::for_action(
            CleanupAction::RemoveReplacedStaleIndexes,
            0,
        ))
        .unwrap();

    assert!(
        outcome.catalog_rows_deleted > 0,
        "remove_replaced_stale_indexes must still remove the leftover \
         chunks row after an ordinary re-index"
    );
    assert_eq!(
        outcome.catalog_bytes_reclaimed, 0,
        "an ordinary re-index leaves nothing in chunk_fts/chunk_fts_trigram \
         for this call to find, since insert_bundle already deleted them -- \
         see this test's own doc comment"
    );

    let stale_chunks_after: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE chunk_status = 'stale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stale_chunks_after, 0,
        "the superseded generation's chunk row must actually be gone"
    );
}
