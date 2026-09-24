//! Headless UI state (view models) and the message vocabulary.
//!
//! Everything here is plain data — testable without a display server.
//! `orbok` populates these structs from backend services; views
//! render them; `update` mutates them. No iced types appear in this
//! module so state logic stays UI-framework-agnostic.

pub mod location;
pub mod model_consent;
pub mod search;

pub use location::{SearchFolderScope, SearchLocation, SearchLocationState, SearchLocationSummary};
pub use model_consent::{ModelDownloadConsent, ModelTrustPresentation};
pub use search::{ResultTrustDisplay, ResultsStatus, SearchUiState};

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use orbok_core::{SearchHistoryEntry, SearchHistoryId, SourceStatus};
use orbok_models::SearchCapability;
use orbok_search::{MatchBadge, ResultRecoveryAction, ResultTrustState, SearchMode};

/// Top-level navigation group for the two-level sidebar + tab layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavGroup {
    Search,
    Ai,
    Settings,
}

/// Top-level pages (GUI external design §3.1 order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Task 104: the states a folder's files are counted in, on the Folders card
/// and the Preparing page alike. One label per state, in one place -- the
/// card composes its line from these, and the Preparing page's cells read the
/// same keys, so the two screens cannot name a state differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCountState {
    /// Prepared and searchable.
    Ready,
    /// Changed since orbok prepared it.
    NeedsUpdate,
    Failed,
    /// Read, but no text found in it. Counted on the Folders card only.
    NoText,
}

impl FileCountState {
    pub fn label_key(self) -> MessageKey {
        match self {
            Self::Ready => MessageKey::IndexingHealthIndexed,
            Self::NeedsUpdate => MessageKey::IndexingHealthStale,
            Self::Failed => MessageKey::IndexingHealthFailed,
            Self::NoText => MessageKey::IndexingHealthNoText,
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

/// Task 092/094: what a reset would remove, counted fresh each time the
/// confirmation dialog opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetCounts {
    pub folders: u64,
    pub files: u64,
    /// Task 094: the number of stored recent searches. Whether the
    /// dialog's line mentions them also depends on the "Remember recent
    /// searches" setting, which this count alone cannot see -- the
    /// renderer combines the two (`storage_view`).
    pub history: u64,
}

/// One source card for the Sources view.
///
/// `status` is `orbok_core::SourceStatus` directly (Task 035): the same
/// enum the catalog's `sources.status` column stores, not a UI-owned
/// re-encoding — `orbok_core` is the project's shared neutral vocabulary,
/// already used directly elsewhere in this module (`SearchHistoryEntry`),
/// so this does not cross the RFC-027 backend-type boundary the way an
/// `orbok_db`/`orbok_fs` type would. RFC-037's richer `SourceState`
/// (`Preparing`/`NeedsUpdate`) has no catalog column to read from — those
/// two are derived at render time from `stale`/`failed` below, not carried
/// as a separate field here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCard {
    pub display_name: String,
    pub display_path: String,
    pub indexed: u64,
    pub stale: u64,
    pub failed: u64,
    /// Task 080: files orbok read but found no text in (a scanned PDF is
    /// the common case) -- a finished, distinct state, not counted as
    /// `indexed` and not left pending as `discovered`.
    pub no_text_found: u64,
    /// Task 108: this folder's jobs that are queued or running. Non-zero is
    /// what makes an Active folder's card say "Preparing"; it is read with
    /// the counts above, so it is as fresh as they are.
    pub unfinished_jobs: u64,
    pub status: SourceStatus,
    pub source_id: String,
    /// Task 114 (RFC-064): whether this folder covers its subfolders --
    /// "This folder and subfolders" (`true`, the default) or "This folder
    /// only". Shown and changed on the card, and nowhere else.
    pub covers_subfolders: bool,
}

impl SourceCard {
    /// Task 108: the card's state label, in the order RFC-037 §17 and Task
    /// 108 give: a folder that cannot be reached says so first (whatever work
    /// is queued), then Preparing, then Needs update, then Ready.
    pub fn state_label_key(&self) -> MessageKey {
        use SourceStatus::*;
        match self.status {
            Missing => MessageKey::SourceStateFolderNotFound,
            PermissionDenied => MessageKey::SourceStateCannotOpen,
            Paused => MessageKey::SourceStatePaused,
            Removed => MessageKey::SourceStateRemoved,
            Active if self.unfinished_jobs > 0 => MessageKey::SourceStatePreparing,
            Active if self.stale > 0 => MessageKey::SourceStateNeedsUpdate,
            Active => MessageKey::SourceStateReady,
        }
    }

    /// Whether this folder is an Active one with work still to do.
    pub fn is_preparing(&self) -> bool {
        self.status == SourceStatus::Active && self.unfinished_jobs > 0
    }
}

/// Task 113: a folder that became part of another. The window drops its card
/// and, if the search was looking at it, looks at the same folder inside the
/// one that now holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedFolder {
    pub source_id: String,
    pub display_name: String,
    pub canonical_path: String,
}

/// Task 113: the folders (`folders`) that are now part of `parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldersCombined {
    pub parent_id: String,
    pub parent_name: String,
    pub folders: Vec<CombinedFolder>,
}

/// A search result ready for display — pure data, no backend types
/// (RFC-027 boundary rule).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResultDisplay {
    pub display_path: String,
    /// The indexed file's canonical path -- what Open file and Show in
    /// folder validate and launch (HANDOFF-041). Never rendered, and never
    /// carried in a `Message`: those carry the result's index, so the path
    /// a launch uses can only be one a search returned.
    pub canonical_path: String,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    pub snippet: Option<String>,
    pub keyword_rank: u32,
    pub badges: Vec<MatchBadge>,
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
    /// Reviewed model facts awaiting explicit consent before network access.
    DownloadConsent {
        presentation: ModelDownloadConsent,
        return_to: ModelConsentReturn,
    },
    /// User submitted a path; file checks complete.
    Checked {
        model_dir: String,
        checks: Vec<WizardFileCheck>,
        all_ok: bool,
    },
    /// All files verified — ready to proceed.
    Ready {
        ready_id: ReadyId,
        model_dir: String,
        provenance: ModelProvenance,
        persistence: ModelPersistenceState,
    },
    /// HuggingFace download in progress.
    Downloading {
        /// Reserved before worker start so identity exhaustion cannot follow
        /// an authoritative activation.
        reserved_ready_id: ReadyId,
        dest_dir: String,
        presentation: ModelDownloadConsent,
        return_to: ModelConsentReturn,
        current_artifact: Option<ModelArtifact>,
        bytes: u64,
        total: u64,
        files_done: u32,
        files_total: u32,
        /// Set once by `Message::CancelDownloadInProgress` (Task 025). While
        /// true, the page shows a cancelling state instead of the Cancel
        /// action, and the eventual terminal message from the worker
        /// (however it resolves) is routed back to `DownloadConsent`
        /// rather than `DownloadFailed` -- an error page would misreport a
        /// cancellation the user asked for. Staying in `Downloading` until
        /// that message arrives, rather than reverting immediately, is
        /// deliberate: it is the only thing preventing a second download
        /// from starting while the first has not yet actually stopped.
        cancelling: bool,
    },
    /// A safe, recoverable delivery failure that retains the reviewed offer.
    DownloadFailed {
        presentation: ModelDownloadConsent,
        return_to: ModelConsentReturn,
        failure: ModelDeliveryFailure,
    },
}

