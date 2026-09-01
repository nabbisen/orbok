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

/// The lied-size archive must not bypass the cap: real content far larger
/// than `max_docx_xml_bytes`, declared size far under it. Confirmed failing
/// against the pre-fix code (gated on `entry.size()`) before landing the
/// fix -- the extractor read the full oversized content and returned no
/// warning.
#[test]
fn docx_with_lied_size_field_is_still_bounded() {
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

    let output = ExtractorRegistry::default()
        .extract_with_context(&validated(&file), &ctx)
        .unwrap();

    assert!(
        output.char_count <= 1024,
        "char_count {} must be bounded by max_docx_xml_bytes regardless of the declared ZIP size",
        output.char_count
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|w| matches!(w, crate::types::ExtractWarning::SizeLimitReached { .. })),
        "SizeLimitReached must be reported even though the ZIP header declared a small size"
    );
}

/// Review 197 §2: `docx_with_lied_size_field_is_still_bounded` above asserts
/// only on the *output* (`char_count`, the warning) -- both survive a
/// mutation that removes the read bound entirely (`take(limit + 1)` ->
/// `take(u64::MAX)`), because the fix's own truncation step still runs
/// after an unbounded read and produces byte-identical output. That test
/// guards the branch shape, not the bound. S-01 was unbounded memory
/// *allocation during decompression* (1.4 MB in, 1.24 GB RSS) -- a
/// property of work done, not of the final bytes. Asserted here by
/// elapsed time, the same technique `no_newline_file_does_not_materialize_the_whole_file`
/// (§5) uses: a `Deflated` entry whose real, decompressed content is ~50 MB
/// (`"word ".repeat(10_000_000)`, trivially compressible so the on-disk
/// archive itself stays small) with its declared size lied down to 10
/// bytes. A bounded read stops after ~1 KB of decompressed output,
/// regardless of what the rest of the stream contains; a read that
/// decompresses the whole entry before truncating cannot avoid the cost of
/// actually producing those ~50 MB.
#[test]
fn docx_with_lied_size_field_does_not_decompress_the_whole_bomb() {
    use std::time::Instant;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bomb_deflated.docx");

    let real_paragraph = "word ".repeat(10_000_000); // ~50 MB decompressed
    let real_xml = docx_xml_wrapping(&real_paragraph);
    assert!(
        real_xml.len() > 40_000_000,
        "fixture must be tens of MB to make unbounded decompression measurable"
    );

    let docx_bytes = build_docx_with_lied_size_and_method(
        real_xml.as_bytes(),
        10,
        zip::CompressionMethod::Deflated,
    );
    assert!(
        docx_bytes.len() < 200_000,
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
