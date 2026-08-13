//! Scanner tests (RFC-004 §19): discovery, change detection, missing
//! marking and restoration, policy skips, cancellation, idempotency,
//! job queueing.

use crate::tests::common::{register_dir_source, register_dir_source_with, scan};
use crate::{ScanRequest, ScanSummary, Scanner};
use orbok_core::{FileStatus, HiddenFilePolicy, JobStatus, SourceId, SymlinkPolicy};
use orbok_db::Catalog;
use orbok_db::repo::{FileRepository, IndexJobRepository};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn status_count(counts: &[(FileStatus, u64)], status: FileStatus) -> u64 {
    counts
        .iter()
        .find(|(s, _)| *s == status)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

/// Task 021: `new_files_discovered_and_jobs_queued` has flaked twice on
/// macOS CI, cleared both times on rerun, with the failing assertion's
/// own log unrecoverable afterward (GitHub serves the latest attempt's
/// log under the superseded attempt's job id). This renders everything
/// needed to distinguish the task's hypotheses from a single failure, no
/// second run required:
///
/// - The full `ScanSummary`, not just whichever count the failing
///   assertion happened to check.
/// - A recursive listing of the temp root with each entry's type --
///   catches an unexpected extra/missing filesystem entry directly.
/// - The `files.canonical_path` this scan actually wrote to the catalog,
///   alongside the path the test itself computed via
///   `fs::canonicalize` -- catches a canonicalization mismatch (macOS's
///   `$TMPDIR` is a symlink into `/private/var/folders/...`, so the
///   scanner and the test disagreeing on which form to store is a live
///   hypothesis, not a hypothetical one).
fn diagnostic_context(
    catalog: &Catalog,
    source_id: &SourceId,
    root: &Path,
    summary: &ScanSummary,
) -> String {
    let mut ctx = String::new();
    let _ = writeln!(ctx, "ScanSummary: {summary:?}");

    let _ = writeln!(
        ctx,
        "temp root (as the test constructed it): {}",
        root.display()
    );
    match fs::canonicalize(root) {
        Ok(canonical) => {
            let _ = writeln!(
                ctx,
                "temp root (test-computed canonical form): {}",
                canonical.display()
            );
        }
        Err(error) => {
            let _ = writeln!(
                ctx,
                "temp root canonicalization failed in the test: {error}"
            );
        }
    }

    let _ = writeln!(ctx, "recursive directory listing:");
    list_dir_into(root, 1, &mut ctx);

    let _ = writeln!(
        ctx,
        "catalog `files` rows for this source (canonical_path as the scanner wrote it, display_path):"
    );
    let conn = catalog.lock();
    match conn.prepare("SELECT canonical_path, display_path FROM files WHERE source_id = ?1") {
        Ok(mut stmt) => {
            let rows = stmt.query_map([source_id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            });
            match rows {
                Ok(rows) => {
                    for row in rows.flatten() {
                        let _ = writeln!(ctx, "  canonical_path={} display_path={}", row.0, row.1);
                    }
                }
                Err(error) => {
                    let _ = writeln!(ctx, "  <query_map failed: {error}>");
                }
            }
        }
        Err(error) => {
            let _ = writeln!(ctx, "  <prepare failed: {error}>");
        }
    }

    ctx
}

fn list_dir_into(dir: &Path, depth: usize, ctx: &mut String) {
    let Ok(entries) = fs::read_dir(dir) else {
        let _ = writeln!(ctx, "{}<unreadable: {}>", "  ".repeat(depth), dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = if path.is_symlink() {
            "symlink"
        } else if path.is_dir() {
            "dir"
        } else if path.is_file() {
            "file"
        } else {
            "other"
        };
        let _ = writeln!(ctx, "{}{} [{kind}]", "  ".repeat(depth), path.display());
        if path.is_dir() && !path.is_symlink() {
            list_dir_into(&path, depth + 1, ctx);
        }
    }
}

// RFC-004 §19 test 1: scan empty source.
#[test]
fn empty_source_scans_clean() {
    let root = tempfile::tempdir().unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());

    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.seen_files, 0);
    assert_eq!(summary.new_files, 0);
    assert_eq!(summary.missing_files, 0);
    assert!(!summary.canceled);
}

