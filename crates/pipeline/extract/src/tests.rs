//! Tests for orbok-extract, validating RFC-005 §18 acceptance cases:
//! normalization rules, paragraph segmentation with exact lines,
//! markdown structure (heading paths, fences), encoding failures,
//! unsupported types, and Japanese text passthrough.

use crate::ExtractorRegistry;
use crate::normalize::normalize_document;
use crate::types::{LocationKind, LocationQuality, SegmentKind};
use orbok_core::{ErrorCategory, OrbokError, SourceId};
use orbok_fs::ValidatedPath;
use std::fs;
use std::path::Path;

fn validated(path: &Path) -> ValidatedPath {
    ValidatedPath {
        source_id: SourceId::generate(),
        canonical: fs::canonicalize(path).unwrap(),
    }
}

// RFC-005 §9: norm-v1 rules are exact.
#[test]
fn norm_v1_rules() {
    // BOM strip + CRLF + lone CR + trailing space + control chars.
    let input = "\u{FEFF}line one  \r\nline\u{0007} two\rline three\t.\n";
    let normalized = normalize_document(input);
    assert_eq!(normalized, "line one\nline two\nline three\t.\n");
}

// Japanese text passes through unmodified (RFC-014 §5: no lossy
// transformation before indexing).
#[test]
fn norm_v1_preserves_japanese() {
    let input = "日本語のテキスト。\n改行も保持される。\n";
    assert_eq!(normalize_document(input), input);
}

// RFC-005 §18: plain text basic extraction with exact line ranges.
#[test]
fn plain_text_paragraph_lines_exact() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "para one line 1\npara one line 2\n\npara two\n").unwrap();

    let out = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap();
    assert_eq!(out.extractor_name, "plain_text");
    assert_eq!(out.normalization_version, "norm-v1");
    assert_eq!(out.segments.len(), 2);
    assert_eq!(out.segments[0].line_start, 1);
    assert_eq!(out.segments[0].line_end, 2);
    assert_eq!(out.segments[1].line_start, 4);
    assert_eq!(out.segments[1].line_end, 4);
    assert!(
        out.segments
            .iter()
            .all(|s| s.location_quality == LocationQuality::Exact)
    );
}

// RFC-005 §18: markdown headings produce heading paths on following
// content; code fences become code segments with exact ranges.
#[test]
fn markdown_structure_and_heading_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("guide.md");
    fs::write(
        &file,
        "# Guide\n\n## Install\n\nRun the installer.\n\n```sh\ncargo install orbok\n```\n\n## Use\n\nOpen the app.\n",
    )
    .unwrap();

    let out = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap();
    assert_eq!(out.extractor_name, "markdown");

    let headings: Vec<_> = out
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::Heading)
        .collect();
    assert_eq!(headings.len(), 3);

    let install_para = out
        .segments
        .iter()
        .find(|s| s.text == "Run the installer.")
        .unwrap();
    assert_eq!(
        install_para.heading_path.as_deref(),
        Some("Guide > Install")
    );
    assert_eq!(install_para.line_start, 5);

    let code = out
        .segments
        .iter()
        .find(|s| s.kind == SegmentKind::CodeBlock)
        .unwrap();
    assert_eq!(code.text, "cargo install orbok");
    assert_eq!(code.line_start, 7);
    assert_eq!(code.line_end, 9);

    // Sibling heading replaces, not nests: "Use" path is Guide > Use.
    let use_para = out
        .segments
        .iter()
        .find(|s| s.text == "Open the app.")
        .unwrap();
    assert_eq!(use_para.heading_path.as_deref(), Some("Guide > Use"));
}

// RFC-005 §18: empty file extracts to zero segments, not an error.
#[test]
fn empty_file_yields_no_segments() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("empty.txt");
    fs::write(&file, "").unwrap();
    let out = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap();
    assert!(out.segments.is_empty());
    assert_eq!(out.char_count, 0);
}

// RFC-005 §13: invalid UTF-8 is a typed EncodingError.
#[test]
fn invalid_utf8_is_encoding_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bad.txt");
    fs::write(&file, [0xFFu8, 0xFE, 0x00, 0x41]).unwrap();
    let err = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap_err();
    match err {
        OrbokError::Extraction { category, .. } => {
            assert_eq!(category, ErrorCategory::EncodingError)
        }
        other => panic!("unexpected error {other:?}"),
    }
}

// RFC-005 §13: unknown extension is a typed UnsupportedType.
#[test]
fn unknown_extension_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("image.xyz");
    fs::write(&file, "binaryish").unwrap();
    let err = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap_err();
    match err {
        OrbokError::Extraction { category, .. } => {
            assert_eq!(category, ErrorCategory::UnsupportedType)
        }
        other => panic!("unexpected error {other:?}"),
    }
}

// Registry selection: markdown wins for .md, plain text takes code.
#[test]
fn registry_selection_by_extension() {
    let registry = ExtractorRegistry::default();
    assert_eq!(registry.select("md").unwrap().name(), "markdown");
    assert_eq!(registry.select("rs").unwrap().name(), "plain_text");
    assert!(registry.select("xyz").is_none());
}

