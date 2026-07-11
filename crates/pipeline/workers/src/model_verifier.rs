//! Startup model verification (design decision §3, RFC-021).
//!
//! Runs at every startup. Checks that the files required for the
//! configured embedding model are present and non-empty. Does **not**
//! run SHA-256 hash verification — that is reserved for the explicit
//! "Validate" action in the Models view (keeps startup under 5 ms).
//!
//! The two required files inside the model directory:
//! - `onnx/model.onnx`  — the weights (typically 20–140 MB)
//! - `tokenizer.json`   — the tokenizer config (~2 MB)

use std::path::Path;

/// Files that must be present in the model directory.
pub const REQUIRED_MODEL_FILES: &[&str] = &["onnx/model.onnx", "tokenizer.json"];

/// Outcome of a startup model verification check.
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyOutcome {
    /// Both required files exist and have size > 0. Semantic search
    /// can be enabled when the inference backend is loaded.
    Ready,

    /// No model directory has ever been configured.
    /// Show the setup wizard — state: "not configured".
    NotConfigured,

    /// The directory was configured but one or more required files are
    /// absent or empty.
    FilesInvalid {
        /// The configured model directory path.
        model_dir: String,
        /// Which required files failed the check.
        issues: Vec<FileIssue>,
    },
}

/// A single file that failed the verification check.
#[derive(Debug, Clone, PartialEq)]
pub struct FileIssue {
    /// Relative path within the model directory (e.g. `onnx/model.onnx`).
    pub relative_path: String,
    /// Human-readable reason.
    pub reason: FileIssueKind,
}

/// Reason a required model file failed verification.
#[derive(Debug, Clone, PartialEq)]
pub enum FileIssueKind {
    NotFound,
    Empty,
    PermissionDenied,
}

impl FileIssueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileIssueKind::NotFound => "not found",
            FileIssueKind::Empty => "empty file (0 bytes)",
            FileIssueKind::PermissionDenied => "permission denied",
        }
    }
}

/// Verify the embedding model directory at startup.
///
/// `model_dir` comes from [`OrbokSettings::embedding_model_dir`].
///
/// # Timing
/// Typical execution: < 2 ms (two `stat` calls). No SHA-256 hashing.
pub fn verify_embedding_model(model_dir: Option<&str>) -> VerifyOutcome {
    let dir_str = match model_dir {
        Some(d) if !d.trim().is_empty() => d,
        _ => return VerifyOutcome::NotConfigured,
    };
    let dir = Path::new(dir_str);
    let mut issues = Vec::new();
    for rel in REQUIRED_MODEL_FILES {
        let full = dir.join(rel);
        match std::fs::metadata(&full) {
            Ok(meta) if meta.len() == 0 => {
                issues.push(FileIssue {
                    relative_path: rel.to_string(),
                    reason: FileIssueKind::Empty,
                });
            }
            Ok(_) => {} // present and non-empty — OK
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                issues.push(FileIssue {
                    relative_path: rel.to_string(),
                    reason: FileIssueKind::PermissionDenied,
                });
            }
            Err(_) => {
                issues.push(FileIssue {
                    relative_path: rel.to_string(),
                    reason: FileIssueKind::NotFound,
                });
            }
        }
    }
    if issues.is_empty() {
        VerifyOutcome::Ready
    } else {
        VerifyOutcome::FilesInvalid {
            model_dir: dir_str.to_string(),
            issues,
        }
    }
}

/// Run the verifier and return a brief log-friendly summary string
/// (never includes file contents — NFR-014).
pub fn verify_outcome_summary(outcome: &VerifyOutcome) -> String {
    match outcome {
        VerifyOutcome::Ready => "embedding model OK".into(),
        VerifyOutcome::NotConfigured => "embedding model not configured".into(),
        VerifyOutcome::FilesInvalid { issues, .. } => {
            let problems: Vec<_> = issues.iter().map(|i| i.reason.as_str()).collect();
            format!("embedding model invalid: {}", problems.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_none_is_not_configured() {
        assert_eq!(verify_embedding_model(None), VerifyOutcome::NotConfigured);
    }

    #[test]
    fn verify_empty_string_is_not_configured() {
        assert_eq!(
            verify_embedding_model(Some("")),
            VerifyOutcome::NotConfigured
        );
        assert_eq!(
            verify_embedding_model(Some("  ")),
            VerifyOutcome::NotConfigured
        );
    }

    #[test]
    fn verify_nonexistent_dir_reports_both_files_missing() {
        let outcome = verify_embedding_model(Some("/nonexistent/orbok-models"));
        match outcome {
            VerifyOutcome::FilesInvalid { issues, .. } => {
                assert_eq!(issues.len(), 2);
                assert!(issues.iter().all(|i| i.reason == FileIssueKind::NotFound));
            }
            other => panic!("expected FilesInvalid, got {other:?}"),
        }
    }

    #[test]
    fn verify_dir_with_valid_files_returns_ready() {
        let dir = tempfile::tempdir().unwrap();
        let onnx_dir = dir.path().join("onnx");
        std::fs::create_dir_all(&onnx_dir).unwrap();
        std::fs::write(onnx_dir.join("model.onnx"), vec![0u8; 1024]).unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
        assert_eq!(
            verify_embedding_model(Some(&dir.path().to_string_lossy())),
            VerifyOutcome::Ready
        );
    }

    #[test]
    fn verify_empty_model_file_reports_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let onnx_dir = dir.path().join("onnx");
        std::fs::create_dir_all(&onnx_dir).unwrap();
        std::fs::write(onnx_dir.join("model.onnx"), b"").unwrap(); // empty!
        std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
        match verify_embedding_model(Some(&dir.path().to_string_lossy())) {
            VerifyOutcome::FilesInvalid { issues, .. } => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].relative_path, "onnx/model.onnx");
                assert_eq!(issues[0].reason, FileIssueKind::Empty);
            }
            other => panic!("expected FilesInvalid, got {other:?}"),
        }
    }

    #[test]
    fn verify_missing_tokenizer_reports_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let onnx_dir = dir.path().join("onnx");
        std::fs::create_dir_all(&onnx_dir).unwrap();
        std::fs::write(onnx_dir.join("model.onnx"), vec![1u8; 512]).unwrap();
        // tokenizer.json deliberately absent
        match verify_embedding_model(Some(&dir.path().to_string_lossy())) {
            VerifyOutcome::FilesInvalid { issues, .. } => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].relative_path, "tokenizer.json");
            }
            other => panic!("expected FilesInvalid, got {other:?}"),
        }
    }

    #[test]
    fn summary_strings_are_log_safe() {
        // Verify summary strings never include file paths (only status).
        let summary = verify_outcome_summary(&VerifyOutcome::FilesInvalid {
            model_dir: "/secret/path".into(),
            issues: vec![FileIssue {
                relative_path: "onnx/model.onnx".into(),
                reason: FileIssueKind::NotFound,
            }],
        });
        assert!(
            !summary.contains("/secret/path"),
            "summary must not include the model dir path"
        );
    }
}

