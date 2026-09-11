//! RFC-059 §7/§10 acceptance criterion 5: the extraction cache now has a
//! finite lifetime (TTL + entry cap), where before Slice 3 it had neither
//! (`EngineOptions::default()` at every open site -- RFC-059 §1's own
//! verified table: "Extraction cache opened unbounded").

use crate::CleanupService;
use orbok_cache::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::{CleanupAction, CleanupPlan};
use orbok_db::Catalog;
use std::time::Duration;

fn setup(root: &std::path::Path) -> (Catalog, CacheService) {
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(root);
    (catalog, cache)
}

/// The namespace's registered configuration (`cache_engines`, RFC-002
/// §7.16) is what the storage dashboard and any future diagnostic reads
/// -- confirming it directly is a faster and more durable check than
/// waiting out a real TTL, and it is what actually distinguishes this
/// slice's fix from doing nothing: every open site used to register
/// `ttl_seconds = NULL, max_entries = NULL` for this namespace.
///
/// RFC-059 Amendment 1 §2a.2 (Review 213 §3): `max_entries` is registered
/// as **NULL** here, deliberately -- a write-time bound evicted entries
/// out from under a running indexing pipeline (a scan's extractions all
/// run before any chunk job reads them back, so the earliest files' text
/// was evicted before their own chunk jobs could use it). The entry cap
/// still exists; it moved to `OrbokCacheNamespace::cleanup_time_entry_cap`,
/// enforced only by the `ClearTemporaryExtraction` cleanup action, which
/// cannot run mid-pipeline.
#[test]
fn extract_segments_namespace_is_registered_with_a_ttl_but_no_write_time_cap() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());

    // Opening the engine is what registers it (CacheService::engine calls
    // register_engine on every open) -- the same call every real open
    // site in the pipeline makes.
    let _engine = cache
        .engine::<Vec<u8>>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )
        .unwrap();

    let (ttl_seconds, max_entries): (Option<i64>, Option<i64>) = catalog
        .lock()
        .query_row(
            "SELECT ttl_seconds, max_entries FROM cache_engines WHERE namespace = ?1",
            [OrbokCacheNamespace::ExtractSegments.as_namespace()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert!(
        ttl_seconds.is_some(),
        "ExtractSegments must be registered with a non-null ttl_seconds -- \
         RFC-059 §1's verified finding was that every open site registered NULL"
    );
    assert!(
        max_entries.is_none(),
        "ExtractSegments must NOT register a write-time max_entries -- \
         Amendment 1 §2a.2 withdrew it after Review 213 found it broke \
         indexing above the cap"
    );
    assert_eq!(
        OrbokCacheNamespace::ExtractSegments.cleanup_time_entry_cap(),
        Some(20_000),
        "the bound itself is unchanged -- only the enforcement point moved"
    );
}

/// Every namespace *other* than ExtractSegments must keep its existing,
/// unbounded default -- this slice bounds the one namespace RFC-059's
/// summary singles out ("holds the complete extracted text of every
/// document"), not every cache in the product.
#[test]
fn other_namespaces_are_unaffected() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());

    for ns in [
        OrbokCacheNamespace::ChunkBundle,
        OrbokCacheNamespace::PreviewCache,
    ] {
        let _engine = cache
            .engine::<Vec<u8>>(&catalog, &ns, ns.default_engine_options())
            .unwrap();
        let (ttl_seconds, max_entries): (Option<i64>, Option<i64>) = catalog
            .lock()
            .query_row(
                "SELECT ttl_seconds, max_entries FROM cache_engines WHERE namespace = ?1",
                [ns.as_namespace()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(ttl_seconds, None, "{:?} must stay unbounded", ns);
        assert_eq!(max_entries, None, "{:?} must stay unbounded", ns);
    }
}

/// The underlying mechanism criterion 5 depends on: `cleanup_expired`
/// (`localcache` 0.21.1) is a structural no-op whenever an engine's own
/// `ttl` is `None` -- confirmed by reading `maintenance.rs`'s own
/// `let Some(ttl) = self.ttl else { return Ok(0) }`. Before Slice 3, that
/// was true at every `ExtractSegments` open site, always -- RFC-059 §1's
/// own verified finding that "purge expired" could never match anything.
/// Exercised here with a short TTL rather than the real 90-day production
/// value (waiting 90 real days is not a test): the mechanism is identical
/// either way -- an engine's own configured TTL compared against each
/// row's stored `updated_at` -- so this proves the fix restores the
/// capability itself, not the specific duration.
#[test]
fn cleanup_expired_removes_entries_once_a_ttl_is_actually_set() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let short_ttl_options = EngineOptions {
        ttl: Some(Duration::from_millis(20)),
        max_entries: None,
    };
    let engine = cache
        .engine::<Vec<u8>>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            short_ttl_options,
        )
        .unwrap();

    let path = dir.path().join("doc.md");
    std::fs::write(&path, "content").unwrap();
    engine.set(&path, &b"payload".to_vec()).unwrap();
    assert_eq!(engine.entry_count().unwrap(), 1);

    std::thread::sleep(Duration::from_millis(60));

    let removed = engine.cleanup_expired().unwrap();
    assert_eq!(
        removed, 1,
        "cleanup_expired must actually remove an entry older than the \
         configured TTL -- before Slice 3 this always returned 0, for \
         every namespace, because no engine anywhere ever had a TTL set"
    );
    assert_eq!(engine.entry_count().unwrap(), 0);
}

