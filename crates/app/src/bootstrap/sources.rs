//! Source registration, scan/index execution, removal, and folder lookup.

use orbok_db::Catalog;

// ── Source management ─────────────────────────────────────────────────

/// Add a folder or file as a new searchable source.
/// Returns a populated `SourceCard` for immediate display in the UI.
pub fn add_source(
    catalog: &Catalog,
    raw_path: &str,
) -> Result<(orbok_ui::state::SourceCard, Option<&'static str>), Box<dyn std::error::Error>> {
    use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
    use orbok_db::repo::{NewSource, SourceRepository};
    use std::path::Path;

    let raw = raw_path.trim();
    if raw.is_empty() {
        return Err("path is empty".into());
    }
    // Resolve tilde and canonicalize.
    let expanded = if let Some(stripped) = raw.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}{stripped}")
    } else {
        raw.to_string()
    };
    let canonical = Path::new(&expanded)
        .canonicalize()
        .map_err(|e| format!("cannot access '{expanded}': {e}"))?
        .to_string_lossy()
        .to_string();

    let source_type = if Path::new(&canonical).is_dir() {
        SourceType::Directory
    } else {
        SourceType::File
    };
    let display_name = Path::new(&canonical)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string());

    let src = SourceRepository::new(catalog).insert(NewSource {
        source_type,
        persistence_mode: PersistenceMode::Persistent,
        display_name: Some(display_name.clone()),
        original_path: expanded,
        canonical_path: canonical.clone(),
        index_mode: IndexMode::Balanced,
        include_patterns: vec![],
        exclude_patterns: vec![],
        hidden_file_policy: HiddenFilePolicy::Exclude,
        symlink_policy: SymlinkPolicy::Ignore,
        max_file_size_bytes: None,
    })?;

    // RFC-003 acceptance: warn before indexing sensitive directories.
    let sensitive = orbok_fs::sensitive_warning(std::path::Path::new(&canonical));
    if let Some(w) = sensitive {
        tracing::warn!(path = %canonical, warning = w, "sensitive source added");
    }

    Ok((
        orbok_ui::state::SourceCard {
            display_name,
            display_path: canonical,
            indexed: 0,
            stale: 0,
            failed: 0,
            active: true,
            source_id: src.source_id.as_str().to_string(),
        },
        sensitive,
    ))
}

/// Enqueue a source's scan, then return promptly (RFC-056 §3, §9 criterion
/// 1 -- Review 162 §2: scanning itself is scheduled work now, not just the
/// `Extract`/`Chunk`/`Embedding` jobs a scan discovers). Execution --
/// walking the source, hashing files, and enqueuing the resulting
/// `Extract`/`Chunk`/`Embedding` jobs -- happens off this call, in the
/// `scheduler_host` background task (RFC-056 §4.1) dispatching the
/// `JobKind::ScanSource` job this enqueues. The returned `IndexHealth`
/// reflects only catalog state as of this call (typically zero newly
/// discovered/indexed files yet), not the eventual result of preparing the
/// source. The caller observes real progress via the `Message::HealthUpdated`
/// events the background task emits as jobs complete.
pub fn scan_and_index_source(
    catalog: &Catalog,
    source_id_str: &str,
) -> Result<orbok_ui::state::IndexHealth, Box<dyn std::error::Error>> {
    use orbok_core::{JobType, SourceId};
    use orbok_db::repo::{IndexJobRepository, SourceRepository};

    let source_id = SourceId::from_string(source_id_str.to_string());
    let src = SourceRepository::new(catalog)
        .get(&source_id)?
        .ok_or("source not found")?;

    IndexJobRepository::new(catalog).enqueue(JobType::Scan, Some(&src.source_id), None)?;

    Ok(super::get_health(catalog))
}

/// Remove a source and its associated indexes from the catalog.
pub fn remove_source(
    catalog: &Catalog,
    source_id_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::SourceId;
    use orbok_db::repo::SourceRepository;
    let source_id = SourceId::from_string(source_id_str.to_string());
    SourceRepository::new(catalog).delete_with_all_data(&source_id)?;
    Ok(())
}

/// Find an existing source whose canonical path matches `canonical_path`.
///
/// Used by the RFC-045 search-in-folder flow to reuse a remembered folder
/// rather than creating a duplicate source record (RFC-045 §6.1, §19.3).
/// Returns `None` when no matching source is found.
pub fn find_source_by_canonical_path(
    catalog: &Catalog,
    canonical_path: &str,
) -> Option<orbok_ui::state::SourceCard> {
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, SourceRepository};
    SourceRepository::new(catalog)
        .list()
        .unwrap_or_default()
        .into_iter()
        .find(|src| src.canonical_path == canonical_path)
        .map(|src| {
            let files = FileRepository::new(catalog);
            let indexed = files
                .count_for_source_with_status(&src.source_id, FileStatus::Indexed)
                .unwrap_or(0);
            let stale = files
                .count_for_source_with_status(&src.source_id, FileStatus::Stale)
                .unwrap_or(0);
            let failed = files
                .count_for_source_with_status(&src.source_id, FileStatus::Failed)
                .unwrap_or(0);
            let display_name = src.display_name.unwrap_or_else(|| "folder".to_string());
            orbok_ui::state::SourceCard {
                display_name,
                display_path: src.canonical_path,
                indexed,
                stale,
                failed,
                active: true,
                source_id: src.source_id.as_str().to_string(),
            }
        })
}