/// RFC-034 (Task 024): the shape of a [`WizardState`] that matters for
/// keyboard-driven `Enter`/`Escape` dispatch, with the heavy per-state
/// payload stripped out. Kept separate from `WizardState` itself (rather
/// than matching on borrowed `WizardState` directly at the call site)
/// because it needs to travel through `iced::Subscription::with`, which
/// requires its payload to be `Hash` -- `WizardState` itself cannot be
/// (it carries `String`s and `Vec`s nested arbitrarily), and cloning the
/// whole state into every keyboard-subscription rebuild would be wasteful
/// besides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WizardKind {
    /// `NotConfigured` or `FileMissing`. Its `DownloadModel` primary action
    /// is bound to the global `Enter` (Task 027). This page also renders a
    /// `text_input` with its own `on_submit(WizardValidate)`; Task 024
    /// left this unbound believing the two could double-fire from one
    /// keypress, but iced's `text_input` captures Enter whenever it
    /// genuinely has focus (`shell.capture_event()`, unconditional on
    /// modifiers) and `key_to_message` only ever runs through
    /// `iced::keyboard::listen()`, which only receives events the widget
    /// tree left uncaptured -- so a captured Enter never reaches this
    /// binding at all. Verified live, not just reasoned from source: see
    /// `shell::confirm_message`'s own comment.
    Setup,
    DownloadConsent,
    /// Primary action is `CancelDownloadInProgress` (Task 025's Cancel
    /// button), bound to `Escape` rather than `Enter` (Task 027 §3.1) --
    /// see `key_to_message`'s own Escape arm.
    Downloading,
    DownloadFailed,
    /// `Checked { all_ok: true, .. }` — primary action is `WizardAccept`.
    CheckedOk,
    /// `Checked { all_ok: false, .. }`. Primary action is `WizardValidate`
    /// itself -- the same action its own `text_input`'s
    /// `on_submit(WizardValidate)` already gives a keyboard path whenever
    /// that input has focus. Left unbound here (unlike `Setup`) because
    /// binding `Enter` to the same message a second time would be
    /// redundant, not because of a conflict -- see `shell::confirm_message`.
    CheckedNotOk,
    /// `Ready { persistence: Idle, .. }` — primary action is `WizardAccept`.
    ReadyIdle,
    /// `Ready { persistence: Failed, .. }` — primary action is
    /// `WizardAccept` (labeled "Retry" on this page).
    ReadyFailed,
    /// `Ready { persistence: InFlight(_), .. }` — nothing to confirm while
    /// a save is already running.
    ReadyInFlight,
    /// `Ready { persistence: LoadFailed(_), .. }` (Task 057) — primary
    /// action is `WizardRetryModelLoad` ("Try again").
    ReadyLoadFailed,
}

impl WizardState {
    pub fn kind(&self) -> WizardKind {
        match self {
            WizardState::NotConfigured | WizardState::FileMissing { .. } => WizardKind::Setup,
            WizardState::DownloadConsent { .. } => WizardKind::DownloadConsent,
            WizardState::Downloading { .. } => WizardKind::Downloading,
            WizardState::DownloadFailed { .. } => WizardKind::DownloadFailed,
            WizardState::Checked { all_ok: true, .. } => WizardKind::CheckedOk,
            WizardState::Checked { all_ok: false, .. } => WizardKind::CheckedNotOk,
            WizardState::Ready {
                persistence: ModelPersistenceState::Idle,
                ..
            } => WizardKind::ReadyIdle,
            WizardState::Ready {
                persistence: ModelPersistenceState::Failed,
                ..
            } => WizardKind::ReadyFailed,
            WizardState::Ready {
                persistence: ModelPersistenceState::InFlight(_),
                ..
            } => WizardKind::ReadyInFlight,
            WizardState::Ready {
                persistence: ModelPersistenceState::LoadFailed(_),
                ..
            } => WizardKind::ReadyLoadFailed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvenance {
    UserSupplied,
    AppManaged,
}

/// Identity of one entry into the Ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyId(u64);

impl ReadyId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Identity of one persistence attempt for a Ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceAttemptId(u64);

impl PersistenceAttemptId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Persistence status retained on the Ready screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPersistenceState {
    Idle,
    InFlight(PersistenceAttemptId),
    Failed,
    /// Task 057: the model choice **was** saved, but the model could not be
    /// loaded for search. Distinct from `Failed`, whose copy says the choice
    /// could not be saved; retrying re-runs loading, not saving. Carries the
    /// attempt so the retried activation is correlated the same way.
    LoadFailed(PersistenceAttemptId),
}

/// Closed artifact vocabulary safe for UI presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelArtifact {
    Tokenizer,
    OnnxModel,
}

/// Closed delivery-failure vocabulary; never contains paths or worker text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelDeliveryFailure {
    StoreUnavailable,
    Connection,
    Verification,
    LocalStorage,
    InternalState,
    /// Never rendered as a failure in practice -- `Downloading.cancelling`
    /// intercepts the terminal message before it reaches this page (see
    /// `model_flow::reduce`'s `DownloadFailed` handling). Kept as a real
    /// variant so `map_delivery_error` and `page_download_failed` stay
    /// exhaustive without a wildcard that would silently swallow a future
    /// new failure kind.
    Cancelled,
}

/// Result of a correlated preference write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPersistenceResult {
    Saved,
    Failed,
}

/// Checked, non-reusing identity allocator owned by the app view model.
#[derive(Debug, Clone)]
pub struct ModelFlowIdentitySequence {
    next_ready: Option<u64>,
    next_persistence_attempt: Option<u64>,
}

impl Default for ModelFlowIdentitySequence {
    fn default() -> Self {
        Self {
            next_ready: Some(1),
            next_persistence_attempt: Some(1),
        }
    }
}

impl ModelFlowIdentitySequence {
    pub fn allocate_ready(&mut self) -> Option<ReadyId> {
        allocate_checked(&mut self.next_ready).map(ReadyId)
    }

    pub fn allocate_persistence_attempt(&mut self) -> Option<PersistenceAttemptId> {
        allocate_checked(&mut self.next_persistence_attempt).map(PersistenceAttemptId)
    }

    #[cfg(test)]
    pub(crate) fn with_next(ready: u64, persistence_attempt: u64) -> Self {
        Self {
            next_ready: Some(ready),
            next_persistence_attempt: Some(persistence_attempt),
        }
    }
}

fn allocate_checked(next: &mut Option<u64>) -> Option<u64> {
    let current = (*next)?;
    *next = current.checked_add(1);
    Some(current)
}

/// Setup state restored when the user backs out of download consent.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelConsentReturn {
    NotConfigured,
    FileMissing {
        previous_dir: String,
        checks: Vec<WizardFileCheck>,
    },
}

impl ModelConsentReturn {
    fn from_wizard(wizard: &WizardState) -> Option<Self> {
        match wizard {
            WizardState::NotConfigured => Some(Self::NotConfigured),
            WizardState::FileMissing {
                previous_dir,
                checks,
            } => Some(Self::FileMissing {
                previous_dir: previous_dir.clone(),
                checks: checks.clone(),
            }),
            _ => None,
        }
    }

    fn into_wizard(self) -> WizardState {
        match self {
            Self::NotConfigured => WizardState::NotConfigured,
            Self::FileMissing {
                previous_dir,
                checks,
            } => WizardState::FileMissing {
                previous_dir,
                checks,
            },
        }
    }
}

/// Task 069: the destructive confirmations, each rendered on exactly one view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Folder removal -- Folders (`sources_view`).
    RemoveSource,
    /// Reset saved app data -- Storage (`storage_view`).
    ResetCatalog,
    /// Clear recent searches -- Settings (`settings_view`).
    ClearRecentSearches,
    /// Task 114: "Stop including subfolders?" -- Folders (`sources_view`).
    NarrowFolder,
    /// Task 099: "Prepare keyword search again" -- Storage, Advanced view.
    DeleteKeywordIndex,
    /// Task 099: "Prepare search by meaning again" -- Storage, Advanced view.
    DeleteVectorIndex,
    /// Task 110: "Add a folder that may contain private files?", asked from
    /// the Folders page (the picker or a typed path).
    AddSensitiveFolderOnFolders,
    /// Task 110: the same question, asked from the search page
    /// (search-in-folder's picker).
    AddSensitiveFolderOnSearch,
}

/// Task 110: which page asked to add a folder, so the question renders where
/// the user is and cancelling puts that page back as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderAddOrigin {
    /// The Folders page: the Add folder picker or the typed path.
    FoldersPage,
    /// The search page: the search-in-folder picker (RFC-045).
    SearchPage,
}

/// Task 110: a folder waiting for the user's answer to "add a folder that may
/// contain private files?" -- nothing has been saved or queued for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFolderAdd {
    /// The path as the user gave it (picked or typed).
    pub path: String,
    pub origin: FolderAddOrigin,
}