// Unclosed fence terminates at EOF without panicking (malformed input
// robustness, RFC-005 §13).
#[test]
fn unclosed_fence_is_robust() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("broken.md");
    fs::write(&file, "# T\n\n```\nnever closed\n").unwrap();
    let out = ExtractorRegistry::default()
        .extract(&validated(&file))
        .unwrap();
    let code = out
        .segments
        .iter()
        .find(|s| s.kind == SegmentKind::CodeBlock)
        .unwrap();
    assert_eq!(code.text, "never closed");
}

// ──────────────────────────────────────────────
// RFC-006 §20 chunker tests
// ──────────────────────────────────────────────

use crate::chunker::chunk;
use crate::types::{ExtractOutput, ExtractedSegment};

fn extract_str(text: &str) -> ExtractOutput {
    let segments = text
        .split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .enumerate()
        .map(|(i, para)| ExtractedSegment {
            kind: SegmentKind::Paragraph,
            text: para.trim().to_string(),
            line_start: (i as u32 * 2) + 1,
            line_end: (i as u32 * 2) + 2,
            location_kind: LocationKind::Lines,
            heading_path: None,
            location_quality: LocationQuality::Exact,
        })
        .collect();
    ExtractOutput {
        extractor_name: "test".into(),
        extractor_version: "v1".into(),
        normalization_version: "norm-v1".into(),
        segments,
        char_count: text.len() as u64,
        warnings: Vec::new(),
    }
}

// RFC-006 §20 test 1: short text → single document chunk.
#[test]
fn short_text_becomes_one_document_chunk() {
    let output = extract_str("Hello world.");
    let specs = chunk(&output, "doc.txt");
    assert!(!specs.is_empty());
    assert_eq!(specs[0].chunk_kind, "document");
}

// RFC-006 §20 test 2: long text → multiple fallback chunks.
#[test]
fn long_text_becomes_multiple_chunks() {
    let long_para = "word ".repeat(500); // ~2500 chars, exceeds MAX_CHARS
    let output = extract_str(&long_para);
    let specs = chunk(&output, "long.txt");
    assert!(
        specs.len() > 1,
        "long text must produce multiple chunks, got {}",
        specs.len()
    );
}

// RFC-006 §20 test 3: Markdown headings create section context.
#[test]
fn markdown_headings_create_section_chunks() {
    use crate::types::ExtractedSegment;
    let segments = vec![
        ExtractedSegment {
            kind: SegmentKind::Heading,
            text: "Authentication".into(),
            line_start: 1,
            line_end: 1,
            location_kind: LocationKind::Lines,
            heading_path: Some("Authentication".into()),
            location_quality: LocationQuality::Exact,
        },
        ExtractedSegment {
            kind: SegmentKind::Paragraph,
            text: "Tokens expire after 24 hours.".into(),
            line_start: 3,
            line_end: 4,
            location_kind: LocationKind::Lines,
            heading_path: Some("Authentication".into()),
            location_quality: LocationQuality::Exact,
        },
        ExtractedSegment {
            kind: SegmentKind::Heading,
            text: "Storage".into(),
            line_start: 6,
            line_end: 6,
            location_kind: LocationKind::Lines,
            heading_path: Some("Storage".into()),
            location_quality: LocationQuality::Exact,
        },
        ExtractedSegment {
            kind: SegmentKind::Paragraph,
            text: "Data is stored locally.".into(),
            line_start: 8,
            line_end: 9,
            location_kind: LocationKind::Lines,
            heading_path: Some("Storage".into()),
            location_quality: LocationQuality::Exact,
        },
    ];
    let output = ExtractOutput {
        extractor_name: "markdown".into(),
        extractor_version: "v1".into(),
        normalization_version: "norm-v1".into(),
        segments,
        char_count: 80,
        warnings: Vec::new(),
    };
    let specs = chunk(&output, "guide.md");
    // Parent + 2 sections.
    assert!(
        specs.len() >= 3,
        "expected parent + 2 section chunks, got {}",
        specs.len()
    );
    let section_kinds: Vec<&str> = specs[1..].iter().map(|s| s.chunk_kind).collect();
    assert!(
        section_kinds.contains(&"section"),
        "expected section chunks"
    );
}

// RFC-006 §20 test 9: chunk hash is stable for identical text.
#[test]
fn chunk_hash_stable_for_identical_text() {
    let output = extract_str("identical content");
    let specs1 = chunk(&output, "f.txt");
    let specs2 = chunk(&output, "f.txt");
    assert_eq!(specs1[0].normalized_text, specs2[0].normalized_text);
}

// RFC-006 §20 test 7: approximate locations don't claim exact quality.
#[test]
fn fallback_chunks_have_approximate_quality() {
    let long_para = "word ".repeat(400);
    let output = extract_str(&long_para);
    let specs = chunk(&output, "long.txt");
    // Any fallback chunk must not claim exact quality.
    for spec in specs.iter().filter(|s| s.chunk_kind == "fallback") {
        assert_eq!(
            spec.location_quality, "approximate",
            "fallback chunk must have approximate quality"
        );
    }
}

// Parent-child: all children except index-0 have parent_idx = Some(0).
#[test]
fn children_point_to_parent() {
    let output = extract_str("Para one.\n\nPara two.\n\nPara three.");
    let specs = chunk(&output, "multi.txt");
    for spec in specs.iter().skip(1) {
        assert_eq!(
            spec.parent_idx,
            Some(0),
            "child chunk {} must point to parent",
            spec.chunk_ordinal
        );
    }
}

// RFC-044 hardening tests in submodules.
mod rfc044_isolation;
mod rfc044_limits;
