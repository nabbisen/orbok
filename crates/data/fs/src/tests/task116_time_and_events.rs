//! Task 116: whether a file was seen by **this scan** is an event, not a time.
//! Both tests drive the scan's clock (`orbok_core::timeutil::with_clock`, the
//! seam this task adds) so a failure that used to depend on the platform's
//! clock resolution happens, or does not, on every platform.

use crate::tests::common::{register_dir_source, scan};
use orbok_core::FileStatus;
use orbok_core::timeutil::with_clock;
use orbok_db::Catalog;
use orbok_db::repo::FileRepository;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn at(secs: u64, nanos: u32) -> SystemTime {
    UNIX_EPOCH + Duration::new(secs, nanos)
}

/// A directory with one prepared-able file and one unsupported file, scanned
/// once (so both have rows).
fn scanned() -> (tempfile::TempDir, Catalog, orbok_db::repo::SourceRecord) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("doc.md"), "ok").unwrap();
    std::fs::write(root.path().join("photo.jpg"), [0xFFu8, 0xD8]).unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root.path());
    scan(&catalog, &source.source_id);
    (root, catalog, source)
}

fn no_file_is_missing(catalog: &Catalog, source: &orbok_db::repo::SourceRecord) {
    let counts = FileRepository::new(catalog)
        .count_by_status(&source.source_id)
        .unwrap();
    let missing = counts
        .iter()
        .find(|(s, _)| *s == FileStatus::Missing)
        .map_or(0, |(_, n)| *n);
    assert_eq!(
        missing, 0,
        "no file the scan saw was marked missing: {counts:?}"
    );
}

/// §2.2: the scan's clock reads `…29.1234`, then `…29.123456`, which is
/// *later*. Written with trailing zeros trimmed, the later string sorts earlier
/// (`…29.123456Z` < `…29.1234Z`), and a file seen after the scan started used to
/// be marked missing. This is what failed on macOS, whose clocks have
/// microsecond resolution; here it happens on any platform if either the
/// formatter or the missing rule regresses.
#[test]
fn a_file_seen_at_a_later_instant_is_not_marked_missing_because_of_how_it_is_spelled() {
    let (_root, catalog, source) = scanned();
    let ticks = std::cell::Cell::new(0u32);
    let summary = with_clock(
        move || {
            ticks.set(ticks.get() + 1);
            // The first reading is the scan's start; every later one is the
            // moment a file is seen.
            if ticks.get() == 1 {
                at(4_000_000_029, 123_400_000)
            } else {
                at(4_000_000_029, 123_456_000)
            }
        },
        || scan(&catalog, &source.source_id),
    );
    assert_eq!(summary.missing_files, 0);
    no_file_is_missing(&catalog, &source);
    assert_eq!(
        summary.unchanged_files, 2,
        "both files were seen and unchanged"
    );
    // The formatter's own property, observed through the scanner: the stamps
    // the scan wrote sort in the order of the instants they name. (The scan no
    // longer decides anything by them; the generation does.)
    let early = orbok_core::system_time_iso8601(at(4_000_000_029, 123_400_000));
    let late = orbok_core::system_time_iso8601(at(4_000_000_029, 123_456_000));
    assert!(early < late, "{early} must sort before {late}");
    let stamps: Vec<String> = {
        let conn = catalog.lock();
        let mut stmt = conn
            .prepare("SELECT last_seen_at FROM files ORDER BY last_seen_at")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(stamps, [early, late], "in the order they were written");
}

/// §2.3: a wall clock that steps **backwards** during a scan (NTP, sleep and
/// resume) makes every file seen after the start look older than the start.
/// Whether a file was seen by this scan is an event, so it is not marked missing.
#[test]
fn a_clock_that_steps_backwards_during_a_scan_does_not_mark_seen_files_missing() {
    let (_root, catalog, source) = scanned();
    let ticks = std::cell::Cell::new(0u32);
    let summary = with_clock(
        move || {
            ticks.set(ticks.get() + 1);
            if ticks.get() == 1 {
                at(4_000_000_100, 0)
            } else {
                at(4_000_000_000, 0) // 100 s earlier
            }
        },
        || scan(&catalog, &source.source_id),
    );
    assert_eq!(summary.missing_files, 0);
    no_file_is_missing(&catalog, &source);
}

/// The other side of the rule: a file that is really gone **is** marked missing,
/// by the generation, whatever the clock says.
#[test]
fn a_file_that_is_gone_is_marked_missing_whatever_the_clock_says() {
    let (root, catalog, source) = scanned();
    std::fs::remove_file(root.path().join("doc.md")).unwrap();
    let ticks = std::cell::Cell::new(0u32);
    let summary = with_clock(
        move || {
            ticks.set(ticks.get() + 1);
            if ticks.get() == 1 {
                at(4_000_000_000, 0)
            } else {
                at(4_000_000_500, 0) // later, as a healthy clock would say
            }
        },
        || scan(&catalog, &source.source_id),
    );
    assert_eq!(summary.missing_files, 1, "only the file that is gone");
}

/// The old spelling of an instant: `time`'s RFC 3339 wrote the fraction with
/// trailing zeros trimmed, and none at all when it was zero.
fn old_spelling(fixed: &str) -> String {
    let (head, fraction) = fixed.trim_end_matches('Z').split_at(19);
    let digits = fraction.trim_start_matches('.').trim_end_matches('0');
    if digits.is_empty() {
        format!("{head}Z")
    } else {
        format!("{head}.{digits}Z")
    }
}

/// §2.5, the equality side: after migration 0011, a rescan re-hashes nothing.
/// The stored hash is deliberately wrong, so a rescan that decides the file
/// looks modified (and hashes it) marks it stale; one that trusts the unchanged
/// metadata leaves it alone. The control, without the rewrite, shows the same
/// test does see the difference.
#[test]
fn after_the_rewrite_a_rescan_hashes_nothing() {
    let migration = include_str!("../../../db/migrations/0011_fixed_width_timestamps.sql");
    for (rewrite, expected_stale) in [(true, 0), (false, 1)] {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("doc.md");
        std::fs::write(&file, "ok").unwrap();
        // An mtime whose fraction ends in zeros, so the old spelling differs
        // from the fixed one (a file system with coarser stamps has none at all,
        // which differs just the same).
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(at(4_000_000_000, 123_400_000))
            .unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        let source = register_dir_source(&catalog, root.path());
        scan(&catalog, &source.source_id);
        {
            // What an older orbok stored: the old spelling, and a hash a real
            // hash would not match.
            let conn = catalog.lock();
            let fixed: String = conn
                .query_row("SELECT modified_at FROM files", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "UPDATE files SET modified_at = ?1, content_hash = 'not-the-real-hash'",
                [old_spelling(&fixed)],
            )
            .unwrap();
            if rewrite {
                conn.execute_batch(migration).unwrap();
            }
        }
        let summary = scan(&catalog, &source.source_id);
        assert_eq!(
            summary.stale_files, expected_stale,
            "rewrite={rewrite}: a rescan that re-hashed would find the wrong hash and mark it stale"
        );
    }
}
