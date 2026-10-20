//! Task 034 §3 (audit S-01): the DOCX per-entry XML size cap was gated on
//! `entry.size()` -- the ZIP header's *declared* uncompressed size, which the
//! file's author writes -- not on bytes actually read from decompression. A
//! real archive with a lied-about size field bypasses the cap entirely.

use crate::ExtractorRegistry;
use crate::types::{ExtractContext, ExtractLimits};
use orbok_core::SourceId;
use orbok_fs::ValidatedPath;
use std::io::Write;
use std::path::Path;

fn validated(path: &Path) -> ValidatedPath {
    ValidatedPath {
        source_id: SourceId::generate(),
        canonical: std::fs::canonicalize(path).unwrap(),
    }
}

/// Build a real DOCX (ZIP) whose `word/document.xml` central-directory *and*
/// local-header "uncompressed size" fields are patched to `declared_size`,
/// while the entry actually decompresses to `real_xml`'s true length. This
/// is a genuine malicious archive, not a mock of `entry.size()` -- `size()`
/// is not an injectable seam on `zip::read::ZipFile`, and patching the same
/// bytes an attacker would patch is what the defect is actually about.
fn build_docx_with_lied_size(real_xml: &[u8], declared_size: u32) -> Vec<u8> {
    build_docx_with_lied_size_and_method(real_xml, declared_size, zip::CompressionMethod::Stored)
}

/// As [`build_docx_with_lied_size`], with the compression method as a
/// parameter (Review 197 §2): `Stored` entries have compressed size ==
/// uncompressed size, which reproduces the size-field bypass but cannot
/// exercise unbounded *decompression* -- there is nothing to decompress.
/// `Deflated` over a highly compressible payload is what lets a small
/// on-disk archive actually contain tens of MB to inflate.
fn build_docx_with_lied_size_and_method(
    real_xml: &[u8],
    declared_size: u32,
    method: zip::CompressionMethod,
) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default().compression_method(method);
        writer
            .start_file("word/document.xml", options)
            .expect("start_file");
        writer.write_all(real_xml).expect("write real xml");
        writer.finish().expect("finish zip");
    }

    // Patch every occurrence of the true uncompressed-size field (local file
    // header at header+22, central directory header at header+24) that
    // matches the real length, to the lied-about declared_size. Stored
    // (uncompressed) entries: compressed size == uncompressed size, so both
    // the compressed-size and uncompressed-size fields hold the same true
    // value today -- patch only the uncompressed-size field at each
    // signature's known offset, leaving compressed-size (and the actual
    // data bytes) untouched, so the entry still really contains real_xml.
    let true_len = real_xml.len() as u32;
    let mut patched_local = 0;
    let mut patched_central = 0;
    let mut i = 0;
    while i + 4 <= buf.len() {
        if &buf[i..i + 4] == b"PK\x03\x04" && i + 26 <= buf.len() {
            let off = i + 22;
            if u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) == true_len {
                buf[off..off + 4].copy_from_slice(&declared_size.to_le_bytes());
                patched_local += 1;
            }
        } else if &buf[i..i + 4] == b"PK\x01\x02" && i + 28 <= buf.len() {
            let off = i + 24;
            if u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) == true_len {
                buf[off..off + 4].copy_from_slice(&declared_size.to_le_bytes());
                patched_central += 1;
            }
        }
        i += 1;
    }
    assert_eq!(
        patched_local, 1,
        "expected exactly one local header to patch"
    );
    assert_eq!(
        patched_central, 1,
        "expected exactly one central directory header to patch"
    );
    buf
}

fn docx_xml_wrapping(paragraph_text: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><w:document><w:body><w:p><w:t>{paragraph_text}</w:t></w:p></w:body></w:document>"
    )
}

