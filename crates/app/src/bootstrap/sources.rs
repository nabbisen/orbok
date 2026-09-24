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
        /// Task 113: added folders that were inside this one and are now
        /// part of it (empty for an ordinary add).
        combined: Vec<orbok_ui::state::CombinedFolder>,
    },
    /// Task 113: `folder` (the chosen folder's own name) lies inside
    /// `parent`, an added folder, so nothing was registered (RFC-064 §3.3).
    AlreadyIncluded {
        folder: String,
        parent: orbok_ui::state::SourceCard,
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
    // Already registered, or inside a registered folder (Task 113): nothing
    // would be added, so there is nothing to ask about.
    if let Ok(sources) = SourceRepository::new(catalog).list()
        && covering_of(&sources, &canonical).is_some()
    {
        return false;
    }
    let canonical = canonical.to_string_lossy().to_string();
    orbok_fs::sensitive_warning(Path::new(&canonical)).is_some()
}

/// Add a folder as a new searchable source, unless its canonical path is
/// already registered ([`AddSourceOutcome::AlreadyRegistered`]) or an added
/// folder covers it ([`AddSourceOutcome::AlreadyIncluded`]). Added folders
/// inside the new one become part of it (Task 113, RFC-064 §3.3): every file
/// belongs to one folder.
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

    let repo = SourceRepository::new(catalog);
    if let Some(existing) = repo.find_by_canonical_path(&canonical)? {
        return Ok(AddSourceOutcome::AlreadyRegistered {
            card: source_card(catalog, existing),
        });
    }
    // Task 113: a folder an added folder covers is never registered again
    // (RFC-064 §3.3). The same folder in another spelling of its case (Windows,
    // macOS) is the same folder.
    let existing = repo.list()?;
    if let Some((covering, rest)) = covering_of(&existing, Path::new(&canonical)) {
        let card = source_card(catalog, covering.clone());
        return Ok(if rest.as_os_str().is_empty() {
            AddSourceOutcome::AlreadyRegistered { card }
        } else {
            AddSourceOutcome::AlreadyIncluded {
                folder: folder_display_name(&canonical),
                parent: card,
            }
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

    // Task 113: added folders inside this one become part of it, in one
    // transaction, with what was prepared for their files. If that fails the
    // new (still empty) folder is not left behind: the add did not happen.
    let inside = folders_inside(&existing, Path::new(&canonical));
    let combined = if inside.is_empty() {
        Vec::new()
    } else {
        let ids: Vec<orbok_core::SourceId> = inside.iter().map(|s| s.source_id.clone()).collect();
        if let Err(e) = repo.absorb(&src.source_id, &ids) {
            if let Err(cleanup) = repo.delete_with_all_data(&src.source_id) {
                tracing::error!("could not undo the new folder after a failed combine: {cleanup}");
            }
            return Err(e);
        }
        inside.iter().map(|s| combined_folder(s)).collect()
    };

    // RFC-003 acceptance: warn before indexing sensitive directories.
    let sensitive = orbok_fs::sensitive_warning(std::path::Path::new(&canonical));
    if let Some(w) = sensitive {
        tracing::warn!(path = %canonical, warning = w, "sensitive source added");
    }

    Ok(AddSourceOutcome::Added {
        card: source_card(catalog, src),
        sensitive,
        combined,
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
        AddSourceOutcome::Added {
            card, sensitive, ..
        } => Ok((card, sensitive)),
        AddSourceOutcome::AlreadyRegistered { card }
        | AddSourceOutcome::AlreadyIncluded { parent: card, .. } => panic!(
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

/// Task 113: an added folder that covers a path chosen to search in.
#[derive(Debug, PartialEq, Eq)]
pub struct CoveringSource {
    pub card: orbok_ui::state::SourceCard,
    /// What the search chip calls the chosen folder: its own name, which for
    /// a subfolder is not the covering folder's.
    pub location_name: String,
    /// The chosen folder's canonical path, in the covering folder's own
    /// spelling, when it lies **inside** it; `None` when it is the covering
    /// folder itself.
    pub limit_path: Option<String>,
}

/// Task 113: the added folder that covers `raw_path` (the folder itself, or
/// one above it), for the search-in-folder flow. `None` when no added folder
/// covers it, or the path cannot be resolved (`add_source` then reports it).
pub fn covering_source(catalog: &Catalog, raw_path: &str) -> Option<CoveringSource> {
    use orbok_db::repo::SourceRepository;
    use std::path::Path;
    let expanded = expand_home(raw_path.trim(), &std::env::var("HOME").unwrap_or_default());
    let canonical = Path::new(&expanded).canonicalize().ok()?;
    let sources = SourceRepository::new(catalog).list().ok()?;
    let (covering, rest) = covering_of(&sources, &canonical)?;
    let card = source_card(catalog, covering.clone());
    if rest.as_os_str().is_empty() {
        return Some(CoveringSource {
            location_name: card.display_name.clone(),
            card,
            limit_path: None,
        });
    }
    // The limit is spelled the way the covering folder's files are, so a
    // path comparison in the query matches them exactly.
    let limit = Path::new(&covering.canonical_path).join(rest);
    Some(CoveringSource {
        location_name: folder_display_name(&limit.to_string_lossy()),
        card,
        limit_path: Some(limit.to_string_lossy().to_string()),
    })
}

/// The registered directory that covers `path` and what lies below it: the
/// top-most one if several do (an older profile can hold overlaps).
fn covering_of<'a>(
    sources: &'a [orbok_db::repo::SourceRecord],
    path: &std::path::Path,
) -> Option<(&'a orbok_db::repo::SourceRecord, std::path::PathBuf)> {
    use std::path::Path;
    sources
        .iter()
        .filter(|s| s.source_type == orbok_core::SourceType::Directory)
        .filter_map(|s| {
            orbok_core::folder_cover::relative_under(Path::new(&s.canonical_path), path)
                .map(|rest| (s, rest))
        })
        .min_by_key(|(s, _)| Path::new(&s.canonical_path).components().count())
}

/// The registered folders strictly inside `path`, shallowest first (the order
/// `SourceRepository::absorb` takes them in).
fn folders_inside<'a>(
    sources: &'a [orbok_db::repo::SourceRecord],
    path: &std::path::Path,
) -> Vec<&'a orbok_db::repo::SourceRecord> {
    use std::path::Path;
    let mut inside: Vec<_> = sources
        .iter()
        .filter(|s| orbok_core::folder_cover::is_inside(path, Path::new(&s.canonical_path)))
        .collect();
    inside.sort_by_key(|s| {
        (
            Path::new(&s.canonical_path).components().count(),
            s.created_at.clone(),
        )
    });
    inside
}

fn combined_folder(source: &orbok_db::repo::SourceRecord) -> orbok_ui::state::CombinedFolder {
    orbok_ui::state::CombinedFolder {
        source_id: source.source_id.as_str().to_string(),
        display_name: source
            .display_name
            .clone()
            .unwrap_or_else(|| folder_display_name(&source.canonical_path)),
        canonical_path: source.canonical_path.clone(),
    }
}

/// Task 113 (RFC-064 §3.3): make every added folder that another added folder
/// covers part of the top one -- the overlaps an older version allowed.
/// Idempotent: once nothing overlaps it changes nothing and returns nothing.
/// One entry per top folder that took others.
pub fn combine_overlapping_folders(
    catalog: &Catalog,
) -> OrbokResult<Vec<orbok_ui::state::FoldersCombined>> {
    use orbok_db::repo::SourceRepository;
    use std::path::Path;
    let repo = SourceRepository::new(catalog);
    let sources = repo.list()?;
    // `covered_by(a, b)`: `b` is a directory that covers `a`, and is not `a`.
    // Two rows for one folder (same path, or the same but for case) are ordered
    // by id, so exactly one of them is the top.
    let covered_by = |a: &orbok_db::repo::SourceRecord, b: &orbok_db::repo::SourceRecord| {
        a.source_id != b.source_id
            && b.source_type == orbok_core::SourceType::Directory
            && orbok_core::folder_cover::relative_under(
                Path::new(&b.canonical_path),
                Path::new(&a.canonical_path),
            )
            .is_some_and(|rest| {
                rest.as_os_str().is_empty() && b.source_id.as_str() < a.source_id.as_str()
                    || !rest.as_os_str().is_empty()
            })
    };
    let mut combined = Vec::new();
    let mut tops: Vec<_> = sources
        .iter()
        .filter(|s| !sources.iter().any(|other| covered_by(s, other)))
        .collect();
    tops.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then(a.source_id.as_str().cmp(b.source_id.as_str()))
    });
    for top in tops {
        let mut inside: Vec<_> = sources.iter().filter(|s| covered_by(s, top)).collect();
        if inside.is_empty() {
            continue;
        }
        inside.sort_by_key(|s| {
            (
                Path::new(&s.canonical_path).components().count(),
                s.created_at.clone(),
            )
        });
        let ids: Vec<_> = inside.iter().map(|s| s.source_id.clone()).collect();
        repo.absorb(&top.source_id, &ids)?;
        combined.push(orbok_ui::state::FoldersCombined {
            parent_id: top.source_id.as_str().to_string(),
            parent_name: top
                .display_name
                .clone()
                .unwrap_or_else(|| folder_display_name(&top.canonical_path)),
            folders: inside.iter().map(|s| combined_folder(s)).collect(),
        });
    }
    Ok(combined)
}
