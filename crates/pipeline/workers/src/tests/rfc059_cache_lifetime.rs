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
#[test]
fn extract_segments_namespace_is_registered_with_a_ttl_and_a_cap() {
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
        max_entries.is_some(),
        "ExtractSegments must be registered with a non-null max_entries"
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