/// End-to-end through the real cleanup action and the real (90-day)
/// production TTL, not a short one: `CleanupService::run_safe(ClearTemporaryExtraction)`
/// must report the reclaim and the entry must stop being retrievable,
/// matching criterion 5's own wording. Waiting 90 real days is not a
/// test, and `cleanup_expired` decides "expired" using the *calling*
/// engine's own configured TTL against each row's stored `updated_at`
/// (confirmed by reading `maintenance.rs`) -- so a short-TTL engine
/// opened separately from `run_safe`'s own (90-day) engine would never
/// observe what `run_safe` itself does. Backdating `updated_at` directly
/// -- a raw SQL write against localcache's own `files` table, the exact
/// column `cleanup_expired` reads -- lets the real 90-day engine
/// genuinely see the entry as expired without waiting for it.
#[test]
fn clear_temporary_extraction_reports_a_reclaim_once_entries_are_expired() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = dir.path().join("orbok-cache.sqlite3");

    {
        let engine = cache
            .engine::<Vec<u8>>(
                &catalog,
                &OrbokCacheNamespace::ExtractSegments,
                OrbokCacheNamespace::ExtractSegments.default_engine_options(),
            )
            .unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "content").unwrap();
        // A payload large enough, and incompressible enough, that its
        // removal moves the needle on `shrink_database`'s reclaimed byte
        // count. `CacheEngine` compresses payloads (`service.rs`'s
        // builder calls `.compress()`) -- a uniform byte pattern
        // compresses to a handful of bytes and defeats this assertion
        // regardless of how many logical bytes it represents; a
        // pseudo-random payload does not.
        let payload: Vec<u8> = (0..64 * 1024)
            .map(|i: u32| (i.wrapping_mul(2654435761) >> 24) as u8)
            .collect();
        engine.set(&path, &payload).unwrap();
    }

    // Backdate the entry's `updated_at` well past the 90-day TTL.
    let ninety_one_days_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 91 * 24 * 60 * 60;
    {
        let raw = rusqlite::Connection::open(&cache_path).unwrap();
        raw.execute(
            "UPDATE files SET updated_at = ?1 WHERE namespace = ?2",
            rusqlite::params![
                ninety_one_days_ago,
                OrbokCacheNamespace::ExtractSegments.as_namespace()
            ],
        )
        .unwrap();
    }

    let svc = CleanupService::new(&catalog, &cache, &cache_path);
    let outcome = svc
        .run_safe(&CleanupPlan::for_action(
            CleanupAction::ClearTemporaryExtraction,
            0,
        ))
        .unwrap();

    let engine_after = cache
        .engine::<Vec<u8>>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )
        .unwrap();
    assert_eq!(
        engine_after.entry_count().unwrap(),
        0,
        "an entry older than the configured TTL must be gone after Clear \
         temporary extraction (RFC-059 §10 criterion 5)"
    );
    assert!(
        outcome.cache_bytes_freed > 0,
        "Clear temporary extraction must report a non-zero byte reclaim \
         (RFC-059 §10 criterion 5) -- got {}",
        outcome.cache_bytes_freed
    );
}

