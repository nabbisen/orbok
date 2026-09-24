//! Source and file lifecycle model (RFC-037 §7, §8, §11).
//!
//! This module defines the state vocabularies for registered folders
//! and their files, plus the `FileFingerprint` used for lightweight
//! change detection. Actual scanning logic lives in `scanner.rs`;
//! persistence lives in `orbok-db`.

// ── Source state ──────────────────────────────────────────────────────

/// Lifecycle state of a registered folder (RFC-037 §7).
///
/// User-facing copy (RFC-037 §6):
/// - `Active`           → "Ready"
/// - `Preparing`        → "Preparing"
/// - `NeedsUpdate`      → "Needs update"
/// - `Paused`           → "Paused"
/// - `FolderNotFound`   → "Folder not found"
/// - `PermissionProblem`→ "Cannot open"
/// - `Removed`          → "Removed"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Active,
    Preparing,
    NeedsUpdate,
    Paused,
    FolderNotFound,
    PermissionProblem,
    Removed,
}

impl SourceState {
    /// Plain-language user copy (RFC-037 §7, RFC-041 §8.1).
    pub fn user_label(self) -> &'static str {
        match self {
            SourceState::Active => "Ready",
            SourceState::Preparing => "Preparing",
            SourceState::NeedsUpdate => "Needs update",
            SourceState::Paused => "Paused",
            SourceState::FolderNotFound => "Folder not found",
            SourceState::PermissionProblem => "Cannot open",
            SourceState::Removed => "Removed",
        }
    }

    /// Whether the folder is searchable in this state.
    pub fn is_searchable(self) -> bool {
        matches!(
            self,
            SourceState::Active | SourceState::NeedsUpdate | SourceState::Preparing
        )
    }

    /// Whether manual refresh is meaningful in this state.
    pub fn can_refresh(self) -> bool {
        matches!(
            self,
            SourceState::Active
                | SourceState::NeedsUpdate
                | SourceState::FolderNotFound
                | SourceState::PermissionProblem
        )
    }

    /// Catalog string for persistence (mirrors `orbok_core::SourceStatus`
    /// but covers the extended RFC-037 vocabulary).
    pub fn as_str(self) -> &'static str {
        match self {
            SourceState::Active => "active",
            SourceState::Preparing => "preparing",
            SourceState::NeedsUpdate => "needs_update",
            SourceState::Paused => "paused",
            SourceState::FolderNotFound => "missing",
            SourceState::PermissionProblem => "permission_denied",
            SourceState::Removed => "removed",
        }
    }
}

// ── File state ────────────────────────────────────────────────────────

/// Lifecycle state of one file inside a registered folder (RFC-037 §8).
///
/// User-facing copy (RFC-037 §8 table):
/// - `Discovered`      → "Waiting"
/// - `Preparing`       → "Preparing"
/// - `Ready`           → "Ready"
/// - `NeedsUpdate`     → "Needs update"
/// - `PartlyPrepared`  → "Partly prepared"
/// - `CouldNotPrepare` → "Could not prepare"
/// - `FileNotFound`    → "File not found"
/// - `Ignored`         → "Skipped"
/// - `NoTextFound`      → "No text found" (Task 080; owner-approved copy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Discovered,
    Preparing,
    Ready,
    NeedsUpdate,
    PartlyPrepared,
    CouldNotPrepare,
    FileNotFound,
    Ignored,
    NoTextFound,
}

impl FileState {
    /// Plain-language user copy (RFC-037 §8).
    pub fn user_label(self) -> &'static str {
        match self {
            FileState::Discovered => "Waiting",
            FileState::Preparing => "Preparing",
            FileState::Ready => "Ready",
            FileState::NeedsUpdate => "Needs update",
            FileState::PartlyPrepared => "Partly prepared",
            FileState::CouldNotPrepare => "Could not prepare",
            FileState::FileNotFound => "File not found",
            FileState::Ignored => "Skipped",
            FileState::NoTextFound => "No text found",
        }
    }

    /// Map from the catalog `files.file_status` string (RFC-004 §7) to
    /// this richer RFC-037 vocabulary.
    pub fn from_catalog_status(s: &str) -> Self {
        match s {
            "discovered" => FileState::Discovered,
            "indexed" => FileState::Ready,
            "no_text_found" => FileState::NoTextFound,
            "stale" => FileState::NeedsUpdate,
            "missing" | "deleted" => FileState::FileNotFound,
            "permission_denied" => FileState::CouldNotPrepare,
            "unsupported" => FileState::Ignored,
            _ => FileState::Discovered,
        }
    }
}

// ── File fingerprint ──────────────────────────────────────────────────

/// Lightweight file identity for change detection (RFC-037 §11.1).
///
/// Metadata check is cheap and runs on every startup scan.
/// Content hash is used only when needed (RFC-037 §11.3).
#[derive(Debug, Clone, PartialEq)]
pub struct FileFingerprint {
    pub size_bytes: u64,
    pub modified_at: Option<String>,
    pub content_hash: Option<String>,
}

impl FileFingerprint {
    /// Build from filesystem metadata.
    pub fn from_metadata(meta: &std::fs::Metadata) -> Self {
        use std::time::UNIX_EPOCH;
        let modified_at = meta.modified().ok().and_then(|t| {
            t.duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| format!("{}", d.as_secs()))
        });
        Self {
            size_bytes: meta.len(),
            modified_at,
            content_hash: None,
        }
    }

    /// Cheap metadata comparison — does not read file contents.
    pub fn metadata_changed(&self, other: &FileFingerprint) -> bool {
        self.size_bytes != other.size_bytes || self.modified_at != other.modified_at
    }
}

// ── Source check result ───────────────────────────────────────────────

/// Outcome of a startup or manual source check (RFC-037 §10).
#[derive(Debug, Clone)]
pub struct SourceCheckResult {
    pub source_state: SourceState,
    pub files_changed: u64,
    pub files_missing: u64,
    pub files_new: u64,
}

/// Check whether a registered folder is still accessible and derive
/// its state (RFC-037 §10.1 startup check).
///
/// This is a lightweight check — no file contents are read.
pub fn check_source_path(path: &std::path::Path) -> SourceState {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => SourceState::Active,
        Ok(_) => SourceState::Active, // Could be a single-file source
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            SourceState::PermissionProblem
        }
        Err(_) => SourceState::FolderNotFound,
    }
}
