//! Search history core types (RFC-042).
//!
//! These are the shared vocabulary types — no DB, no UI, no iced deps.
//! `orbok-db` builds the repository on top of these; `orbok-ui` holds
//! the view-model slices.
//!
//! Design constraints (RFC-042 §7, §8):
//! - A history entry stores *instructions*, not frozen results.
//! - No snippets, embeddings, ranking scores, or document text.
//! - Max 20 entries by default; deduplication on (search_text, filters).

use serde::{Deserialize, Serialize};

// ── Identifiers ───────────────────────────────────────────────────────

/// Opaque identifier for a `SearchHistoryEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SearchHistoryId(pub String);

impl SearchHistoryId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── Stored filter ─────────────────────────────────────────────────────

/// A narrowing choice as stored in search history (RFC-042 §7.2).
///
/// This mirrors `ActiveFilter` from `orbok-search` but is self-contained
/// so `orbok-core` does not depend on `orbok-search`. Conversion is
/// provided by `impl From<&ActiveFilter> for StoredSearchFilter` in
/// `orbok-search`.
///
/// Each variant carries a human-readable `label` (the chip text at the
/// time of the search). The label is stored so the history list can be
/// displayed without rehydrating the filter — i.e. the UI shows exactly
/// what the user typed / chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredSearchFilter {
    Folder {
        id: String,
        label: String,
    },
    Kind {
        value: StoredKindFilter,
        label: String,
    },
    Changed {
        value: StoredChangedFilter,
        label: String,
    },
    ReadyStatus {
        value: StoredReadyFilter,
        label: String,
    },
    SearchStyle {
        value: StoredSearchStyle,
        label: String,
    },
    Language {
        value: StoredLanguageFilter,
        label: String,
    },
}

impl StoredSearchFilter {
    /// The user-facing label stored at search time.
    pub fn label(&self) -> &str {
        match self {
            Self::Folder { label, .. }
            | Self::Kind { label, .. }
            | Self::Changed { label, .. }
            | Self::ReadyStatus { label, .. }
            | Self::SearchStyle { label, .. }
            | Self::Language { label, .. } => label,
        }
    }

    /// Whether this filter still refers to a valid folder id. Used when
    /// restoring: if the folder no longer exists in the catalog the filter
    /// is dropped (RFC-042 §9 step 3).
    pub fn folder_id(&self) -> Option<&str> {
        match self {
            Self::Folder { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }
}

/// Compact storage mirror of `KindFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredKindFilter {
    Documents,
    Pdfs,
    Notes,
    Code,
    Spreadsheets,
}

/// Compact storage mirror of `ChangedFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredChangedFilter {
    AnyTime,
    Today,
    ThisWeek,
    ThisMonth,
    ThisYear,
}

/// Compact storage mirror of `ReadyFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredReadyFilter {
    Ready,
    NeedsUpdate,
    FileNotFound,
    PartlyPrepared,
}

/// Compact storage mirror of `SearchStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredSearchStyle {
    BestResults,
    ExactWords,
    Meaning,
}

/// Compact storage mirror of `LanguageFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredLanguageFilter {
    Any,
    English,
    Japanese,
    Mixed,
}

// ── History entry ─────────────────────────────────────────────────────

/// One recent search (RFC-042 §7.1). Stores instructions, not results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    pub id: SearchHistoryId,
    /// The search text as the user typed it.
    pub search_text: String,
    /// Active narrowing choices at the time of the search.
    pub filters: Vec<StoredSearchFilter>,
    /// ISO-8601 UTC timestamp: when this entry was first created.
    pub created_at: String,
    /// ISO-8601 UTC timestamp: when this entry was last used (searched again
    /// or created). Updated on deduplication (RFC-042 §8.4).
    pub last_used_at: String,
    /// How many results the search returned. `None` on first store before
    /// count is known; kept for display purposes only.
    pub previous_result_count: Option<usize>,
    /// Locale at search time — lets the history list display the entry in
    /// the locale it was created with.
    pub locale: String,
}

impl SearchHistoryEntry {
    /// A short summary suitable for screen-reader labels (RFC-042 §15):
    /// `"<text>, <filter>, <filter>, <time>"`.
    pub fn accessible_label(&self) -> String {
        let mut parts = vec![self.search_text.clone()];
        for f in &self.filters {
            parts.push(f.label().to_string());
        }
        parts.push(self.last_used_at.clone());
        parts.join(", ")
    }
}

// ── Settings ──────────────────────────────────────────────────────────

/// Per-user search history preferences (RFC-042 §7.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistorySettings {
    /// Master on/off switch. When `false`, no new entries are created.
    pub remember_recent_searches: bool,
    /// Maximum number of entries to keep. Oldest entries are evicted.
    pub max_entries: usize,
    /// If `true`, history is cleared automatically when strict privacy mode
    /// is enabled (RFC-042 §14).
    pub clear_when_privacy_strict: bool,
}

impl Default for SearchHistorySettings {
    fn default() -> Self {
        Self {
            remember_recent_searches: true,
            max_entries: 20,
            clear_when_privacy_strict: true,
        }
    }
}
