//! User-facing notices (UX review §7): friendly, actionable messages that
//! replace silent failures and raw error strings.
//!
//! Lower layers (download, scanner, search) produce technical errors. The UI
//! must never show those directly. Instead they are mapped to a [`UserNotice`]
//! with a plain title, an explanation, and a suggested next action.

use crate::i18n::{Locale, MessageKey, tr};

/// A friendly, actionable message shown to the user. Covers both problems
/// (download failed) and confirmations (folder added).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserNotice {
    // ── Problems ──────────────────────────────────────────────────────
    FolderCouldNotBeAdded,
    SearchDidNotFinish,
    /// Task 065: opening a result was refused because the file is no longer
    /// where orbok found it.
    FileCouldNotBeFound,
    /// Task 065: the file is there, but no app opened it (or the file
    /// manager could not show it).
    FileCouldNotBeOpened,
    /// Task 070: opening a result was refused because orbok may not open the
    /// file (a folder rule, or permission denied).
    FileNotAllowed,
    /// Task 070: the folder list could not be read to check the file; the
    /// same action can be tried again.
    FileCheckFailed,
    // ── Confirmations ─────────────────────────────────────────────────
    FolderAdded,
    /// "Add folder" picked a folder that is already registered (Task 047);
    /// nothing was added.
    FolderAlreadyAdded,
    /// Task 113: the folder chosen lies inside a folder already added, so
    /// nothing was added. Both are named (RFC-064 §3.3).
    FolderAlreadyIncluded {
        folder: String,
        parent: String,
    },
    /// Task 113: folders that were inside a newly added folder (or, at
    /// startup, inside another added one) are now part of it.
    FoldersCombined {
        folders: Vec<String>,
        parent: String,
    },
    SearchReady,
    PreviewsCleared,
    /// RFC-059 §8 Slice 4 (Review 214 §4 Q2): "Clear old search results"
    /// finished -- its own title, not `PreviewsCleared`'s.
    SearchCacheCleared,
    /// "Clear extracted text" finished (RFC-059, erases the namespace
    /// outright -- Review 214 §4 Q1).
    ExtractedTextCleared,
    /// "Remove old data from updated files" finished. Its own byte-reclaim
    /// figure is commonly zero (RFC-059 Amendment 1 §2a.3: the reclaim
    /// usually already happened at re-index time), so its body -- like
    /// every notice here -- never claims a specific amount of space freed.
    ReplacedDataRemoved,
    DiagnosticsFileCreated,
    // ── RFC-040: diagnostics problem ──────────────────────────────────
    DiagnosticsFileFailed,
    // ── RFC-042: search history ───────────────────────────────────────
    /// Confirmation: recent searches were cleared.
    RecentSearchesCleared,
    /// Info: a narrowing choice was dropped on reopen (folder gone).
    RecentSearchFilterDropped,
    // ── RFC-061 §8: failures that were silently swallowed before ───────
    /// A theme/text-scale/reduced-motion write failed; the UI shows the
    /// new value but the next launch may show the old one.
    SettingCouldNotBeSaved,
    /// `reset_catalog` failed; some app data may not have been cleared.
    CatalogResetFailed,
    /// `remove_source` failed; the folder is still registered.
    SourceCouldNotBeRemoved,
    /// Task 075: clearing recent searches failed; the list is unchanged.
    RecentSearchesNotCleared,
    /// Task 075: removing one recent search failed; it is still listed.
    RecentSearchNotRemoved,
    /// Task 075: a Safe cleanup action itself failed (not its cache handle,
    /// which is `StorageUnavailable`).
    CleanupDidNotFinish,
    /// Task 075: checking a folder for changes failed.
    FolderNotChecked,
    /// The model store or cache handle could not be opened for an
    /// in-session action (download, clear previews, clear search data,
    /// full reset) -- was a panic (`.expect(...)`) before RFC-061 §8(d).
    StorageUnavailable,
    /// The background preparation task could not start at all (runtime
    /// context, catalog, or cache open failure) -- orbok looks healthy
    /// but never prepares anything, for the rest of the session.
    IndexingCouldNotStart,
    /// Task 057: background preparation could not load a model that was
    /// saved during this session. Its action asks it to load the model again.
    ModelCouldNotBeLoaded,
}

impl UserNotice {
    /// Whether this notice reports a problem (vs. a success confirmation).
    /// The view can use this to choose tone, but never relies on colour alone.
    ///
    /// Task 064: defined through [`Self::tone`], so the class a notice belongs
    /// to has one source. A problem (`Danger`/`Warning`) stays until the user
    /// dismisses it or presses its action, across view switches; an info
    /// notice (`Success`/`Info`) is cleared when the view changes and never
    /// replaces a problem that is showing.
    pub fn is_problem(&self) -> bool {
        use snora::design::Tone;
        matches!(self.tone(), Tone::Danger | Tone::Warning)
    }

