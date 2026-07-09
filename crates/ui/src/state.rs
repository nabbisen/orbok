//! Headless UI state (view models) and the message vocabulary.
//!
//! Everything here is plain data — testable without a display server.
//! `orbok` populates these structs from backend services; views
//! render them; `update` mutates them. No iced types appear in this
//! module so state logic stays UI-framework-agnostic.

pub mod location;
pub mod search;

pub use location::{SearchFolderScope, SearchLocation, SearchLocationState, SearchLocationSummary};
pub use search::{ResultTrustDisplay, ResultsStatus, SearchUiState};

use crate::i18n::Locale;
use crate::notice::UserNotice;
use orbok_core::{SearchHistoryEntry, SearchHistoryId};
use orbok_models::SearchCapability;
use orbok_search::{ResultRecoveryAction, SearchMode};

/// Top-level navigation group for the two-level sidebar + tab layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavGroup {
    Search,
    Ai,
    Settings,
}

/// Top-level pages (GUI external design §3.1 order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewId {
    Search,
    Sources,
    Indexing,
    Storage,
    Models,
    Settings,
}

impl ViewId {
    pub const ALL: &'static [ViewId] = &[
        ViewId::Search,
        ViewId::Sources,
        ViewId::Indexing,
        ViewId::Storage,
        ViewId::Models,
        ViewId::Settings,
    ];

    /// Which top-level navigation group this view belongs to.
    pub fn group(self) -> NavGroup {
        match self {
            ViewId::Search | ViewId::Sources => NavGroup::Search,
            ViewId::Indexing | ViewId::Storage | ViewId::Models => NavGroup::Ai,
            ViewId::Settings => NavGroup::Settings,
        }
    }

    /// Default view to activate when the user first enters a group.
    pub fn group_default(group: NavGroup) -> Self {
        match group {
            NavGroup::Search => ViewId::Search,
            NavGroup::Ai => ViewId::Indexing,
            NavGroup::Settings => ViewId::Settings,
        }
    }
}

/// Sidebar index-health summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexHealth {
    pub indexed: u64,
    pub stale: u64,
    pub failed: u64,
    pub queued: u64,
}

/// One source card for the Sources view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCard {
    pub display_name: String,
    pub display_path: String,
    pub indexed: u64,
    pub stale: u64,
    pub failed: u64,
    pub active: bool,
    pub source_id: String,
}

/// A search result ready for display — pure data, no backend types
/// (RFC-027 boundary rule).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResultDisplay {
    pub display_path: String,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    pub snippet: Option<String>,
    pub keyword_rank: u32,
    pub badges: Vec<String>,
    /// Trust state and recovery actions for this result (RFC-038).
    pub trust: ResultTrustDisplay,
}

/// One required file and its check result shown in the wizard.
#[derive(Debug, Clone, PartialEq)]
pub struct WizardFileCheck {
    pub relative_path: String,
    pub found: bool,
    pub size_mb: Option<f64>,
}

/// Which stage of the startup wizard the user is on.
#[derive(Debug, Clone, PartialEq)]
pub enum WizardState {
    /// First launch or model never configured.
    NotConfigured,
    /// Was configured, but files are gone.
    FileMissing {
        previous_dir: String,
        checks: Vec<WizardFileCheck>,
    },
    /// User submitted a path; file checks complete.
    Checked {
        model_dir: String,
        checks: Vec<WizardFileCheck>,
        all_ok: bool,
    },
    /// All files verified — ready to proceed.
    Ready { model_dir: String },
    /// HuggingFace download in progress.
    Downloading {
        dest_dir: String,
        /// Filename currently being downloaded.
        current_file: String,
        bytes: u64,
        total: Option<u64>,
        files_done: u32,
        files_total: u32,
    },
}

