//! Task 128 test 2: the snippet builder. A result's title already shows the
//! chunk's full heading chain (`SearchResult::title`/`heading_path`), so
//! the snippet must not repeat the chunk's own leading heading line, must
//! never leak raw Markdown `#` syntax, and must be cut exactly once -- at a
//! word boundary when one exists, marked with `…`, and at a grapheme
//! boundary (never mid-character) when it does not, for scripts with no
//! spaces such as Japanese.

use orbok_core::{ChunkId, FileId};
use orbok_db::repo::ChunkRecord;
use orbok_extract::{ExtractOutput, ExtractedSegment, LocationKind, LocationQuality, SegmentKind};
use orbok_fs::{GuardedSource, PathGuard};

/// Same shape as `task034_snippet_robustness.rs`'s own `record`/`guard_over`
/// -- kept local rather than shared, since the two files test different
/// concerns and a shared helper module is more ceremony than four lines
/// repeated once.
fn record(line_start: u32, line_end: u32) -> ChunkRecord {
    ChunkRecord {
        chunk_id: ChunkId::generate(),
        file_id: FileId::generate(),
        chunk_ordinal: 0,
        heading_path: None,
        line_start,
        line_end,
        byte_start: None,
        byte_end: None,
        location_quality: "exact".to_string(),
        location_kind: "lines".to_string(),
    }
}

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
        source_id: SourceId::from_string("s-task128-test".to_string()),
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

// ── `load_snippet_from` (line-numbered text files, e.g. Markdown) ─────────

/// A Markdown chunk whose first line is its own heading yields text without
/// it: the title already shows the full heading chain, so repeating the
/// heading as the snippet's own first line said it a third time (the
/// architect's own screenshot, `kyoto-autumn-trip.md`).
#[test]
fn a_markdown_chunk_drops_its_own_leading_heading() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("kyoto-autumn-trip.md");
    std::fs::write(
        &file,
        "## Budget\n\
         Train tickets, lodging, and the amount we set aside in the \
         emergency fund for unexpected costs.\n",
    )
    .unwrap();

    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let rec = record(1, 2);
    let snippet = snippets
        .load(&rec, file.to_str().unwrap())
        .unwrap()
        .unwrap();

    assert!(
        !snippet.contains('#'),
        "no raw Markdown heading marker must reach the display, got {snippet:?}"
    );
    assert!(
        snippet.starts_with("Train tickets"),
        "the snippet must start at the chunk's own text, not its heading, got {snippet:?}"
    );
}

/// A plain-text chunk (no heading line) is unchanged -- the heading-drop
/// rule only ever removes a genuine Markdown ATX heading, never ordinary
/// prose that happens to be short enough to be mistaken for one.
#[test]
fn a_plain_text_chunk_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "Real readable text on line one.\n").unwrap();

    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let rec = record(1, 1);
    let snippet = snippets
        .load(&rec, file.to_str().unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(snippet, "Real readable text on line one.");
}

/// A heading immediately followed by a blank line (the ordinary Markdown
/// shape) drops both -- the blank line is part of the heading's own
/// formatting, not the chunk's text.
#[test]
fn a_heading_s_own_blank_line_is_dropped_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    std::fs::write(
        &file,
        "### Train tickets\n\nBought in advance for the best fare.\n",
    )
    .unwrap();

    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let rec = record(1, 3);
    let snippet = snippets
        .load(&rec, file.to_str().unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(snippet, "Bought in advance for the best fare.");
}

/// A heading line that is not leading (a stray `#`-prefixed line inside the
/// chunk's own body, however that might arise) is kept, but loses its
/// Markdown marker rather than showing raw syntax -- only the *leading*
/// heading is dropped outright, since only that one duplicates the title.
#[test]
fn a_non_leading_heading_line_is_kept_but_unmarked() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    std::fs::write(&file, "Some lead-in text.\n## A later heading\n").unwrap();

    let snippets = crate::snippet::SnippetSource::from_guard(guard_over(dir.path()));
    let rec = record(1, 2);
    let snippet = snippets
        .load(&rec, file.to_str().unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(snippet, "Some lead-in text.\nA later heading");
}

