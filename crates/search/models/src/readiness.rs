//! Local model file readiness check (RFC-043 §7).
//!
//! orbok checks local model files before starting any download. This
//! avoids re-downloading files that are already present and valid,
//! and prevents treating partially downloaded files as usable.
//!
//! User-supplied folders receive the intentionally lightweight RFC-043 check.
//! App-managed folders additionally require the exact sizes and SHA-256 values
//! from RFC-050 Appendix B. Provenance and local integrity remain separate.

use crate::trust::{
    DEFAULT_TRUSTED_MODEL, TrustRootBinding, TrustedModelFile, TrustedModelManifest,
    trust_root_binding,
};
use sha2::{Digest, Sha256};
use std::io::Read as _;
use std::path::Path;

// ── Per-file status ───────────────────────────────────────────────────

/// Status of one required model file (RFC-043 §7.2).
///
/// User-facing copy must not expose these names directly:
/// - `Ready`       → "Ready"
/// - `Missing`     → "Needed"
/// - `Partial`     → "Needs to finish"
/// - `Invalid`     → "Needs to be replaced"
/// - `CannotCheck` → "Could not check"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFileStatus {
    /// File exists, is non-empty, and passes available validation.
    Ready,
    /// File does not exist.
    Missing,
    /// A `.part` temporary file exists but the final file is absent or empty.
    Partial,
    /// File exists but failed validation (empty, wrong type, corrupted).
    Invalid,
    /// File access failed unexpectedly (permissions etc.).
    CannotCheck,
}

/// Whether local bytes were authenticated against the reviewed trust root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFileIntegrity {
    /// Exact size and SHA-256 match reviewed source-controlled metadata.
    TrustedDigest,
    /// A user-supplied file passed only the lightweight usability check.
    Unverified,
    /// No complete usable file was available for integrity checking.
    NotAvailable,
    /// Complete bytes were present but did not match trusted metadata.
    Mismatch,
}

/// Origin boundary for a readiness report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvenance {
    /// Files in orbok's managed store, governed by Appendix B.
    AppManaged,
    /// A folder selected by the user and never mutated by trusted delivery.
    UserSupplied,
}

impl LocalFileStatus {
    /// User-facing copy (RFC-043 §7.2, avoiding technical terms).
    pub fn user_label(&self) -> &'static str {
        match self {
            LocalFileStatus::Ready => "Ready",
            LocalFileStatus::Missing => "Needed",
            LocalFileStatus::Partial => "Needs to finish",
            LocalFileStatus::Invalid => "Needs to be replaced",
            LocalFileStatus::CannotCheck => "Could not check",
        }
    }

    /// Whether this file needs any download or repair work.
    pub fn needs_work(&self) -> bool {
        !matches!(self, LocalFileStatus::Ready)
    }
}

// ── Per-file readiness entry ──────────────────────────────────────────

/// Readiness information for one required model file (RFC-043 §10.2).
#[derive(Debug, Clone)]
pub struct FileReadiness {
    /// Logical name (e.g. "tokenizer", "model").
    logical_name: &'static str,
    /// Relative path within the model directory.
    relative_path: &'static str,
    /// Current status of the local file.
    status: LocalFileStatus,
    /// Byte-integrity claim, kept separate from file usability/provenance.
    integrity: LocalFileIntegrity,
}

impl FileReadiness {
    pub fn logical_name(&self) -> &'static str {
        self.logical_name
    }

    pub fn relative_path(&self) -> &'static str {
        self.relative_path
    }

    pub fn status(&self) -> LocalFileStatus {
        self.status
    }

    pub fn integrity(&self) -> LocalFileIntegrity {
        self.integrity
    }
}

// ── Model-level readiness ─────────────────────────────────────────────

/// Overall model readiness (RFC-043 §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelReadiness {
    /// Every required file is present and valid.
    Ready,
    /// At least one file is missing.
    NeedsDownload,
    /// At least one file is partial or invalid (but none missing).
    NeedsRepair,
    /// File access failed unexpectedly.
    CannotCheck,
}

// ── Readiness report ──────────────────────────────────────────────────

/// Full readiness report for a model directory (RFC-043 §7.2–7.3).
#[derive(Debug, Clone)]
pub struct ModelReadinessReport {
    provenance: ModelProvenance,
    overall: ModelReadiness,
    files: Vec<FileReadiness>,
    trust_root: Option<TrustRootBinding>,
}

impl ModelReadinessReport {
    pub fn provenance(&self) -> ModelProvenance {
        self.provenance
    }

    pub fn overall(&self) -> ModelReadiness {
        self.overall
    }

    pub fn files(&self) -> &[FileReadiness] {
        &self.files
    }

    /// Files that require download or repair.
    pub fn files_needing_work(&self) -> Vec<&FileReadiness> {
        self.files
            .iter()
            .filter(|f| f.status.needs_work())
            .collect()
    }

    /// How many files are already ready.
    pub fn ready_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.status == LocalFileStatus::Ready)
            .count()
    }

    pub fn total_count(&self) -> usize {
        self.files.len()
    }

    pub(crate) fn matches_trust_root(&self, binding: TrustRootBinding) -> bool {
        self.trust_root == Some(binding)
    }

    #[cfg(test)]
    pub(crate) fn set_file_state_for_test(
        &mut self,
        relative_path: &str,
        status: LocalFileStatus,
        integrity: LocalFileIntegrity,
    ) {
        let file = self
            .files
            .iter_mut()
            .find(|file| file.relative_path == relative_path)
            .expect("test file must exist");
        file.status = status;
        file.integrity = integrity;
        self.overall = derive_overall_readiness(&self.files);
    }

    #[cfg(test)]
    pub(crate) fn remove_file_for_test(&mut self, relative_path: &str) {
        self.files
            .retain(|file| file.relative_path != relative_path);
        self.overall = derive_overall_readiness(&self.files);
    }
}

