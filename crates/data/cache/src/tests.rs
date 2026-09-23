//! Tests for orbok-cache, validating Appendix A acceptance criteria:
//! separate payload DB, freshness-checked reads, plan-driven cleanup
//! that cannot touch the catalog, engine registration, usage stats.

use crate::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::SourceId;
use orbok_core::{CleanupAction, CleanupPlan};
use orbok_db::{CACHE_FILE_NAME, CATALOG_FILE_NAME, Catalog};
use orbok_fs::ValidatedPath;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Segments {
    lines: Vec<String>,
}

fn validated(path: &std::path::Path) -> ValidatedPath {
    ValidatedPath {
        source_id: SourceId::generate(),
        canonical: fs::canonicalize(path).unwrap(),
    }
}

// Appendix A §3: payloads live in orbok-cache.sqlite3, not the catalog.
#[test]
fn payloads_live_in_separate_database() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());

    let file = dir.path().join("doc.md");
    fs::write(&file, "hello").unwrap();
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();
    CacheService::put(
        &engine,
        &validated(&file),
        &Segments {
            lines: vec!["hello".into()],
        },
    )
    .unwrap();

    assert!(dir.path().join(CACHE_FILE_NAME).exists());
    // The catalog contains a registration row but no payload tables.
    let conn = catalog.lock();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM cache_engines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

// Appendix A §8: freshness-checked read hits while unchanged, misses
// after modification (cache never serves stale payloads as fresh).
#[test]
fn get_fresh_misses_after_source_change() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();

    let file = dir.path().join("doc.md");
    fs::write(&file, "v1").unwrap();
    let path = validated(&file);
    let payload = Segments {
        lines: vec!["v1".into()],
    };
    CacheService::put(&engine, &path, &payload).unwrap();

    assert_eq!(
        CacheService::get_fresh(&engine, &path).unwrap(),
        Some(payload)
    );

    // Change the file: full-hash verification must reject the entry.
    fs::write(&file, "v2 with different size").unwrap();
    assert_eq!(CacheService::get_fresh(&engine, &path).unwrap(), None);
}

// Task 093: `namespaces_are_distinct_and_classed` removed -- it asserted
// `EmbeddingBundle`'s per-model namespace strings stayed distinct and
// compared its class against `PreviewCache`'s. Both variants are retired
// (neither ever had a producer; Review Request 270 §3, Review 270 §3),
// and `ExtractSegments` -- the only namespace left -- is not parameterized,
// so there is nothing left for this test to assert.

// RFC-001 §14 carried into the cache layer: destructive plans rejected;
// safe plans clean payloads while the catalog is untouched.
#[test]
fn cleanup_is_plan_driven_and_safe() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();

    // A payload whose source file disappears becomes orphaned.
    let file = dir.path().join("gone.md");
    fs::write(&file, "bye").unwrap();
    let path = validated(&file);
    CacheService::put(&engine, &path, &Segments { lines: vec![] }).unwrap();
    fs::remove_file(&file).unwrap();

    // Destructive plan: rejected before touching anything.
    let reset = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    assert!(service.run_safe_cleanup(&catalog, &reset).is_err());

    // Safe plan: orphaned entry removed.
    let plan = CleanupPlan::for_action(CleanupAction::ClearTemporaryExtraction, 0);
    let outcome = service.run_safe_cleanup(&catalog, &plan).unwrap();
    assert!(outcome.removed_entries >= 1);
}

// Appendix A §11: usage stats per namespace for storage accounting.
#[test]
fn usage_reports_entries_and_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();

    let file = dir.path().join("doc.md");
    fs::write(&file, "data").unwrap();
    CacheService::put(
        &engine,
        &validated(&file),
        &Segments {
            lines: vec!["data".into()],
        },
    )
    .unwrap();

    let usage = service
        .usage(&catalog, &[OrbokCacheNamespace::ExtractSegments])
        .unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].entries, 1);
    assert!(usage[0].payload_bytes > 0);
    assert_eq!(usage[0].namespace, "extract-segments:v2");
}

