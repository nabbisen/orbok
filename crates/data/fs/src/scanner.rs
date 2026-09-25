//! File scanner and change detection (RFC-004).
//!
//! The scanner is the authority for source traversal and file catalog
//! state (Appendix A §13: localcache freshness checks happen later, in
//! workers, never here). Per-file failures never abort the scan
//! (RFC-004 §16); cancellation leaves the catalog valid because every
//! file is committed individually.

use crate::hashing::sha256_file;
use crate::policy::{
    CompiledPolicy, FileTypeClass, classify_file_type, is_generated_folder, platform_hidden,
};
use orbok_core::{FileStatus, JobType, OrbokResult, SourceId, system_time_iso8601};
use orbok_db::Catalog;
use orbok_db::repo::{
    ChunkRepository, FileRepository, IndexJobRepository, NewFile, ObservedMetadata, SourceRecord,
    SourceRepository,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Scan request (RFC-004 §20).
#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub source_id: SourceId,
    /// Hash even when the metadata fast-check says unchanged.
    pub force_hash: bool,
    /// Queue `extract` jobs for new/stale files (RFC-004 §13).
    pub enqueue_index_jobs: bool,
}

/// Per-file classification produced during a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcomeKind {
    New,
    Unchanged,
    Stale,
    Unsupported,
    PermissionDenied,
    Failed,
}

/// Scan summary counts (RFC-004 §14).
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub seen_files: u64,
    pub new_files: u64,
    pub unchanged_files: u64,
    pub stale_files: u64,
    pub missing_files: u64,
    pub unsupported_files: u64,
    pub permission_denied_files: u64,
    pub failed_files: u64,
    pub queued_index_jobs: u64,
    /// Task 120: files erased because the folder they were in is now skipped.
    /// They are out of the folder, not missing.
    pub erased_files: u64,
    /// The erased files' paths, for the caller to evict from the extraction
    /// cache, which is outside the catalog.
    pub erased_paths: Vec<String>,
    pub duration_ms: u64,
    pub canceled: bool,
}

/// The scanner. Holds the catalog handle; one scan call per source.
pub struct Scanner<'a> {
    catalog: &'a Catalog,
}

