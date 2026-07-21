//! Typed presentation boundary for RFC-050 Phase 4 model consent.
//!
//! This module contains display data and state only. It cannot start a
//! transfer; the application adapter may do that only after receiving the
//! explicit confirmation message while this consent state is active.

use orbok_models::trust::DEFAULT_TRUSTED_MODEL;

/// The trust statement shown beside a model selected by the app or the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTrustPresentation {
    /// The offered bytes are pinned and must be verified before activation.
    AppWillVerify,
    /// Bytes must match the reviewed, source-controlled RFC-050 trust root.
    AppVerified,
    /// The folder passed usability checks, but its provenance was not verified.
    UserSupplied,
}

/// Immutable facts that must be presented before the default-model download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDownloadConsent {
    pub provider: &'static str,
    pub source: &'static str,
    pub model_name: &'static str,
    pub immutable_revision: &'static str,
    pub exact_size_bytes: u64,
    pub license: &'static str,
    pub destination: String,
    pub trust: ModelTrustPresentation,
}

impl ModelDownloadConsent {
    /// Build presentation data exclusively from the reviewed Appendix B root.
    pub fn trusted_default(destination: String) -> Self {
        let exact_size_bytes = DEFAULT_TRUSTED_MODEL
            .files
            .iter()
            .try_fold(0_u64, |total, file| {
                total.checked_add(file.exact_size_bytes)
            })
            .expect("reviewed default-model file sizes must fit in u64");

        Self {
            provider: "Hugging Face",
            source: DEFAULT_TRUSTED_MODEL.model.id,
            model_name: DEFAULT_TRUSTED_MODEL.model.display_name,
            immutable_revision: DEFAULT_TRUSTED_MODEL.model.revision,
            exact_size_bytes,
            license: DEFAULT_TRUSTED_MODEL.model.license,
            destination,
            trust: ModelTrustPresentation::AppWillVerify,
        }
    }
}
