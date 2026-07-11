//! Plugin extractor interface (RFC-028 §7).
//!
//! This module defines the security-boundary types for external
//! extractor plugins. In v0.8, plugin *loading* is not yet implemented
//! (dynamic linking is deferred), but the interface is defined so that:
//!
//! 1. Built-in extractors can be registered with the same manifest.
//! 2. The security contract is formalized before any loading code exists.
//!
//! ## Security model (RFC-028 §6)
//!
//! - A plugin extractor receives only a `ValidatedPath` — it cannot
//!   request arbitrary filesystem access. The PathGuard boundary
//!   (RFC-003 §8) applies before any plugin receives a path.
//! - Plugin failures are isolated: a panic in a plugin extractor must
//!   not crash the orbok process (RFC-005 §13).
//! - User consent is required before a non-built-in plugin is used;
//!   the manifest provides the metadata for that consent dialog.
//! - Plugin logging must follow NFR-014: no document contents logged.
//!
//! ## Dynamic loading (future)
//!
//! When RFC-028 is fully activated, plugin `.so`/`.dll` files will be
//! located via the `PluginRegistry`. Until then, `PluginRegistry` only
//! holds the built-in extractors.

use crate::types::DocumentExtractor;

/// Metadata attached to every extractor plugin for display and consent.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    /// Stable identifier (e.g. `"excel-xlsx-v1"`). Must be unique.
    pub plugin_id: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Comma-separated list of handled file extensions.
    pub extensions: &'static [&'static str],
    /// Author name.
    pub author: &'static str,
    /// License (user sees this in the consent dialog).
    pub license: &'static str,
    /// Whether this plugin is built-in (no user consent required) or
    /// external (user must explicitly allow).
    pub builtin: bool,
    /// Privacy statement: what the plugin does NOT do.
    pub privacy_note: &'static str,
}

/// A plugin extractor: manifest metadata + the extraction implementation.
pub struct PluginExtractor {
    pub manifest: PluginManifest,
    pub extractor: Box<dyn DocumentExtractor>,
}

impl PluginExtractor {
    /// Wrap a built-in extractor with its manifest.
    pub fn builtin(manifest: PluginManifest, extractor: Box<dyn DocumentExtractor>) -> Self {
        debug_assert!(
            manifest.builtin,
            "use PluginExtractor::external for non-built-in plugins"
        );
        Self {
            manifest,
            extractor,
        }
    }
}

/// The plugin registry (RFC-028 §8).
///
/// In v0.8, only built-in plugins are registered. Dynamic loading is
/// gated behind `RFC-028` being fully activated.
pub struct PluginRegistry {
    plugins: Vec<PluginExtractor>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        use crate::docx::DocxExtractor;
        use crate::html::HtmlExtractor;
        use crate::markdown::MarkdownExtractor;
        use crate::pdf::PdfExtractor;
        use crate::text::PlainTextExtractor;
        let mut reg = Self {
            plugins: Vec::new(),
        };
        reg.register_builtin(
            PluginManifest {
                plugin_id: "docx-v1",
                display_name: "Microsoft Word (DOCX)",
                extensions: &["docx"],
                author: "orbok built-in",
                license: "Apache-2.0",
                builtin: true,
                privacy_note: "Does not transmit content externally.",
            },
            Box::new(DocxExtractor),
        );
        reg.register_builtin(
            PluginManifest {
                plugin_id: "html-v1",
                display_name: "HTML",
                extensions: &["html", "htm"],
                author: "orbok built-in",
                license: "Apache-2.0",
                builtin: true,
                privacy_note: "Does not transmit content externally.",
            },
            Box::new(HtmlExtractor),
        );
        reg.register_builtin(
            PluginManifest {
                plugin_id: "markdown-v1",
                display_name: "Markdown",
                extensions: &["md", "markdown"],
                author: "orbok built-in",
                license: "Apache-2.0",
                builtin: true,
                privacy_note: "Does not transmit content externally.",
            },
            Box::new(MarkdownExtractor),
        );
        reg.register_builtin(
            PluginManifest {
                plugin_id: "plain-text-v1",
                display_name: "Plain Text",
                extensions: &[
                    "txt", "log", "rs", "py", "js", "ts", "go", "sql", "toml", "yaml", "yml",
                    "json", "xml", "css", "html", "htm",
                ],
                author: "orbok built-in",
                license: "Apache-2.0",
                builtin: true,
                privacy_note: "Does not transmit content externally.",
            },
            Box::new(PlainTextExtractor),
        );
        reg.register_builtin(
            PluginManifest {
                plugin_id: "pdf-lopdf-v1",
                display_name: "PDF (lopdf)",
                extensions: &["pdf"],
                author: "orbok built-in",
                license: "Apache-2.0",
                builtin: true,
                privacy_note: "Extracts text locally. Does not transmit content externally.",
            },
            Box::new(PdfExtractor),
        );
        reg
    }
}

impl PluginRegistry {
    fn register_builtin(
        &mut self,
        manifest: PluginManifest,
        extractor: Box<dyn DocumentExtractor>,
    ) {
        self.plugins
            .push(PluginExtractor::builtin(manifest, extractor));
    }

    /// Find the plugin that handles the given extension.
    pub fn find_for_extension(&self, ext: &str) -> Option<&PluginExtractor> {
        let ext_lower = ext.to_ascii_lowercase();
        self.plugins
            .iter()
            .find(|p| p.manifest.extensions.contains(&ext_lower.as_str()))
    }

    /// All registered plugin manifests (for the Models/Settings view).
    pub fn manifests(&self) -> Vec<&PluginManifest> {
        self.plugins.iter().map(|p| &p.manifest).collect()
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}