/// The whole-app view model.
#[derive(Debug, Clone)]
pub struct AppState {
    pub active_view: ViewId,
    pub locale: Locale,
    pub query: String,
    pub last_query: Option<String>,
    pub search_mode: SearchMode,
    pub search_results: Vec<SearchResultDisplay>,
    pub search_running: bool,
    pub selected_result: Option<usize>,
    /// RFC-041: progressive search/filter UI state.
    pub search_ui: SearchUiState,
    /// RFC-045: where the current search looks (selected folder, scope,
    /// recent folders). Defaults to no selected location — the first-run
    /// "choose a folder when you search" state.
    pub search_location: SearchLocationState,
    pub storage_rows: Vec<(String, u64, u64)>,
    pub health: IndexHealth,
    pub sources: Vec<SourceCard>,
    pub capability: SearchCapability,
    pub storage_total_bytes: u64,
    /// Active startup wizard, or `None` when startup succeeded.
    pub wizard: Option<WizardState>,
    /// Text-input path the user is typing in the wizard.
    pub wizard_path_input: String,
    /// Text input for the "add source" path field.
    pub source_path_input: String,
    /// When false (default), hide technical detail. Mature users can toggle on.
    pub show_advanced: bool,
    /// Active user-facing notice (problem or confirmation), or `None`.
    pub notice: Option<UserNotice>,
    /// Awaiting user confirmation before running reset catalog.
    pub confirm_reset: bool,
    /// RFC-042: whether "Remember recent searches" is on (reflects the
    /// persisted setting; mirrored here so the settings toggle renders).
    pub remember_recent_searches: bool,
    /// RFC-042: awaiting confirmation before clearing recent searches.
    pub confirm_clear_history: bool,
    /// Snora Design tokens, derived from `theme`. The single styling source of
    /// truth for the whole view tree (RFC-032).
    pub tokens: snora::design::Tokens,
    /// The user's selected theme. `System` is resolved to a concrete preset at
    /// startup in `orbok`; `tokens` always holds the resolved bundle.
    pub theme: crate::theme::Theme,
    /// User-selected text scale multiplier (RFC-035). Applied via the `*_s`
    /// helpers in `theme.rs`; views read `state.text_scale` alongside tokens.
    pub text_scale: crate::theme::TextScale,
    /// When true, suppress non-essential animation (RFC-035). Defaulted from
    /// the OS preference at startup in `orbok`. Currently a no-op gate:
    /// wired now so any future animation checks it rather than being retrofitted.
    pub reduced_motion: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_view: ViewId::Search,
            locale: Locale::default(),
            query: String::new(),
            last_query: None,
            search_mode: SearchMode::Auto,
            search_results: Vec::new(),
            search_running: false,
            selected_result: None,
            search_ui: SearchUiState::default(),
            search_location: SearchLocationState::default(),
            storage_rows: Vec::new(),
            health: IndexHealth::default(),
            sources: Vec::new(),
            capability: SearchCapability::KeywordOnly,
            storage_total_bytes: 0,
            wizard: None,
            wizard_path_input: String::new(),
            source_path_input: String::new(),
            show_advanced: false,
            notice: None,
            confirm_reset: false,
            remember_recent_searches: true,
            confirm_clear_history: false,
            tokens: snora::design::Tokens::light(),
            theme: crate::theme::Theme::default(),
            text_scale: crate::theme::TextScale::default(),
            reduced_motion: false,
        }
    }
}