// ── Deep verification (RFC-029: checksum integrity check) ────────────

/// A manifest file written alongside downloaded model files.
/// Contains the SHA-256 hash of each file at download time so the
/// explicit "Validate" action can detect corruption or tampering.
///
/// File: `{model_dir}/orbok-manifest.json`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelManifest {
    /// Map of relative file path → lowercase hex SHA-256 digest.
    pub sha256: std::collections::HashMap<String, String>,
}

impl ModelManifest {
    /// Load a manifest from the model directory. Returns `None` if the
    /// manifest file does not exist (e.g. model was placed manually).
    pub fn load(model_dir: &Path) -> Option<Self> {
        let path = model_dir.join("orbok-manifest.json");
        let bytes = std::fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Persist the manifest to the model directory.
    pub fn save(&self, model_dir: &Path) -> std::io::Result<()> {
        let path = model_dir.join("orbok-manifest.json");
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, &json)
    }

    /// Compute the SHA-256 hex digest of a file.
    pub fn sha256_of_file(path: &Path) -> std::io::Result<String> {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(path)?;
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(digest)
    }
}

/// Result of a deep (checksum) verification.
#[derive(Debug, Clone, PartialEq)]
pub enum DeepVerifyOutcome {
    /// All files present and checksums match the stored manifest.
    Valid,
    /// No manifest found — model was placed manually; cannot verify checksums.
    NoManifest,
    /// One or more checksums did not match.
    ChecksumMismatch(Vec<String>),
    /// A required file is missing (manifest exists but file is gone).
    FileMissing(Vec<String>),
}

/// Run a deep integrity check using the stored manifest (RFC-029).
/// Called only from the explicit "Validate" button in the Models view —
/// never at startup (would add 50–500 ms for large models).
pub fn verify_embedding_model_deep(model_dir: &Path) -> DeepVerifyOutcome {
    let manifest = match ModelManifest::load(model_dir) {
        Some(m) => m,
        None => return DeepVerifyOutcome::NoManifest,
    };

    let mut missing = Vec::new();
    let mut mismatched = Vec::new();

    for (rel, expected) in &manifest.sha256 {
        let full = model_dir.join(rel);
        match ModelManifest::sha256_of_file(&full) {
            Ok(actual) if actual == *expected => {} // OK
            Ok(_) => mismatched.push(rel.clone()),
            Err(_) => missing.push(rel.clone()),
        }
    }

    if !missing.is_empty() {
        DeepVerifyOutcome::FileMissing(missing)
    } else if !mismatched.is_empty() {
        DeepVerifyOutcome::ChecksumMismatch(mismatched)
    } else {
        DeepVerifyOutcome::Valid
    }
}

#[cfg(test)]
mod deep_verify_tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut sha256 = std::collections::HashMap::new();
        sha256.insert("onnx/model.onnx".into(), "abc123".into());
        let m = ModelManifest { sha256 };
        m.save(dir.path()).unwrap();
        let loaded = ModelManifest::load(dir.path()).unwrap();
        assert_eq!(loaded.sha256["onnx/model.onnx"], "abc123");
    }

    #[test]
    fn no_manifest_returns_no_manifest() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            verify_embedding_model_deep(dir.path()),
            DeepVerifyOutcome::NoManifest
        );
    }

    #[test]
    fn valid_checksums_return_valid() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"model weights";
        std::fs::write(dir.path().join("model.bin"), content).unwrap();
        let hash = ModelManifest::sha256_of_file(&dir.path().join("model.bin")).unwrap();
        let mut sha256 = std::collections::HashMap::new();
        sha256.insert("model.bin".into(), hash);
        ModelManifest { sha256 }.save(dir.path()).unwrap();
        assert_eq!(
            verify_embedding_model_deep(dir.path()),
            DeepVerifyOutcome::Valid
        );
    }

    #[test]
    fn corrupted_file_returns_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.bin"), b"original").unwrap();
        let hash = ModelManifest::sha256_of_file(&dir.path().join("model.bin")).unwrap();
        let mut sha256 = std::collections::HashMap::new();
        sha256.insert("model.bin".into(), hash);
        ModelManifest { sha256 }.save(dir.path()).unwrap();
        // Corrupt the file
        std::fs::write(dir.path().join("model.bin"), b"corrupted!").unwrap();
        assert_eq!(
            verify_embedding_model_deep(dir.path()),
            DeepVerifyOutcome::ChecksumMismatch(vec!["model.bin".into()])
        );
    }
}