impl Confirmation {
    /// The one view this confirmation renders on.
    pub fn view(self) -> ViewId {
        match self {
            Self::RemoveSource => ViewId::Sources,
            Self::NarrowFolder => ViewId::Sources,
            Self::ResetCatalog => ViewId::Storage,
            Self::ClearRecentSearches => ViewId::Settings,
            Self::DeleteKeywordIndex => ViewId::Storage,
            Self::DeleteVectorIndex => ViewId::Storage,
            Self::AddSensitiveFolderOnFolders => ViewId::Sources,
            Self::AddSensitiveFolderOnSearch => ViewId::Search,
        }
    }
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
    /// Task 081: RFC-011 §11's storage categories, each either a real
    /// measurement or explicitly `Unknown` -- never a false zero standing
    /// in for "not measured". Empty means "never measured this session",
    /// the RFC-011 §13.1 empty state (`storage_view` reads it that way).
    pub storage_rows: Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
    /// A measurement is in flight (`Message::StorageMeasurementRequested`
    /// sent, no `StorageDataReady`/failure yet) -- so "Calculate now" does
    /// not show as idle while its own `Task::perform` is running.
    pub storage_measuring: bool,
    /// Task 081 §2: the cache database file's own size on disk, alongside
    /// `storage_rows` -- not one of RFC-011's eight categories (several of
    /// them live inside this one file, so it is not additive with them).
    /// `None` before the first measurement, same as an empty `storage_rows`.
    pub storage_cache_file_bytes: Option<u64>,
    pub health: IndexHealth,
    pub sources: Vec<SourceCard>,
    /// RFC-034 (Task 024): keyboard-driven selection into `sources`,
    /// mirroring `selected_result` for the Sources view -- arrow keys move
    /// it, `Enter` removes the selected source, `Escape` clears it.
    pub selected_source: Option<usize>,
    pub capability: SearchCapability,
    /// Provenance of the active embedding model, independent of capability.
    pub active_model_provenance: Option<ModelProvenance>,
    /// Active startup wizard, or `None` when startup succeeded.
    pub wizard: Option<WizardState>,
    /// Text-input path the user is typing in the wizard.
    pub wizard_path_input: String,
    /// App-populated, path-aware facts for the reviewed default-model offer.
    pub model_download_consent: Option<ModelDownloadConsent>,
    /// Non-reusing identities used to correlate Ready and persistence events.
    pub model_flow_ids: ModelFlowIdentitySequence,
    /// Text input for the "add source" path field.
    pub source_path_input: String,
    /// Task 047: an add-folder dialog is open, so another must not be opened.
    pub add_source_picker_in_progress: bool,
    /// When false (default), hide technical detail. Mature users can toggle on.
    pub show_advanced: bool,
    /// Active user-facing notice (problem or confirmation), or `None`.
    pub notice: Option<UserNotice>,
    /// Task 060: what the notice's action button does -- the concrete retry
    /// its raise site knew (re-running *that* search, re-opening *that*
    /// confirmation). `None` means the notice renders no action button, only
    /// its dismiss control. Cleared with the notice.
    pub notice_action: Option<Box<Message>>,
    /// Awaiting user confirmation before running reset catalog.
    pub confirm_reset: bool,
    /// Task 092: what the open reset confirmation will remove, fetched
    /// fresh when it opens. `None` until the count arrives, on a failed
    /// read, and after the dialog closes -- never a placeholder zero
    /// (`storage_view` renders the line only when this is `Some`).
    pub reset_counts: Option<ResetCounts>,
    /// Task 099: awaiting confirmation before deleting the keyword index.
    pub confirm_delete_keyword_index: bool,
    /// Task 110: the private-folder question, when one is open.
    pub pending_folder_add: Option<PendingFolderAdd>,
    /// Task 099: awaiting confirmation before deleting the vector index.
    pub confirm_delete_vector_index: bool,
    /// Task 099: what the open rebuild confirmation (either one -- at most
    /// one is open at a time) will prepare again, fetched fresh when it
    /// opens. `None` until the count arrives, on a failed read, after the
    /// dialog closes, and when the count is genuinely zero (§2.3: "never
    /// a zero" -- `storage_view` renders the line only when this is
    /// `Some` and non-zero).
    pub rebuild_file_count: Option<u64>,
    /// Task 062: the folder whose removal confirmation is open, if any.
    /// Removal erases what orbok prepared for it (RFC-059), so it always
    /// asks first. At most one confirmation is open at a time.
    pub confirm_remove_source: Option<String>,
    /// Task 114: the folder whose "stop including subfolders" question is
    /// open, if any. Narrowing erases what orbok prepared for the files below
    /// the folder's top level (RFC-064 §3.2), so it always asks first.
    pub confirm_narrow_source: Option<String>,
    /// Task 114: how many files the open narrowing question would drop,
    /// fetched off the update thread when it opens. `None` until it arrives,
    /// on a failed read, and when it is genuinely zero -- no count, no line,
    /// never a zero.
    pub narrow_file_count: Option<u64>,
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
            storage_measuring: false,
            storage_cache_file_bytes: None,
            health: IndexHealth::default(),
            sources: Vec::new(),
            selected_source: None,
            capability: SearchCapability::KeywordOnly,
            active_model_provenance: None,
            wizard: None,
            wizard_path_input: String::new(),
            model_download_consent: None,
            model_flow_ids: ModelFlowIdentitySequence::default(),
            source_path_input: String::new(),
            add_source_picker_in_progress: false,
            show_advanced: false,
            notice: None,
            notice_action: None,
            confirm_reset: false,
            reset_counts: None,
            confirm_delete_keyword_index: false,
            pending_folder_add: None,
            confirm_delete_vector_index: false,
            rebuild_file_count: None,
            confirm_remove_source: None,
            confirm_narrow_source: None,
            narrow_file_count: None,
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
    /// Task 060: show a notice whose action button sends `action` -- the
    /// concrete retry of what failed.
    ShowNoticeWithAction {
        notice: UserNotice,
        action: Box<Message>,
    },
    /// Task 060: the notice's action button. orbok takes
    /// `AppState::take_notice_action`, which clears the notice, and
    /// dispatches it.
    NoticeActionPressed,
    /// Task 060: retry a search that failed -- restores exactly that query
    /// and submits it, whatever has been typed since.
    RetrySearch(String),
    ClearNotice,
    // Storage cleanup
    CleanSnippets,
    CleanSearchCache,
    /// RFC-059 §8 Slice 4: exposes `CleanupAction::ClearTemporaryExtraction`,
    /// implemented since M10 but reachable from no UI until this.
    CleanTemporaryExtraction,
    /// RFC-059 §8 Slice 4: exposes `CleanupAction::RemoveReplacedStaleIndexes`
    /// -- added after RFC-059 Slice 2 landed, so it actually frees bytes
    /// instead of reporting rows deleted while reclaiming nothing (the
    /// RFC's own §8 ordering).
    RemoveReplacedStaleIndexes,
    /// Task 062: open the removal confirmation for this folder (the card's
    /// Remove button, or Delete on a selected folder).
    AskRemoveSource(String), // source_id
    /// Task 062: the removal confirmation's Remove (or Enter while it is
    /// open). orbok takes the id from state and dispatches `SourceRemoved`.
    ConfirmRemoveSource,
    /// Task 062: the removal confirmation's Cancel.
    CancelRemoveSource,
    AskResetCatalog,
    ConfirmResetCatalog,
    /// Task 075: the catalog was reset; the list, health, results and
    /// storage rows now clear.
    CatalogResetSucceeded,
    CancelResetCatalog,
    /// Task 092: the confirmation's own counts, fetched fresh when it
    /// opens (the same `Task::perform` pattern `StorageDataReady` uses).
    ResetCountsReady(ResetCounts),
    /// Task 092: the count could not be read -- the dialog opens and works
    /// regardless (Reset is never gated on this), it simply shows no line.
    ResetCountsFailed,
    /// Task 099: open the "prepare keyword search again" confirmation
    /// (Storage, Advanced view).
    AskDeleteKeywordIndex,
    /// Task 099: that confirmation's own action button (or Enter while
    /// visible). Closes the dialog; the delete and rebuild-marking run off
    /// the update thread (Task 097's shape).
    ConfirmDeleteKeywordIndex,
    CancelDeleteKeywordIndex,
    /// Task 099: open the "prepare search by meaning again" confirmation.
    AskDeleteVectorIndex,
    ConfirmDeleteVectorIndex,
    CancelDeleteVectorIndex,
    /// Task 099: how many files either rebuild confirmation's own line
    /// will name, fetched fresh when it opens -- shared by both dialogs,
    /// since only one can be open at a time (RFC-011 §14, Task 062's
    /// dialog shape).
    RebuildCountsReady(u64),
    /// Task 099: the count could not be read -- same rule as
    /// `ResetCountsFailed`, and "never a zero" (Task 099 §2.3): a
    /// genuinely zero count also shows no line, not just an unreadable one.
    RebuildCountsFailed,
    // Wizard navigation
    WizardBack,
    QueryChanged(String),
    SubmitSearch,
    /// RFC-061 §7 Slice 5: `SubmitSearch`'s own search now runs off the
    /// update thread (`iced::Task::perform`); this carries its outcome back,
    /// plus the query that was actually searched (not re-read from
    /// `AppState.query` on arrival, which may have changed if the user kept
    /// typing while the search was in flight) -- `orbok`'s handler needs it
    /// for RFC-042 history recording. `FolderPicked`/`SearchAgain`'s own
    /// resumed searches don't record history, so they map straight onto
    /// `SearchResultsReady`/`SearchError` below instead of this variant.
    SubmitSearchCompleted {
        query: String,
        outcome: Result<Vec<SearchResultDisplay>, String>,
    },
    SearchResultsReady(Vec<SearchResultDisplay>),
    /// A search failed. Task 060: carries the query that failed, so its
    /// notice can re-run exactly that query.
    SearchError {
        query: String,
        error: String,
    },
    SelectResult(usize),
    /// HANDOFF-041: open the result at this index in its default
    /// application. An index, not a path (§1.3).
    OpenResult(usize),
    /// HANDOFF-041: show the result at this index in the file manager.
    RevealResult(usize),
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
    /// HANDOFF-038: orbok queued the result's file for re-preparation, so
    /// its row now says so.
    ResultPreparationQueued {
        result_idx: usize,
    },
    PersistLocale(Locale),
    SetLocale(Locale),
    // RFC-034: keyboard navigation messages
    /// Focus the global search text input (Ctrl/Cmd+K).
    FocusSearch,
    /// Close any active overlay/dialog and restore focus to trigger (Escape).
    DismissOverlay,
    /// Move result selection down (Arrow Down, when not typing, Search view).
    SelectNextResult,
    /// Move result selection up (Arrow Up, when not typing, Search view).
    SelectPrevResult,
    /// Move source selection down (Arrow Down, when not typing, Sources
    /// view) -- RFC-034 (Task 024), mirrors `SelectNextResult`.
    SelectNextSource,
    /// Move source selection up (Arrow Up, when not typing, Sources view).
    SelectPrevSource,
    /// Move keyboard focus to the next focusable widget (`Tab`) -- RFC-034
    /// §2.1.1 (Task 024). Reaches text inputs only: `button` does not
    /// implement `Focusable` in iced 0.14, so this cannot reach the 37
    /// button sites this task otherwise binds directly.
    FocusNext,
    /// Move keyboard focus to the previous focusable widget (`Shift+Tab`).
    FocusPrevious,
    /// Task 081: ask for a fresh measurement -- the "Calculate now" button,
    /// switching to the Storage view, and (`main.rs`) after any cleanup
    /// action or a reset succeeds. Dispatches the real measurement off the
    /// update thread (`Task::perform`, the same pattern search uses).
    StorageMeasurementRequested,
    StorageDataReady {
        rows: Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
        /// Task 081 §2: the cache database file's own size, `None` if it
        /// could not be read (never a silent zero).
        cache_file_bytes: Option<u64>,
    },
    /// Task 081: the measurement itself could not run (the catalog or the
    /// cache could not be reached). Raises `StorageUnavailable`
    /// (`notice_retry::storage_measurement_failed`, app crate) and leaves
    /// `storage_rows` exactly as it was -- Task 075's rule: a failure is
    /// never presented as a fresh zero.
    StorageMeasurementFailed,
    // Startup wizard
    WizardPathChanged(String),
    WizardValidate,
    WizardChecked {
        model_dir: String,
        checks: Vec<WizardFileCheck>,
        all_ok: bool,
    },
    WizardAccept,
    ModelPersistenceCompleted {
        ready_id: ReadyId,
        persistence_attempt_id: PersistenceAttemptId,
        model_dir: String,
        provenance: ModelProvenance,
        result: ModelPersistenceResult,
    },
    /// Task 055 §1(c): the saved model was resolved for search (or could not
    /// be). Only this -- never `ModelPersistenceCompleted` -- may make
    /// `capability` read `Hybrid`, because only this arrives after search
    /// actually holds the model.
    ModelActivationCompleted {
        ready_id: ReadyId,
        persistence_attempt_id: PersistenceAttemptId,
        activated: bool,
    },
    /// Task 057: "Try again" on the wizard's load-failed step -- re-runs
    /// loading the saved model, not saving it.
    WizardRetryModelLoad,
    /// Task 057: "Try again" on the notice that background preparation could
    /// not load the model -- asks it to load the model again.
    RetryModelLoad,
    WizardSkip,
    // Source management
    SourcePathChanged(String),
    /// Task 105: Enter in the Folders page's path field. Adds the typed path
    /// (an empty field does nothing); the Add folder button keeps opening the
    /// picker, so each control does one thing.
    SubmitSourcePath,
    /// Task 110: a folder that may contain private files was picked or typed;
    /// ask before adding. Raised by orbok; nothing is saved until the answer.
    AskAddSensitiveFolder(PendingFolderAdd),
    /// The answer was no: nothing is saved, no notice, the typed text and the
    /// pending query are kept.
    CancelAddSensitiveFolder,
    /// The answer was "Add anyway": orbok takes the pending folder and adds it
    /// (`take_confirmed_folder_add`).
    ConfirmAddSensitiveFolder,
    /// The Folders-page add, after the answer.
    AddFolderConfirmed(String),
    /// The search-in-folder add, after the answer.
    FolderPickedConfirmed(std::path::PathBuf),
    RequestAddSource,
    /// RFC-061 §7 Slice 5: the OS folder picker `RequestAddSource` opens
    /// returned `path` -- mirrors RFC-045's `FolderPicked`, but for the
    /// Sources-management "Add source" flow rather than the search-in-folder
    /// one (different follow-up: create/scan the source, no search to
    /// resume).
    AddSourceFolderPicked(std::path::PathBuf),
    /// The "Add source" folder picker was cancelled -- neutral, no error.
    AddSourceFolderPickerCancelled,
    SourceAdded(SourceCard),
    /// Task 073: a request to remove this folder, sent once its
    /// confirmation is confirmed. Changes no state: `orbok` removes it from
    /// the catalog, then sends `SourceRemovalSucceeded` or raises
    /// `SourceCouldNotBeRemoved`.
    SourceRemoved(String), // source_id
    /// Task 073: the catalog removed this folder; the list now drops it.
    SourceRemovalSucceeded(String), // source_id
    /// Task 114: the card's button for a folder that covers its subfolders --
    /// opens "Stop including subfolders?" for it. Changes nothing yet.
    AskNarrowFolder(String), // source_id
    /// Task 114: its counted line, fetched off the update thread.
    NarrowCountReady(String, u64), // source_id, files
    /// Task 114: the count could not be read: the question shows no line.
    NarrowCountFailed,
    CancelNarrowFolder,
    /// Task 114: taken by orbok (`take_confirmed_narrowing`) and dispatched as
    /// [`Message::NarrowFolderRequested`].
    ConfirmNarrowFolder,
    /// Task 114: a request, sent once the question is confirmed. Changes no
    /// state: orbok erases off the update thread, then sends `FolderNarrowed`
    /// (or raises the failure notice) and a card refresh.
    NarrowFolderRequested(String), // source_id
    /// Task 114: the catalog now holds this folder as "this folder only".
    FolderNarrowed(String), // source_id
    /// Task 114: the card's button for a "this folder only" folder. Asks
    /// nothing: it adds nothing the user did not ask for (RFC-064 §3.2).
    WidenFolder(String), // source_id
    /// Task 113: the catalog made these folders part of another; the list
    /// drops their cards, anything that pointed at one is pointed at the
    /// folder that holds it, and a notice says what happened.
    FoldersCombined(FoldersCombined),
    /// RFC-037 §10.2 manual refresh (Task 035): "[Check again]" for a
    /// missing/permission-denied source, "[Prepare again]" for an active
    /// one — same message either way, `orbok`'s handler calls the same
    /// `bootstrap::check_and_refresh_source` regardless of which label the
    /// view showed, since the action is identical and only the label
    /// depends on current state.
    SourceRefreshRequested(String), // source_id
    // Download
    DownloadModel,
    ConfirmModelDownload,
    CancelModelDownload,
    RetryModelDownload,
    /// Task 025: stop an in-progress download, as opposed to
    /// `CancelModelDownload`, which withdraws consent before one starts.
    CancelDownloadInProgress,
    DownloadFileProgress {
        artifact: ModelArtifact,
        bytes: u64,
        total: u64,
        files_done: u32,
        files_total: u32,
    },
    DownloadAllComplete {
        dest_dir: String,
    },
    DownloadFailed(ModelDeliveryFailure),
    // Startup population
    HealthUpdated(IndexHealth),
    SourcesLoaded(Vec<SourceCard>),
    /// Task 108: fresh counts and states for folders already on screen,
    /// sent while preparation runs. Unlike `SourcesLoaded` it replaces a card
    /// only where the folder is already listed, in place: it adds no card,
    /// removes none and never touches the selection, so a read that raced a
    /// removal cannot bring the folder back.
    SourceCardsRefreshed(Vec<SourceCard>),
    // RFC-043: model readiness
    ModelReadinessChecked {
        ready: bool,
        needs_download: bool,
        needs_repair: bool,
    },
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
    /// Task 105: "Choose a folder" on the search page's no-folder line, the
    /// control behind the prompt: opens the same picker a submitted search
    /// opens. Ignored while one is already open (Task 047's rule).
    ChooseSearchFolder,
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
    /// Task 075: the catalog removed this history entry; the list drops it.
    RecentSearchRemoved(SearchHistoryId),
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
        let view_before = self.active_view;
        self.apply(message);
        // Task 069: a confirmation renders on one view, so changing view
        // closes every confirmation -- whatever message changed it (the tab
        // bar's `Switch`, the sidebar's `SwitchGroup`, a shortcut).
        if self.active_view != view_before {
            self.confirm_remove_source = None;
            self.cancel_narrowing();
            self.confirm_reset = false;
            self.confirm_clear_history = false;
            self.cancel_folder_add();
        }
        // Task 064: an info notice belongs to the view it was raised on, so a
        // view change clears it -- compared here, once, rather than in each
        // message that can switch views. A problem notice stays.
        if self.active_view != view_before && self.notice.as_ref().is_some_and(|n| !n.is_problem())
        {
            self.clear_notice();
        }
        self.normalise_location_scope();
    }

    fn apply(&mut self, message: &Message) {
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
            Message::AskResetCatalog => {
                self.confirm_reset = true;
                self.confirm_remove_source = None;
                // Task 092: cleared, not left stale from a previous
                // opening -- the router dispatches a fresh count
                // alongside this; until it lands, the dialog shows no
                // line rather than a number from before.
                self.reset_counts = None;
            }
            Message::ResetCountsReady(counts) => self.reset_counts = Some(*counts),
            Message::ResetCountsFailed => self.reset_counts = None,
            Message::AskRemoveSource(id) => {
                // Task 073: only a folder in the list can be asked about --
                // otherwise there is no dialog to show, and nothing opens.
                if self.sources.iter().any(|card| &card.source_id == id) {
                    self.confirm_remove_source = Some(id.clone());
                    self.confirm_reset = false;
                }
            }
            Message::CancelRemoveSource => self.confirm_remove_source = None,
            Message::AskNarrowFolder(id) => {
                // Only a folder in the list that covers its subfolders can be
                // asked about: there is no dialog to show for any other.
                if self
                    .sources
                    .iter()
                    .any(|card| &card.source_id == id && card.covers_subfolders)
                {
                    self.confirm_narrow_source = Some(id.clone());
                    // Not left over from an earlier opening.
                    self.narrow_file_count = None;
                    self.confirm_remove_source = None;
                    self.confirm_reset = false;
                }
            }
            Message::NarrowCountReady(id, count) => {
                if self.confirm_narrow_source.as_ref() == Some(id) {
                    self.narrow_file_count = (*count > 0).then_some(*count);
                }
            }
            Message::NarrowCountFailed => self.narrow_file_count = None,
            Message::CancelNarrowFolder => self.cancel_narrowing(),
            Message::ConfirmNarrowFolder => {} // handled by orbok: take_confirmed_narrowing
            Message::NarrowFolderRequested(_) => {} // handled by orbok
            Message::WidenFolder(_) => {}      // handled by orbok
            Message::FolderNarrowed(id) => self.apply_folder_narrowed(id),
            Message::ConfirmRemoveSource => {} // handled by orbok: take_confirmed_removal
            Message::CancelResetCatalog => {
                self.confirm_reset = false;
                self.reset_counts = None;
            }
            Message::ConfirmResetCatalog => {
                // Task 075: a request. orbok resets the catalog, then sends
                // `CatalogResetSucceeded`; nothing clears before that.
                self.confirm_reset = false;
            }
            Message::AskDeleteKeywordIndex => {
                self.confirm_delete_keyword_index = true;
                self.confirm_delete_vector_index = false;
                self.confirm_reset = false;
                self.confirm_remove_source = None;
                // Task 099 (mirrors Task 092): cleared, not left stale from
                // a previous opening -- the router dispatches a fresh count
                // alongside this.
                self.rebuild_file_count = None;
            }
            Message::AskDeleteVectorIndex => {
                self.confirm_delete_vector_index = true;
                self.confirm_delete_keyword_index = false;
                self.confirm_reset = false;
                self.confirm_remove_source = None;
                self.rebuild_file_count = None;
            }
            Message::RebuildCountsReady(count) => {
                // §2.3: "never a zero" -- a genuinely zero count shows no
                // line, the same as an unreadable one.
                self.rebuild_file_count = (*count > 0).then_some(*count);
            }
            Message::RebuildCountsFailed => self.rebuild_file_count = None,
            Message::CancelDeleteKeywordIndex => {
                self.confirm_delete_keyword_index = false;
                self.rebuild_file_count = None;
            }
            Message::CancelDeleteVectorIndex => {
                self.confirm_delete_vector_index = false;
                self.rebuild_file_count = None;
            }
            Message::ConfirmDeleteKeywordIndex => {
                // The request itself only closes the confirmation -- the
                // delete and rebuild-marking run off the update thread
                // (Task 097's shape), landing as `RebuildIndexSucceeded`
                // or a `CleanupDidNotFinish` notice.
                self.confirm_delete_keyword_index = false;
            }
            Message::ConfirmDeleteVectorIndex => {
                self.confirm_delete_vector_index = false;
            }
            Message::CleanSnippets
            | Message::CleanSearchCache
            | Message::CleanTemporaryExtraction
            | Message::RemoveReplacedStaleIndexes => {
                // Actual work done in orbok; the per-action done-notice
                // arrives via Message::ShowNotice (Review 214 §4 Q2, owner
                // decision 2026-09-12: each of the four Safe-cleanup
                // actions gets its own notice title, not one shared
                // "CleanupDone" that could not distinguish which action
                // just ran).
            }
            Message::WizardBack => {
                // Return to the initial setup step.
                self.wizard = Some(crate::state::WizardState::NotConfigured);
                self.wizard_path_input = String::new();
            }
            Message::ShowNotice(n) => self.raise_notice(n.clone(), None),
            Message::ShowNoticeWithAction { notice, action } => {
                self.raise_notice(notice.clone(), Some(action.clone()));
            }
            Message::NoticeActionPressed => {} // handled by orbok
            Message::RetrySearch(query) => self.query = query.clone(),
            // Task 057: orbok re-sends the model change to background
            // preparation; the notice it answers is dismissed here.
            Message::ClearNotice | Message::RetryModelLoad => self.clear_notice(),
            Message::QueryChanged(query) => {
                self.query = query.clone();
                self.search_ui.text = query.clone();
            }
            Message::SubmitSearch => {
                let trimmed = self.query.trim();
                if !trimmed.is_empty() {
                    // Task 068: a submitted search resolves the pending one.
                    self.search_location.pending_query = None;
                    self.last_query = Some(trimmed.to_string());
                    self.search_running = true;
                    self.search_results.clear();
                    self.selected_result = None;
                    self.search_ui.results_status = ResultsStatus::Searching;
                }
            }
            Message::SubmitSearchCompleted { .. } => {} // handled in orbok: dispatches SearchResultsReady/SearchError
            Message::SearchResultsReady(results) => {
                let count = results.len();
                self.search_results = results.clone();
                self.search_running = false;
                self.selected_result = None;
                self.search_ui.trust_details_open.clear();
                // Task 065: new results resolve a failed search, and make a
                // launch-failure notice stale -- its Show-in-folder or Try
                // again retry is a result index, which would now point at a
                // different file.
                // Other problem notices stay (Task 064).
                if matches!(
                    self.notice,
                    Some(
                        UserNotice::SearchDidNotFinish
                            | UserNotice::FileCouldNotBeFound
                            | UserNotice::FileCouldNotBeOpened
                            | UserNotice::FileNotAllowed
                            | UserNotice::FileCheckFailed
                    )
                ) {
                    self.clear_notice();
                }
                self.search_ui.results_status = self.results_status_for(count);
            }
            Message::SearchError { query, .. } => {
                self.search_running = false;
                self.search_ui.results_status = ResultsStatus::Problem {
                    friendly_message: tr(self.locale, MessageKey::NoticeSearchFailBody).to_string(),
                };
                // Task 060: Try again re-runs the query that failed.
                self.raise_notice(
                    UserNotice::SearchDidNotFinish,
                    Some(Box::new(Message::RetrySearch(query.clone()))),
                );
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
            // RFC-038: trust recovery actions. Removing a row and opening a
            // detail change only what is shown, so they happen here.
            // `PrepareAgain` and `CheckFolder` touch the catalog: orbok does
            // them, and `ResultPreparationQueued` reports the first.
            // `OpenAnyway` and `ShowInFolder` are `result_launch`'s.
            Message::TrustRecoveryAction { result_idx, action } => match action {
                ResultRecoveryAction::RemoveFromResults => self.remove_result(*result_idx),
                ResultRecoveryAction::ViewDetails => {
                    if let Some(result) = self.search_results.get(*result_idx)
                        && !self
                            .search_ui
                            .trust_details_open
                            .contains(&result.canonical_path)
                    {
                        self.search_ui
                            .trust_details_open
                            .push(result.canonical_path.clone());
                    }
                }
                ResultRecoveryAction::PrepareAgain
                | ResultRecoveryAction::CheckFolder
                | ResultRecoveryAction::OpenAnyway
                | ResultRecoveryAction::ShowInFolder => {}
            },
            Message::ResultPreparationQueued { result_idx } => {
                if let Some(result) = self.search_results.get_mut(*result_idx) {
                    result.trust = ResultTrustDisplay {
                        state: ResultTrustState::StillBeingPrepared,
                        recovery_actions: Vec::new(),
                        warnings: Vec::new(),
                    };
                }
            }
            Message::SelectResult(idx) => self.selected_result = Some(*idx),
            Message::OpenResult(_) | Message::RevealResult(_) => {} // handled by orbok
            Message::SetSearchMode(mode) => self.search_mode = *mode,
            Message::PersistLocale(locale) | Message::SetLocale(locale) => self.locale = *locale,
            // RFC-034 keyboard navigation: FocusSearch is handled in orbok
            // (it issues an iced focus task); DismissOverlay closes any overlay.
            Message::FocusSearch => {} // focus task issued by orbok
            Message::DismissOverlay => {
                // Close whichever overlay is open, in priority order.
                // RFC-034 (Task 024): extended with the wizard's own
                // zero-confirmation "give up" fallback and, failing that,
                // whichever list selection the active view owns -- the
                // same "innermost open thing closes first" shape this
                // arm already had, just with more things now able to be
                // open.
                if self.pending_folder_add.is_some() {
                    self.cancel_folder_add();
                } else if self.confirm_narrow_source.is_some() {
                    self.cancel_narrowing();
                } else if self.confirm_remove_source.is_some() {
                    self.confirm_remove_source = None;
                } else if self.confirm_reset {
                    self.confirm_reset = false;
                } else if self.confirm_delete_keyword_index {
                    self.confirm_delete_keyword_index = false;
                    self.rebuild_file_count = None;
                } else if self.confirm_delete_vector_index {
                    self.confirm_delete_vector_index = false;
                    self.rebuild_file_count = None;
                } else if self.confirm_clear_history {
                    self.confirm_clear_history = false;
                } else if self.notice.is_some() {
                    self.clear_notice();
                } else {
                    match self.wizard.as_ref().map(WizardState::kind) {
                        // Task 059: both failed Ready pages too, so neither is
                        // a dead end. Skipping from the load-failed page
                        // leaves the model *saved*, so the next startup tries
                        // to load it again -- correct, since the failure may
                        // have been transient, and forgetting a saved model
                        // is not what Skip means.
                        Some(
                            WizardKind::Setup
                            | WizardKind::CheckedOk
                            | WizardKind::CheckedNotOk
                            | WizardKind::DownloadFailed
                            | WizardKind::ReadyFailed
                            | WizardKind::ReadyLoadFailed,
                        ) => {
                            // Same zero-confirmation fallback the
                            // mouse-only Skip button already performs on
                            // these pages -- Escape gives keyboard users
                            // the identical way out (Task 024, origin:
                            // Owner Task 003 Part B, "nothing worked at
                            // all").
                            self.skip_wizard();
                        }
                        Some(WizardKind::DownloadConsent) => {
                            // Mirrors the page's own Cancel button
                            // exactly -- same code `CancelModelDownload`
                            // already runs.
                            if let Some(WizardState::DownloadConsent { return_to, .. }) =
                                self.wizard.take()
                            {
                                self.wizard = Some(return_to.into_wizard());
                            }
                        }
                        Some(WizardKind::Downloading | WizardKind::ReadyInFlight) => {
                            // Downloading now has a mouse-reachable way out
                            // (Task 025's Cancel button), but Escape does
                            // not reach it: `Message::CancelDownloadInProgress`
                            // needs a backend effect (the cancellation
                            // flag) that this pure-state `update` cannot
                            // issue, and `DismissOverlay` was deliberately
                            // left unrouted through `model_flow.rs` rather
                            // than reopen Task 024's already-reviewed
                            // keyboard dispatch mid-task -- a known,
                            // reported gap (Task 025 review), not a silent
                            // one. Ready-while-saving still has no way out
                            // at all either way.
                        }
                        Some(WizardKind::ReadyIdle) => {
                            // Ready has no Skip/Cancel via mouse either;
                            // same reasoning as above.
                        }
                        None => {
                            if self.active_view == ViewId::Search && self.selected_result.is_some()
                            {
                                self.selected_result = None;
                            } else if self.active_view == ViewId::Sources
                                && self.selected_source.is_some()
                            {
                                self.selected_source = None;
                            }
                        }
                    }
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
            // RFC-034 (Task 024): mirrors SelectNextResult/SelectPrevResult
            // exactly, for the Sources view's own list.
            Message::SelectNextSource => {
                if !self.sources.is_empty() {
                    self.selected_source = Some(match self.selected_source {
                        None => 0,
                        Some(i) => (i + 1).min(self.sources.len() - 1),
                    });
                }
            }
            Message::SelectPrevSource => {
                if !self.sources.is_empty() {
                    self.selected_source = Some(match self.selected_source {
                        None | Some(0) => 0,
                        Some(i) => i - 1,
                    });
                }
            }
            // RFC-034 (Task 024): the actual focus movement is an iced
            // Task, issued by `orbok` (see `FocusSearch`'s own comment
            // for why this split exists); nothing in `AppState` changes.
            Message::FocusNext | Message::FocusPrevious => {}
            // Task 081: `main.rs` runs the real measurement (a catalog/
            // cache read, off the update thread) and dispatches this
            // request itself; the reducer only marks the request as sent,
            // so `storage_view` can show it is in flight.
            Message::StorageMeasurementRequested => self.storage_measuring = true,
            Message::StorageDataReady {
                rows,
                cache_file_bytes,
            } => {
                self.storage_measuring = false;
                self.storage_rows = rows.clone();
                self.storage_cache_file_bytes = *cache_file_bytes;
            }
            Message::StorageMeasurementFailed => {
                self.storage_measuring = false;
                // Task 081: the numbers already on screen (or the RFC-011
                // §13.1 empty state) are left exactly as they were --
                // never replaced by a fresh zero. "Try again" re-sends the
                // same request.
                self.raise_notice(
                    UserNotice::StorageUnavailable,
                    Some(Box::new(Message::StorageMeasurementRequested)),
                );
            }
            Message::WizardPathChanged(p) => self.wizard_path_input = p.clone(),
            Message::WizardValidate => {} // handled in orbok update
            Message::WizardChecked {
                model_dir: _,
                checks: _,
                all_ok: _,
            }
            | Message::WizardAccept
            | Message::ModelPersistenceCompleted { .. }
            | Message::ModelActivationCompleted { .. }
            | Message::WizardRetryModelLoad => {}
            Message::WizardSkip => self.skip_wizard(),
            Message::DownloadModel => {
                let return_to = self
                    .wizard
                    .as_ref()
                    .and_then(ModelConsentReturn::from_wizard);
                if let (Some(presentation), Some(return_to)) =
                    (self.model_download_consent.clone(), return_to)
                {
                    self.wizard = Some(WizardState::DownloadConsent {
                        presentation,
                        return_to,
                    });
                }
            }
            Message::ConfirmModelDownload | Message::RetryModelDownload => {}
            Message::CancelModelDownload => {
                if let Some(WizardState::DownloadConsent { return_to, .. }) = self.wizard.take() {
                    self.wizard = Some(return_to.into_wizard());
                }
            }
            Message::DownloadFileProgress { .. }
            | Message::DownloadAllComplete { .. }
            | Message::DownloadFailed(_)
            | Message::CancelDownloadInProgress => {} // handled in model_flow.rs
            Message::SourcePathChanged(p) => self.source_path_input = p.clone(),
            // Task 047: orbok opens the dialog and adds the folder; its arms
            // return before this reducer runs, so they forward these three
            // messages here explicitly.
            Message::SubmitSourcePath => {} // handled by orbok (Task 105)
            Message::AskAddSensitiveFolder(pending) => {
                self.pending_folder_add = Some(pending.clone());
                self.confirm_remove_source = None;
            }
            Message::CancelAddSensitiveFolder => self.cancel_folder_add(),
            Message::ConfirmAddSensitiveFolder => {} // handled by orbok: take_confirmed_folder_add
            Message::AddFolderConfirmed(_) | Message::FolderPickedConfirmed(_) => {} // orbok
            Message::ChooseSearchFolder => {}        // handled by orbok (Task 105)
            Message::RequestAddSource => self.add_source_picker_in_progress = true,
            Message::AddSourceFolderPicked(_) => self.add_source_picker_in_progress = false,
            Message::AddSourceFolderPickerCancelled => self.add_source_picker_in_progress = false,
            Message::SourceAdded(card) => {
                self.sources.push(card.clone());
                self.source_path_input = String::new();
                self.raise_notice(UserNotice::FolderAdded, None);
                // RFC-034 (Task 024): the list changed shape; matches
                // `SearchResultsReady`'s own reset of `selected_result`
                // rather than risk a stale/misleading index.
                self.selected_source = None;
            }
            // Task 073: the list changes only after the catalog does.
            Message::SourceRemoved(_) => {} // handled by orbok: source_removal::remove
            Message::SourceRemovalSucceeded(id) => {
                self.sources.retain(|s| s.source_id != *id);
                self.selected_source = None;
                // The removal the notice reported as failed has now
                // happened; "Folder not removed" would be untrue. Other
                // problem notices stay (Task 064), as `SearchResultsReady`
                // clears only the failures it resolves (Task 065).
                if self.notice == Some(UserNotice::SourceCouldNotBeRemoved) {
                    self.clear_notice();
                }
            }
            Message::FoldersCombined(combined) => self.apply_folders_combined(combined),
            Message::SourceRefreshRequested(_) => {} // handled by orbok; result arrives via SourcesLoaded/HealthUpdated
            Message::HealthUpdated(health) => {
                self.health = *health;
            }
            Message::SourcesLoaded(cards) => {
                self.sources = cards.clone();
                self.selected_source = None;
            }
            Message::SourceCardsRefreshed(cards) => {
                for fresh in cards {
                    if let Some(card) = self
                        .sources
                        .iter_mut()
                        .find(|c| c.source_id == fresh.source_id)
                    {
                        *card = fresh.clone();
                    }
                }
            }
            // RFC-043: model readiness
            Message::ModelReadinessChecked { .. } => {} // handled by orbok
            // RFC-040: diagnostics
            Message::DiagnosticsCreateBundle => {} // handled by orbok
            Message::DiagnosticsBundleCreated(_) => {
                self.raise_notice(UserNotice::DiagnosticsFileCreated, None);
            }
            Message::DiagnosticsBundleFailed => {
                self.raise_notice(
                    UserNotice::DiagnosticsFileFailed,
                    Some(Box::new(Message::DiagnosticsCreateBundle)),
                );
            }
            Message::DiagnosticsOptInChanged { .. } => {} // handled by orbok
            // RFC-045: search-in-folder flow
            Message::ChooseFolderRequested => {
                // Guard: block duplicate picker dialogs on rapid Search clicks.
                self.search_location.picker_in_progress = true;
                // Task 068: the search this picker is for.
                let query = self.query.trim();
                self.search_location.pending_query = (!query.is_empty()).then(|| query.to_string());
            }
            Message::FolderPickerCancelled => {
                // RFC-045 §8.2: cancel is neutral — no error, query preserved.
                self.search_location.picker_in_progress = false;
                self.search_location.pending_query = None;
            }
            Message::FolderPicked(_) => {
                // Handled in orbok (source create/reuse); result arrives
                // via SearchLocationSelected. Keep picker_in_progress = true
                // until the source record is ready.
            }
            Message::SearchLocationSelected(location) => {
                self.search_location.picker_in_progress = false;
                self.search_location.selected = Some(location.clone());
                // Task 068: no "Searching" here. This message issues no search
                // task, so setting it left a first search stuck forever;
                // `SubmitSearch`, which does issue one, sets it.
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
                if self.search_ui.restoring_history_id.as_ref() == Some(id) {
                    self.search_ui.restoring_history_id = None;
                }
            }
            // Task 075: a request; the entry goes on `RecentSearchRemoved`.
            Message::RemoveRecentSearch(_) => {} // handled by orbok
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
            Message::RecentSearchRemoved(id) => {
                self.search_ui.history.retain(|e| e.id != *id);
            }
            Message::CatalogResetSucceeded => {
                self.sources.clear();
                self.selected_source = None;
                self.health = crate::state::IndexHealth::default();
                self.search_results.clear();
                self.reset_counts = None;
                // Task 081: cleared, not left stale -- `main.rs` follows
                // this with `StorageMeasurementRequested` so the page
                // shows the measured post-reset state, not the RFC-011
                // §13.1 empty state a plain clear would leave it in.
                self.storage_rows.clear();
                // Task 094: a reset now clears recent searches in the
                // catalog too (`CleanupExecutor::run_reset_catalog`); this
                // is the on-screen half, the same shape
                // `RecentSearchesCleared` already uses. The "Remember
                // recent searches" setting itself is untouched -- it lives
                // in `settings.json`, a file reset never opens, not in
                // this catalog.
                self.search_ui.history.clear();
                self.search_ui.history_panel_open = false;
            }
            Message::HistoryLoaded(entries) => {
                self.search_ui.history = entries.clone();
            }
            Message::ToggleRememberRecentSearches(on) => {
                // The setting shows the new value at once, as theme and text
                // size do (`SettingCouldNotBeSaved` reports a failed save).
                // Task 075: turning it off clears history, but the visible
                // list empties only on `RecentSearchesCleared`, which orbok
                // sends once the catalog has cleared it (RFC-042 §13.4).
                self.remember_recent_searches = *on;
            }
        }
    }

    /// Abandon the model-setup wizard, falling back to keyword-only
    /// search. Shared by `Message::WizardSkip` (the mouse-only button)
    /// and `Message::DismissOverlay` (RFC-034 §2.1.1 / Task 024's
    /// keyboard equivalent) so the two can never drift apart.
    /// Show `notice`, with the retry its action button sends, or none.
    fn raise_notice(&mut self, notice: UserNotice, action: Option<Box<Message>>) {
        // Task 064: an info notice never replaces an unresolved problem; it is
        // dropped, and the problem keeps its own action. A new problem
        // replaces whatever is showing -- the latest failure wins.
        if !notice.is_problem() && self.notice.as_ref().is_some_and(UserNotice::is_problem) {
            return;
        }
        self.notice = Some(notice);
        self.notice_action = action;
    }

    /// Task 113: every place the window holds a folder by id, for folders
    /// that are now part of `combined.parent_id`.
    fn apply_folders_combined(&mut self, combined: &FoldersCombined) {
        let absorbed = |id: &str| combined.folders.iter().find(|f| f.source_id == id);
        self.sources
            .retain(|card| absorbed(&card.source_id).is_none());
        self.selected_source = None;
        // A removal question about a folder that no longer exists has no
        // dialog to show (Task 073).
        if self
            .confirm_remove_source
            .as_deref()
            .is_some_and(|id| absorbed(id).is_some())
        {
            self.confirm_remove_source = None;
        }
        if self
            .confirm_narrow_source
            .as_deref()
            .is_some_and(|id| absorbed(id).is_some())
        {
            self.cancel_narrowing();
        }
        self.search_location
            .recent_locations
            .retain(|summary| absorbed(summary.source_id.as_str()).is_none());
        // A search that was looking at a combined folder keeps looking at
        // the same files: the folder that holds it now, limited to it.
        if let Some(location) = self.search_location.selected.clone()
            && let Some(source_id) = location.source_id()
            && let Some(folder) = absorbed(source_id.as_str())
        {
            let (name, limit) = match location.limit_path() {
                Some(limit) => (location.display_name().to_string(), limit.to_string()),
                None => (folder.display_name.clone(), folder.canonical_path.clone()),
            };
            self.search_location.selected = Some(
                SearchLocation::within(
                    orbok_core::SourceId::from_string(combined.parent_id.clone()),
                    name,
                    limit,
                )
                .with_scope(location.scope()),
            );
        }
        self.raise_notice(
            UserNotice::FoldersCombined {
                folders: combined
                    .folders
                    .iter()
                    .map(|f| f.display_name.clone())
                    .collect(),
                parent: combined.parent_name.clone(),
            },
            None,
        );
    }

    fn clear_notice(&mut self) {
        self.notice = None;
        self.notice_action = None;
    }

    /// The results status for a list of `count` results (Task 075's reducer
    /// arms and HANDOFF-038's row removal share it).
    fn results_status_for(&self, count: usize) -> ResultsStatus {
        if count == 0 {
            if self.search_ui.has_active_filters() {
                ResultsStatus::EmptyAfterFiltering
            } else {
                ResultsStatus::EmptyAfterSearch
            }
        } else {
            ResultsStatus::Ready { total_count: count }
        }
    }

    /// HANDOFF-038 `RemoveFromResults`: drop one row from the visible list --
    /// state only, nothing on disk. Everything that holds a result index is
    /// kept consistent: the selection follows its row, and a launch-failure
    /// notice whose retry is an index (Task 065) goes, as it does when the
    /// results are replaced.
    fn remove_result(&mut self, index: usize) {
        if index >= self.search_results.len() {
            return;
        }
        let removed = self.search_results.remove(index);
        self.search_ui
            .trust_details_open
            .retain(|path| *path != removed.canonical_path);
        self.selected_result = match self.selected_result {
            Some(selected) if selected == index => None,
            Some(selected) if selected > index => Some(selected - 1),
            other => other,
        };
        self.search_ui.results_status = self.results_status_for(self.search_results.len());
        if matches!(
            self.notice,
            Some(
                UserNotice::FileCouldNotBeFound
                    | UserNotice::FileCouldNotBeOpened
                    | UserNotice::FileNotAllowed
                    | UserNotice::FileCheckFailed
            )
        ) {
            self.clear_notice();
        }
    }

    /// Task 073: the folder the removal confirmation is for -- its card in
    /// the list, or `None` when no confirmation is open or its folder is not
    /// listed. The one lookup behind both the dialog `sources_view` renders
    /// and `visible_confirmation`, so the two can never disagree.
    pub fn removal_target(&self) -> Option<&SourceCard> {
        let id = self.confirm_remove_source.as_ref()?;
        self.sources.iter().find(|card| &card.source_id == id)
    }

    /// Task 114: the folder the narrowing question is for -- its card, or
    /// `None` when no question is open, its folder is not listed, or it no
    /// longer covers its subfolders (there is then nothing to stop including).
    pub fn narrow_target(&self) -> Option<&SourceCard> {
        let id = self.confirm_narrow_source.as_ref()?;
        self.sources
            .iter()
            .find(|card| &card.source_id == id && card.covers_subfolders)
    }

    /// Task 114: close the narrowing question without changing anything.
    pub fn cancel_narrowing(&mut self) {
        self.confirm_narrow_source = None;
        self.narrow_file_count = None;
    }

    /// Task 114: the narrowing question was confirmed. Returns the request to
    /// dispatch for the folder it was opened for, and closes the question.
    pub fn take_confirmed_narrowing(&mut self) -> Option<Message> {
        self.narrow_file_count = None;
        self.confirm_narrow_source
            .take()
            .map(Message::NarrowFolderRequested)
    }

    /// Task 114: whether the selected search location is a folder set to
    /// "this folder only": its scope is then fixed, and offers no toggle
    /// (RFC-064 §3.4). A location limited to a subfolder never is: it is a
    /// folder *inside* the one it names.
    pub fn search_location_is_folder_only(&self) -> bool {
        let Some(location) = self.search_location.selected.as_ref() else {
            return false;
        };
        location.limit_path().is_none()
            && location.source_id().is_some_and(|id| {
                self.sources
                    .iter()
                    .any(|card| card.source_id == id.as_str() && !card.covers_subfolders)
            })
    }

    /// Task 114: a remembered "and subfolders" for a folder that has since
    /// been narrowed is shown, and searched, as "only": the state never holds
    /// the contradiction. Run after every message, so the several ways the
    /// folder list can change (a reload, a refresh, the narrowing itself) all
    /// meet it.
    fn normalise_location_scope(&mut self) {
        if self.search_location_is_folder_only()
            && self
                .search_location
                .selected
                .as_ref()
                .map(SearchLocation::scope)
                == Some(SearchFolderScope::FolderAndSubfolders)
        {
            self.search_location
                .set_scope(SearchFolderScope::FolderOnly);
        }
    }

    /// Task 114: the folder is now "this folder only". A search that was
    /// limited to one of its subfolders has nothing left to look at, so that
    /// location is cleared (the query stays); the scope of one on the folder
    /// itself follows [`Self::normalise_location_scope`] once its card says so.
    fn apply_folder_narrowed(&mut self, id: &str) {
        let limited_inside = self.search_location.selected.as_ref().is_some_and(|l| {
            l.source_id().is_some_and(|s| s.as_str() == id) && l.limit_path().is_some()
        });
        if limited_inside {
            self.search_location.clear();
        }
    }

    /// Task 069: the confirmation the user can actually see -- its flag set,
    /// its own view active, and no wizard replacing the view. Both the views
    /// and the keyboard context use this, so Enter can never confirm
    /// something that is not on screen.
    pub fn visible_confirmation(&self) -> Option<Confirmation> {
        if self.wizard.is_some() {
            return None;
        }
        [
            (self.confirm_reset, Confirmation::ResetCatalog),
            // Task 073: visible only when its folder's card, which the
            // dialog renders, is in the list.
            (self.removal_target().is_some(), Confirmation::RemoveSource),
            // Task 114: visible only when its folder's card, which the
            // dialog names, is in the list and still covers its subfolders.
            (self.narrow_target().is_some(), Confirmation::NarrowFolder),
            (
                self.confirm_clear_history,
                Confirmation::ClearRecentSearches,
            ),
            (
                self.confirm_delete_keyword_index,
                Confirmation::DeleteKeywordIndex,
            ),
            (
                self.confirm_delete_vector_index,
                Confirmation::DeleteVectorIndex,
            ),
            (
                self.pending_folder_add
                    .as_ref()
                    .is_some_and(|p| p.origin == FolderAddOrigin::FoldersPage),
                Confirmation::AddSensitiveFolderOnFolders,
            ),
            (
                self.pending_folder_add
                    .as_ref()
                    .is_some_and(|p| p.origin == FolderAddOrigin::SearchPage),
                Confirmation::AddSensitiveFolderOnSearch,
            ),
        ]
        .into_iter()
        .find(|(open, confirmation)| *open && confirmation.view() == self.active_view)
        .map(|(_, confirmation)| confirmation)
    }

    /// Task 110: close the private-folder question without adding anything.
    /// Cancelling is neutral (RFC-045 §8.2): for a search-in-folder question
    /// the search page goes back to what it was before the picker (the typed
    /// query stays in the box); for a Folders-page question the typed text
    /// stays in the field.
    pub fn cancel_folder_add(&mut self) {
        if let Some(pending) = self.pending_folder_add.take()
            && pending.origin == FolderAddOrigin::SearchPage
        {
            self.search_location.picker_in_progress = false;
            self.search_location.pending_query = None;
        }
    }

    /// Task 110: the question was answered "Add anyway". Returns the add to
    /// dispatch for the folder it was about, and closes the dialog.
    pub fn take_confirmed_folder_add(&mut self) -> Option<Message> {
        self.pending_folder_add
            .take()
            .map(|pending| match pending.origin {
                FolderAddOrigin::FoldersPage => Message::AddFolderConfirmed(pending.path),
                FolderAddOrigin::SearchPage => {
                    Message::FolderPickedConfirmed(std::path::PathBuf::from(pending.path))
                }
            })
    }

    /// Task 062: the removal confirmation was confirmed. Returns the removal
    /// to dispatch -- the one existing `SourceRemoved` path -- for the folder
    /// the dialog was opened for, and closes the dialog.
    pub fn take_confirmed_removal(&mut self) -> Option<Message> {
        self.confirm_remove_source
            .take()
            .map(Message::SourceRemoved)
    }

    /// Task 060: the notice's action button was pressed. Returns the concrete
    /// retry to dispatch, and clears the notice so it does not linger over
    /// the retried action's own outcome.
    pub fn take_notice_action(&mut self) -> Option<Message> {
        let action = self.notice_action.take().map(|action| *action);
        self.notice = None;
        action
    }

    fn skip_wizard(&mut self) {
        self.capability = SearchCapability::KeywordOnly;
        // Task 053: a Conceptual selection made while a model was active
        // would now return nothing, from a button the view has disabled.
        if self.search_mode == SearchMode::Conceptual {
            self.search_mode = SearchMode::Auto;
        }
        self.active_model_provenance = None;
        self.wizard = None;
        self.wizard_path_input = String::new();
    }
}
