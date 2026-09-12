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
    DownloadDidNotFinish,
    FolderCouldNotBeAdded,
    SearchDidNotFinish,
    FilesMovedOrMissing,
    /// The added folder may contain sensitive files (SSH keys, browser profiles, etc.).
    SensitiveSourceAdded,
    // ── Confirmations ─────────────────────────────────────────────────
    FolderAdded,
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
    /// The model store or cache handle could not be opened for an
    /// in-session action (download, clear previews, clear search data,
    /// full reset) -- was a panic (`.expect(...)`) before RFC-061 §8(d).
    StorageUnavailable,
    /// The background preparation task could not start at all (runtime
    /// context, catalog, or cache open failure) -- orbok looks healthy
    /// but never prepares anything, for the rest of the session.
    IndexingCouldNotStart,
}

impl UserNotice {
    /// Whether this notice reports a problem (vs. a success confirmation).
    /// The view can use this to choose tone, but never relies on colour alone.
    pub fn is_problem(&self) -> bool {
        matches!(
            self,
            Self::SensitiveSourceAdded
                | Self::DownloadDidNotFinish
                | Self::FolderCouldNotBeAdded
                | Self::SearchDidNotFinish
                | Self::FilesMovedOrMissing
                | Self::DiagnosticsFileFailed
                | Self::SettingCouldNotBeSaved
                | Self::CatalogResetFailed
                | Self::SourceCouldNotBeRemoved
                | Self::StorageUnavailable
                | Self::IndexingCouldNotStart
        )
    }

    /// Map this notice to a Snora Design tone. Problem notices use Danger or
    /// Warning; confirmations use Success or Info. The tone drives the
    /// WCAG-AA-verified colors in `snora::design::notice::Notice`.
    pub fn tone(&self) -> snora::design::Tone {
        use snora::design::Tone;
        match self {
            // Hard failures the user must notice.
            Self::DownloadDidNotFinish
            | Self::FolderCouldNotBeAdded
            | Self::SearchDidNotFinish
            | Self::DiagnosticsFileFailed
            | Self::SettingCouldNotBeSaved
            | Self::CatalogResetFailed
            | Self::SourceCouldNotBeRemoved
            | Self::StorageUnavailable
            | Self::IndexingCouldNotStart => Tone::Danger,
            // Cautions: action succeeded but the user should be aware.
            Self::FilesMovedOrMissing | Self::SensitiveSourceAdded => Tone::Warning,
            // Positive confirmations.
            Self::FolderAdded | Self::SearchReady => Tone::Success,
            // Neutral/informational.
            Self::PreviewsCleared
            | Self::SearchCacheCleared
            | Self::ExtractedTextCleared
            | Self::ReplacedDataRemoved
            | Self::DiagnosticsFileCreated => Tone::Info,
            Self::RecentSearchesCleared | Self::RecentSearchFilterDropped => Tone::Info,
        }
    }

    pub fn title(&self, locale: Locale) -> &'static str {
        let key = match self {
            Self::DownloadDidNotFinish => MessageKey::NoticeDownloadFailTitle,
            Self::FolderCouldNotBeAdded => MessageKey::NoticeFolderFailTitle,
            Self::SearchDidNotFinish => MessageKey::NoticeSearchFailTitle,
            Self::FilesMovedOrMissing => MessageKey::NoticeFilesMissingTitle,
            Self::SensitiveSourceAdded => MessageKey::NoticeSensitiveSourceTitle,
            Self::FolderAdded => MessageKey::NoticeFolderAddedTitle,
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
            Self::StorageUnavailable => MessageKey::NoticeStorageUnavailableTitle,
            Self::IndexingCouldNotStart => MessageKey::NoticePreparationCouldNotStartTitle,
        };
        tr(locale, key)
    }

    pub fn body(&self, locale: Locale) -> &'static str {
        let key = match self {
            Self::DownloadDidNotFinish => MessageKey::NoticeDownloadFailBody,
            Self::FolderCouldNotBeAdded => MessageKey::NoticeFolderFailBody,
            Self::SearchDidNotFinish => MessageKey::NoticeSearchFailBody,
            Self::FilesMovedOrMissing => MessageKey::NoticeFilesMissingBody,
            Self::SensitiveSourceAdded => MessageKey::NoticeSensitiveSourceBody,
            Self::FolderAdded => MessageKey::NoticeFolderAddedBody,
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
            Self::StorageUnavailable => MessageKey::NoticeStorageUnavailableBody,
            Self::IndexingCouldNotStart => MessageKey::NoticePreparationCouldNotStartBody,
        };
        tr(locale, key)
    }

    /// Suggested next-action label, if the notice offers a recovery action.
    /// Confirmations return `None` (they are dismissed, not acted upon).
    pub fn action(&self, locale: Locale) -> Option<&'static str> {
        let key = match self {
            Self::DownloadDidNotFinish | Self::SearchDidNotFinish => {
                MessageKey::NoticeActionTryAgain
            }
            Self::FolderCouldNotBeAdded => MessageKey::NoticeActionChooseFolder,
            Self::FilesMovedOrMissing => MessageKey::NoticeActionChooseFolder,
            Self::SensitiveSourceAdded => return None, // informational only
            Self::FolderAdded
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
            | Self::StorageUnavailable => MessageKey::NoticeActionTryAgain,
            // No in-app action can restart the background task; the body
            // text names the recovery step (restart orbok) as prose instead.
            Self::IndexingCouldNotStart => return None,
        };
        Some(tr(locale, key))
    }
}