// ── `segment_text_for` (cached extraction segments: PDF/DOCX/HTML) ────────

fn segment(text: &str) -> ExtractedSegment {
    ExtractedSegment {
        kind: SegmentKind::Paragraph,
        text: text.to_string(),
        line_start: 1,
        line_end: 1,
        location_kind: LocationKind::Paragraphs,
        heading_path: None,
        location_quality: LocationQuality::Exact,
    }
}

fn extraction(segments: Vec<ExtractedSegment>) -> ExtractOutput {
    ExtractOutput {
        extractor_name: "test".to_string(),
        extractor_version: "1".to_string(),
        normalization_version: "norm-v1".to_string(),
        segments,
        char_count: 0,
        warnings: vec![],
    }
}

/// The cached-extraction path (PDF/DOCX/HTML, RFC-060 §6) follows the same
/// heading-drop rule as the text-file path -- one rule, shared, not a
/// per-format copy of it (the whole point of `drop_heading_lines` living in
/// `snippet.rs` rather than inside `load_snippet_from` itself).
#[test]
fn a_cached_segment_s_leading_heading_is_dropped_the_same_way() {
    let rec = record(1, 1);
    let out = extraction(vec![segment("## Budget\nTrain tickets and lodging.")]);
    let snippet = crate::snippet::segment_text_for(&rec, &out).unwrap();
    assert!(!snippet.contains('#'));
    assert_eq!(snippet, "Train tickets and lodging.");
}

// ── `cut_for_display`: one cut, one place, at a word boundary ─────────────

/// English prose longer than the display cap is cut at the nearest
/// preceding word boundary, not mid-word, and marked with `…`.
#[test]
fn a_long_snippet_is_cut_at_a_word_boundary_and_marked() {
    let long = "Train tickets, lodging, and the amount we set aside in the emergency fund \
                for unexpected costs during the entire length of the trip, which turned out \
                to be considerably more than we had originally planned for.";
    let cut = crate::snippet::cut_for_display(long);
    assert!(
        cut.ends_with('…'),
        "a cut snippet must end with …, got {cut:?}"
    );
    let before_ellipsis = cut.strip_suffix('…').unwrap();
    assert!(
        long.starts_with(before_ellipsis),
        "the kept text must be an unbroken prefix of the original, got {before_ellipsis:?}"
    );
    assert!(
        before_ellipsis.ends_with(|c: char| !c.is_whitespace()),
        "the kept text must not end in trailing whitespace, got {before_ellipsis:?}"
    );
    let next_char = long[before_ellipsis.len()..].chars().next().unwrap();
    assert!(
        next_char.is_whitespace(),
        "the cut must land right after a whole word, not inside one -- the next \
         original character must be a space, got {next_char:?}"
    );
}

/// Japanese has no spaces to cut at: the fallback is the display limit
/// itself, always a whole grapheme, never split mid-character.
#[test]
fn japanese_text_with_no_spaces_is_cut_at_a_character_boundary() {
    // 140 repetitions of a 3-byte-per-character Japanese sentence fragment,
    // comfortably past the display cap, with no whitespace anywhere in it.
    let long: String = "てきとうなぶんしょうをながくつづけてひづけをたしかめるぶん。".repeat(5);
    assert!(long.chars().count() > 120);
    let cut = crate::snippet::cut_for_display(&long);
    assert!(
        cut.ends_with('…'),
        "a cut snippet must end with …, got {cut:?}"
    );
    let before_ellipsis = cut.strip_suffix('…').unwrap();
    assert_eq!(
        before_ellipsis.chars().count(),
        120,
        "with no word boundary available, the cut must land exactly at the display \
         limit, got {before_ellipsis:?}"
    );
    assert!(
        long.starts_with(before_ellipsis),
        "the kept text must be an unbroken, validly-encoded prefix of the original \
         -- a `String` that failed to build at all would mean a character was split"
    );
}

/// Text no longer than the display cap is returned unchanged -- no `…`
/// appears where nothing was cut.
#[test]
fn a_short_snippet_is_not_marked() {
    let short = "Train tickets and lodging.";
    assert_eq!(crate::snippet::cut_for_display(short), short);
}