// RFC-004 §19 test 2: new files discovered and recorded; §19 test 12:
// extract jobs queued for new files.
#[test]
fn new_files_discovered_and_jobs_queued() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.md"), "# A").unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    fs::write(root.path().join("sub/b.txt"), "B").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());

    let summary = scan(&catalog, &source.source_id);
    // Task 021: captured once, right after the scan, and reused for
    // every assertion below -- nothing mutates the catalog or the temp
    // root between here and the end of the test, so one snapshot
    // describes the state every assertion below is checking.
    let ctx = diagnostic_context(&catalog, &source.source_id, root.path(), &summary);
    assert_eq!(summary.new_files, 2, "{ctx}");
    assert_eq!(summary.queued_index_jobs, 2, "{ctx}");

    let files = FileRepository::new(&catalog);
    let counts = files.count_by_status(&source.source_id).unwrap();
    assert_eq!(status_count(&counts, FileStatus::Discovered), 2, "{ctx}");

    // Records carry hash + display path.
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let expected_path = canonical_root.join("a.md");
    let rec = files
        .get_by_path(&source.source_id, &expected_path.to_string_lossy())
        .unwrap_or_else(|error| panic!("get_by_path query failed: {error}\n{ctx}"))
        .unwrap_or_else(|| {
            panic!(
                "get_by_path returned None for {}\n{ctx}",
                expected_path.display()
            )
        });
    assert!(rec.content_hash.is_some(), "{ctx}");
    assert_eq!(rec.display_path, "a.md", "{ctx}");

    let jobs = IndexJobRepository::new(&catalog);
    assert_eq!(jobs.list_queued(10).unwrap().len(), 2, "{ctx}");
    assert!(
        jobs.count_by_status()
            .unwrap()
            .contains(&(JobStatus::Queued, 2)),
        "{ctx}"
    );
}

// RFC-004 §19 test 3 + 11: rescan without changes is a no-op
// (idempotent); unchanged via metadata fast path.
#[test]
fn unchanged_rescan_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.md"), "# A").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());

    scan(&catalog, &source.source_id);
    let second = scan(&catalog, &source.source_id);
    assert_eq!(second.new_files, 0);
    assert_eq!(second.unchanged_files, 1);
    assert_eq!(second.stale_files, 0);
    assert_eq!(second.missing_files, 0);
    // No duplicate rows.
    let counts = FileRepository::new(&catalog)
        .count_by_status(&source.source_id)
        .unwrap();
    let total: u64 = counts.iter().map(|(_, n)| n).sum();
    assert_eq!(total, 1);
}

// RFC-004 §19 test 4: modified file detected as stale and re-queued.
#[test]
fn modified_file_marked_stale() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.md");
    fs::write(&path, "v1").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    scan(&catalog, &source.source_id);

    // Pretend the first version was indexed.
    let files = FileRepository::new(&catalog);
    let canonical = fs::canonicalize(&path).unwrap();
    let rec = files
        .get_by_path(&source.source_id, &canonical.to_string_lossy())
        .unwrap()
        .unwrap();
    files.set_status(&rec.file_id, FileStatus::Indexed).unwrap();

    // Modify content (size change guarantees the fast check trips).
    fs::write(&path, "version two, longer").unwrap();
    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.stale_files, 1);
    assert!(summary.queued_index_jobs >= 1);

    let rec = files
        .get_by_path(&source.source_id, &canonical.to_string_lossy())
        .unwrap()
        .unwrap();
    assert_eq!(rec.file_status, FileStatus::Stale);
}

// Same size + same content but touched mtime: hash confirms unchanged
// (RFC-004 §9.1/§9.2 hash confirmation step).
#[test]
fn touched_but_identical_content_stays_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.md");
    fs::write(&path, "same").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    scan(&catalog, &source.source_id);

    // Force a metadata difference with identical content.
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    let file = fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(future).unwrap();
    drop(file);

    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.stale_files, 0);
    assert_eq!(summary.unchanged_files, 1);
}

