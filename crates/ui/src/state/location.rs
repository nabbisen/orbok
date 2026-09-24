//! Search-location view-model types (RFC-045 §5, §17).
//!
//! A *search location* is "where the current search looks" (RFC-045 §5.1).
//! These are plain data — no iced types — so the state stays testable
//! without a display server, mirroring the `search` sibling module.
//!
//! User-facing copy never lives here: the chosen folder's display *name*
//! is carried as a plain `String`, and the friendly chip label is built
//! through the i18n catalog (see [`crate::i18n::search_location_chip`]),
//! so RFC-031 (every visible string is translated and compile-checked)
//! still holds.
//!
//! This module is introduced by PR 1 of the RFC-045 task plan: it adds
//! the data types and default state only. No folder picker, message, or
//! view behavior changes here — those land in later PRs.

use orbok_core::id::SourceId;

// ── Search scope ──────────────────────────────────────────────────────

/// Whether a folder search includes nested subfolders (RFC-045 §5.3).
///
/// The default is [`SearchFolderScope::FolderAndSubfolders`]: most users
/// expect a folder search to include files inside nested folders, which
/// reduces surprise when documents live a few levels down (RFC-045 §6.3).
///
/// In P0 the scope is a **search-time restriction**, not a separate
/// remembered-folder identity (RFC-045 §6.3): switching between
/// "and subfolders" and "only" for the same folder must never create a
/// second remembered folder. The folder can still be prepared
/// recursively; the scope simply narrows which prepared files are
/// eligible for the current search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchFolderScope {
    /// Search the chosen folder and everything beneath it.
    #[default]
    FolderAndSubfolders,
    /// Restrict the search to files directly in the chosen folder.
    FolderOnly,
}

impl SearchFolderScope {
    /// Whether this scope reaches into nested subfolders.
    pub fn includes_subfolders(self) -> bool {
        matches!(self, SearchFolderScope::FolderAndSubfolders)
    }
}

// ── Search location ───────────────────────────────────────────────────

/// Where the current search looks (RFC-045 §17).
///
/// For P0 the only case is a [`SearchLocation::Remembered`] folder: a
/// folder chosen from search is created or reused as a remembered folder
/// (RFC-045 §6.2, §8.4). The `Transient` ("just this time") case from
/// RFC-045 §9 is deferred to P1 and is intentionally not represented yet,
/// so the build stays warning-free until it is actually constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchLocation {
    /// A folder orbok remembers and prepares over time. Internally backed
    /// by a source record (RFC-045 §13); the UI only ever shows the
    /// friendly `display_name`, never the `source_id`.
    Remembered {
        source_id: SourceId,
        display_name: String,
        scope: SearchFolderScope,
        /// Task 113: when the location is a folder **inside** the remembered
        /// one (the user chose a subfolder of an added folder), that
        /// subfolder's canonical path. `source_id` is then the added folder
        /// that covers it, and `display_name` is the subfolder's name.
        limit_path: Option<String>,
    },
}

impl SearchLocation {
    /// Construct a remembered-folder location with the default scope
    /// (`FolderAndSubfolders`).
    pub fn remembered(source_id: SourceId, display_name: impl Into<String>) -> Self {
        SearchLocation::Remembered {
            source_id,
            display_name: display_name.into(),
            scope: SearchFolderScope::default(),
            limit_path: None,
        }
    }

    /// A location that is a folder inside the remembered folder `source_id`
    /// (Task 113): nothing is registered for it; the search is limited to
    /// files under `limit_path`.
    pub fn within(
        source_id: SourceId,
        display_name: impl Into<String>,
        limit_path: impl Into<String>,
    ) -> Self {
        SearchLocation::Remembered {
            source_id,
            display_name: display_name.into(),
            scope: SearchFolderScope::default(),
            limit_path: Some(limit_path.into()),
        }
    }

    /// The subfolder this location is limited to, if it is one (Task 113).
    pub fn limit_path(&self) -> Option<&str> {
        match self {
            SearchLocation::Remembered { limit_path, .. } => limit_path.as_deref(),
        }
    }

    /// The folder's friendly display name (e.g. `Documents`).
    pub fn display_name(&self) -> &str {
        match self {
            SearchLocation::Remembered { display_name, .. } => display_name,
        }
    }

    /// The remembered folder's source id, if this location is backed by
    /// one. Always `Some` in P0; kept as an `Option` so the P1 transient
    /// case can return `None` without a signature change.
    pub fn source_id(&self) -> Option<&SourceId> {
        match self {
            SearchLocation::Remembered { source_id, .. } => Some(source_id),
        }
    }

    /// The current search scope for this location.
    pub fn scope(&self) -> SearchFolderScope {
        match self {
            SearchLocation::Remembered { scope, .. } => *scope,
        }
    }

    /// Return a copy of this location with a different scope, preserving
    /// folder identity. Changing scope must not change which remembered
    /// folder this is (RFC-045 §6.3) — only the search-time restriction.
    pub fn with_scope(mut self, scope: SearchFolderScope) -> Self {
        match &mut self {
            SearchLocation::Remembered { scope: s, .. } => *s = scope,
        }
        self
    }
}

// ── Recent / remembered folder summary ────────────────────────────────

/// A compact remembered-folder entry for the recent-folder chips
/// (RFC-045 §7.4). Carries only what a chip needs: a friendly name and
/// the source id to select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLocationSummary {
    pub source_id: SourceId,
    pub display_name: String,
}

// ── Search-location state ─────────────────────────────────────────────

/// The "where to search" portion of the search UI state (RFC-045 §17).
///
/// Sits alongside `SearchUiState` in `AppState`. Defaults to no selected
/// location (the first-run empty state, RFC-045 §7.1), an empty recent
/// list, and no picker in flight.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchLocationState {
    /// The current search location, or `None` before a folder is chosen.
    pub selected: Option<SearchLocation>,
    /// Recent / remembered folders offered as quick chips (RFC-045 §7.4).
    pub recent_locations: Vec<SearchLocationSummary>,
    /// True while the OS folder picker is open, to guard against opening
    /// duplicate dialogs on repeated Search clicks (RFC-045 §19.0).
    pub picker_in_progress: bool,
    /// Task 068: the query in the box when the picker opened for a search,
    /// resumed once a folder is picked. Held here because the portal picker
    /// is not modal -- the box can change while it is open.
    pub pending_query: Option<String>,
}

impl SearchLocationState {
    /// Whether a search location is currently selected.
    pub fn has_selected(&self) -> bool {
        self.selected.is_some()
    }

    /// Clear the selected location (RFC-045 §11.3). Search text is held
    /// elsewhere (`AppState::query`) and is intentionally untouched.
    pub fn clear(&mut self) {
        self.selected = None;
    }

    /// Change the scope of the selected location in place, preserving the
    /// remembered-folder identity (RFC-045 §6.3). No-op when nothing is
    /// selected.
    pub fn set_scope(&mut self, scope: SearchFolderScope) {
        if let Some(location) = self.selected.take() {
            self.selected = Some(location.with_scope(scope));
        }
    }
}
