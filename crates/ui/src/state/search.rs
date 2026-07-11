//! Search-specific view-model types (RFC-041 §16, RFC-038 §12).
//!
//! These are plain data — no iced types. They live here so `state.rs`
//! stays focused on the top-level app model.

use orbok_core::{SearchHistoryEntry, SearchHistoryId};
use orbok_search::{ActiveFilter, ResultRecoveryAction, ResultTrustState, SuggestedFilter};

// ── Results status ────────────────────────────────────────────────────

/// Search results lifecycle in the UI (RFC-041 §16.4).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ResultsStatus {
    /// User has never submitted a search yet.
    #[default]
    NotSearchedYet,
    /// Background preparation is running before the first search.
    Preparing,
    /// A search is in flight.
    Searching,
    /// Filters changed and results are refreshing.
    Updating,
    /// Results ready; `total_count` includes all active-filter matches.
    Ready { total_count: usize },
    /// No folders have been added yet.
    EmptyBeforeAnyFolder,
    /// Search ran but found nothing (no active filters).
    EmptyAfterSearch,
    /// Search found results but active filters reduced them to zero.
    EmptyAfterFiltering,
    /// Something went wrong; `friendly_message` is safe to show directly.
    Problem { friendly_message: String },
}

// ── Trust display ─────────────────────────────────────────────────────

/// Trust badge and actions surfaced for one result card (RFC-038 §6).
#[derive(Debug, Clone, PartialEq)]
pub struct ResultTrustDisplay {
    pub state: ResultTrustState,
    pub recovery_actions: Vec<ResultRecoveryAction>,
}

impl Default for ResultTrustDisplay {
    fn default() -> Self {
        Self {
            state: ResultTrustState::Ready,
            recovery_actions: Vec::new(),
        }
    }
}

// ── Search UI state ───────────────────────────────────────────────────

/// Complete search UI state (RFC-041 §16.1).
///
/// Sits inside `AppState` and is updated by search-related `Message`
/// variants. All filter operations preserve `text` (RFC-041 §15.4).
#[derive(Debug, Clone, Default)]
pub struct SearchUiState {
    /// Current text in the search input.
    pub text: String,
    /// Active narrowing chips shown in "Narrowed by".
    pub active_filters: Vec<ActiveFilter>,
    /// Quick chip suggestions shown after results appear.
    pub suggested_filters: Vec<SuggestedFilter>,
    /// Whether the "More ways to narrow" panel is open.
    pub more_panel_open: bool,
    /// Current results lifecycle.
    pub results_status: ResultsStatus,
    /// Index of the selected result card, if any.
    pub selected_result_idx: Option<usize>,
    /// RFC-042: cached recent search entries (loaded at startup, refreshed
    /// after every successful search or history mutation).
    pub history: Vec<SearchHistoryEntry>,
    /// RFC-042: whether the Recent searches panel is open.
    pub history_panel_open: bool,
    /// RFC-042: set while a history entry is being restored; drives the
    /// "Searching again…" status copy (RFC-042 §9 step 5).
    pub restoring_history_id: Option<SearchHistoryId>,
}

impl SearchUiState {
    /// Apply a suggested filter at index `i`.
    /// Does nothing if `i` is out of range or the filter is already active.
    pub fn apply_suggested(&mut self, i: usize) {
        if let Some(s) = self.suggested_filters.get(i).cloned() {
            if !orbok_search::filter::is_already_active(&self.active_filters, &s.filter) {
                self.active_filters.push(s.filter);
            }
        }
    }

    /// Remove the active filter at index `i` (RFC-041 §15.2).
    pub fn remove_filter(&mut self, i: usize) {
        if i < self.active_filters.len() {
            self.active_filters.remove(i);
        }
    }

    /// Remove all active filters without clearing search text (RFC-041 §15.3).
    pub fn clear_filters(&mut self) {
        self.active_filters.clear();
        self.suggested_filters.clear();
    }

    /// Whether any narrowing is active.
    pub fn has_active_filters(&self) -> bool {
        !self.active_filters.is_empty()
    }
}