impl<'a> Scanner<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Scan one active source (RFC-004 §10). `cancel` may be flipped by
    /// the UI at any time; the scan stops at the next file boundary.
    pub fn scan(&self, request: &ScanRequest, cancel: &AtomicBool) -> OrbokResult<ScanSummary> {
        let started = Instant::now();
        let mut summary = ScanSummary::default();

        let sources = SourceRepository::new(self.catalog);
        let source = sources
            .get(&request.source_id)?
            .ok_or(orbok_core::OrbokError::SourceNotFound)?;
        let policy = CompiledPolicy::from_source(&source);
        let root = PathBuf::from(&source.canonical_path);
        // Task 116: this scan's number. Every file it sees records it, and the
        // files that end without it are the ones it did not see.
        let generation = sources.begin_scan(&source.source_id)?;

        let files = FileRepository::new(self.catalog);
        let jobs = IndexJobRepository::new(self.catalog);
        let chunks = ChunkRepository::new(self.catalog);

        let mut stack = vec![root.clone()];
        'walk: while let Some(dir) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                summary.canceled = true;
                break 'walk;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    summary.permission_denied_files += 1;
                    continue;
                }
                Err(_) => {
                    summary.failed_files += 1;
                    continue;
                }
            };
            for entry in entries {
                if cancel.load(Ordering::Relaxed) {
                    summary.canceled = true;
                    break 'walk;
                }
                let Ok(entry) = entry else {
                    summary.failed_files += 1;
                    continue;
                };
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();

                let Ok(file_type) = entry.file_type() else {
                    summary.failed_files += 1;
                    continue;
                };
                if skip_component(&policy, &source, &name, &entry) {
                    if file_type.is_dir() {
                        self.erase_skipped(&source, &path, &mut summary);
                    }
                    continue;
                }
                if file_type.is_symlink() {
                    // RFC-003 §6.2: v1 default Ignore; FollowWithinSource
                    // resolves and verifies containment.
                    if !symlink_allowed(&policy, &root, &path) {
                        continue;
                    }
                }
                if path.is_dir() {
                    // Task 120: a folder a tool generated has nothing prepared,
                    // and anything an earlier scan prepared there is erased.
                    if is_generated_folder(&path) {
                        self.erase_skipped(&source, &path, &mut summary);
                        continue;
                    }
                    // Task 114: "this folder only" reads the folder's direct
                    // entries and does not descend.
                    if source.covers_subfolders {
                        stack.push(path);
                    }
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                summary.seen_files += 1;
                let outcome = self.process_file(
                    &source,
                    &policy,
                    &files,
                    &jobs,
                    &chunks,
                    &path,
                    request,
                    &mut summary,
                );
                match outcome {
                    Ok(()) => {}
                    Err(_) => summary.failed_files += 1,
                }
            }
        }

        if !summary.canceled {
            summary.missing_files = files.mark_missing_unseen(&source.source_id, generation)?;
            // RFC-037 §8/§12, Task 035 §5.3: a file's own file_status
            // flipping to missing has no effect, by itself, on whether its
            // content still surfaces in search -- see
            // `ChunkRepository::deactivate_for_missing_files`'s own doc
            // comment for why.
            chunks.deactivate_for_missing_files(&source.source_id)?;
            sources.touch_scanned(&source.source_id)?;
        }
        summary.duration_ms = started.elapsed().as_millis() as u64;
        tracing::info!(
            source = source.source_id.as_str(),
            seen = summary.seen_files,
            new = summary.new_files,
            stale = summary.stale_files,
            missing = summary.missing_files,
            "scan finished"
        );
        Ok(summary)
    }

    /// Task 120: a folder this scan skips holds nothing prepared. Rows an
    /// earlier scan wrote under it (before it was skipped: `AppData`, a Cargo
    /// `target`) are erased -- with their chunks and keyword-index rows, as a
    /// narrowed folder's files are -- and not marked missing: the files are
    /// still on the disk, orbok just does not look at them. Failing to erase
    /// affects this folder only and is counted like any per-entry failure.
    fn erase_skipped(&self, source: &SourceRecord, dir: &Path, summary: &mut ScanSummary) {
        // "This folder only" never descends, so it holds nothing below.
        if !source.covers_subfolders {
            return;
        }
        match SourceRepository::new(self.catalog)
            .erase_files_under(&source.source_id, &dir.to_string_lossy())
        {
            Ok(paths) => {
                summary.erased_files += paths.len() as u64;
                summary.erased_paths.extend(paths);
            }
            Err(error) => {
                tracing::warn!(%error, "could not erase what a skipped folder held");
                summary.failed_files += 1;
            }
        }
    }

    /// Catalog one regular file. Failure here affects this file only
    /// (RFC-004 §16).
    #[allow(clippy::too_many_arguments)]
    fn process_file(
        &self,
        source: &SourceRecord,
        policy: &CompiledPolicy,
        files: &FileRepository<'_>,
        jobs: &IndexJobRepository<'_>,
        chunks: &ChunkRepository<'_>,
        path: &Path,
        request: &ScanRequest,
        summary: &mut ScanSummary,
    ) -> OrbokResult<()> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !policy.file_included(&file_name) {
            return Ok(());
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                summary.permission_denied_files += 1;
                self.upsert_status_only(source, files, path, FileStatus::PermissionDenied)?;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        if !policy.size_allowed(metadata.len()) {
            return Ok(()); // over limit: skipped, not cataloged (RFC-004 §10)
        }

        let supported = classify_file_type(path) == FileTypeClass::Supported;
        let canonical = path.to_string_lossy().into_owned();
        let observed = ObservedMetadata {
            file_size_bytes: metadata.len(),
            modified_at: metadata.modified().ok().map(system_time_iso8601),
            platform_file_key: platform_file_key(&metadata),
            content_hash: None,
        };

        let existing = files.get_by_path(&source.source_id, &canonical)?;
        match existing {
            None => {
                // New file: hash if supported (content identity required
                // before "indexed", RFC-004 §9.2).
                let mut observed = observed;
                let status = if supported {
                    observed.content_hash = Some(sha256_file(path)?);
                    FileStatus::Discovered
                } else {
                    summary.unsupported_files += 1;
                    FileStatus::Unsupported
                };
                let new_file = NewFile {
                    source_id: source.source_id.clone(),
                    original_path: canonical.clone(),
                    canonical_path: canonical.clone(),
                    display_path: display_path(&source.canonical_path, &canonical),
                    extension: path
                        .extension()
                        .map(|e| e.to_string_lossy().to_ascii_lowercase()),
                    metadata: observed,
                    status,
                };
                // Task 114: a file below the top level is written only while
                // the folder still covers its subfolders -- decided by the
                // insert itself, not by the copy of the folder this scan read
                // when it started, which narrowing may since have changed.
                let Some(record) = (if is_below_top_level(source, path) {
                    files.insert_below_top_level(new_file)?
                } else {
                    Some(files.insert(new_file)?)
                }) else {
                    return Ok(());
                };
                if supported {
                    summary.new_files += 1;
                    if request.enqueue_index_jobs {
                        jobs.enqueue(
                            JobType::Extract,
                            Some(&source.source_id),
                            Some(&record.file_id),
                        )?;
                        summary.queued_index_jobs += 1;
                    }
                }
            }
            Some(record) => {
                // RFC-004 §11: a missing file that reappears with the
                // same content returns to its previous state (Indexed if
                // it ever was, otherwise Discovered).
                let restored_status = (record.file_status == FileStatus::Missing).then(|| {
                    if record.last_indexed_at.is_some() {
                        FileStatus::Indexed
                    } else {
                        FileStatus::Discovered
                    }
                });
                // Fast check (RFC-004 §9.1). `modified_at` strings are
                // RFC 3339 with nanosecond precision where the
                // filesystem provides it — same-second overwrites with
                // unchanged size are still detected (the defect class
                // fixed in localcache 0.20.0). On coarse-timestamp
                // filesystems, `force_hash` remains the escape hatch.
                let metadata_unchanged = record.file_size_bytes == observed.file_size_bytes
                    && record.modified_at == observed.modified_at;
                if metadata_unchanged && !request.force_hash {
                    match restored_status {
                        Some(status) => {
                            files.update_observed(&record.file_id, &observed, status)?;
                            if status == FileStatus::Indexed {
                                chunks.reactivate_last_stale_generation(&record.file_id)?;
                            }
                        }
                        None => files.touch_seen(&record.file_id)?,
                    }
                    summary.unchanged_files += 1;
                    return Ok(());
                }
                // Metadata changed (or forced): confirm with hash.
                let mut observed = observed;
                let new_hash = sha256_file(path)?;
                if record.content_hash.as_deref() == Some(new_hash.as_str()) {
                    match restored_status {
                        Some(status) => {
                            files.update_observed(&record.file_id, &observed, status)?;
                            if status == FileStatus::Indexed {
                                chunks.reactivate_last_stale_generation(&record.file_id)?;
                            }
                        }
                        None => files.touch_seen(&record.file_id)?,
                    }
                    summary.unchanged_files += 1;
                    return Ok(());
                }
                observed.content_hash = Some(new_hash);
                let status = match record.file_status {
                    FileStatus::Indexed | FileStatus::Stale => FileStatus::Stale,
                    _ => FileStatus::Discovered,
                };
                files.update_observed(&record.file_id, &observed, status)?;
                summary.stale_files += 1;
                if request.enqueue_index_jobs {
                    jobs.enqueue(
                        JobType::Extract,
                        Some(&source.source_id),
                        Some(&record.file_id),
                    )?;
                    summary.queued_index_jobs += 1;
                }
            }
        }
        Ok(())
    }

    fn upsert_status_only(
        &self,
        source: &SourceRecord,
        files: &FileRepository<'_>,
        path: &Path,
        status: FileStatus,
    ) -> OrbokResult<()> {
        let canonical = path.to_string_lossy().into_owned();
        match files.get_by_path(&source.source_id, &canonical)? {
            Some(record) => files.set_status_seen(&record.file_id, status),
            None => {
                let new_file = NewFile {
                    source_id: source.source_id.clone(),
                    original_path: canonical.clone(),
                    canonical_path: canonical.clone(),
                    display_path: display_path(&source.canonical_path, &canonical),
                    extension: None,
                    metadata: ObservedMetadata::default(),
                    status,
                };
                if is_below_top_level(source, path) {
                    files.insert_below_top_level(new_file).map(|_| ())
                } else {
                    files.insert(new_file).map(|_| ())
                }
            }
        }
    }
}

