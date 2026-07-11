//! Privacy modes and local data visibility (RFC-039 §5–§6, §17).
//!
//! This is the shared vocabulary for privacy settings. It lives in
//! `orbok-core` so that `orbok`, `orbok-ui`, and any future
//! diagnostics layer can all refer to the same types without a
//! circular dependency.

use serde::{Deserialize, Serialize};

// ── Privacy mode ──────────────────────────────────────────────────────

/// Top-level privacy mode for the app (RFC-039 §5).
///
/// User-facing copy:
/// - `Standard`    → "Documents are processed on this computer only."
/// - `Strict`      → "Strict privacy reduces what orbok remembers."
/// - `Portable`    → "orbok stores app data next to this copy of the app."
/// - `Diagnostics` → "Include extra details for troubleshooting."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// Default — safe for most users.
    #[default]
    Standard,
    /// Reduced local footprint for sensitive environments.
    Strict,
    /// Data lives next to the portable app copy.
    Portable,
    /// Temporary opt-in for troubleshooting (must be explicitly enabled).
    Diagnostics,
}

impl PrivacyMode {
    /// Stable settings string for persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyMode::Standard => "standard",
            PrivacyMode::Strict => "strict",
            PrivacyMode::Portable => "portable",
            PrivacyMode::Diagnostics => "diagnostics",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "strict" => PrivacyMode::Strict,
            "portable" => PrivacyMode::Portable,
            "diagnostics" => PrivacyMode::Diagnostics,
            _ => PrivacyMode::Standard,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self::parse(s)
    }

    /// Whether recent searches should be stored in this mode (RFC-039 §10).
    pub fn allows_recent_searches(self) -> bool {
        !matches!(self, PrivacyMode::Strict)
    }

    /// Whether snippet / preview caching is allowed (RFC-039 §11).
    pub fn allows_snippet_persistence(self) -> bool {
        !matches!(self, PrivacyMode::Strict)
    }

    /// Whether sensitive diagnostics opt-ins are shown (RFC-039 §14).
    pub fn allows_diagnostics_sensitive_optins(self) -> bool {
        !matches!(self, PrivacyMode::Strict)
    }
}

// ── Privacy settings ──────────────────────────────────────────────────

/// Fine-grained privacy preferences (RFC-039 §17).
///
/// `mode` governs defaults; individual fields may further restrict
/// behavior. Strict mode forces some fields to their most private value
/// regardless of what the user previously selected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub mode: PrivacyMode,
    /// Whether to persist recent search queries.
    pub remember_recent_searches: bool,
    /// Whether to store snippet previews across sessions.
    pub persist_snippets: bool,
    /// Whether to clear temporary previews when the app exits.
    pub clear_temporary_previews_on_exit: bool,
    /// Whether diagnostics may include raw filesystem paths.
    pub diagnostics_include_paths: bool,
    /// Whether diagnostics may include recent search queries.
    pub diagnostics_include_recent_searches: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            mode: PrivacyMode::Standard,
            remember_recent_searches: true,
            persist_snippets: true,
            clear_temporary_previews_on_exit: false,
            diagnostics_include_paths: false,
            diagnostics_include_recent_searches: false,
        }
    }
}

impl PrivacySettings {
    /// Apply strict-mode overrides (RFC-039 §9).
    ///
    /// Strict mode forces the most private values for settings it
    /// controls, regardless of individual field values.
    pub fn with_mode_applied(mut self) -> Self {
        if self.mode == PrivacyMode::Strict {
            self.remember_recent_searches = false;
            self.persist_snippets = false;
            self.diagnostics_include_paths = false;
            self.diagnostics_include_recent_searches = false;
        }
        self
    }

    /// Effective value for recent searches, accounting for mode.
    pub fn effective_recent_searches(&self) -> bool {
        self.mode.allows_recent_searches() && self.remember_recent_searches
    }

    /// Effective value for snippet persistence, accounting for mode.
    pub fn effective_snippet_persistence(&self) -> bool {
        self.mode.allows_snippet_persistence() && self.persist_snippets
    }
}

// ── Local data category ───────────────────────────────────────────────

/// Classified local data categories for the storage dashboard
/// and cleanup controls (RFC-039 §6, §15, §16).
///
/// User-facing labels must avoid technical terms — see RFC-039 §15
/// for the mapping (`KeywordIndex` → "Search data", etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalDataCategory {
    SourcePaths,
    FileMetadata,
    ExtractedText,
    KeywordIndex,
    Embeddings,
    Snippets,
    TemporaryPreviews,
    RecentSearches,
    Logs,
    Diagnostics,
    ModelFiles,
    Settings,
}

impl LocalDataCategory {
    /// Plain-language user label (RFC-039 §15 — no "cache/catalog/vector").
    pub fn user_label(self) -> &'static str {
        match self {
            LocalDataCategory::SourcePaths => "Folder list",
            LocalDataCategory::FileMetadata => "File information",
            LocalDataCategory::ExtractedText => "Prepared text",
            LocalDataCategory::KeywordIndex => "Search data",
            LocalDataCategory::Embeddings => "Better search data",
            LocalDataCategory::Snippets => "Temporary previews",
            LocalDataCategory::TemporaryPreviews => "Temporary previews",
            LocalDataCategory::RecentSearches => "Recent searches",
            LocalDataCategory::Logs => "Logs",
            LocalDataCategory::Diagnostics => "Support files",
            LocalDataCategory::ModelFiles => "Search helper",
            LocalDataCategory::Settings => "App settings",
        }
    }
}

// ── Diagnostics policy ────────────────────────────────────────────────

/// Policy governing what a diagnostics export may include (RFC-040 §12).
///
/// All sensitive fields default to `false`. Strict privacy mode
/// prevents enabling them.
#[derive(Debug, Clone)]
pub struct DiagnosticsPolicy {
    pub include_raw_paths: bool,
    pub include_folder_names: bool,
    pub include_recent_searches: bool,
    pub include_detailed_logs: bool,
    pub privacy_mode: PrivacyMode,
}

impl Default for DiagnosticsPolicy {
    fn default() -> Self {
        Self {
            include_raw_paths: false,
            include_folder_names: false,
            include_recent_searches: false,
            include_detailed_logs: false,
            privacy_mode: PrivacyMode::Standard,
        }
    }
}

impl DiagnosticsPolicy {
    /// Build from privacy settings, enforcing strict-mode restrictions.
    pub fn from_privacy(settings: &PrivacySettings) -> Self {
        let strict = settings.mode == PrivacyMode::Strict;
        Self {
            include_raw_paths: false,    // never enabled by default
            include_folder_names: false, // opt-in only
            include_recent_searches: if strict {
                false
            } else {
                settings.diagnostics_include_recent_searches
            },
            include_detailed_logs: false,
            privacy_mode: settings.mode,
        }
    }

    /// Whether this policy permits showing sensitive opt-in checkboxes.
    pub fn allows_sensitive_optins(&self) -> bool {
        self.privacy_mode.allows_diagnostics_sensitive_optins()
    }
}