    /// Map this notice to a Snora Design tone. Problem notices use Danger or
    /// Warning; confirmations use Success or Info. The tone drives the
    /// WCAG-AA-verified colors in `snora::design::notice::Notice`.
    pub fn tone(&self) -> snora::design::Tone {
        use snora::design::Tone;
        match self {
            // Hard failures the user must notice.
            Self::FolderCouldNotBeAdded
            | Self::SearchDidNotFinish
            | Self::DiagnosticsFileFailed
            | Self::SettingCouldNotBeSaved
            | Self::CatalogResetFailed
            | Self::SourceCouldNotBeRemoved
            | Self::RecentSearchesNotCleared
            | Self::RecentSearchNotRemoved
            | Self::CleanupDidNotFinish
            | Self::FolderNotChecked
            | Self::StorageUnavailable
            | Self::IndexingCouldNotStart
            | Self::ModelCouldNotBeLoaded => Tone::Danger,
            // Cautions: action succeeded but the user should be aware.
            Self::FileCouldNotBeFound
            | Self::FileCouldNotBeOpened
            | Self::FileNotAllowed
            | Self::FileCheckFailed => Tone::Warning,
            // Positive confirmations.
            Self::FolderAdded | Self::SearchReady => Tone::Success,
            // Neutral/informational.
            Self::PreviewsCleared
            | Self::SearchCacheCleared
            | Self::ExtractedTextCleared
            | Self::ReplacedDataRemoved
            | Self::DiagnosticsFileCreated => Tone::Info,
            Self::RecentSearchesCleared
            | Self::RecentSearchFilterDropped
            | Self::FolderAlreadyAdded
            | Self::FolderAlreadyIncluded { .. }
            | Self::FoldersCombined { .. } => Tone::Info,
        }
    }