/// The lied-size archive must never be read successfully with no sign of
/// the lie. Confirmed failing against the pre-fix code (gated on
/// `entry.size()`) before landing the original Task 034 fix -- the
/// extractor read the full oversized content and returned no warning.
///
/// **Task 134:** since `zip` 9.0.0, the crate's own reader independently
/// validates actual decompressed bytes against the entry's declared
/// uncompressed size and returns an `io::Error` ("File is larger than its
/// declared uncompressed size") the moment the two disagree -- before this
/// task's own `take(limit + 1)` bound even has a chance to truncate
/// anything. That is a stricter, earlier rejection than the bounded,
/// truncated-with-a-warning result this test asserted against `zip` 2.x, so
/// the assertion now checks the thing that is actually true and still the
/// point of Task 034: the lie is never read successfully, and this is a
/// clean `OrbokError::Extraction`, not a panic or (as the pre-fix code did)
/// a silent oversized read. `docx_with_lied_size_field_does_not_decompress_
/// the_whole_bomb` below keeps this task's own bound under test on its own,
/// using a declared size that is *not* a lie, so a future `zip` upgrade
/// that removed its own check could not quietly make that test meaningless
/// too.
#[test]
fn docx_with_lied_size_field_is_rejected_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bomb.docx");

    // Real content: ~200 KiB of a repeated word (highly compressible, but
    // Stored here so the archive itself stays simple to patch -- the
    // decompression-bomb *ratio* is not what this test is about; the point
    // is that entry.size() lies, and the fix must not trust it either way).
    let real_paragraph = "word ".repeat(40_000); // ~200 KB
    let real_xml = docx_xml_wrapping(&real_paragraph);
    assert!(
        real_xml.len() > 100_000,
        "fixture must exceed the limit below"
    );

    let docx_bytes = build_docx_with_lied_size(real_xml.as_bytes(), 10);
    std::fs::write(&file, &docx_bytes).unwrap();

    let limits = ExtractLimits {
        max_docx_xml_bytes: 1024, // far under the real content
        max_zip_entry_bytes: u64::MAX,
        ..Default::default()
    };
    let ctx = ExtractContext { limits };

    let err = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ctx)
        .expect_err("a lied-about declared size must never be read as if it were honest");

    assert!(
        matches!(
            err,
            orbok_core::OrbokError::Extraction {
                category: orbok_core::ErrorCategory::ParserError,
                ..
            }
        ),
        "expected a clean ParserError extraction failure, got {err:?}"
    );
}

/// Review 197 §2: `docx_with_lied_size_field_is_rejected_cleanly` above
/// asserts only that a lied-about size is refused -- it says nothing about
/// *this* task's own bound, since (Task 134) `zip` 9.x's own declared-size
/// check now rejects that specific archive well before `take(limit + 1)`
/// ever runs. S-01 was unbounded memory *allocation during decompression*
/// (1.4 MB in, 1.24 GB RSS) -- a property of work done, not of the final
/// bytes, and specifically a property of *this task's own* bound, not of
/// whatever `zip` happens to check this year. Asserted here by elapsed
/// time, the same technique `no_newline_file_does_not_materialize_the_whole_file`
/// (§5) uses: a `Deflated` entry whose real, decompressed content is ~50 MB
/// (`"word ".repeat(10_000_000)`, trivially compressible so the on-disk
/// archive itself stays small), with its declared size left *accurate* --
/// not lied about -- so `zip`'s own check has nothing to catch and
/// `take(limit + 1)` is the only thing standing between this call and a
/// full 50 MB decompression. A bounded read stops after ~1 KB of
/// decompressed output, regardless of what the rest of the stream
/// contains; a read that decompresses the whole entry before truncating
/// cannot avoid the cost of actually producing those ~50 MB.
#[test]
fn docx_with_accurate_size_does_not_decompress_the_whole_bomb() {
    use std::time::Instant;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bomb_deflated.docx");

    let real_paragraph = "word ".repeat(10_000_000); // ~50 MB decompressed
    let real_xml = docx_xml_wrapping(&real_paragraph);
    assert!(
        real_xml.len() > 40_000_000,
        "fixture must be tens of MB to make unbounded decompression measurable"
    );

    let declared_size = real_xml.len() as u32;
    let docx_bytes = build_docx_with_lied_size_and_method(
        real_xml.as_bytes(),
        declared_size,
        zip::CompressionMethod::Deflated,
    );
    // zip 9.x's own compressor produces a slightly larger archive for this
    // input than the 2.x baseline this threshold was first set against
    // (measured: ~243 KB) -- the property under test is "small on-disk
    // archive, huge decompressed content", not a specific byte count.
    assert!(
        docx_bytes.len() < 500_000,
        "the on-disk archive must stay small -- {} bytes -- so a slow \
         decompression time can only come from inflating the entry, not \
         from reading the archive itself",
        docx_bytes.len()
    );
    std::fs::write(&file, &docx_bytes).unwrap();

    let limits = ExtractLimits {
        max_docx_xml_bytes: 1024,
        max_zip_entry_bytes: u64::MAX,
        ..Default::default()
    };
    let ctx = ExtractContext { limits };

    let start = Instant::now();
    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ctx)
        .unwrap();
    let elapsed = start.elapsed();

    assert!(output.char_count <= 1024);
    assert!(
        elapsed.as_millis() < 50,
        "extracting a DOCX with a lied-about small size must not fully \
         decompress the real ~50 MB entry before truncating -- took \
         {elapsed:?}"
    );
}

