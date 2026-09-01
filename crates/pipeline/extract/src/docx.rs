//! DOCX text extractor (Microsoft Word 2007+; RFC-044 §16.4 resource limits).
//!
//! DOCX files are ZIP archives containing XML. This extractor reads
//! `word/document.xml` and strips XML tags to recover paragraph text.
//! Location quality is `Approximate` — DOCX does not provide byte
//! offsets; only paragraph order is preserved.
//!
//! Security: the file is opened with `zip::ZipArchive` which bounds
//! reads to the archive contents. No external entity expansion.

use crate::normalize::normalize_document;
use crate::types::{
    DocumentExtractor, ExtractContext, ExtractOutput, ExtractWarning, ExtractedSegment,
    LocationKind, LocationQuality, SegmentKind, read_error_category,
};
use orbok_core::{ErrorCategory, OrbokError, OrbokResult, versions::NORMALIZATION_VERSION};
use orbok_fs::ValidatedPath;
use std::io::Read;

const EXTRACTOR_NAME: &str = "docx";
const EXTRACTOR_VERSION: &str = "v1";

pub struct DocxExtractor;

impl DocumentExtractor for DocxExtractor {
    fn name(&self) -> &'static str {
        EXTRACTOR_NAME
    }

    fn version(&self) -> &'static str {
        EXTRACTOR_VERSION
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &["docx"]
    }

    fn extract_with_context(
        &self,
        path: &ValidatedPath,
        context: &ExtractContext,
    ) -> OrbokResult<ExtractOutput> {
        let limits = &context.limits;
        let mut warnings = Vec::new();

        // RFC-044 §9.5: check file size before opening ZIP.
        let meta = std::fs::metadata(&path.canonical).map_err(|e| OrbokError::Extraction {
            category: read_error_category(&e),
            message: e.to_string(),
        })?;
        if meta.len() > limits.max_zip_entry_bytes {
            return Err(OrbokError::Extraction {
                category: ErrorCategory::FileTooLarge,
                message: format!(
                    "DOCX file is {} bytes, limit is {}",
                    meta.len(),
                    limits.max_zip_entry_bytes
                ),
            });
        }

        let file = std::fs::File::open(&path.canonical).map_err(|e| OrbokError::Extraction {
            category: read_error_category(&e),
            message: e.to_string(),
        })?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| OrbokError::Extraction {
            category: ErrorCategory::ParserError,
            message: format!("docx zip: {e}"),
        })?;

        // RFC-044 §9.5: enforce per-entry XML size limit by bytes actually
        // read from decompression, never by `entry.size()` -- that field is
        // the uncompressed size *declared in the ZIP header*, which the
        // file's author writes. A small declared value used to take the
        // unconditional `read_to_string` branch below, unbounded regardless
        // of what decompression actually produced (Task 034 §3, audit
        // S-01). `take(limit + 1)` reads one byte past the cap so "hit the
        // cap" is distinguishable from "the entry was exactly the cap" --
        // `read` returning `limit` either means EOF is next or is truncated
        // right at the boundary; `limit + 1` bytes read only happens when
        // there really was more. `read_to_end` also fixes a second, latent
        // defect the old over-limit branch carried: a single `entry.read()`
        // call may return far fewer bytes than requested, silently
        // truncating to an arbitrary prefix shorter than the cap.
        let xml = match zip.by_name("word/document.xml") {
            Ok(mut entry) => {
                let limit = limits.max_docx_xml_bytes;
                let mut buf = Vec::new();
                let read = (&mut entry)
                    .take(limit + 1)
                    .read_to_end(&mut buf)
                    .map_err(|e| OrbokError::Extraction {
                        category: ErrorCategory::ParserError,
                        message: format!("docx xml read: {e}"),
                    })?;
                if read as u64 > limit {
                    warnings.push(ExtractWarning::SizeLimitReached {
                        limit_name: "max_docx_xml_bytes".into(),
                    });
                    buf.truncate(limit as usize);
                }
                // Best-effort UTF-8; invalid bytes → replacement chars.
                String::from_utf8_lossy(&buf).into_owned()
            }
            Err(_) => {
                return Err(OrbokError::Extraction {
                    category: ErrorCategory::UnsupportedFormat,
                    message: "no word/document.xml in archive".into(),
                });
            }
        };

        // Extract text runs from w:p paragraphs.
        let paragraphs = extract_paragraphs(&xml);
        let mut segments = Vec::new();
        let mut total_chars = 0u64;

        for (para_idx, para_text) in paragraphs.iter().enumerate() {
            // RFC-044 §9.5: segment and char limits.
            if segments.len() >= limits.max_segments {
                warnings.push(ExtractWarning::SizeLimitReached {
                    limit_name: "max_segments".into(),
                });
                break;
            }

            let norm = normalize_document(para_text);
            if norm.trim().is_empty() {
                continue;
            }
            let para_chars = norm.chars().count() as u64;
            if total_chars + para_chars > limits.max_extracted_chars {
                warnings.push(ExtractWarning::SizeLimitReached {
                    limit_name: "max_extracted_chars".into(),
                });
                break;
            }
            total_chars += para_chars;

            segments.push(ExtractedSegment {
                kind: SegmentKind::Paragraph,
                text: norm,
                line_start: (para_idx + 1) as u32,
                line_end: (para_idx + 1) as u32,
                location_kind: LocationKind::Paragraphs,
                heading_path: None,
                location_quality: LocationQuality::Approximate,
            });
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

/// Extract paragraph text from DOCX word/document.xml by collecting
/// all `w:t` text runs within each `w:p` paragraph element.
fn extract_paragraphs(xml: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current_para = String::new();
    let mut in_para = false;
    let mut pos = 0;
    let bytes = xml.as_bytes();

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            let end = bytes[pos..]
                .iter()
                .position(|&b| b == b'>')
                .map(|p| pos + p + 1)
                .unwrap_or(bytes.len());
            let tag = &xml[pos..end];
            if tag.starts_with("<w:p ") || tag == "<w:p>" {
                in_para = true;
                current_para.clear();
            } else if tag.starts_with("</w:p>") {
                if in_para && !current_para.trim().is_empty() {
                    paragraphs.push(current_para.trim().to_string());
                }
                in_para = false;
                current_para.clear();
            } else if in_para && (tag.starts_with("<w:t") || tag.starts_with("<w:t>")) {
                let text_start = end;
                let text_end = xml[text_start..]
                    .find("</w:t>")
                    .map(|p| text_start + p)
                    .unwrap_or(text_start);
                let text = &xml[text_start..text_end];
                let clean: String = {
                    let mut t = String::new();
                    let mut in_tag = false;
                    for c in text.chars() {
                        match c {
                            '<' => in_tag = true,
                            '>' => in_tag = false,
                            _ if !in_tag => t.push(c),
                            _ => {}
                        }
                    }
                    t
                };
                current_para.push_str(&clean);
                pos = text_end;
                continue;
            }
            pos = end;
        } else {
            pos += 1;
        }
    }
    paragraphs
}