// ── Readiness check ───────────────────────────────────────────────────

/// Check local model files and return a readiness report (RFC-043 §7).
///
/// Called: at startup, before the wizard, before download, after
/// download, after the user chooses an existing folder, and on retry.
///
/// This is a user-supplied-folder check: it performs no network access and
/// makes no app-verified integrity claim.
pub fn check_model_readiness(model_dir: &Path) -> ModelReadinessReport {
    check_model_readiness_inner(
        model_dir,
        &DEFAULT_TRUSTED_MODEL,
        ModelProvenance::UserSupplied,
    )
}

/// Explicit name for the user-supplied-folder readiness boundary.
pub fn check_user_supplied_model_readiness(model_dir: &Path) -> ModelReadinessReport {
    check_model_readiness(model_dir)
}

/// Check orbok-managed files against the normative Appendix B trust root.
///
/// Alternate-manifest injection is deliberately absent from the public API:
///
/// ```compile_fail
/// use orbok_models::check_app_managed_model_readiness_against;
/// ```
pub fn check_app_managed_model_readiness(model_dir: &Path) -> ModelReadinessReport {
    check_model_readiness_inner(
        model_dir,
        &DEFAULT_TRUSTED_MODEL,
        ModelProvenance::AppManaged,
    )
}

/// Test-only manifest injection. It is absent from normal production builds
/// and cannot mint trusted provenance for another crate.
#[cfg(test)]
pub(crate) fn check_app_managed_model_readiness_against(
    model_dir: &Path,
    manifest: &'static TrustedModelManifest,
) -> ModelReadinessReport {
    check_model_readiness_inner(model_dir, manifest, ModelProvenance::AppManaged)
}

fn check_model_readiness_inner(
    model_dir: &Path,
    manifest: &'static TrustedModelManifest,
    provenance: ModelProvenance,
) -> ModelReadinessReport {
    let mut files = Vec::new();

    for trusted_file in manifest.files {
        let full_path = model_dir.join(trusted_file.relative_path);
        let part_path = model_dir.join(format!("{}.part", trusted_file.relative_path));
        let (status, integrity) =
            check_single_file(&full_path, &part_path, trusted_file, provenance);
        files.push(FileReadiness {
            logical_name: trusted_file.logical_name,
            relative_path: trusted_file.relative_path,
            status,
            integrity,
        });
    }

    let overall = derive_overall_readiness(&files);
    ModelReadinessReport {
        provenance,
        overall,
        files,
        trust_root: (provenance == ModelProvenance::AppManaged)
            .then(|| trust_root_binding(manifest)),
    }
}

fn check_single_file(
    path: &Path,
    part_path: &Path,
    trusted_file: &TrustedModelFile,
    provenance: ModelProvenance,
) -> (LocalFileStatus, LocalFileIntegrity) {
    // Check the final path first.
    match std::fs::metadata(path) {
        Ok(meta) => {
            if !meta.is_file() {
                return (LocalFileStatus::Invalid, LocalFileIntegrity::Mismatch);
            }
            if meta.len() == 0 {
                return (LocalFileStatus::Invalid, LocalFileIntegrity::Mismatch);
            }
            if provenance == ModelProvenance::UserSupplied {
                return (LocalFileStatus::Ready, LocalFileIntegrity::Unverified);
            }
            if meta.len() != trusted_file.exact_size_bytes {
                return (LocalFileStatus::Invalid, LocalFileIntegrity::Mismatch);
            }
            match sha256_file(path) {
                Ok(digest) if digest == trusted_file.sha256 => {
                    (LocalFileStatus::Ready, LocalFileIntegrity::TrustedDigest)
                }
                Ok(_) => (LocalFileStatus::Invalid, LocalFileIntegrity::Mismatch),
                Err(_) => (
                    LocalFileStatus::CannotCheck,
                    LocalFileIntegrity::NotAvailable,
                ),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Final file missing — check for a .part file.
            if part_path.exists() {
                (LocalFileStatus::Partial, LocalFileIntegrity::NotAvailable)
            } else {
                (LocalFileStatus::Missing, LocalFileIntegrity::NotAvailable)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => (
            LocalFileStatus::CannotCheck,
            LocalFileIntegrity::NotAvailable,
        ),
        Err(_) => (
            LocalFileStatus::CannotCheck,
            LocalFileIntegrity::NotAvailable,
        ),
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

fn derive_overall_readiness(files: &[FileReadiness]) -> ModelReadiness {
    if files
        .iter()
        .any(|f| f.status == LocalFileStatus::CannotCheck)
    {
        return ModelReadiness::CannotCheck;
    }
    if files.iter().all(|f| f.status == LocalFileStatus::Ready) {
        return ModelReadiness::Ready;
    }
    if files.iter().any(|f| f.status == LocalFileStatus::Missing) {
        return ModelReadiness::NeedsDownload;
    }
    // Partial or Invalid but no Missing.
    ModelReadiness::NeedsRepair
}