/// Sanity check that the harness itself is trustworthy: an archive whose
/// declared size is accurate (not lied about) and genuinely under the limit
/// extracts normally, with no warning. Without this, a bug in
/// `build_docx_with_lied_size` that always triggered the limit would make
/// the test above pass for the wrong reason.
#[test]
fn docx_with_accurate_small_size_extracts_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clean.docx");

    let real_xml = docx_xml_wrapping("hello world");
    let true_len = real_xml.len() as u32;
    let docx_bytes = build_docx_with_lied_size(real_xml.as_bytes(), true_len);
    std::fs::write(&file, &docx_bytes).unwrap();

    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ExtractContext::default())
        .unwrap();

    assert!(
        output.warnings.is_empty(),
        "accurate, small DOCX must extract without warnings, got {:?}",
        output.warnings
    );
    assert!(
        output
            .segments
            .iter()
            .any(|s| s.text.contains("hello world")),
        "expected the real paragraph text to be extracted"
    );
}

/// Task 134 §2: a truncated archive -- the other malformation named
/// alongside a lied-about size -- fails cleanly, never panics. Cutting off
/// the tail removes the end-of-central-directory record, so
/// `zip::ZipArchive::new` itself refuses to open the archive at all;
/// `docx.rs`'s existing `.map_err(...)` on that call (not the later read)
/// is what turns this into a clean `OrbokError::Extraction`. This was
/// already true before this task -- both call sites `docx.rs` can fail at
/// were already behind `.map_err`, with no `.unwrap()`/`.expect()`/`panic!`
/// anywhere on the path a malformed archive reaches -- so this test was
/// never red; it is here to keep it that way.
#[test]
fn docx_truncated_archive_fails_cleanly_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("truncated.docx");

    let real_xml = docx_xml_wrapping("hello world");
    let true_len = real_xml.len() as u32;
    let docx_bytes = build_docx_with_lied_size(real_xml.as_bytes(), true_len);
    assert!(
        docx_bytes.len() > 100,
        "fixture must be long enough to truncate meaningfully"
    );

    // Cut off the tail: the end-of-central-directory record (and part of
    // the central directory itself) is gone, so there is no valid archive
    // structure to parse at all -- not merely a truncated entry.
    let truncated = &docx_bytes[..docx_bytes.len() - 50];
    std::fs::write(&file, truncated).unwrap();

    let err = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ExtractContext::default())
        .expect_err("a truncated archive must never extract successfully");

    assert!(
        matches!(
            err,
            orbok_core::OrbokError::Extraction {
                category: orbok_core::ErrorCategory::ParserError,
                ..
            }
        ),
        "expected a clean ParserError extraction failure, got {err:?}"
    );
}
