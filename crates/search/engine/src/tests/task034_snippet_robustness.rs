//! Task 034 §5 (audit F-03, S-18): three independent `load_snippet`
//! defects. Real correctness of PDF/DOCX/HTML snippets is RFC-060's; these
//! are robustness fixes only.

use orbok_core::{ChunkId, FileId};
use orbok_db::repo::ChunkRecord;

fn record(line_start: u32, line_end: u32, location_quality: &str) -> ChunkRecord {
    ChunkRecord {
        chunk_id: ChunkId::generate(),
        file_id: FileId::generate(),
        chunk_ordinal: 0,
        heading_path: None,
        line_start,
        line_end,
        byte_start: None,
        byte_end: None,
        location_quality: location_quality.to_string(),
    }
}

/// A file with no newline byte must not be read in full before the 8-line /
/// 400-char cap is applied -- `BufRead::lines()` would otherwise allocate
/// one `String` covering the entire file to produce that single "line".
/// Asserted by elapsed time: a capped read (`Read::take(64 * 1024)`) is
/// microseconds regardless of file size; an unbounded read of a 200 MB
/// single line measured ~40ms locally (`BufReader::lines()` on the raw
/// file, repeated, consistently 35-45ms) -- comfortably distinguishable
/// from a 64 KiB-bounded read at any reasonable deadline. Confirmed this
/// exceeds the deadline below against the pre-fix code before landing the
/// fix.
#[test]
fn no_newline_file_does_not_materialize_the_whole_file() {
    use std::time::Instant;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("no_newline.txt");
    // 200 MB, single line, no newline byte anywhere.
    std::fs::write(&file, "x".repeat(200 * 1024 * 1024)).unwrap();

    let rec = record(1, 1, "exact");
    let start = Instant::now();
    let snippet = crate::snippet::load_snippet(&rec, file.to_str().unwrap());
    let elapsed = start.elapsed();

    assert!(snippet.is_some(), "a snippet should still be produced");
    assert!(
        elapsed.as_millis() < 20,
        "load_snippet on a no-newline file must be bounded by the 64 KiB read \
         cap, not file size -- took {elapsed:?}"
    );
}

/// A stored `line_end < line_start` (a corrupted or malformed location) must
/// not panic. `(end - start + 1)` underflows in `usize` arithmetic when
/// `end < start`, which is a debug-build panic (and a huge wraparound take
/// count in release).
#[test]
fn inverted_line_range_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "line one\nline two\nline three\n").unwrap();

    // line_end (2) < line_start (5): a malformed/corrupted stored range.
    let rec = record(5, 2, "exact");
    let result =
        std::panic::catch_unwind(|| crate::snippet::load_snippet(&rec, file.to_str().unwrap()));

    assert!(
        result.is_ok(),
        "load_snippet must not panic on an inverted line range, got a panic: {result:?}"
    );
}

/// Interim guard for the wrong-bytes defect (audit F-03): PDF/DOCX/HTML
/// chunks store `Approximate` location quality, and `load_snippet` treats
/// every `line_start`/`line_end` as a literal text-file line number --
/// which for those formats it is not (paragraph/page ordinals instead). A
/// missing snippet is honest; a binary excerpt presented as document text
/// is not. Real correctness (locating actual text) is RFC-060's; this only
/// stops the visible damage until that lands.
#[test]
fn non_exact_location_quality_yields_no_snippet() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.txt");
    std::fs::write(&file, "real readable text on line one\n").unwrap();

    let approximate = record(1, 1, "approximate");
    assert_eq!(
        crate::snippet::load_snippet(&approximate, file.to_str().unwrap()),
        None,
        "non-exact location_quality must yield no snippet rather than the wrong bytes"
    );

    // Positive control: the same file, same lines, "exact" quality DOES
    // still produce a snippet -- the guard is not simply always-None.
    let exact = record(1, 1, "exact");
    assert!(
        crate::snippet::load_snippet(&exact, file.to_str().unwrap()).is_some(),
        "exact location_quality must still produce a snippet"
    );
}
