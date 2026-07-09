//! Search filter model (RFC-041 §12, §16).
//!
//! Filters are applied *after* results appear, not before search.
//! Each filter carries a user-facing `label` string so active chips
//! remain stable even if the underlying data changes.

use serde::{Deserialize, Serialize};

// ── Per-filter value types ────────────────────────────────────────────

/// File kind / document type filter (RFC-041 §12.2).
///
/// User label: "Kind". Options surface as chips like "PDFs", "Notes".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindFilter {
    Documents,
    Pdfs,
    Notes,
    Code,
    Spreadsheets,
}

impl KindFilter {
    pub fn label(&self) -> &'static str {
        match self {
            KindFilter::Documents => "Documents",
            KindFilter::Pdfs => "PDFs",
            KindFilter::Notes => "Notes",
            KindFilter::Code => "Code",
            KindFilter::Spreadsheets => "Spreadsheets",
        }
    }

    /// File extensions included in this kind.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            KindFilter::Documents => &["docx", "doc", "odt", "rtf"],
            KindFilter::Pdfs => &["pdf"],
            KindFilter::Notes => &["md", "markdown", "txt"],
            KindFilter::Code => &[
                "rs", "py", "js", "ts", "java", "c", "h", "cpp", "hpp", "go", "rb", "sh", "toml",
                "yaml", "yml", "json", "sql",
            ],
            KindFilter::Spreadsheets => &["csv", "xlsx", "xls", "ods"],
        }
    }
}

/// Modified-time filter (RFC-041 §12.3). User label: "Changed".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangedFilter {
    AnyTime,
    Today,
    ThisWeek,
    ThisMonth,
    ThisYear,
}

impl ChangedFilter {
    pub fn label(&self) -> &'static str {
        match self {
            ChangedFilter::AnyTime => "Any time",
            ChangedFilter::Today => "Today",
            ChangedFilter::ThisWeek => "This week",
            ChangedFilter::ThisMonth => "This month",
            ChangedFilter::ThisYear => "This year",
        }
    }
}

/// Ready-status filter (RFC-041 §12.4). User label: "Ready status".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadyFilter {
    Ready,
    NeedsUpdate,
    FileNotFound,
    PartlyPrepared,
}

impl ReadyFilter {
    pub fn label(&self) -> &'static str {
        match self {
            ReadyFilter::Ready => "Ready",
            ReadyFilter::NeedsUpdate => "Needs update",
            ReadyFilter::FileNotFound => "File not found",
            ReadyFilter::PartlyPrepared => "Partly prepared",
        }
    }
}

/// Search style — shown only in Advanced view (RFC-041 §12.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchStyle {
    BestResults,
    ExactWords,
    Meaning,
}

impl SearchStyle {
    pub fn label(self) -> &'static str {
        match self {
            SearchStyle::BestResults => "Best results",
            SearchStyle::ExactWords => "Exact words",
            SearchStyle::Meaning => "Meaning",
        }
    }
}

/// Language filter — shown only when detection is reliable (RFC-041 §12.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageFilter {
    Any,
    English,
    Japanese,
    Mixed,
}

impl LanguageFilter {
    pub fn label(&self) -> &'static str {
        match self {
            LanguageFilter::Any => "Any language",
            LanguageFilter::English => "English",
            LanguageFilter::Japanese => "Japanese",
            LanguageFilter::Mixed => "Mixed",
        }
    }
}

// ── Active filter ─────────────────────────────────────────────────────

/// One active narrowing choice in the "Narrowed by" row (RFC-041 §16.2).
///
/// The `label` field is set when the filter is created and cached so
/// that active chips never flicker even if the backing data changes.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveFilter {
    Folder {
        /// Opaque source ID from the catalog.
        id: String,
        label: String,
    },
    Kind {
        value: KindFilter,
        label: String,
    },
    Changed {
        value: ChangedFilter,
        label: String,
    },
    ReadyStatus {
        value: ReadyFilter,
        label: String,
    },
    SearchStyle {
        value: SearchStyle,
        label: String,
    },
    Language {
        value: LanguageFilter,
        label: String,
    },
}

impl ActiveFilter {
    /// Stable user-facing label for the active chip.
    pub fn label(&self) -> &str {
        match self {
            ActiveFilter::Folder { label, .. }
            | ActiveFilter::Kind { label, .. }
            | ActiveFilter::Changed { label, .. }
            | ActiveFilter::ReadyStatus { label, .. }
            | ActiveFilter::SearchStyle { label, .. }
            | ActiveFilter::Language { label, .. } => label,
        }
    }
}

// ── Suggested filter ──────────────────────────────────────────────────

