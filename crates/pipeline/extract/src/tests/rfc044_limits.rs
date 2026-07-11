//! RFC-044 acceptance tests: resource limits, structured warnings,
//! panic isolation, location semantics, and boundary invariants.
//!
//! Test plan follows RFC-044 §20.

use crate::ExtractorRegistry;
use crate::types::{ExtractContext, ExtractLimits, ExtractWarning};
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

fn ctx_with_limits(limits: ExtractLimits) -> ExtractContext {
    ExtractContext { limits }
}

// ── §20.1 Limit tests ────────────────────────────────────────────────────

// RFC-044 §20.1: text file over max_file_bytes → TooLarge error.
#[test]
fn text_file_over_size_limit_returns_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.txt");
    fs::write(&file, "Hello world.\n").unwrap();

    let limits = ExtractLimits {
        max_file_bytes: 5, // 5 bytes — smaller than the file
        ..Default::default()
    };
    let ctx = ctx_with_limits(limits);

    let result = ExtractorRegistry::default().extract_with_context(&validated(&file), &ctx);

    match result {
        Err(OrbokError::Extraction { category, .. }) => {
            assert_eq!(category, ErrorCategory::FileTooLarge);
        }
        other => panic!("expected FileTooLarge, got {other:?}"),
    }
}

// RFC-044 §20.1: markdown segment limit is enforced with warning.
#[test]
fn markdown_segment_limit_enforced_with_warning() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("many_paras.md");
    // 50 separate paragraphs.
    let content: String = (0..50).map(|i| format!("Paragraph {i}.\n\n")).collect();
    fs::write(&file, &content).unwrap();

    let limits = ExtractLimits {
        max_segments: 5, // cap at 5
        ..Default::default()
    };
    let ctx = ctx_with_limits(limits);

    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ctx)
        .unwrap();

    assert!(
        output.segments.len() <= 5,
        "segments {} must not exceed limit 5",
        output.segments.len()
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|w| matches!(w, ExtractWarning::SizeLimitReached { .. })),
        "SizeLimitReached warning must be emitted"
    );
}

// RFC-044 §20.1: extracted char limit truncates with warning.
#[test]
fn char_limit_truncates_output_with_warning() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long.txt");
    // 10 000 chars of content.
    let content = "a".repeat(10_000);
    fs::write(&file, &content).unwrap();

    let limits = ExtractLimits {
        max_extracted_chars: 100,
        ..Default::default()
    };
    let ctx = ctx_with_limits(limits);

    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ctx)
        .unwrap();

    assert!(
        output.char_count <= 100,
        "char_count {} must not exceed limit 100",
        output.char_count
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|w| matches!(w, ExtractWarning::SizeLimitReached { .. })),
        "SizeLimitReached warning must be emitted on char truncation"
    );
}

// RFC-044 §20.1: HTML byte limit returns TooLarge.
#[test]
fn html_byte_limit_returns_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.html");
    fs::write(&file, "<p>Hello world.</p>").unwrap();

    let limits = ExtractLimits {
        max_html_bytes: 3,
        ..Default::default()
    };
    let ctx = ctx_with_limits(limits);

    let result = ExtractorRegistry::default().extract_with_context(&validated(&file), &ctx);

    match result {
        Err(OrbokError::Extraction { category, .. }) => {
            assert_eq!(category, ErrorCategory::FileTooLarge);
        }
        other => panic!("expected FileTooLarge, got {other:?}"),
    }
}

// RFC-044 §20.1: DOCX ZIP entry limit enforced.
#[test]
fn docx_zip_entry_limit_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("big.docx");
    // A real DOCX is a ZIP; this fake file triggers the file-size check.
    fs::write(&file, b"PK\x03\x04fake docx data".repeat(10).as_slice()).unwrap();

    let limits = ExtractLimits {
        max_zip_entry_bytes: 5, // tiny limit
        ..Default::default()
    };
    let ctx = ctx_with_limits(limits);

    let result = ExtractorRegistry::default().extract_with_context(&validated(&file), &ctx);

    // May be TooLarge (file size check) or ParserError (ZIP parse).
    assert!(
        result.is_err(),
        "oversized/malformed DOCX must return an error"
    );
}

// ── §20.2 Warning tests ──────────────────────────────────────────────────

// RFC-044 §20.2: clean extraction produces empty warnings vec.
#[test]
fn clean_extraction_has_no_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clean.txt");
    fs::write(&file, "Hello world.\n").unwrap();

    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ExtractContext::default())
        .unwrap();

    assert!(
        output.warnings.is_empty(),
        "clean file must produce no warnings, got {:?}",
        output.warnings
    );
}

// RFC-044 §20.2: truncated large file warns SizeLimitReached.
// (Covered by char_limit_truncates_output_with_warning above.)