/// UI messages.
#[derive(Debug, Clone)]
pub enum Message {
    Switch(ViewId),
    SwitchGroup(NavGroup),
    ToggleAdvanced,
    SetTheme(crate::theme::Theme),
    /// User selected a text scale step (RFC-035).
    SetTextScale(crate::theme::TextScale),
    /// User toggled reduced-motion preference (RFC-035).
    SetReducedMotion(bool),
    ShowNotice(UserNotice),
    ClearNotice,
    // Storage cleanup
    CleanSnippets,
    CleanSearchCache,
    AskResetCatalog,
    ConfirmResetCatalog,
    CancelResetCatalog,
    CleanupDone, // backend notifies completion
    // Wizard navigation
    WizardBack,
    QueryChanged(String),
    SubmitSearch,
    SearchResultsReady(Vec<SearchResultDisplay>),
    SearchError(String),
    SelectResult(usize),
    OpenSourceFile(String),
    SetSearchMode(SearchMode),
    // RFC-041: filter / narrow / browse-around messages
    ApplySuggestedFilter(usize),
    RemoveFilter(usize),
    ClearFilters,
    OpenMoreWays,
    CloseMoreWays,
    SearchInResultFolder(usize),
    ShowNearbyFiles(usize),
    ShowSimilarFiles(usize),
    // RFC-038: result trust recovery actions
    TrustRecoveryAction {
        result_idx: usize,
        action: ResultRecoveryAction,
    },
    PersistLocale(Locale),
    SetLocale(Locale),
    // RFC-034: keyboard navigation messages
    /// Focus the global search text input (Ctrl/Cmd+K).
    FocusSearch,
    /// Close any active overlay/dialog and restore focus to trigger (Escape).
    DismissOverlay,
    /// Move result selection down (Arrow Down, when not typing).
    SelectNextResult,
    /// Move result selection up (Arrow Up, when not typing).
    SelectPrevResult,
    StorageDataReady(Vec<(String, u64, u64)>),
    // Startup wizard
    WizardPathChanged(String),
    WizardValidate,
    WizardChecked {
        model_dir: String,
        checks: Vec<WizardFileCheck>,
        all_ok: bool,
    },
    WizardAccept,
    WizardSkip,
    // Source management
    SourcePathChanged(String),
    RequestAddSource,
    SourceAdded(SourceCard),
    SourceRemoved(String), // source_id
    ScanCompleted(IndexHealth),
    // Download
    DownloadModel,
    DownloadStarted {
        dest_dir: String,
    },
    DownloadFileProgress {
        file: String,
        bytes: u64,
        total: Option<u64>,
        files_done: u32,
        files_total: u32,
    },
    DownloadAllComplete {
        dest_dir: String,
    },
    DownloadFailed(String),
    // Startup population
    HealthUpdated(IndexHealth),
    SourcesLoaded(Vec<SourceCard>),
    // RFC-043: model readiness
    ModelReadinessChecked {
        ready: bool,
        needs_download: bool,
        needs_repair: bool,
    },
    // RFC-039: privacy mode
    SetPrivacyMode(String),
    PrivacySettingChanged {
        key: String,
        value: bool,
    },
    ClearTemporaryPreviews,
    // RFC-040: diagnostics
    DiagnosticsCreateBundle,
    DiagnosticsBundleCreated(String),
    DiagnosticsBundleFailed,
    DiagnosticsOptInChanged {
        key: String,
        value: bool,
    },
    // RFC-045: search-in-folder flow
    /// User submitted a search but no folder is selected: open the OS folder
    /// picker. Sets `picker_in_progress = true` to block duplicate dialogs.
    ChooseFolderRequested,
    /// The OS folder picker was cancelled — keep query, show no error
    /// (RFC-045 §8.2).
    FolderPickerCancelled,
    /// The OS folder picker returned `path`. The app will create or reuse a
    /// remembered folder record then dispatch `SearchLocationSelected`.
    FolderPicked(std::path::PathBuf),
    /// A search location is now ready (folder created or reused). Carries the
    /// ready location so `AppState` can store it and resume the pending search.
    SearchLocationSelected(SearchLocation),
    /// User clicked ✕ on the folder chip — clears the selected location but
    /// preserves the typed query (RFC-045 §11.3).
    SearchLocationCleared,
    /// User switched between "and subfolders" / "only" for the current
    /// location (RFC-045 §6.3). Does not create a duplicate source record.
    SearchScopeChanged(crate::state::location::SearchFolderScope),
    /// User clicked a recent-folder chip — reuse that remembered folder as
    /// the current search location (RFC-045 §7.4).
    RecentFolderSelected(orbok_core::id::SourceId),
    // RFC-042: search history
    /// Open the Recent searches panel.
    OpenRecentSearches,
    /// Close the Recent searches panel.
    CloseRecentSearches,
    /// User pressed "Search again" for a history entry.
    SearchAgain(SearchHistoryId),
    /// The history entry has been fully restored; carry the restored id so
    /// the UI can clear `restoring_history_id`.
    RecentSearchRestored(SearchHistoryId),
    /// Remove a single history entry.
    RemoveRecentSearch(SearchHistoryId),
    /// User pressed "Clear recent searches" — show confirmation.
    AskClearRecentSearches,
    /// User pressed Cancel in the clear confirmation.
    CancelClearRecentSearches,
    /// User confirmed "Clear recent searches".
    ConfirmClearRecentSearches,
    /// Recent searches cleared — carry refreshed (empty) history list.
    RecentSearchesCleared,
    /// History list refreshed from the DB (after upsert or clear).
    HistoryLoaded(Vec<SearchHistoryEntry>),
    /// Toggle "Remember recent searches" setting.
    ToggleRememberRecentSearches(bool),
}

