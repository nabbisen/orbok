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
    DiagnosticsFileCreated,
    // ── RFC-040: diagnostics problem ──────────────────────────────────
    DiagnosticsFileFailed,
    // ── RFC-042: search history ───────────────────────────────────────
    /// Confirmation: recent searches were cleared.
    RecentSearchesCleared,
    /// Info: a narrowing choice was dropped on reopen (folder gone).
    RecentSearchFilterDropped,
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
            | Self::DiagnosticsFileFailed => Tone::Danger,
            // Cautions: action succeeded but the user should be aware.
            Self::FilesMovedOrMissing | Self::SensitiveSourceAdded => Tone::Warning,
            // Positive confirmations.
            Self::FolderAdded | Self::SearchReady => Tone::Success,
            // Neutral/informational.
            Self::PreviewsCleared | Self::DiagnosticsFileCreated => Tone::Info,
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
            Self::DiagnosticsFileCreated => MessageKey::DiagnosticsFileCreated,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFailed,
            Self::RecentSearchesCleared => MessageKey::RecentSearchesClearedNotice,
            Self::RecentSearchFilterDropped => MessageKey::DroppedFilterNotice,
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
            Self::PreviewsCleared => MessageKey::NoticePreviewsClearedBody,
            Self::DiagnosticsFileCreated => MessageKey::DiagnosticsFileCreated,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFailed,
            Self::RecentSearchesCleared => MessageKey::RecentSearchesClearedNotice,
            Self::RecentSearchFilterDropped => MessageKey::DroppedFilterNotice,
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
            | Self::DiagnosticsFileCreated => return None,
            Self::RecentSearchesCleared | Self::RecentSearchFilterDropped => return None,
            Self::DiagnosticsFileFailed => MessageKey::DiagnosticsCreateFile,
        };
        Some(tr(locale, key))
    }
}
