//! Download plan: maps readiness → actions (RFC-043 §10).
//!
//! A `DownloadPlan` tells the download worker exactly which files to
//! fetch, which to skip, and what concurrency limit to apply. It is
//! always derived from a fresh `ModelReadinessReport`; nothing is
//! downloaded without a plan.

use crate::readiness::{
    LocalFileIntegrity, LocalFileStatus, ModelProvenance, ModelReadinessReport,
};
use crate::trust::{DEFAULT_TRUSTED_MODEL, TrustedModelManifest, trust_root_binding};
use std::path::PathBuf;

// ── Constants ─────────────────────────────────────────────────────────

/// Maximum concurrent file downloads (RFC-043 §11.1).
pub const DEFAULT_MODEL_DOWNLOAD_CONCURRENCY: usize = 2;
pub const MAX_MODEL_DOWNLOAD_CONCURRENCY: usize = 2;

// ── Download action ───────────────────────────────────────────────────

/// What to do with one model file (RFC-043 §10.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadAction {
    /// File is already valid — skip.
    Skip,
    /// File is missing — download fresh.
    Download,
    /// File is invalid — replace (delete and re-download).
    Replace,
    /// File is partial — restart the download.
    Retry,
}

impl DownloadAction {
    /// Whether this action requires network activity.
    pub fn requires_download(&self) -> bool {
        !matches!(self, DownloadAction::Skip)
    }
}

// ── Per-file plan ─────────────────────────────────────────────────────

/// Plan for one required model file (RFC-043 §10.2).
#[derive(Debug, Clone)]
pub struct ModelFilePlan {
    pub logical_name: &'static str,
    pub relative_path: &'static str,
    pub remote_url: &'static str,
    pub expected_sha256: &'static str,
    pub exact_size_bytes: u64,
    pub max_transfer_bytes: u64,
    pub local_status: LocalFileStatus,
    pub action: DownloadAction,
    /// Temporary path used during download (RFC-043 §9.1).
    pub temp_path_suffix: &'static str,
}

impl ModelFilePlan {
    /// Full path for the temporary `.part` file.
    pub fn temp_path(&self, model_dir: &std::path::Path) -> PathBuf {
        model_dir.join(format!("{}.part", self.relative_path))
    }

    /// Full final path for the completed validated file.
    pub fn final_path(&self, model_dir: &std::path::Path) -> PathBuf {
        model_dir.join(self.relative_path)
    }
}

// ── Download plan ─────────────────────────────────────────────────────

/// Complete download plan for a model (RFC-043 §10.1).
#[derive(Debug, Clone)]
pub struct DownloadPlan {
    /// Source-controlled trust-root identity used to construct this plan.
    pub manifest_id: &'static str,
    /// Maximum concurrent file downloads (RFC-043 §11).
    pub max_concurrent: usize,
    pub files: Vec<ModelFilePlan>,
}

impl DownloadPlan {
    /// Files that actually require download work.
    pub fn files_to_download(&self) -> Vec<&ModelFilePlan> {
        self.files
            .iter()
            .filter(|f| f.action.requires_download())
            .collect()
    }

    /// Files that will be skipped.
    pub fn files_to_skip(&self) -> Vec<&ModelFilePlan> {
        self.files
            .iter()
            .filter(|f| f.action == DownloadAction::Skip)
            .collect()
    }

    /// Whether anything actually needs downloading.
    pub fn has_work(&self) -> bool {
        self.files.iter().any(|f| f.action.requires_download())
    }
}

// ── Plan builder ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadPlanError {
    UserSuppliedFolder,
    TrustRootMismatch,
    ReadinessManifestMismatch,
    IncoherentReadiness,
    CannotCheckFiles,
    InvalidTrustedManifest,
}

/// Build a download plan from a fresh readiness report (RFC-043 §10.4).
///
/// Called after every app-managed readiness check. The plan is always derived
/// from current local state plus the reviewed manifest; user-supplied folders
/// are rejected.
pub fn build_download_plan(
    report: &ModelReadinessReport,
) -> Result<DownloadPlan, DownloadPlanError> {
    build_download_plan_for_manifest(report, &DEFAULT_TRUSTED_MODEL)
}

/// Test-only manifest injection. It is absent from normal production builds.
#[cfg(test)]
pub(crate) fn build_download_plan_against(
    report: &ModelReadinessReport,
    manifest: &'static TrustedModelManifest,
) -> Result<DownloadPlan, DownloadPlanError> {
    build_download_plan_for_manifest(report, manifest)
}