/// Whether `path` is not a direct entry of the folder (Task 114).
fn is_below_top_level(source: &SourceRecord, path: &Path) -> bool {
    path.parent() != Some(Path::new(&source.canonical_path))
}

/// Hidden/excluded component skipping for directory descent and files.
///
/// Task 120: "hidden" is what the platform means -- a name starting with `.`
/// everywhere, and on Windows the Hidden or System attribute, on macOS the
/// `UF_HIDDEN` flag. The folder the user added is never asked (it is not an
/// entry of itself); this rule applies to what is inside it.
fn skip_component(
    policy: &CompiledPolicy,
    source: &SourceRecord,
    name: &str,
    entry: &std::fs::DirEntry,
) -> bool {
    if policy.component_excluded(name) {
        return true;
    }
    source.hidden_file_policy == orbok_core::HiddenFilePolicy::Exclude
        && (CompiledPolicy::component_hidden(name) || platform_hidden(entry))
}

/// Symlink admission per policy (RFC-003 §12.2): resolved target must
/// stay inside the source root for FollowWithinSource; Ignore admits
/// nothing.
fn symlink_allowed(policy: &CompiledPolicy, root: &Path, path: &Path) -> bool {
    match policy.symlink_policy {
        orbok_core::SymlinkPolicy::Ignore => false,
        orbok_core::SymlinkPolicy::FollowWithinSource
        | orbok_core::SymlinkPolicy::FollowAllWithWarning => match std::fs::canonicalize(path) {
            Ok(resolved) => resolved.starts_with(root),
            Err(_) => false,
        },
    }
}

/// UI display path: relative to the source root where possible.
fn display_path(root: &str, canonical: &str) -> String {
    canonical
        .strip_prefix(root)
        .map(|rest| rest.trim_start_matches(['/', '\\']).to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| canonical.to_string())
}

/// Unix: device+inode identity (RFC-004 §9.3).
#[cfg(unix)]
fn platform_file_key(metadata: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn platform_file_key(_metadata: &std::fs::Metadata) -> Option<String> {
    None
}