// Regression for the defect fixed in localcache 0.20.0 (schema v5):
// a file overwritten immediately — same byte length, different content —
// must be detected as stale. With second-precision mtimes this overwrite
// was invisible to metadata checks; nanosecond mtimes plus orbok's
// MetadataThenFullHash default catch it either way.
#[test]
fn same_size_immediate_overwrite_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();

    let file = dir.path().join("doc.md");
    fs::write(&file, "AAAA").unwrap();
    let path = validated(&file);
    let payload = Segments {
        lines: vec!["AAAA".into()],
    };
    CacheService::put(&engine, &path, &payload).unwrap();

    // Overwrite within the same instant: identical length, new content.
    fs::write(&file, "BBBB").unwrap();
    assert_eq!(
        CacheService::get_fresh(&engine, &path).unwrap(),
        None,
        "same-size immediate overwrite must invalidate the cached payload"
    );
}

// ── Task 079: retired namespaces ────────────────────────────────────────

/// Write one entry directly under a raw namespace string, bypassing
/// [`OrbokCacheNamespace`] entirely -- the same shape a namespace this
/// project no longer produces would have been written in, and the same
/// technique `purge_retired_namespaces`/`retired_namespace_usage` use to
/// address it back.
fn write_raw_entry(db_path: &std::path::Path, namespace: &str, path: &std::path::Path) {
    let engine = localcache::CacheEngine::<serde_json::Value>::builder()
        .database(db_path)
        .namespace(namespace.to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap();
    engine
        .set(path, &serde_json::json!({"stale": "payload"}))
        .unwrap();
}

fn raw_engine(
    db_path: &std::path::Path,
    namespace: &str,
) -> localcache::CacheEngine<serde_json::Value> {
    localcache::CacheEngine::<serde_json::Value>::builder()
        .database(db_path)
        .namespace(namespace.to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap()
}

/// Task 079 test 1, extended by Task 093: `purge_retired_namespaces`
/// deletes every entry under every retired namespace -- all three now
/// (`extract-segments:v1` from Task 077, `chunk-bundle:v1`/
/// `preview-cache:v1` from Task 093) -- and leaves the current, live
/// namespace untouched.
#[test]
fn purging_retired_namespaces_deletes_them_and_leaves_the_live_namespace_alone() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join(CATALOG_FILE_NAME)).unwrap();
    let service = CacheService::new(dir.path());
    let db_path = dir.path().join(CACHE_FILE_NAME);

    for (retired_namespace, file_name) in [
        ("extract-segments:v1", "old-extract.md"),
        ("chunk-bundle:v1", "old-chunk.md"),
        ("preview-cache:v1", "old-preview.md"),
    ] {
        let old_file = dir.path().join(file_name);
        fs::write(&old_file, "old").unwrap();
        let old_canonical = fs::canonicalize(&old_file).unwrap();
        write_raw_entry(&db_path, retired_namespace, &old_canonical);
    }

    let live_file = dir.path().join("live.md");
    fs::write(&live_file, "live").unwrap();
    let engine = service
        .engine::<Segments>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )
        .unwrap();
    let live_path = validated(&live_file);
    CacheService::put(
        &engine,
        &live_path,
        &Segments {
            lines: vec!["live".into()],
        },
    )
    .unwrap();

    let removed = service.purge_retired_namespaces().unwrap();
    assert_eq!(
        removed, 3,
        "exactly the three seeded entries must be removed"
    );

    for retired_namespace in ["extract-segments:v1", "chunk-bundle:v1", "preview-cache:v1"] {
        let raw = raw_engine(&db_path, retired_namespace);
        assert!(
            raw.keys(None).unwrap().is_empty(),
            "{retired_namespace} must be empty after purging"
        );
    }
    assert_eq!(
        CacheService::get_fresh(&engine, &live_path).unwrap(),
        Some(Segments {
            lines: vec!["live".into()]
        }),
        "the live namespace must be untouched"
    );
}

