//! Privacy settings and local data visibility (RFC-039 §6, §17, as amended
//! 2026-09-25).
//!
//! This is the shared vocabulary for privacy settings. It lives in
//! `orbok-core` so that `orbok`, `orbok-ui`, and any future
//! diagnostics layer can all refer to the same types without a
//! circular dependency.

use serde::{Deserialize, Serialize};

// ── Privacy settings ──────────────────────────────────────────────────

/// Privacy preferences (RFC-039 §17, as amended 2026-09-25, Task 115).
///
/// Privacy is orbok's fixed safe defaults -- everything is processed on this
/// computer, and text-bearing caches are cleared from Storage -- plus **one**
/// control, *Remember recent searches*. There is no privacy *mode*: the mode
/// that used to sit here could not be set from any screen, and its only
/// effect was reachable by hand-editing `settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Whether to persist recent search queries. The whole truth about
    /// whether a search is recorded.
    pub remember_recent_searches: bool,
    /// Whether diagnostics may include raw filesystem paths (RFC-040).
    pub diagnostics_include_paths: bool,
    /// Whether diagnostics may include recent search queries (RFC-040).
    pub diagnostics_include_recent_searches: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            remember_recent_searches: true,
            diagnostics_include_paths: false,
            diagnostics_include_recent_searches: false,
        }
    }
}

impl PrivacySettings {
    /// Whether recent searches are recorded: just the toggle.
    pub fn effective_recent_searches(&self) -> bool {
        self.remember_recent_searches
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
/// All sensitive fields default to `false`. RFC-040 is accepted and unbuilt;
/// this policy is what it will read. It has no privacy-mode input: the mode
/// was removed (RFC-039 amendment, Task 115), and it behaves as the Standard
/// mode always did -- sensitive opt-ins may be offered, none is on.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticsPolicy {
    pub include_raw_paths: bool,
    pub include_folder_names: bool,
    pub include_recent_searches: bool,
    pub include_detailed_logs: bool,
}

impl DiagnosticsPolicy {
    /// Build from privacy settings.
    pub fn from_privacy(settings: &PrivacySettings) -> Self {
        Self {
            include_raw_paths: false,    // never enabled by default
            include_folder_names: false, // opt-in only
            include_recent_searches: settings.diagnostics_include_recent_searches,
            include_detailed_logs: false,
        }
    }

    /// Whether this policy permits showing sensitive opt-in checkboxes.
    /// Fixed at the Standard default until RFC-040 decides otherwise.
    pub fn allows_sensitive_optins(&self) -> bool {
        true
    }
}
