//! Backend file-access boundary (RFC-003 §8).
//!
//! Before any backend code reads a file it must obtain a
//! [`ValidatedPath`] from a [`PathGuard`]. Validation performs, in
//! order: canonicalization, active-source membership, symlink-escape
//! detection, hidden-file policy, and size limit. Requests for paths
//! outside every active source fail with
//! [`OrbokError::PathOutsideSources`] — the guard never trusts
//! caller-provided paths (frontend or otherwise).

use crate::policy::CompiledPolicy;
use orbok_core::{HiddenFilePolicy, OrbokError, OrbokResult, SourceId, SymlinkPolicy};
use orbok_db::repo::SourceRecord;
use std::path::{Path, PathBuf};

/// One active source root with its compiled policy.
#[derive(Debug, Clone)]
pub struct GuardedSource {
    pub source_id: SourceId,
    pub canonical_root: PathBuf,
    pub policy: CompiledPolicy,
}

impl GuardedSource {
    pub fn from_record(record: &SourceRecord) -> Self {
        Self {
            source_id: record.source_id.clone(),
            canonical_root: PathBuf::from(&record.canonical_path),
            policy: CompiledPolicy::from_source(record),
        }
    }
}

/// A path that passed every boundary check. Only this type may be
/// handed to file readers.
#[derive(Debug, Clone)]
pub struct ValidatedPath {
    pub source_id: SourceId,
    pub canonical: PathBuf,
}

/// The access boundary over the currently active sources.
pub struct PathGuard {
    sources: Vec<GuardedSource>,
}

impl PathGuard {
    /// Build a guard over active sources only (paused/missing/removed
    /// sources grant no access).
    pub fn new(sources: Vec<GuardedSource>) -> Self {
        Self { sources }
    }

    /// Canonicalize a path the platform-aware way (RFC-003 §11):
    /// resolves symlinks, `..`, and case differences where the platform
    /// does.
    pub fn canonicalize(path: &Path) -> OrbokResult<PathBuf> {
        std::fs::canonicalize(path)
            .map_err(|e| OrbokError::PathCanonicalization(format!("{}: {e}", path.display())))
    }

    /// RFC-003 §8 validation sequence. `requested` may be any path; the
    /// canonical form decides membership, so symlinks escaping a source
    /// are rejected regardless of how the request was spelled.
    pub fn validate(&self, requested: &Path) -> OrbokResult<ValidatedPath> {
        let canonical = Self::canonicalize(requested)?;

        let source = self
            .sources
            .iter()
            .find(|s| canonical.starts_with(&s.canonical_root))
            .ok_or(OrbokError::PathOutsideSources)?;

        // Symlink policy: when the request path itself differs from its
        // canonical form below the root, a link was traversed.
        if source.policy.symlink_policy == SymlinkPolicy::Ignore {
            let requested_inside = requested.starts_with(&source.canonical_root);
            if requested_inside && requested != canonical {
                // A symlink inside the source resolved elsewhere (still
                // inside, or membership above would have failed) — the
                // Ignore policy does not follow it.
                if is_symlinked_below(&source.canonical_root, requested)? {
                    return Err(OrbokError::PolicyBlocked("symlink_policy_blocked"));
                }
            }
        }

        // Hidden-file policy applies to components below the root.
        if source.policy.hidden_file_policy == HiddenFilePolicy::Exclude
            && hidden_below_root(&source.canonical_root, &canonical)
        {
            return Err(OrbokError::PolicyBlocked("hidden_file_excluded"));
        }

        // Size limit for files.
        if let Ok(metadata) = std::fs::metadata(&canonical)
            && metadata.is_file()
            && !source.policy.size_allowed(metadata.len())
        {
            return Err(OrbokError::PolicyBlocked("file_too_large"));
        }

        Ok(ValidatedPath {
            source_id: source.source_id.clone(),
            canonical,
        })
    }
}

/// True when any component strictly below `root` is hidden (dotted).
fn hidden_below_root(root: &Path, canonical: &Path) -> bool {
    let Ok(relative) = canonical.strip_prefix(root) else {
        return false;
    };
    relative
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

/// True when any component of `path` strictly below `root` is a symlink.
fn is_symlinked_below(root: &Path, path: &Path) -> OrbokResult<bool> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(false);
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Ok(true);
        }
    }
    Ok(false)
}
