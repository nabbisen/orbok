//! Source registration, scan/index execution, removal, and folder lookup.

use orbok_core::{OrbokError, OrbokResult};
use orbok_db::Catalog;

// ── Source management ─────────────────────────────────────────────────

/// What [`add_source`] did with a path.
#[derive(Debug)]
pub enum AddSourceOutcome {
    Added {
        card: orbok_ui::state::SourceCard,
        sensitive: Option<&'static str>,
    },
    /// The canonical path is already registered (RFC-045 §19.3, Task 047):
    /// nothing was inserted, scanned or warned about. `card` is the existing
    /// source.
    AlreadyRegistered { card: orbok_ui::state::SourceCard },
}

/// A leading `~` is the user's home directory.
pub(crate) fn expand_home(raw: &str, home: &str) -> String {
    match raw.strip_prefix('~') {
        Some(rest) => format!("{home}{rest}"),
        None => raw.to_string(),
    }
}

/// Task 110: whether adding `raw_path` should first ask "add a folder that
/// may contain private files?" -- it is a folder orbok would newly register
/// and `orbok_fs::sensitive_warning` flags its location. A path that cannot
/// be resolved, is not a folder, or is already registered is **not** asked
/// about: the first two fail in `add_source` with the ordinary notice, and
/// the third adds nothing. Reads only; nothing is saved.
pub fn needs_private_folder_question(catalog: &Catalog, raw_path: &str) -> bool {
    use orbok_db::repo::SourceRepository;
    use std::path::Path;
    let expanded = expand_home(raw_path.trim(), &std::env::var("HOME").unwrap_or_default());
    let Ok(canonical) = Path::new(&expanded).canonicalize() else {
        return false;
    };
    if !canonical.is_dir() {
        return false;
    }
    let canonical = canonical.to_string_lossy().to_string();
    if matches!(
        SourceRepository::new(catalog).find_by_canonical_path(&canonical),
        Ok(Some(_))
    ) {
        return false;
    }
    orbok_fs::sensitive_warning(Path::new(&canonical)).is_some()
}

