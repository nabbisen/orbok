//! Extraction types (RFC-005 §6–§8; RFC-044 hardening).

use orbok_core::{ErrorCategory, OrbokResult};
use orbok_fs::ValidatedPath;
use serde::{Deserialize, Serialize};

// ── Location semantics ──────────────────────────────────────────────────

/// What the position fields (`line_start` / `line_end`) on a segment
/// actually mean in the source format (RFC-044 §12).
///
/// The UI must use this field before deciding how to label a location —
/// never assume "line" for all formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// Position fields are 1-based line numbers.
    Lines,
    /// Position fields are 1-based page numbers.
    Pages,
    /// Position fields are 1-based paragraph indices.
    Paragraphs,
    /// Position fields are approximate block indices.
    Blocks,
    /// Position meaning is unknown or not applicable.
    Unknown,
}

// ── Segment classification ──────────────────────────────────────────────

/// Segment classification (RFC-005 §8; feeds RFC-006 chunking).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Heading,
    Paragraph,
    CodeBlock,
    ListItem,
    Table,
    Other,
}

/// How precise the recorded location is (RFC-006 §8 vocabulary, shared
/// here because extraction produces the locations).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationQuality {
    Exact,
    Approximate,
    PageOnly,
    Unknown,
}

// ── Resource limits ─────────────────────────────────────────────────────

/// Per-extraction resource limits (RFC-044 §9).
///
/// Conservative defaults keep extraction bounded on any machine.
/// Values are configurable by the app layer; do not hard-code them
/// in extractors.
#[derive(Debug, Clone)]
pub struct ExtractLimits {
    /// Maximum file size to read at all.
    pub max_file_bytes: u64,
    /// Maximum total extracted characters across all segments.
    pub max_extracted_chars: u64,
    /// Maximum number of segments to produce.
    pub max_segments: usize,
    /// Maximum PDF pages to process.
    pub max_pdf_pages: usize,
    /// Maximum uncompressed size of a single DOCX ZIP entry.
    pub max_docx_xml_bytes: u64,
    /// Maximum uncompressed size of any ZIP entry.
    pub max_zip_entry_bytes: u64,
    /// Maximum HTML file size.
    pub max_html_bytes: u64,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024, // 64 MiB
            max_extracted_chars: 5_000_000,
            max_segments: 20_000,
            max_pdf_pages: 1_000,
            max_docx_xml_bytes: 32 * 1024 * 1024,  // 32 MiB
            max_zip_entry_bytes: 64 * 1024 * 1024, // 64 MiB
            max_html_bytes: 32 * 1024 * 1024,      // 32 MiB
        }
    }
}

/// Context passed into every extractor call (RFC-044 §9.3).
#[derive(Debug, Clone, Default)]
pub struct ExtractContext {
    pub limits: ExtractLimits,
}

// ── Structured warnings ─────────────────────────────────────────────────

/// Warnings about partial or degraded extraction (RFC-044 §10).
///
/// A non-empty `warnings` list means the output is honest but incomplete.
/// The UI maps these to plain-language messages; raw variant names must
/// not appear in default user-facing copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExtractWarning {
    /// Generic content was skipped; `reason` is for logs only.
    SomeContentSkipped { reason: String },
    /// These PDF pages could not be read.
    SomePagesUnreadable { pages: Vec<u32> },
    /// PDF has pages but no extractable text — likely scanned.
    PossiblyScannedPdf,
    /// A resource limit stopped extraction early.
    SizeLimitReached { limit_name: String },
    /// File uses an encoding orbok could not read.
    EncodingUnsupported,
    /// A document part (e.g. footnotes, embedded object) was skipped.
    UnsupportedDocumentPart { part: String },
    /// Location fields are approximate, not exact.
    ApproximateLocationOnly,
    /// Malformed content was partially recovered and included.
    MalformedContentRecovered,
}

// ── Core segment and output types ───────────────────────────────────────