/// RFC-059 Amendment 1 §2a.2 (Review 213 §3): the cleanup-time cap
/// mechanism itself, in isolation from the real 20,000 production value
/// (writing and evicting 20,000 real entries is not a unit test). Five
/// entries, a cap of three, staggered `last_accessed_at`/`updated_at` via
/// raw SQL (deterministic, not a sleep-ordered race) so eviction order is
/// unambiguous: the two least-recently-accessed must go, the three most
/// recent must survive.
#[test]
fn cleanup_time_cap_evicts_the_least_recently_accessed_entries_first() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let cache_path = dir.path().join("orbok-cache.sqlite3");
    let namespace = OrbokCacheNamespace::ExtractSegments;

    let engine = cache
        .engine::<Vec<u8>>(&catalog, &namespace, namespace.default_engine_options())
        .unwrap();
    let mut paths = Vec::new();
    for i in 0..5 {
        let path = dir.path().join(format!("doc-{i}.md"));
        std::fs::write(&path, format!("content {i}")).unwrap();
        engine.set(&path, &b"payload".to_vec()).unwrap();
        paths.push(path);
    }
    drop(engine);

    // Stagger access times: doc-0 oldest, doc-4 newest. `last_accessed_at`
    // is what `delete_lru_n`'s query orders by (`updated_at` only as a
    // tiebreaker), so set both explicitly rather than relying on write
    // order or real elapsed time.
    {
        let raw = rusqlite::Connection::open(&cache_path).unwrap();
        for (i, path) in paths.iter().enumerate() {
            let ts = i as i64;
            raw.execute(
                "UPDATE files SET last_accessed_at = ?1, updated_at = ?1 \
                 WHERE namespace = ?2 AND path = ?3",
                rusqlite::params![ts, namespace.as_namespace(), path.to_string_lossy()],
            )
            .unwrap();
        }
    }

    crate::cleanup_service::enforce_extract_segments_cleanup_cap(&cache_path, 3).unwrap();

    let remaining: Vec<String> = {
        let raw = rusqlite::Connection::open(&cache_path).unwrap();
        let mut stmt = raw
            .prepare("SELECT path FROM files WHERE namespace = ?1 ORDER BY path")
            .unwrap();
        stmt.query_map([namespace.as_namespace()], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(
        remaining.len(),
        3,
        "the cap must trim the namespace to exactly 3 entries, got {remaining:?}"
    );
    for evicted in &paths[0..2] {
        assert!(
            !remaining.contains(&evicted.to_string_lossy().to_string()),
            "doc-0 and doc-1 (the two oldest by last_accessed_at) must be evicted, \
             remaining: {remaining:?}"
        );
    }
    for kept in &paths[2..5] {
        assert!(
            remaining.contains(&kept.to_string_lossy().to_string()),
            "doc-2, doc-3, doc-4 (the three most recently accessed) must survive, \
             remaining: {remaining:?}"
        );
    }
}

/// RFC-059 §10 criterion 9 (Amendment 1): indexing a corpus larger than
/// any configured extraction-cache bound leaves every file with active
/// chunks -- no chunk job fails on a cache miss.
///
/// **Mutation-tested, not merely written**: with the fix in place (write-time
/// `max_entries: None`, `OrbokCacheNamespace::default_engine_options`),
/// this passes. Temporarily restoring a write-time
/// `max_entries: Some(3)` for `ExtractSegments` in that same function --
/// reproducing exactly the pre-Amendment-1 behaviour Review 213 found by
/// execution -- made this test fail with a chunk job left `failed` and a
/// non-zero failure count, confirming the mechanism this test guards is
/// real; restored afterward, byte-identical (`git diff` empty).
#[test]
fn indexing_above_any_cache_bound_leaves_every_file_with_active_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(dir.path());

    let canonical_root = std::fs::canonicalize(dir.path())
        .unwrap()
        .to_string_lossy()
        .to_string();
    let mut file_ids = Vec::new();
    for i in 0..5 {
        let name = format!("doc-{i}.md");
        let path = dir.path().join(&name);
        std::fs::write(&path, format!("distinct content for file number {i}")).unwrap();
        let canonical = std::fs::canonicalize(&path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        file_ids.push((name, canonical));
    }

    let src = orbok_db::repo::SourceRepository::new(&catalog)
        .insert(orbok_db::repo::NewSource {
            source_type: orbok_core::SourceType::Directory,
            persistence_mode: orbok_core::PersistenceMode::Persistent,
            display_name: None,
            original_path: canonical_root.clone(),
            canonical_path: canonical_root,
            index_mode: orbok_core::IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: orbok_core::HiddenFilePolicy::Exclude,
            symlink_policy: orbok_core::SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();

    let mut expected_file_ids = Vec::new();
    for (name, path) in &file_ids {
        let file = orbok_db::repo::FileRepository::new(&catalog)
            .insert(orbok_db::repo::NewFile {
                source_id: src.source_id.clone(),
                original_path: path.clone(),
                canonical_path: path.clone(),
                display_path: name.clone(),
                extension: Some("md".into()),
                metadata: orbok_db::repo::ObservedMetadata {
                    file_size_bytes: std::fs::metadata(path).unwrap().len(),
                    modified_at: Some("2026-01-01T00:00:00Z".into()),
                    platform_file_key: None,
                    content_hash: Some(format!("hash-{name}")),
                },
                status: orbok_core::FileStatus::Discovered,
            })
            .unwrap();
        orbok_db::repo::IndexJobRepository::new(&catalog)
            .enqueue(
                orbok_core::JobType::Extract,
                Some(&src.source_id),
                Some(&file.file_id),
            )
            .unwrap();
        expected_file_ids.push(file.file_id);
    }

    let extractor = crate::ExtractionWorker::new(&catalog, &cache);
    let chunker = crate::ChunkAndIndexWorker::new(&catalog, &cache);
    // No embedding model is available in this sandbox (no .onnx file
    // reachable, matching the established constraint for every other
    // model-dependent test in this project -- e.g. RFC013_MODEL_DIR).
    // `embed_worker: None` makes `run_pending` mark every Embedding job
    // `failed` with category `model_missing` (RFC-008 §15's own named,
    // *expected* terminal status for "no model configured" -- not a bug,
    // and not what this criterion is testing). This test verifies the
    // half of criterion 9 that is checkable without a real model: no
    // Extract or Chunk job fails on a cache miss, and every file gets
    // active chunks. The embedding half ("no embedding job completes
    // empty") is not exercised here, disclosed rather than fabricated --
    // the same gap RFC-058 Review Request 209 §3 left open for row 7.
    crate::run_pending(&catalog, &extractor, &chunker, None, 50).unwrap();

    // Excludes Embedding jobs' expected `model_missing` category (no model
    // configured in this sandbox, see the comment above) -- this counts
    // only Extract/Chunk failures, the ones a write-time entry cap caused.
    let failed_jobs: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs \
             WHERE status = 'failed' AND (error_category IS NULL OR error_category != 'model_missing')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        failed_jobs, 0,
        "no Extract or Chunk job may fail on an extraction-cache miss -- \
         the failure mode a write-time entry cap caused (RFC-059 Amendment \
         1 §2a.2)"
    );

    for file_id in &expected_file_ids {
        let active_chunks: i64 = catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE file_id = ?1 AND chunk_status = 'active'",
                rusqlite::params![file_id.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            active_chunks > 0,
            "file {file_id:?} must have at least one active chunk -- \
             every file in the corpus must be fully indexed regardless of \
             any configured extraction-cache bound"
        );
    }
}
