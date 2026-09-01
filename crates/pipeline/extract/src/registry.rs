//! Extractor registry (RFC-005 §6; RFC-044 §11 panic isolation).
//!
//! `ExtractorRegistry` is the single entry point for extraction.
//! `extract_safely` wraps every extractor call in `catch_unwind` so
//! a panic in a parser crate cannot crash the orbok process.

use crate::docx::DocxExtractor;
use crate::html::HtmlExtractor;
use crate::markdown::MarkdownExtractor;
use crate::pdf::PdfExtractor;
use crate::text::PlainTextExtractor;
use crate::types::{DocumentExtractor, ExtractContext, ExtractOutput};
use orbok_core::{ErrorCategory, OrbokError, OrbokResult};
use orbok_fs::ValidatedPath;

/// Registry of the available extractors. Markdown takes precedence over
/// plain text for `.md`; everything claims by extension.
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn DocumentExtractor>>,
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self {
            extractors: vec![
                Box::new(MarkdownExtractor),
                Box::new(DocxExtractor),
                Box::new(HtmlExtractor),
                Box::new(PlainTextExtractor),
                Box::new(PdfExtractor),
            ],
        }
    }
}

impl ExtractorRegistry {
    /// Build a registry with a custom set of extractors (useful in tests).
    pub fn new_with(extractors: Vec<Box<dyn DocumentExtractor>>) -> Self {
        Self { extractors }
    }

    /// The extractor claiming `extension`, if any.
    pub fn select(&self, extension: &str) -> Option<&dyn DocumentExtractor> {
        let ext = extension.to_ascii_lowercase();
        self.extractors
            .iter()
            .find(|e| e.supported_extensions().contains(&ext.as_str()))
            .map(|e| e.as_ref())
    }

    /// Extract using resource limits. Unknown types are a typed
    /// `UnsupportedType` failure — never a panic, never a silent skip.
    ///
    /// Prefer this over [`extract`] for production code paths.
    pub fn extract_with_context(
        &self,
        path: &ValidatedPath,
        context: &ExtractContext,
    ) -> OrbokResult<ExtractOutput> {
        let extension = path
            .canonical
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        match self.select(extension) {
            Some(extractor) => {
                tracing::debug!(extractor = extractor.name(), "extracting");
                extractor.extract_with_context(path, context)
            }
            None => Err(OrbokError::Extraction {
                category: ErrorCategory::UnsupportedType,
                message: format!("no extractor for extension '{extension}'"),
            }),
        }
    }

    /// Extract with panic isolation (RFC-044 §11).
    ///
    /// Wraps the extractor call in `catch_unwind`. A parser panic is
    /// caught and returned as `ErrorCategory::ParserPanic` instead of
    /// crashing the worker thread.
    ///
    /// The user-facing layer must translate `ParserPanic` to a plain
    /// message like "This file could not be prepared."
    pub fn extract_safely(
        &self,
        path: &ValidatedPath,
        context: &ExtractContext,
    ) -> OrbokResult<ExtractOutput> {
        let extension = path
            .canonical
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let extractor = match self.select(&extension) {
            Some(e) => e,
            None => {
                return Err(OrbokError::Extraction {
                    category: ErrorCategory::UnsupportedType,
                    message: format!("no extractor for extension '{extension}'"),
                });
            }
        };

        // Clone what we need to move into the closure.
        let path_clone = path.clone();
        let context_clone = context.clone();
        // NOTE: AssertUnwindSafe is appropriate here — extraction is
        // read-only on the path and context; no shared mutable state is
        // accessed inside the closure that could be left inconsistent by
        // a panic. (Task 034 §10, external audit: `SAFETY:` is reserved
        // for `unsafe` code; this closure is safe code.)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extractor.extract_with_context(&path_clone, &context_clone)
        }));

        match result {
            Ok(inner) => inner,
            Err(_payload) => {
                tracing::error!(
                    path = %path.canonical.display(),
                    extractor = extractor.name(),
                    "extractor panicked — recovered safely"
                );
                Err(OrbokError::Extraction {
                    category: ErrorCategory::ParserPanic,
                    message: "extractor panicked while reading this file".into(),
                })
            }
        }
    }

    /// Legacy entry point (no limits, no panic isolation).
    ///
    /// Kept for compatibility during the migration period. New code
    /// should call [`extract_safely`] instead.
    pub fn extract(&self, path: &ValidatedPath) -> OrbokResult<ExtractOutput> {
        self.extract_safely(path, &ExtractContext::default())
    }
}