    pub fn title(&self, locale: Locale) -> &'static str {
        let key = match self {
            Self::FolderCouldNotBeAdded => MessageKey::NoticeFolderFailTitle,
            Self::SearchDidNotFinish => MessageKey::NoticeSearchFailTitle,
            Self::FileCouldNotBeFound => MessageKey::NoticeFileNotFoundTitle,
            Self::FileCouldNotBeOpened | Self::FileNotAllowed | Self::FileCheckFailed => {
                MessageKey::NoticeFileNotOpenedTitle
            }
            Self::FolderAdded => MessageKey::NoticeFolderAddedTitle,
            Self::FolderAlreadyAdded => MessageKey::NoticeFolderAlreadyAddedTitle,
            Self::FolderAlreadyIncluded { .. } => MessageKey::NoticeFolderAlreadyIncludedTitle,
            Self::FoldersCombined { .. } => MessageKey::NoticeFoldersCombinedTitle,
            Self::SearchReady => MessageKey::NoticeSearchReadyTitle,
            Self::PreviewsCleared => MessageKey::NoticePreviewsClearedTitle,
            Self::SearchCacheCleared => MessageKey::NoticeSearchCacheClearedTitle,
            Self::ExtractedTextCleared => MessageKey::NoticeExtractedTextClearedTitle,
            Self::ReplacedDataRemoved => MessageKey::NoticeReplacedDataRemovedTitle,
            Self::DiagnosticsFileCreated => MessageKey::DiagnosticsFileCreated,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFailed,
            Self::RecentSearchesCleared => MessageKey::RecentSearchesClearedNotice,
            Self::RecentSearchFilterDropped => MessageKey::DroppedFilterNotice,
            Self::SettingCouldNotBeSaved => MessageKey::NoticeSettingSaveFailTitle,
            Self::CatalogResetFailed => MessageKey::NoticeResetFailTitle,
            Self::SourceCouldNotBeRemoved => MessageKey::NoticeSourceRemoveFailTitle,
            Self::RecentSearchesNotCleared => MessageKey::NoticeRecentSearchesNotClearedTitle,
            Self::RecentSearchNotRemoved => MessageKey::NoticeRecentSearchNotRemovedTitle,
            Self::CleanupDidNotFinish => MessageKey::NoticeCleanupDidNotFinishTitle,
            Self::FolderNotChecked => MessageKey::NoticeFolderNotCheckedTitle,
            Self::StorageUnavailable => MessageKey::NoticeStorageUnavailableTitle,
            Self::IndexingCouldNotStart => MessageKey::NoticePreparationCouldNotStartTitle,
            Self::ModelCouldNotBeLoaded => MessageKey::ModelLoadFailedTitle,
        };
        tr(locale, key)
    }

    pub fn body(&self, locale: Locale) -> String {
        let key = match self {
            // The two notices whose sentence names the user's folders.
            Self::FolderAlreadyIncluded { folder, parent } => {
                return crate::i18n::fmt_folder_already_included_body(locale, folder, parent);
            }
            Self::FoldersCombined { folders, parent } => {
                let names: Vec<&str> = folders.iter().map(String::as_str).collect();
                return crate::i18n::fmt_folders_combined_body(locale, &names, parent);
            }
            Self::FolderCouldNotBeAdded => MessageKey::NoticeFolderFailBody,
            Self::SearchDidNotFinish => MessageKey::NoticeSearchFailBody,
            Self::FileCouldNotBeFound => MessageKey::NoticeFileNotFoundBody,
            Self::FileCouldNotBeOpened => MessageKey::NoticeFileNotOpenedBody,
            Self::FileNotAllowed => MessageKey::NoticeFileNotAllowedBody,
            Self::FileCheckFailed => MessageKey::NoticeFileCheckFailedBody,
            Self::FolderAdded => MessageKey::NoticeFolderAddedBody,
            Self::FolderAlreadyAdded => MessageKey::NoticeFolderAlreadyAddedBody,
            Self::SearchReady => MessageKey::NoticeSearchReadyBody,
            Self::PreviewsCleared
            | Self::SearchCacheCleared
            | Self::ExtractedTextCleared
            | Self::ReplacedDataRemoved => MessageKey::NoticeCleanupBody,
            Self::DiagnosticsFileCreated => MessageKey::DiagnosticsFileCreated,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFailed,
            Self::RecentSearchesCleared => MessageKey::RecentSearchesClearedNotice,
            Self::RecentSearchFilterDropped => MessageKey::DroppedFilterNotice,
            Self::SettingCouldNotBeSaved => MessageKey::NoticeSettingSaveFailBody,
            Self::CatalogResetFailed => MessageKey::NoticeResetFailBody,
            Self::SourceCouldNotBeRemoved => MessageKey::NoticeSourceRemoveFailBody,
            Self::RecentSearchesNotCleared => MessageKey::NoticeRecentSearchesNotClearedBody,
            Self::RecentSearchNotRemoved => MessageKey::NoticeRecentSearchNotRemovedBody,
            Self::CleanupDidNotFinish => MessageKey::NoticeCleanupDidNotFinishBody,
            Self::FolderNotChecked => MessageKey::NoticeFolderNotCheckedBody,
            Self::StorageUnavailable => MessageKey::NoticeStorageUnavailableBody,
            Self::IndexingCouldNotStart => MessageKey::NoticePreparationCouldNotStartBody,
            Self::ModelCouldNotBeLoaded => MessageKey::ModelLoadFailed,
        };
        tr(locale, key).to_string()
    }

    /// The action button's label, if this kind of notice can offer one.
    /// The button renders only when the raise site also stored the concrete
    /// retry in `AppState::notice_action` (Task 060): a label with no retry
    /// behind it is not shown.
    /// Confirmations return `None` (they are dismissed, not acted upon).
    pub fn action(&self, locale: Locale) -> Option<&'static str> {
        let key = match self {
            Self::SearchDidNotFinish => MessageKey::NoticeActionTryAgain,
            Self::FolderCouldNotBeAdded => MessageKey::NoticeActionChooseFolder,
            Self::FileCouldNotBeFound => MessageKey::NoticeActionGoToFolders,
            // Rendered only when a Show-in-folder retry was stored: an Open
            // failure has one, a Reveal failure does not.
            Self::FileCouldNotBeOpened => MessageKey::NoticeActionShowInFolder,
            // Task 070: retrying cannot change a permission.
            Self::FileNotAllowed => return None,
            // The same action on the same result.
            Self::FileCheckFailed => MessageKey::NoticeActionTryAgain,
            Self::FolderAdded
            | Self::FolderAlreadyAdded
            | Self::FolderAlreadyIncluded { .. }
            | Self::FoldersCombined { .. }
            | Self::SearchReady
            | Self::PreviewsCleared
            | Self::SearchCacheCleared
            | Self::ExtractedTextCleared
            | Self::ReplacedDataRemoved
            | Self::DiagnosticsFileCreated => return None,
            Self::RecentSearchesCleared | Self::RecentSearchFilterDropped => return None,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFile,
            Self::SettingCouldNotBeSaved
            | Self::CatalogResetFailed
            | Self::SourceCouldNotBeRemoved
            | Self::RecentSearchesNotCleared
            | Self::RecentSearchNotRemoved
            | Self::CleanupDidNotFinish
            | Self::FolderNotChecked
            | Self::StorageUnavailable => MessageKey::NoticeActionTryAgain,
            // No in-app action can restart the background task; the body
            // text names the recovery step (restart orbok) as prose instead.
            Self::IndexingCouldNotStart => return None,
            Self::ModelCouldNotBeLoaded => MessageKey::ModelLoadRetry,
        };
        Some(tr(locale, key))
    }
}