fn build_download_plan_for_manifest(
    report: &ModelReadinessReport,
    manifest: &'static TrustedModelManifest,
) -> Result<DownloadPlan, DownloadPlanError> {
    manifest
        .validate()
        .map_err(|_| DownloadPlanError::InvalidTrustedManifest)?;
    if report.provenance() != ModelProvenance::AppManaged {
        return Err(DownloadPlanError::UserSuppliedFolder);
    }
    if !report.matches_trust_root(trust_root_binding(manifest)) {
        return Err(DownloadPlanError::TrustRootMismatch);
    }
    if report.files().len() != manifest.files.len()
        || report.files().iter().any(|readiness| {
            manifest
                .file_by_path(readiness.relative_path())
                .is_none_or(|trusted| trusted.logical_name != readiness.logical_name())
        })
        || manifest.files.iter().any(|trusted| {
            report
                .files()
                .iter()
                .filter(|readiness| {
                    readiness.relative_path() == trusted.relative_path
                        && readiness.logical_name() == trusted.logical_name
                })
                .count()
                != 1
        })
    {
        return Err(DownloadPlanError::ReadinessManifestMismatch);
    }

    let files = report
        .files()
        .iter()
        .map(|readiness| -> Result<ModelFilePlan, DownloadPlanError> {
            let trusted = manifest
                .file_by_path(readiness.relative_path())
                .expect("readiness file set was validated against the manifest");
            let action = plan_action(readiness.status(), readiness.integrity())?;
            Ok(ModelFilePlan {
                logical_name: trusted.logical_name,
                relative_path: trusted.relative_path,
                remote_url: trusted.url,
                expected_sha256: trusted.sha256,
                exact_size_bytes: trusted.exact_size_bytes,
                max_transfer_bytes: trusted.max_transfer_bytes,
                local_status: readiness.status(),
                action,
                temp_path_suffix: ".part",
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DownloadPlan {
        manifest_id: manifest.manifest_id,
        max_concurrent: DEFAULT_MODEL_DOWNLOAD_CONCURRENCY,
        files,
    })
}

fn plan_action(
    status: LocalFileStatus,
    integrity: LocalFileIntegrity,
) -> Result<DownloadAction, DownloadPlanError> {
    match (status, integrity) {
        (LocalFileStatus::Ready, LocalFileIntegrity::TrustedDigest) => Ok(DownloadAction::Skip),
        (LocalFileStatus::Missing, LocalFileIntegrity::NotAvailable) => {
            Ok(DownloadAction::Download)
        }
        (LocalFileStatus::Partial, LocalFileIntegrity::NotAvailable) => Ok(DownloadAction::Retry),
        (LocalFileStatus::Invalid, LocalFileIntegrity::Mismatch) => Ok(DownloadAction::Replace),
        (LocalFileStatus::CannotCheck, _) => Err(DownloadPlanError::CannotCheckFiles),
        _ => Err(DownloadPlanError::IncoherentReadiness),
    }
}

// ── Download progress types ───────────────────────────────────────────

/// Per-file download status (RFC-043 §13.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDownloadStatus {
    Pending,
    Checking,
    Downloading,
    Validating,
    Complete,
    Failed,
    Skipped,
}

/// Per-file download progress (RFC-043 §13.1).
#[derive(Debug, Clone)]
pub struct FileDownloadProgress {
    pub relative_path: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub status: FileDownloadStatus,
}

/// Overall combined progress (RFC-043 §13.2).
#[derive(Debug, Clone)]
pub enum OverallDownloadProgress {
    Known {
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Step {
        completed_files: usize,
        total_files: usize,
    },
    Indeterminate,
}

/// Friendly download problem — safe to show to users (RFC-043 §16.4).
#[derive(Debug, Clone)]
pub enum FriendlyDownloadProblem {
    NetworkUnavailable,
    ServerBusy,
    NotEnoughSpace,
    CannotWriteFiles,
    CannotCheckFiles,
    ValidationFailed,
    Unexpected,
}

impl FriendlyDownloadProblem {
    /// User-facing copy (RFC-043 §20, avoiding technical terms).
    pub fn user_message(&self) -> &'static str {
        match self {
            FriendlyDownloadProblem::NetworkUnavailable => {
                "Download did not finish. Please check your connection and try again."
            }
            FriendlyDownloadProblem::ServerBusy => {
                "The download is taking longer than expected. Please try again later."
            }
            FriendlyDownloadProblem::NotEnoughSpace => {
                "More space is needed to finish the download."
            }
            FriendlyDownloadProblem::CannotWriteFiles => {
                "orbok could not save the search helper files here. Please choose another location or check folder permissions."
            }
            FriendlyDownloadProblem::CannotCheckFiles => {
                "orbok could not check the search helper files. Please choose the folder again."
            }
            FriendlyDownloadProblem::ValidationFailed => {
                "Some downloaded files could not be used. orbok can download them again."
            }
            FriendlyDownloadProblem::Unexpected => "Download did not finish. Please try again.",
        }
    }
}