impl AppState {
    pub fn update(&mut self, message: &Message) {
        match message {
            Message::Switch(view) => self.active_view = *view,
            Message::SwitchGroup(group) => self.active_view = ViewId::group_default(*group),
            Message::ToggleAdvanced => self.show_advanced = !self.show_advanced,
            Message::SetTheme(theme) => {
                self.theme = *theme;
                self.tokens = theme.tokens();
            }
            Message::SetTextScale(scale) => self.text_scale = *scale,
            Message::SetReducedMotion(val) => self.reduced_motion = *val,
            Message::AskResetCatalog => self.confirm_reset = true,
            Message::CancelResetCatalog => self.confirm_reset = false,
            Message::ConfirmResetCatalog => {
                self.confirm_reset = false;
                // Actual reset handled in orbok; UI pre-clears state.
                self.sources.clear();
                self.health = crate::state::IndexHealth::default();
                self.search_results.clear();
                self.storage_rows.clear();
                self.storage_total_bytes = 0;
            }
            Message::CleanSnippets | Message::CleanSearchCache => {
                // Actual work done in orbok; state update arrives via CleanupDone.
            }
            Message::CleanupDone => {
                self.notice = Some(UserNotice::PreviewsCleared);
            }
            Message::WizardBack => {
                // Return to the initial setup step.
                self.wizard = Some(crate::state::WizardState::NotConfigured);
                self.wizard_path_input = String::new();
            }
            Message::ShowNotice(n) => self.notice = Some(n.clone()),
            Message::ClearNotice => self.notice = None,
            Message::QueryChanged(query) => {
                self.query = query.clone();
                self.search_ui.text = query.clone();
            }
            Message::SubmitSearch => {
                let trimmed = self.query.trim();
                if !trimmed.is_empty() {
                    self.last_query = Some(trimmed.to_string());
                    self.search_running = true;
                    self.search_results.clear();
                    self.selected_result = None;
                    self.search_ui.results_status = ResultsStatus::Searching;
                }
            }
            Message::SearchResultsReady(results) => {
                let count = results.len();
                self.search_results = results.clone();
                self.search_running = false;
                self.selected_result = None;
                self.notice = None;
                self.search_ui.results_status = if count == 0 {
                    if self.search_ui.has_active_filters() {
                        ResultsStatus::EmptyAfterFiltering
                    } else {
                        ResultsStatus::EmptyAfterSearch
                    }
                } else {
                    ResultsStatus::Ready { total_count: count }
                };
            }
            Message::SearchError(_) => {
                self.search_running = false;
                self.search_ui.results_status = ResultsStatus::Problem {
                    friendly_message: "Search did not finish. Please try again.".into(),
                };
                self.notice = Some(UserNotice::SearchDidNotFinish);
            }
            // RFC-041: filter operations
            Message::ApplySuggestedFilter(i) => self.search_ui.apply_suggested(*i),
            Message::RemoveFilter(i) => self.search_ui.remove_filter(*i),
            Message::ClearFilters => self.search_ui.clear_filters(),
            Message::OpenMoreWays => self.search_ui.more_panel_open = true,
            Message::CloseMoreWays => self.search_ui.more_panel_open = false,
            Message::SearchInResultFolder(_idx) => {} // handled by orbok
            Message::ShowNearbyFiles(_idx) => {}      // handled by orbok
            Message::ShowSimilarFiles(_idx) => {}     // handled by orbok
            // RFC-038: trust recovery actions
            Message::TrustRecoveryAction { .. } => {} // handled by orbok
            Message::SelectResult(idx) => self.selected_result = Some(*idx),
            Message::OpenSourceFile(_) => {} // handled by orbok
            Message::SetSearchMode(mode) => self.search_mode = *mode,
            Message::PersistLocale(locale) | Message::SetLocale(locale) => self.locale = *locale,
            // RFC-034 keyboard navigation: FocusSearch is handled in orbok
            // (it issues an iced focus task); DismissOverlay closes any overlay.
            Message::FocusSearch => {} // focus task issued by orbok
            Message::DismissOverlay => {
                // Close whichever overlay is open, in priority order.
                if self.confirm_reset {
                    self.confirm_reset = false;
                } else if self.notice.is_some() {
                    self.notice = None;
                }
            }
            Message::SelectNextResult => {
                if !self.search_results.is_empty() {
                    self.selected_result = Some(match self.selected_result {
                        None => 0,
                        Some(i) => (i + 1).min(self.search_results.len() - 1),
                    });
                }
            }
            Message::SelectPrevResult => {
                if !self.search_results.is_empty() {
                    self.selected_result = Some(match self.selected_result {
                        None | Some(0) => 0,
                        Some(i) => i - 1,
                    });
                }
            }
            Message::StorageDataReady(rows) => self.storage_rows = rows.clone(),
            Message::WizardPathChanged(p) => self.wizard_path_input = p.clone(),
            Message::WizardValidate => {} // handled in orbok update
            Message::WizardChecked {
                model_dir,
                checks,
                all_ok,
            } => {
                self.wizard = Some(if *all_ok {
                    WizardState::Ready {
                        model_dir: model_dir.clone(),
                    }
                } else {
                    WizardState::Checked {
                        model_dir: model_dir.clone(),
                        checks: checks.clone(),
                        all_ok: false,
                    }
                });
            }
            Message::WizardAccept => {
                // orbok writes the model dir to OrbokSettings; ui
                // transitions to full capability.
                self.capability = SearchCapability::Hybrid;
                self.wizard = None;
                self.wizard_path_input = String::new();
            }
            Message::WizardSkip => {
                self.capability = SearchCapability::KeywordOnly;
                self.wizard = None;
                self.wizard_path_input = String::new();
            }
            Message::DownloadModel => {
                // Transition handled in orbok main.rs (needs the data_dir).
                // The UI just switches to a "waiting" state until DownloadStarted arrives.
            }
            Message::DownloadStarted { dest_dir } => {
                self.wizard = Some(WizardState::Downloading {
                    dest_dir: dest_dir.clone(),
                    current_file: String::new(),
                    bytes: 0,
                    total: None,
                    files_done: 0,
                    files_total: 2,
                });
            }
            Message::DownloadFileProgress {
                file,
                bytes,
                total,
                files_done,
                files_total,
            } => {
                if let Some(WizardState::Downloading {
                    current_file,
                    bytes: b,
                    total: t,
                    files_done: fd,
                    files_total: ft,
                    ..
                }) = &mut self.wizard
                {
                    *current_file = file.clone();
                    *b = *bytes;
                    *t = *total;
                    *fd = *files_done;
                    *ft = *files_total;
                }
            }
            Message::DownloadAllComplete { dest_dir } => {
                // Switch directly to wizard-accepted flow.
                self.wizard = Some(WizardState::Ready {
                    model_dir: dest_dir.clone(),
                });
            }
            Message::DownloadFailed(_reason) => {
                // Return to NotConfigured so the user can try again.
                self.wizard = Some(WizardState::NotConfigured);
            }
            Message::SourcePathChanged(p) => self.source_path_input = p.clone(),
            Message::RequestAddSource => {} // handled in orbok
            Message::SourceAdded(card) => {
                self.sources.push(card.clone());
                self.source_path_input = String::new();
                self.notice = Some(UserNotice::FolderAdded);
            }
            Message::SourceRemoved(id) => self.sources.retain(|s| s.source_id != *id),
            Message::ScanCompleted(health) | Message::HealthUpdated(health) => {
                self.health = *health;
                // Update per-source counts from the fresh health data.
            }
            Message::SourcesLoaded(cards) => self.sources = cards.clone(),
            // RFC-043: model readiness
            Message::ModelReadinessChecked { .. } => {} // handled by orbok
            // RFC-039: privacy
            Message::SetPrivacyMode(_) => {} // handled by orbok
            Message::PrivacySettingChanged { .. } => {} // handled by orbok
            Message::ClearTemporaryPreviews => {} // handled by orbok
            // RFC-040: diagnostics
            Message::DiagnosticsCreateBundle => {} // handled by orbok
            Message::DiagnosticsBundleCreated(_) => {
                self.notice = Some(UserNotice::DiagnosticsFileCreated);
            }
            Message::DiagnosticsBundleFailed => {
                self.notice = Some(UserNotice::DiagnosticsFileFailed);
            }
            Message::DiagnosticsOptInChanged { .. } => {} // handled by orbok
            // RFC-045: search-in-folder flow
            Message::ChooseFolderRequested => {
                // Guard: block duplicate picker dialogs on rapid Search clicks.
                self.search_location.picker_in_progress = true;
            }
            Message::FolderPickerCancelled => {
                // RFC-045 §8.2: cancel is neutral — no error, query preserved.
                self.search_location.picker_in_progress = false;
            }
            Message::FolderPicked(_) => {
                // Handled in orbok (source create/reuse); result arrives
                // via SearchLocationSelected. Keep picker_in_progress = true
                // until the source record is ready.
            }
            Message::SearchLocationSelected(location) => {
                self.search_location.picker_in_progress = false;
                self.search_location.selected = Some(location.clone());
                // After a location becomes ready, treat as a fresh search
                // so results reflect the new scope.
                if !self.query.trim().is_empty() {
                    self.search_running = true;
                    self.search_results.clear();
                    self.selected_result = None;
                    self.search_ui.results_status = ResultsStatus::Searching;
                }
            }
            Message::SearchLocationCleared => {
                // RFC-045 §11.3: clear chip, preserve query.
                self.search_location.clear();
            }
            Message::SearchScopeChanged(scope) => {
                // RFC-045 §6.3: scope change never duplicates the source record.
                self.search_location.set_scope(*scope);
            }
            Message::RecentFolderSelected(source_id) => {
                // Find the recent summary and promote it to the selected location.
                if let Some(summary) = self
                    .search_location
                    .recent_locations
                    .iter()
                    .find(|s| &s.source_id == source_id)
                    .cloned()
                {
                    self.search_location.selected = Some(SearchLocation::remembered(
                        summary.source_id,
                        summary.display_name,
                    ));
                }
            }
            // RFC-042: search history
            Message::OpenRecentSearches => {
                self.search_ui.history_panel_open = true;
            }
            Message::CloseRecentSearches => {
                self.search_ui.history_panel_open = false;
            }
            Message::SearchAgain(id) => {
                self.search_ui.restoring_history_id = Some(id.clone());
                self.search_ui.history_panel_open = false;
                self.search_ui.results_status = ResultsStatus::Searching;
                // Actual restore (text + filters) happens in orbok once the
                // entry is loaded; RecentSearchRestored finalises the state.
            }
            Message::RecentSearchRestored(id) => {
                if self.search_ui.restoring_history_id.as_ref() == Some(&id) {
                    self.search_ui.restoring_history_id = None;
                }
            }
            Message::RemoveRecentSearch(id) => {
                self.search_ui.history.retain(|e| e.id != *id);
                // Persist handled by orbok.
            }
            Message::AskClearRecentSearches => {
                // Drives the confirmation dialog rendered by the view layer.
                self.confirm_clear_history = true;
            }
            Message::CancelClearRecentSearches => {
                self.confirm_clear_history = false;
            }
            Message::ConfirmClearRecentSearches => {
                // Handled in orbok (DB clear); result arrives via
                // RecentSearchesCleared.
                self.confirm_clear_history = false;
            }
            Message::RecentSearchesCleared => {
                self.search_ui.history.clear();
                self.search_ui.history_panel_open = false;
                self.confirm_clear_history = false;
            }
            Message::HistoryLoaded(entries) => {
                self.search_ui.history = entries.clone();
            }
            Message::ToggleRememberRecentSearches(on) => {
                // UI reflects the new state immediately; orbok persists it.
                self.remember_recent_searches = *on;
                if !*on {
                    // Turning off also empties the visible list (RFC-042 §13.4).
                    self.search_ui.history.clear();
                }
            }
        }
    }
}
