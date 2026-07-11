//! Result trust model (RFC-038 §5, §7, §12).
//!
//! Every search result carries a `ResultTrustState` computed from the
//! file's catalog state, its source state, and any extraction warnings.
//! The UI layer maps trust states to plain-language badges and recovery
//! actions; raw variant names must not appear in default user-facing copy.

use orbok_extract::ExtractWarning;

// ── Trust state ───────────────────────────────────────────────────────

/// Result trust level (RFC-038 §5).
///
/// UI copy (RFC-038 §14):
/// - `Ready`            → no badge shown
/// - `NeedsUpdate`      → "Needs update"
/// - `FileNotFound`     → "File not found"
/// - `StillBeingPrepared` → "Still being prepared"
/// - `PartlyPrepared`   → "Partly prepared"
/// - `CannotOpen`       → "Cannot open"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTrustState {
    Ready,
    NeedsUpdate,
    FileNotFound,
    StillBeingPrepared,
    PartlyPrepared,
    CannotOpen,
}

impl ResultTrustState {
    /// Whether a trust badge should be shown in the default (non-advanced) UI.
    /// `Ready` results stay clean — no badge (RFC-038 §6.1).
    pub fn show_badge_by_default(self) -> bool {
        !matches!(self, ResultTrustState::Ready)
    }
}

// ── Warning summary (de-duplicated from ExtractWarning) ───────────────

/// Simplified warning summary for the UI layer (RFC-038 §12).
///
/// The full `ExtractWarning` set from RFC-044 maps here; the UI only
/// shows the important summary by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultWarningSummary {
    SomePagesUnreadable,
    PossiblyScannedPdf,
    SizeLimitReached,
    UnsupportedDocumentPart,
    ApproximateLocation,
}

impl ResultWarningSummary {
    /// Map from an `ExtractWarning` to its UI summary (RFC-038 §7).
    pub fn from_extract_warning(w: &ExtractWarning) -> Option<Self> {
        match w {
            ExtractWarning::SomePagesUnreadable { .. } => Some(Self::SomePagesUnreadable),
            ExtractWarning::PossiblyScannedPdf => Some(Self::PossiblyScannedPdf),
            ExtractWarning::SizeLimitReached { .. } => Some(Self::SizeLimitReached),
            ExtractWarning::UnsupportedDocumentPart { .. } => Some(Self::UnsupportedDocumentPart),
            ExtractWarning::ApproximateLocationOnly => Some(Self::ApproximateLocation),
            // Encoding and malformed issues already surface as CannotOpen/PartlyPrepared
            // through the file state; no separate summary needed.
            ExtractWarning::EncodingUnsupported
            | ExtractWarning::MalformedContentRecovered
            | ExtractWarning::SomeContentSkipped { .. } => None,
        }
    }
}

// ── Recovery actions ──────────────────────────────────────────────────

/// Recovery action offered on a non-ready result (RFC-038 §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRecoveryAction {
    /// Queue this file for re-preparation.
    PrepareAgain,
    /// Check whether the source folder is still available.
    CheckFolder,
    /// Remove this result from the visible list.
    RemoveFromResults,
    /// Open the file even though it may be outdated.
    OpenAnyway,
    /// Open the containing folder in the OS file manager.
    ShowInFolder,
    /// Show extraction details in Advanced view.
    ViewDetails,
}

// ── Full trust bundle ─────────────────────────────────────────────────

/// The complete trust description for one search result (RFC-038 §12).
#[derive(Debug, Clone)]
pub struct SearchResultTrust {
    pub state: ResultTrustState,
    pub warnings: Vec<ResultWarningSummary>,
    pub recovery_actions: Vec<ResultRecoveryAction>,
}

impl SearchResultTrust {
    /// A fully-trusted, clean result with no badge or actions.
    pub fn ready() -> Self {
        Self {
            state: ResultTrustState::Ready,
            warnings: Vec::new(),
            recovery_actions: Vec::new(),
        }
    }

    /// Derive trust from catalog file status string and extraction warnings.
    ///
    /// `file_status` is the `files.file_status` catalog string
    /// (RFC-004 §7 vocabulary); `extract_warnings` comes from
    /// `ExtractOutput.warnings` (RFC-044 §10).
    pub fn from_catalog(file_status: &str, extract_warnings: &[ExtractWarning]) -> Self {
        let (state, mut actions) = trust_from_file_status(file_status, extract_warnings);
        let warnings: Vec<ResultWarningSummary> = extract_warnings
            .iter()
            .filter_map(ResultWarningSummary::from_extract_warning)
            .collect();

        // PartlyPrepared always offers ViewDetails in advanced mode.
        if state == ResultTrustState::PartlyPrepared
            && !warnings.is_empty()
            && !actions.contains(&ResultRecoveryAction::ViewDetails)
        {
            actions.push(ResultRecoveryAction::ViewDetails);
        }

        Self {
            state,
            warnings,
            recovery_actions: actions,
        }
    }
}

/// Derive trust state and recovery actions from the catalog file status.
fn trust_from_file_status(
    file_status: &str,
    extract_warnings: &[ExtractWarning],
) -> (ResultTrustState, Vec<ResultRecoveryAction>) {
    match file_status {
        "indexed" => {
            // May be degraded by extraction warnings.
            let partly = extract_warnings.iter().any(|w| {
                matches!(
                    w,
                    ExtractWarning::SomePagesUnreadable { .. }
                        | ExtractWarning::PossiblyScannedPdf
                        | ExtractWarning::SizeLimitReached { .. }
                        | ExtractWarning::UnsupportedDocumentPart { .. }
                        | ExtractWarning::MalformedContentRecovered
                )
            });
            if partly {
                (
                    ResultTrustState::PartlyPrepared,
                    vec![ResultRecoveryAction::OpenAnyway],
                )
            } else {
                (ResultTrustState::Ready, Vec::new())
            }
        }
        "stale" => (
            ResultTrustState::NeedsUpdate,
            vec![
                ResultRecoveryAction::PrepareAgain,
                ResultRecoveryAction::OpenAnyway,
            ],
        ),
        "missing" | "deleted" => (
            ResultTrustState::FileNotFound,
            vec![
                ResultRecoveryAction::CheckFolder,
                ResultRecoveryAction::RemoveFromResults,
            ],
        ),
        "discovered" => (ResultTrustState::StillBeingPrepared, Vec::new()),
        "permission_denied" => (
            ResultTrustState::CannotOpen,
            vec![ResultRecoveryAction::ShowInFolder],
        ),
        "failed" => (
            ResultTrustState::PartlyPrepared,
            vec![
                ResultRecoveryAction::PrepareAgain,
                ResultRecoveryAction::ViewDetails,
            ],
        ),
        _ => (ResultTrustState::Ready, Vec::new()),
    }
}
