//! Task 034 §5 (audit F-03, S-18): three independent `load_snippet`
//! defects. Real correctness of PDF/DOCX/HTML snippets is RFC-060's; these
//! are robustness fixes only.

use orbok_core::{ChunkId, FileId};
use orbok_db::repo::ChunkRecord;
use orbok_fs::{GuardedSource, PathGuard};
use std::io::Read;

/// A guard admitting one directory, so these tests exercise the same
/// boundary production uses (RFC-060 §5: `load_snippet` validates before
/// opening). Built from a `SourceRecord` because `CompiledPolicy` is only
/// constructible that way.
fn guard_over(root: &std::path::Path) -> PathGuard {
    use orbok_core::{
        HiddenFilePolicy, IndexMode, PersistenceMode, SourceId, SourceStatus, SourceType,
        SymlinkPolicy,
    };
    use orbok_db::repo::SourceRecord;
    let canonical = std::fs::canonicalize(root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record = SourceRecord {
        source_id: SourceId::from_string("s-snippet-test".to_string()),
        source_type: SourceType::Directory,
        persistence_mode: PersistenceMode::Persistent,
        display_name: None,
        original_path: canonical.clone(),
        canonical_path: canonical,
        status: SourceStatus::Active,
        index_mode: IndexMode::Balanced,
        include_patterns: vec![],
        exclude_patterns: vec![],
        hidden_file_policy: HiddenFilePolicy::Exclude,
        symlink_policy: SymlinkPolicy::Ignore,
        max_file_size_bytes: None,
        covers_subfolders: true,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        last_scanned_at: None,
    };
    PathGuard::new(vec![GuardedSource::from_record(&record)])
}

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
        location_kind: "lines".to_string(),
    }
}

/// A counting wrapper around any `Read`, so a test can assert exactly how
/// many bytes a call consumed instead of inferring it from elapsed time
/// (Task 045: wall-clock was a contaminated proxy for a byte count -- a
/// shared CI runner can make a correctly-bounded read take 40ms, and a
/// fast enough disk can make an *unbounded* read finish inside any
/// deadline this test could set). The count lives behind a shared `Cell`
/// rather than a plain field, since `load_snippet_from` takes ownership of
/// the reader and the test needs to read the count back afterward.
struct Counting<R> {
    inner: R,
    count: std::rc::Rc<std::cell::Cell<u64>>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get() + n as u64);
        Ok(n)
    }
}

/// Newline-free `x` bytes up to `remaining`, then an error. With the 64 KiB
/// cap in place the error is never reached; without it the read fails in
/// milliseconds instead of hanging (Review 217 §3: an unbounded source made
/// the missing-cap mutation hold a CI runner until the 360-minute default).
struct Ceiling {
    remaining: u64,
}

impl Read for Ceiling {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::Error::other("read past the ceiling"));
        }
        let n = buf.len().min(self.remaining as usize);
        buf[..n].fill(b'x');
        self.remaining -= n as u64;
        Ok(n)
    }
}

/// A file with no newline byte must not be read in full before the 8-line /
/// 400-char cap is applied -- `BufRead::lines()` would otherwise allocate
/// one `String` covering the entire file to produce that single "line".
/// Asserted by the actual byte count read, not by timing, from a 1 MiB
/// newline-free source (sixteen times the cap) that errors past its end: if
/// the 64 KiB cap (`Read::take` in `load_snippet_from`) is missing, the
/// read hits that error and the assertions below fail.
#[test]
fn no_newline_file_does_not_materialize_the_whole_file() {
    let rec = record(1, 1, "exact");
    let count = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let source = Counting {
        inner: Ceiling {
            remaining: 1024 * 1024,
        },
        count: count.clone(),
    };

    let snippet = crate::snippet::load_snippet_from(&rec, source);
    let bytes_read = count.get();

    assert!(snippet.is_some(), "a snippet should still be produced");
    assert!(
        bytes_read <= 64 * 1024,
        "load_snippet_from must read at most the 64 KiB cap regardless of \
         source length -- read {bytes_read} bytes"
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
    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snippets.load(&rec, file.to_str().unwrap())
    }));

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

    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let approximate = record(1, 1, "approximate");
    assert_eq!(
        snippets.load(&approximate, file.to_str().unwrap()).unwrap(),
        None,
        "non-exact location_quality must yield no snippet rather than the wrong bytes"
    );

    // Positive control: the same file, same lines, "exact" quality DOES
    // still produce a snippet -- the guard is not simply always-None.
    let exact = record(1, 1, "exact");
    assert!(
        snippets
            .load(&exact, file.to_str().unwrap())
            .unwrap()
            .is_some(),
        "exact location_quality must still produce a snippet"
    );
}