/// A post-results chip suggestion (RFC-041 §13, §16.3).
///
/// `estimated_result_count` must not appear in the default UI.
#[derive(Debug, Clone)]
pub struct SuggestedFilter {
    pub filter: ActiveFilter,
    /// Estimated result count if this filter were applied.
    /// Do not show this number in the default UI (RFC-041 §16.3).
    pub estimated_result_count: usize,
}

// ── Filter application ────────────────────────────────────────────────

/// Check whether a result's file extension matches a `KindFilter`.
pub fn extension_matches_kind(extension: &str, kind: &KindFilter) -> bool {
    let ext = extension.to_ascii_lowercase();
    kind.extensions().contains(&ext.as_str())
}

/// Check whether a filter is already in the active list.
pub fn is_already_active(filters: &[ActiveFilter], candidate: &ActiveFilter) -> bool {
    filters.iter().any(|f| match (f, candidate) {
        (ActiveFilter::Folder { id: a, .. }, ActiveFilter::Folder { id: b, .. }) => a == b,
        (ActiveFilter::Kind { value: a, .. }, ActiveFilter::Kind { value: b, .. }) => a == b,
        (ActiveFilter::Changed { value: a, .. }, ActiveFilter::Changed { value: b, .. }) => a == b,
        (
            ActiveFilter::ReadyStatus { value: a, .. },
            ActiveFilter::ReadyStatus { value: b, .. },
        ) => a == b,
        (
            ActiveFilter::SearchStyle { value: a, .. },
            ActiveFilter::SearchStyle { value: b, .. },
        ) => a == b,
        (ActiveFilter::Language { value: a, .. }, ActiveFilter::Language { value: b, .. }) => {
            a == b
        }
        _ => false,
    })
}

// ── History conversion ────────────────────────────────────────────────

use orbok_core::{
    StoredChangedFilter, StoredKindFilter, StoredLanguageFilter, StoredReadyFilter,
    StoredSearchFilter, StoredSearchStyle,
};

impl From<&ActiveFilter> for StoredSearchFilter {
    fn from(f: &ActiveFilter) -> Self {
        match f {
            ActiveFilter::Folder { id, label } => StoredSearchFilter::Folder {
                id: id.clone(),
                label: label.clone(),
            },
            ActiveFilter::Kind { value, label } => StoredSearchFilter::Kind {
                value: match value {
                    KindFilter::Documents => StoredKindFilter::Documents,
                    KindFilter::Pdfs => StoredKindFilter::Pdfs,
                    KindFilter::Notes => StoredKindFilter::Notes,
                    KindFilter::Code => StoredKindFilter::Code,
                    KindFilter::Spreadsheets => StoredKindFilter::Spreadsheets,
                },
                label: label.clone(),
            },
            ActiveFilter::Changed { value, label } => StoredSearchFilter::Changed {
                value: match value {
                    ChangedFilter::AnyTime => StoredChangedFilter::AnyTime,
                    ChangedFilter::Today => StoredChangedFilter::Today,
                    ChangedFilter::ThisWeek => StoredChangedFilter::ThisWeek,
                    ChangedFilter::ThisMonth => StoredChangedFilter::ThisMonth,
                    ChangedFilter::ThisYear => StoredChangedFilter::ThisYear,
                },
                label: label.clone(),
            },
            ActiveFilter::ReadyStatus { value, label } => StoredSearchFilter::ReadyStatus {
                value: match value {
                    ReadyFilter::Ready => StoredReadyFilter::Ready,
                    ReadyFilter::NeedsUpdate => StoredReadyFilter::NeedsUpdate,
                    ReadyFilter::FileNotFound => StoredReadyFilter::FileNotFound,
                    ReadyFilter::PartlyPrepared => StoredReadyFilter::PartlyPrepared,
                },
                label: label.clone(),
            },
            ActiveFilter::SearchStyle { value, label } => StoredSearchFilter::SearchStyle {
                value: match value {
                    SearchStyle::BestResults => StoredSearchStyle::BestResults,
                    SearchStyle::ExactWords => StoredSearchStyle::ExactWords,
                    SearchStyle::Meaning => StoredSearchStyle::Meaning,
                },
                label: label.clone(),
            },
            ActiveFilter::Language { value, label } => StoredSearchFilter::Language {
                value: match value {
                    LanguageFilter::Any => StoredLanguageFilter::Any,
                    LanguageFilter::English => StoredLanguageFilter::English,
                    LanguageFilter::Japanese => StoredLanguageFilter::Japanese,
                    LanguageFilter::Mixed => StoredLanguageFilter::Mixed,
                },
                label: label.clone(),
            },
        }
    }
}