/// One extracted, normalized segment with source location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedSegment {
    pub kind: SegmentKind,
    /// Normalized text (norm-v1).
    pub text: String,
    /// 1-based inclusive position range; meaning depends on `location_kind`.
    pub line_start: u32,
    pub line_end: u32,
    /// What the position fields represent for this format (RFC-044 §12).
    pub location_kind: LocationKind,
    /// Heading trail ("Guide > Install > Linux"), when structure exists.
    pub heading_path: Option<String>,
    pub location_quality: LocationQuality,
}

/// Extraction result for one file (RFC-005 §7; RFC-044 §10.3).
///
/// This payload is cached under the `extract-segments:v1` namespace
/// (Appendix A §7). Adding `warnings` is backward-compatible: existing
/// cache payloads deserialize with an empty warnings vec via the
/// `#[serde(default)]` attribute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractOutput {
    pub extractor_name: String,
    pub extractor_version: String,
    pub normalization_version: String,
    pub segments: Vec<ExtractedSegment>,
    pub char_count: u64,
    /// Structured warnings about partial or degraded extraction.
    /// Empty means the file was fully and cleanly processed.
    #[serde(default)]
    pub warnings: Vec<ExtractWarning>,
}

// ── Neutral chunk type (RFC-044 §14 Option B) ───────────────────────────

/// A chunk ready for the pipeline, with no dependency on `orbok-db`.
///
/// The pipeline layer (`orbok-workers`) maps this to
/// `orbok_db::repo::ChunkSpec`. This keeps `orbok-extract` free of any
/// database dependency (RFC-044 §14.6).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedChunk {
    pub chunk_kind: &'static str,
    pub chunk_ordinal: u32,
    pub heading_path: Option<String>,
    pub title: Option<String>,
    pub normalized_text: String,
    pub location_kind: LocationKind,
    pub line_start: u32,
    pub line_end: u32,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub location_quality: &'static str,
    pub parent_idx: Option<usize>,
}

// ── DocumentExtractor trait ─────────────────────────────────────────────

/// A document extractor (RFC-005 §6; RFC-044 §9.3 / §18.4).
///
/// Implementations must:
/// - read only through the [`ValidatedPath`] they are given;
/// - honour the limits in [`ExtractContext`];
/// - return typed failure categories, never panic on malformed input;
/// - populate `location_kind` correctly for their format.
pub trait DocumentExtractor: Send + Sync {
    /// Stable name recorded in `extraction_records.extractor_name`.
    fn name(&self) -> &'static str;
    /// Version recorded for staleness detection (RFC-005 §9).
    fn version(&self) -> &'static str;
    /// Extensions (lowercase, no dot) this extractor handles.
    fn supported_extensions(&self) -> &'static [&'static str];

    /// Extract and normalize, honoring resource limits.
    ///
    /// This is the primary entry point. The default implementation
    /// delegates to [`extract_legacy`] for backward compatibility during
    /// the migration period; built-in extractors override this directly.
    fn extract_with_context(
        &self,
        path: &ValidatedPath,
        context: &ExtractContext,
    ) -> OrbokResult<ExtractOutput> {
        // Default: forward to the legacy signature.
        // Remove once all built-in extractors are migrated.
        let _ = context; // context used by overrides
        self.extract(path)
    }

    /// Legacy entry point (no limits). Kept for the migration period;
    /// callers should prefer [`extract_with_context`].
    fn extract(&self, path: &ValidatedPath) -> OrbokResult<ExtractOutput>;
}

// ── Helper ──────────────────────────────────────────────────────────────

/// Classify a read failure (RFC-005 §13).
pub fn read_error_category(e: &std::io::Error) -> ErrorCategory {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => ErrorCategory::PermissionDenied,
        std::io::ErrorKind::NotFound => ErrorCategory::SourceMissing,
        _ => ErrorCategory::ReadError,
    }
}