// RFC-004 §19 tests 5 + 6: deleted file -> missing (never deleted);
// restored file -> back from missing.
#[test]
fn deleted_then_restored_file_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.md");
    fs::write(&path, "content").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    scan(&catalog, &source.source_id);

    fs::remove_file(&path).unwrap();
    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.missing_files, 1);

    let files = FileRepository::new(&catalog);
    let canonical_root = fs::canonicalize(root.path()).unwrap();
    let key = canonical_root.join("a.md");
    let rec = files
        .get_by_path(&source.source_id, &key.to_string_lossy())
        .unwrap()
        .unwrap();
    assert_eq!(rec.file_status, FileStatus::Missing);

    // Restore identical content: record leaves missing without re-queue
    // when the hash still matches — scanner treats it as a change check.
    fs::write(&path, "content").unwrap();
    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.missing_files, 0);
    let rec = files
        .get_by_path(&source.source_id, &key.to_string_lossy())
        .unwrap()
        .unwrap();
    assert_ne!(rec.file_status, FileStatus::Missing);
}

// RFC-004 §19 test 7: hidden files excluded under default policy; test
// 8: excluded directories not descended.
#[test]
fn hidden_and_excluded_components_skipped() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".hidden.md"), "h").unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    fs::write(root.path().join(".git/config.md"), "g").unwrap();
    fs::create_dir(root.path().join("node_modules")).unwrap();
    fs::write(root.path().join("node_modules/pkg.md"), "n").unwrap();
    fs::write(root.path().join("visible.md"), "v").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    let summary = scan(&catalog, &source.source_id);

    assert_eq!(summary.seen_files, 1);
    assert_eq!(summary.new_files, 1);
}

// Hidden policy Include admits dotfiles.
#[test]
fn hidden_policy_include_admits_dotfiles() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".notes.md"), "h").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source_with(
        &catalog,
        root.path(),
        HiddenFilePolicy::Include,
        SymlinkPolicy::Ignore,
    );
    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.new_files, 1);
}

// RFC-004 §19 test 9: symlinks ignored under default policy.
#[cfg(unix)]
#[test]
fn symlinks_ignored_by_default() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("target.md"), "t").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("target.md"),
        root.path().join("link.md"),
    )
    .unwrap();
    fs::write(root.path().join("real.md"), "r").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.new_files, 1, "only the real file");
}

// RFC-004 §19 test 10: max file size respected.
#[test]
fn oversized_files_skipped() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("small.md"), "ok").unwrap();
    fs::write(root.path().join("big.md"), vec![b'x'; 4096]).unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let mut source = register_dir_source(&catalog, root.path());
    // Tighten the limit directly in the catalog row.
    {
        let conn = catalog.lock();
        conn.execute(
            "UPDATE sources SET max_file_size_bytes = 1024 WHERE source_id = ?1",
            [source.source_id.as_str()],
        )
        .unwrap();
    }
    source.max_file_size_bytes = Some(1024);

    let summary = scan(&catalog, &source.source_id);
    assert_eq!(summary.new_files, 1);
}

// Unsupported types cataloged as unsupported, not failed (RFC-004 §10).
#[test]
fn unsupported_types_cataloged() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("photo.jpg"), [0xFFu8, 0xD8]).unwrap();
    fs::write(root.path().join("doc.md"), "ok").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    let summary = scan(&catalog, &source.source_id);

    assert_eq!(summary.new_files, 1);
    assert_eq!(summary.unsupported_files, 1);
    let counts = FileRepository::new(&catalog)
        .count_by_status(&source.source_id)
        .unwrap();
    assert_eq!(status_count(&counts, FileStatus::Unsupported), 1);
    // No extract job for the unsupported file.
    assert_eq!(summary.queued_index_jobs, 1);
}

// RFC-004 §19 test 13: cancel mid-scan leaves a valid catalog and does
// not mark unseen files missing.
#[test]
fn cancellation_leaves_catalog_valid() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..20 {
        fs::write(root.path().join(format!("f{i}.md")), format!("{i}")).unwrap();
    }
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());

    // First, a full scan so files exist.
    scan(&catalog, &source.source_id);

    // Cancel immediately: nothing is touched, nothing marked missing.
    let scanner = Scanner::new(&catalog);
    let cancel = AtomicBool::new(true);
    let summary = scanner
        .scan(
            &ScanRequest {
                source_id: source.source_id.clone(),
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &cancel,
        )
        .unwrap();
    assert!(summary.canceled);
    assert_eq!(summary.missing_files, 0);

    let counts = FileRepository::new(&catalog)
        .count_by_status(&source.source_id)
        .unwrap();
    assert_eq!(status_count(&counts, FileStatus::Missing), 0);
}