/// Add a folder as a new searchable source, unless its canonical
/// path is already registered.
pub fn add_source(catalog: &Catalog, raw_path: &str) -> OrbokResult<AddSourceOutcome> {
    use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
    use orbok_db::repo::{NewSource, SourceRepository};
    use std::path::Path;

    let raw = raw_path.trim();
    if raw.is_empty() {
        // RFC-061 Slice 2: no dedicated "invalid input" variant exists in
        // `OrbokError` and adding one for this single, narrow validation
        // message is not this slice's job (it is about threading existing
        // structured errors through, not growing the taxonomy). `PathCanonicalization`
        // is the closest existing bucket -- an empty path is a path that
        // cannot be resolved, the same family as the `canonicalize()`
        // failure two lines below, just caught earlier.
        return Err(OrbokError::PathCanonicalization("path is empty".into()));
    }
    let expanded = expand_home(raw, &std::env::var("HOME").unwrap_or_default());
    let canonical = Path::new(&expanded)
        .canonicalize()
        .map_err(|e| OrbokError::PathCanonicalization(format!("cannot access '{expanded}': {e}")))?
        .to_string_lossy()
        .to_string();

    if let Some(existing) = SourceRepository::new(catalog).find_by_canonical_path(&canonical)? {
        return Ok(AddSourceOutcome::AlreadyRegistered {
            card: source_card(catalog, existing),
        });
    }

    // Task 105: only a folder can be added. Single files were dropped (Task
    // 109, RFC-003 Amendment 1); the picker offers folders only, which hid a
    // registration of a file until a typed path could reach this.
    if !Path::new(&canonical).is_dir() {
        return Err(OrbokError::PathCanonicalization(format!(
            "'{canonical}' is not a folder"
        )));
    }
    let source_type = SourceType::Directory;
    let display_name = folder_display_name(&canonical);

    let src = SourceRepository::new(catalog).insert(NewSource {
        source_type,
        persistence_mode: PersistenceMode::Persistent,
        display_name: Some(display_name),
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

    Ok(AddSourceOutcome::Added {
        card: source_card(catalog, src),
        sensitive,
    })
}

/// Test call sites that need a fresh source: panics if the path was already
/// registered, so no test silently accepts either outcome.
#[cfg(test)]
pub(crate) fn add_source_expect_added(
    catalog: &Catalog,
    raw_path: &str,
) -> OrbokResult<(orbok_ui::state::SourceCard, Option<&'static str>)> {
    match add_source(catalog, raw_path)? {
        AddSourceOutcome::Added { card, sensitive } => Ok((card, sensitive)),
        AddSourceOutcome::AlreadyRegistered { card } => panic!(
            "expected a new source, but {} is already registered as {}",
            card.display_path, card.source_id
        ),
    }
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
) -> OrbokResult<orbok_ui::state::IndexHealth> {
    use orbok_core::{JobType, SourceId};
    use orbok_db::repo::{IndexJobRepository, SourceRepository};

    let source_id = SourceId::from_string(source_id_str.to_string());
    let src = SourceRepository::new(catalog)
        .get(&source_id)?
        .ok_or(OrbokError::SourceNotFound)?;

    IndexJobRepository::new(catalog).enqueue(JobType::Scan, Some(&src.source_id), None)?;

    Ok(super::get_health(catalog))
}

/// Check a registered folder's path and, if reachable, enqueue a scan
/// (RFC-037 §10.1 startup check / §10.2 manual refresh — Task 035 §4: the
/// two are the same operation, invoked either once per source at startup or
/// once by explicit user action). `orbok_fs::source_lifecycle::check_source_path`
/// is a lightweight `stat()`-only check (RFC-037 §10.1's "check permission
/// lightly"), never a directory walk — that stays inside `Scanner::scan`,
/// unchanged.
///
/// `SourceState`'s richer RFC-037 vocabulary (`Preparing`, `NeedsUpdate`)
/// cannot round-trip through `sources.status`: the catalog's CHECK
/// constraint (`crates/data/db/migrations/0001_baseline.sql`) only allows
/// `active`/`paused`/`missing`/`permission_denied`/`removed`, matching
/// `orbok_core::SourceStatus` exactly. `check_source_path` only ever
/// returns `Active`/`FolderNotFound`/`PermissionProblem`, so this stays
/// within that constraint without needing a schema change; `Preparing`/
/// `NeedsUpdate` remain UI-derived (from job/file counts), never persisted.
///
/// Enqueuing, not scanning inline: this returns promptly, the same as
/// `scan_and_index_source` below (RFC-056 §3) — a `Scan` job goes in the
/// hosted scheduler's queue and the existing resource policy (RFC-057)
/// decides when it runs. No second execution path.
pub fn check_and_refresh_source(
    catalog: &Catalog,
    source_id_str: &str,
) -> OrbokResult<orbok_ui::state::IndexHealth> {
    use orbok_core::{SourceId, SourceStatus};
    use orbok_db::repo::SourceRepository;
    use orbok_fs::source_lifecycle::{SourceState, check_source_path};
    use std::path::Path;

    let source_id = SourceId::from_string(source_id_str.to_string());
    let repo = SourceRepository::new(catalog);
    let src = repo.get(&source_id)?.ok_or(OrbokError::SourceNotFound)?;

    match check_source_path(Path::new(&src.canonical_path)) {
        SourceState::Active => {
            repo.set_status(&source_id, SourceStatus::Active)?;
            scan_and_index_source(catalog, source_id_str)
        }
        SourceState::FolderNotFound => {
            repo.set_status(&source_id, SourceStatus::Missing)?;
            Ok(super::get_health(catalog))
        }
        SourceState::PermissionProblem => {
            repo.set_status(&source_id, SourceStatus::PermissionDenied)?;
            Ok(super::get_health(catalog))
        }
        // check_source_path never returns the remaining SourceState
        // variants (Preparing/NeedsUpdate/Paused/Removed) -- they describe
        // states this function does not derive from a filesystem check.
        other => unreachable!("check_source_path returned an unexpected state: {other:?}"),
    }
}

/// Remove a source and its associated indexes from the catalog.
pub fn remove_source(catalog: &Catalog, source_id_str: &str) -> OrbokResult<()> {
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
    use orbok_db::repo::SourceRepository;
    SourceRepository::new(catalog)
        .find_by_canonical_path(canonical_path)
        .ok()
        .flatten()
        .map(|src| source_card(catalog, src))
}

/// A folder's name as the user sees it when the catalog holds none: the last
/// component of its path, never a word of ours. A path with no last component
/// (a filesystem root) is shown whole.
pub(super) fn folder_display_name(canonical_path: &str) -> String {
    std::path::Path::new(canonical_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| canonical_path.to_string())
}

/// The one place a folder card is built (Task 108): the folder's own status
/// and its counts, read together, so every caller shows the same card for the
/// same record. A count that cannot be read shows as 0 beside a card that
/// exists; that is not a claim about which folders the catalog holds, which
/// the caller's own read of the folder list guards (Task 075).
pub(super) fn source_card(
    catalog: &Catalog,
    src: orbok_db::repo::SourceRecord,
) -> orbok_ui::state::SourceCard {
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, IndexJobRepository};
    let files = FileRepository::new(catalog);
    let count = |status| {
        files
            .count_for_source_with_status(&src.source_id, status)
            .unwrap_or(0)
    };
    let unfinished_jobs = IndexJobRepository::new(catalog)
        .count_unfinished_for_source(&src.source_id)
        .unwrap_or(0);
    let display_name = src
        .display_name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| folder_display_name(&src.canonical_path));
    orbok_ui::state::SourceCard {
        display_name,
        indexed: count(FileStatus::Indexed),
        stale: count(FileStatus::Stale),
        failed: count(FileStatus::Failed),
        no_text_found: count(FileStatus::NoTextFound),
        unfinished_jobs,
        status: src.status,
        source_id: src.source_id.as_str().to_string(),
        display_path: src.canonical_path,
    }
}