/// Task 079 test 3: `retired_namespace_usage` reports the retired
/// namespace's entries and bytes before a purge, and zero after.
#[test]
fn retired_namespace_usage_reports_bytes_before_purge_and_zero_after() {
    let dir = tempfile::tempdir().unwrap();
    let service = CacheService::new(dir.path());
    let db_path = dir.path().join(CACHE_FILE_NAME);

    for name in ["a.md", "b.md", "c.md"] {
        let file = dir.path().join(name);
        fs::write(&file, "stale payload text").unwrap();
        write_raw_entry(
            &db_path,
            "extract-segments:v1",
            &fs::canonicalize(&file).unwrap(),
        );
    }

    // Task 093: `RETIRED_NAMESPACES` grew to three entries
    // (`chunk-bundle:v1`/`preview-cache:v1` retired alongside
    // `extract-segments:v1`); this test only seeds the first, so the other
    // two report zero entries throughout -- asserted below rather than
    // assumed.
    let before = service.retired_namespace_usage().unwrap();
    assert_eq!(before.len(), 3, "three retired namespaces are now listed");
    assert_eq!(before[0].namespace, "extract-segments:v1");
    assert_eq!(before[0].entries, 3);
    assert!(
        before[0].payload_bytes > 0,
        "three real payloads must report a non-zero byte total"
    );
    for unseeded in &before[1..] {
        assert_eq!(
            unseeded.entries, 0,
            "{} was never seeded in this test",
            unseeded.namespace
        );
    }

    service.purge_retired_namespaces().unwrap();

    let after = service.retired_namespace_usage().unwrap();
    assert_eq!(after[0].entries, 0);
    assert_eq!(after[0].payload_bytes, 0);
}

/// Task 079 test 4: a typo in `RETIRED_NAMESPACES` that names a namespace
/// this project still produces would delete live data on the next purge.
/// This asserts every retired string is distinct from every namespace
/// [`OrbokCacheNamespace`] can currently produce.
///
/// Task 093: `EmbeddingBundle`'s own prefix check is gone along with the
/// variant -- it is retired too (never had a producer), just not listed
/// in `RETIRED_NAMESPACES` itself, since no concrete `model_id`/
/// `vector_format` instance of it was ever written for this to address.
#[test]
fn retired_namespaces_are_never_a_live_namespace() {
    let live_fixed = [OrbokCacheNamespace::ExtractSegments.as_namespace()];
    for retired in crate::RETIRED_NAMESPACES {
        assert!(
            !live_fixed.iter().any(|live| live == retired),
            "{retired} is a live namespace -- purging it would delete current data"
        );
    }
}

/// Task 079 §1.3: is a one-time startup purge of a large retired
/// namespace cheap enough to run unconditionally? 20,000 entries, matching
/// `EXTRACTION_CACHE_CLEANUP_ENTRY_CAP` -- the same scale RFC-059's own
/// measurements use elsewhere in this project.
///
/// A measurement, not a gate -- `#[ignore]`d so CI never times it. Run:
/// `cargo test -p orbok-cache --release --lib task079_purge_cost -- --ignored --nocapture`
#[test]
#[ignore]
fn task079_purge_cost_at_20000_retired_rows() {
    use std::time::Instant;
    const ROWS: usize = 20_000;
    let dir = tempfile::tempdir().unwrap();
    let service = CacheService::new(dir.path());
    let db_path = dir.path().join(CACHE_FILE_NAME);

    let engine = localcache::CacheEngine::<serde_json::Value>::builder()
        .database(&db_path)
        .namespace("extract-segments:v1".to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap();
    let files_dir = dir.path().join("files");
    fs::create_dir_all(&files_dir).unwrap();
    for i in 0..ROWS {
        let file = files_dir.join(format!("f{i}.md"));
        fs::write(&file, "stale payload text, roughly realistic length here").unwrap();
        engine
            .set(&file, &serde_json::json!({"stale": "payload", "i": i}))
            .unwrap();
    }
    drop(engine);

    let start = Instant::now();
    let removed = service.purge_retired_namespaces().unwrap();
    let elapsed = start.elapsed();
    println!("{ROWS} retired rows: purge_retired_namespaces took {elapsed:?}, removed {removed}");
    assert_eq!(removed, ROWS as u64);
}
