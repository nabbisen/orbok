//! PDF text extraction via lopdf (RFC-022 §6; RFC-044 §16.5 hardening).
//!
//! ## RFC-022 evaluation
//!
//! | Backend | Language | Japanese | License | Notes |
//! |---|---|---|---|---|
//! | **lopdf** (selected) | Rust | UTF-8 text only | MIT | Fast, pure Rust, page-level |
//! | pdfium | Rust binding | Full Unicode | Apache 2.0 | Requires native library |
//!
//! Selected: lopdf for v0.7. Pure Rust, no FFI, adequate for text-heavy
//! PDFs. Limitation: scanned / image-only PDFs produce no text.
//!
//! ## Security (RFC-015 §14)
//!
//! PDF parsing is treated as hostile input. Panics from lopdf are caught
//! in `ExtractorRegistry::extract_safely` (RFC-044 §11). All errors are
//! returned as typed `OrbokError::Extraction`.
//!
//! ## Location quality
//!
//! lopdf reports text at page granularity. All segments carry
//! `LocationKind::Pages` and `LocationQuality::PageOnly`.
//! UI must not label these as "line N".

use crate::normalize::normalize_document as normalize_text;
use crate::types::{
    DocumentExtractor, ExtractContext, ExtractOutput, ExtractWarning, ExtractedSegment,
    LocationKind, LocationQuality, SegmentKind, read_error_category,
};
use orbok_core::{ErrorCategory, OrbokError, OrbokResult, versions::NORMALIZATION_VERSION};
use orbok_fs::ValidatedPath;

const EXTRACTOR_NAME: &str = "pdf-lopdf";
const EXTRACTOR_VERSION: &str = "v1";

pub struct PdfExtractor;

impl DocumentExtractor for PdfExtractor {
    fn name(&self) -> &'static str {
        EXTRACTOR_NAME
    }

    fn version(&self) -> &'static str {
        EXTRACTOR_VERSION
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn extract_with_context(
        &self,
        path: &ValidatedPath,
        context: &ExtractContext,
    ) -> OrbokResult<ExtractOutput> {
        let limits = &context.limits;
        let mut warnings = Vec::new();

        // RFC-044 §9.5: check file size before loading PDF.
        let meta = std::fs::metadata(&path.canonical).map_err(|e| OrbokError::Extraction {
            category: read_error_category(&e),
            message: e.to_string(),
        })?;
        if meta.len() > limits.max_file_bytes {
            return Err(OrbokError::Extraction {
                category: ErrorCategory::FileTooLarge,
                message: format!(
                    "PDF is {} bytes, limit is {}",
                    meta.len(),
                    limits.max_file_bytes
                ),
            });
        }

        let doc = lopdf::Document::load(&path.canonical).map_err(|e| {
            let category =
                if e.to_string().contains("password") || e.to_string().contains("encrypt") {
                    ErrorCategory::EncryptedDocument
                } else {
                    ErrorCategory::ParserError
                };
            OrbokError::Extraction {
                category,
                message: format!("lopdf: {e}"),
            }
        })?;

        let pages: Vec<(u32, u16)> = doc.page_iter().collect();
        let total_pages = pages.len();

        // RFC-044 §9.5: page count limit.
        let pages_to_process = if total_pages > limits.max_pdf_pages {
            warnings.push(ExtractWarning::SizeLimitReached {
                limit_name: "max_pdf_pages".into(),
            });
            &pages[..limits.max_pdf_pages]
        } else {
            &pages[..]
        };

        let mut segments = Vec::new();
        let mut total_chars = 0u64;
        let mut unreadable_pages = Vec::new();

        for (page_idx, _) in pages_to_process.iter().enumerate() {
            let page_num = (page_idx + 1) as u32;

            // RFC-044 §9.5: extracted char limit.
            if total_chars >= limits.max_extracted_chars {
                warnings.push(ExtractWarning::SizeLimitReached {
                    limit_name: "max_extracted_chars".into(),
                });
                break;
            }

            // RFC-060 Amendment 1 §4a.1: `extract_text` takes 1-based page
            // *numbers* (it resolves them through `lopdf`'s own
            // `page_number -> object_id` map internally), not the page's
            // object ID. Passing the object ID here (`*obj_id`, the tuple
            // this loop used to destructure) silently returned no text for
            // any page whose object ID didn't equal its page number --
            // true of essentially every real PDF, since fonts, the page
            // tree, and content streams all consume object numbers first.
            match doc.extract_text(&[page_num]) {
                Ok(text) => {
                    if text.trim().is_empty() {
                        continue;
                    }
                    let normalized = normalize_text(&text);
                    if normalized.trim().is_empty() {
                        continue;
                    }
                    let page_chars = normalized.len() as u64;
                    total_chars += page_chars;
                    segments.push(ExtractedSegment {
                        kind: SegmentKind::Other,
                        text: normalized,
                        line_start: page_num,
                        line_end: page_num,
                        location_kind: LocationKind::Pages,
                        heading_path: Some(format!("Page {page_num}")),
                        location_quality: LocationQuality::PageOnly,
                    });
                }
                Err(_) => {
                    // Page-level failure: record and continue (RFC-005 §13).
                    unreadable_pages.push(page_num);
                }
            }
        }

        // Emit warnings for unreadable pages.
        if !unreadable_pages.is_empty() {
            warnings.push(ExtractWarning::SomePagesUnreadable {
                pages: unreadable_pages,
            });
        }

        // Detect scanned/image-only PDF (RFC-025).
        if total_pages > 0 && total_chars == 0 {
            tracing::debug!(
                path = %path.canonical.display(),
                pages = total_pages,
                "PDF produced no text — may be scanned/image-only"
            );
            warnings.push(ExtractWarning::PossiblyScannedPdf);
        }

        Ok(ExtractOutput {
            extractor_name: EXTRACTOR_NAME.to_string(),
            extractor_version: EXTRACTOR_VERSION.to_string(),
            normalization_version: NORMALIZATION_VERSION.to_string(),
            segments,
            char_count: total_chars,
            warnings,
        })
    }

    fn extract(&self, path: &ValidatedPath) -> OrbokResult<ExtractOutput> {
        self.extract_with_context(path, &ExtractContext::default())
    }
}

/// Detect whether a PDF appears to be scanned/image-only (RFC-025).
pub fn is_scanned_pdf(output: &ExtractOutput, page_count: usize) -> bool {
    page_count > 0 && output.char_count == 0
}

/// Helper: try to get page count from a PDF without failing.
pub fn pdf_page_count(path: &std::path::Path) -> usize {
    lopdf::Document::load(path)
        .map(|d| d.get_pages().len())
        .unwrap_or(0)
}
