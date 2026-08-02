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
        // canonical form, a link was traversed somewhere along it.
        // `requested != canonical` is a sound fast path — canonicalization
        // resolves symlinks, so equal spellings mean none were crossed —
        // but it cannot be gated on `requested.starts_with(root)`: on
        // platforms where the source root itself sits behind a symlink
        // (e.g. macOS's `/var` -> `/private/var`), a `requested` spelled
        // via the non-canonical root would never satisfy that prefix test,
        // silently skipping the whole check. `is_symlinked_below` locates
        // the root by canonical identity instead, so no spelling of a
        // request that resolves inside the source is missed.
        if source.policy.symlink_policy == SymlinkPolicy::Ignore
            && requested != canonical
            && is_symlinked_below(&source.canonical_root, requested)?
        {
            return Err(OrbokError::PolicyBlocked("symlink_policy_blocked"));
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
///
/// Walks `path` in its own spelling rather than `root`'s: `path` may be
/// spelled differently from the canonical `root` (a non-canonical source
/// root, a differently-spelled ancestor, or both), and canonicalizing
/// `path` up front would resolve away the very links this is looking for.
/// The root is located by canonical identity as the walk proceeds, not by
/// string prefix, so it is found under any spelling.
fn is_symlinked_below(root: &Path, path: &Path) -> OrbokResult<bool> {
    let mut current = PathBuf::new();
    let mut inside_root = false;
    for component in path.components() {
        current.push(component);
        if !inside_root {
            if std::fs::canonicalize(&current).is_ok_and(|c| c.as_path() == root) {
                inside_root = true;
            }
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Ok(true);
        }
    }
    Ok(false)
}
