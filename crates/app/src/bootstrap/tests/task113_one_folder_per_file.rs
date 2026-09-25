//! Task 113 (RFC-064 §3.3): every file belongs to one folder. Adding a
//! folder inside an added one registers nothing; adding one above added
//! folders makes them part of it; overlapping folders in an existing profile
//! are combined once at startup.

use crate::bootstrap::{self, AddSourceOutcome};
use orbok_core::SourceId;
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;
use orbok_fs::{ScanRequest, Scanner};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// `root/a/x.md`, `root/a/b/y.md`, `root/a/b/c/z.md` and `root/a2/w.md`.
struct Tree {
    _dir: tempfile::TempDir,
    root: PathBuf,
    catalog: Catalog,
}

fn tree() -> Tree {
    let (dir, root) = seeded_dir();
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    Tree {
        _dir: dir,
        root,
        catalog,
    }
}

fn seeded_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    // Canonical, so a path built from `root` is spelled as the catalog spells it.
    let root = dir.path().canonicalize().unwrap();
    for (rel, text) in [
        ("a/x.md", "# x\n\nalpha"),
        ("a/b/y.md", "# y\n\nbeta"),
        ("a/b/c/z.md", "# z\n\ngamma"),
        ("a2/w.md", "# w\n\ndelta"),
    ] {
        let path = native(&root, rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    (dir, root)
}

/// `rel` (written with `/`) below `base`, built component by component so
/// the separators are the platform's own, as canonical paths are.
fn native(base: &std::path::Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn scan(catalog: &Catalog, source_id: &str) {
    Scanner::new(catalog)
        .scan(
            &ScanRequest {
                source_id: SourceId::from_string(source_id.to_string()),
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
}

fn add(t: &Tree, rel: &str) -> AddSourceOutcome {
    bootstrap::add_source(&t.catalog, &native(&t.root, rel).to_string_lossy()).unwrap()
}

fn add_and_scan(t: &Tree, rel: &str) -> orbok_ui::state::SourceCard {
    let AddSourceOutcome::Added { card, .. } = add(t, rel) else {
        panic!("expected {rel} to be added as a new folder");
    };
    scan(&t.catalog, &card.source_id);
    card
}

/// Register a folder the way an older version did, with no covering check.
fn register_raw(t: &Tree, rel: &str) -> String {
    use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
    let path = native(&t.root, rel).to_string_lossy().to_string();
    let id = SourceRepository::new(&t.catalog)
        .insert(orbok_db::repo::NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some(rel.rsplit('/').next().unwrap().to_string()),
            original_path: path.clone(),
            canonical_path: path,
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap()
        .source_id
        .as_str()
        .to_string();
    scan(&t.catalog, &id);
    id
}

fn source_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap()
}

/// `(canonical_path relative to root, source_id, display_path)` for every file row.
fn file_rows(t: &Tree) -> Vec<(String, String, String)> {
    let conn = t.catalog.lock();
    let mut stmt = conn
        .prepare(
            "SELECT canonical_path, source_id, display_path FROM files ORDER BY canonical_path",
        )
        .unwrap();
    stmt.query_map([], |r| {
        let path: String = r.get(0)?;
        let relative = std::path::Path::new(&path)
            .strip_prefix(&t.root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        Ok((
            relative,
            r.get(1)?,
            r.get::<_, String>(2)?.replace('\\', "/"),
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn each_file_once(t: &Tree) {
    let rows = file_rows(t);
    let mut paths: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    paths.sort();
    let before = paths.len();
    paths.dedup();
    assert_eq!(before, paths.len(), "a file has two rows: {rows:?}");
}

fn jobs_naming(catalog: &Catalog, source_id: &str) -> i64 {
    catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE source_id = ?1",
            [source_id],
            |r| r.get(0),
        )
        .unwrap()
}

/// §2.1 (and the record of today's behaviour: two folders, two rows for
/// every file under `b`).
#[test]
fn a_folder_inside_an_added_folder_registers_nothing() {
    let t = tree();
    let a = add_and_scan(&t, "a");
    assert_eq!(file_rows(&t).len(), 3, "x, y and z");

    match add(&t, "a/b") {
        AddSourceOutcome::AlreadyIncluded { folder, parent } => {
            assert_eq!(folder, "b", "the notice names the folder chosen");
            assert_eq!(
                parent.source_id, a.source_id,
                "and the folder that holds it"
            );
        }
        other => panic!("a folder inside an added folder must not be added: {other:?}"),
    }
    assert_eq!(source_count(&t.catalog), 1, "no second folder");
    each_file_once(&t);
    assert_eq!(file_rows(&t).len(), 3);
}

/// §2.1: the same through a deeper folder, and through a spelling that is not
/// canonical.
#[test]
fn a_deeper_folder_and_another_spelling_are_included_too() {
    let t = tree();
    let a = add_and_scan(&t, "a");
    let plain = native(&t.root, "a/b/c");
    let spellings = [
        plain.to_string_lossy().to_string(),
        native(&t.root, "a/b/../b/c").to_string_lossy().to_string(),
        format!("{}{}", plain.display(), std::path::MAIN_SEPARATOR),
    ];
    for spelling in &spellings {
        match bootstrap::add_source(&t.catalog, spelling).unwrap() {
            AddSourceOutcome::AlreadyIncluded { parent, .. } => {
                assert_eq!(parent.source_id, a.source_id, "{spelling}")
            }
            other => panic!("{spelling}: {other:?}"),
        }
    }
    assert_eq!(source_count(&t.catalog), 1);
}

/// §2.3: `a2` is not inside `a`. A string prefix would say it is.
#[test]
fn a_sibling_whose_name_starts_like_an_added_folder_is_not_included() {
    let t = tree();
    add_and_scan(&t, "a");
    let AddSourceOutcome::Added { combined, .. } = add(&t, "a2") else {
        panic!("`a2` is a different folder from `a` and must be added");
    };
    assert!(combined.is_empty());
    assert_eq!(source_count(&t.catalog), 2);
}

/// The other direction of §2.3: adding `a2` above nothing, then `a`, must not
/// combine `a2` into `a`.
#[test]
fn a_folder_above_does_not_take_a_sibling_with_a_longer_name() {
    let t = tree();
    add_and_scan(&t, "a2");
    let AddSourceOutcome::Added { combined, .. } = add(&t, "a") else {
        panic!("`a` must be added");
    };
    assert!(combined.is_empty(), "`a2` is not part of `a`: {combined:?}");
    assert_eq!(source_count(&t.catalog), 2);
}

/// §2.2 (the catalog half; the prepared chunks are kept is in the scheduler
/// host's tests): `a/b` is prepared, then `a` is added.
#[test]
fn a_folder_above_added_folders_takes_their_files_with_it() {
    let t = tree();
    let b = add_and_scan(&t, "a/b");
    let files_before = file_rows(&t);
    let jobs_before: i64 = t
        .catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type != 'scan'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(jobs_before > 0, "the scan queued work for b's files");

    let AddSourceOutcome::Added {
        card: a, combined, ..
    } = add(&t, "a")
    else {
        panic!("`a` is above `b` and must be added");
    };

    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].source_id, b.source_id);
    assert_eq!(combined[0].display_name, "b");
    assert_eq!(source_count(&t.catalog), 1, "one folder remains");
    each_file_once(&t);
    let files_after = file_rows(&t);
    assert_eq!(files_after.len(), files_before.len(), "no file was lost");
    for (path, source_id, display_path) in &files_after {
        assert_eq!(source_id, &a.source_id, "{path} now belongs to `a`");
        assert_eq!(
            display_path,
            path.strip_prefix("a/").unwrap(),
            "and is shown relative to `a`"
        );
    }
    assert_eq!(jobs_naming(&t.catalog, &b.source_id), 0, "no job names `b`");
    let jobs_after: i64 = t
        .catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type != 'scan'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        jobs_after, jobs_before,
        "b's queued work moved with its files"
    );
}

/// §1.3: a chain `a` ⊃ `b` ⊃ `c` combines into the top folder, whichever
/// order it was added in.
#[test]
fn a_chain_combines_into_the_top_folder() {
    let t = tree();
    add_and_scan(&t, "a/b/c");
    let AddSourceOutcome::Added { combined, .. } = add(&t, "a/b") else {
        panic!("a/b is above a/b/c");
    };
    assert_eq!(combined.len(), 1);
    let AddSourceOutcome::Added { card, combined, .. } = add(&t, "a") else {
        panic!("a is above a/b");
    };
    assert_eq!(combined.len(), 1);
    assert_eq!(source_count(&t.catalog), 1);
    each_file_once(&t);
    for (path, source_id, _) in file_rows(&t) {
        assert_eq!(source_id, card.source_id, "{path}");
    }
}

/// §1.3 with several folders combined at once.
#[test]
fn several_folders_are_combined_by_one_add() {
    let t = tree();
    add_and_scan(&t, "a/b");
    assert!(matches!(
        add(&t, "a/b/c"),
        AddSourceOutcome::AlreadyIncluded { .. }
    ));
    add_and_scan(&t, "a2");
    let AddSourceOutcome::Added { combined, .. } = add(&t, "") else {
        panic!("the root is above everything");
    };
    let mut names: Vec<_> = combined.iter().map(|c| c.display_name.clone()).collect();
    names.sort();
    assert_eq!(names, ["a2", "b"]);
    assert_eq!(source_count(&t.catalog), 1);
    each_file_once(&t);
}

/// Search in a subfolder (§1.4): the covering folder and the subfolder's path.
#[test]
fn a_chosen_subfolder_is_found_inside_the_added_folder() {
    let t = tree();
    let a = add_and_scan(&t, "a");
    let sub = |rel: &str| native(&t.root, rel).to_string_lossy().to_string();

    let inside = bootstrap::covering_source(&t.catalog, &sub("a/b")).expect("a covers a/b");
    assert_eq!(inside.card.source_id, a.source_id);
    assert_eq!(
        inside.location_name, "b",
        "the chip shows the subfolder's name"
    );
    assert_eq!(inside.limit_path.as_deref(), Some(sub("a/b").as_str()));

    let itself = bootstrap::covering_source(&t.catalog, &sub("a")).unwrap();
    assert_eq!(itself.card.source_id, a.source_id);
    assert_eq!(
        itself.limit_path, None,
        "the added folder itself has no limit"
    );

    assert!(
        bootstrap::covering_source(&t.catalog, &sub("a2")).is_none(),
        "the component rule"
    );
    assert!(bootstrap::covering_source(&t.catalog, &sub("nowhere")).is_none());
}

/// §2.5: an overlapping pair a profile already holds is combined, and a
/// second call finds nothing.
#[test]
fn overlapping_folders_are_combined_once() {
    let t = tree();
    let outer = register_raw(&t, "a");
    register_raw(&t, "a/b");
    register_raw(&t, "a/b/c");
    let other = register_raw(&t, "a2");
    assert_eq!(source_count(&t.catalog), 4);
    assert!(file_rows(&t).len() > 4, "the overlap made duplicate rows");

    let first = bootstrap::combine_overlapping_folders(&t.catalog).unwrap();
    assert_eq!(first.len(), 1, "one top folder took the others: {first:?}");
    assert_eq!(first[0].parent_id, outer);
    assert_eq!(first[0].parent_name, "a");
    let mut names: Vec<_> = first[0]
        .folders
        .iter()
        .map(|f| f.display_name.clone())
        .collect();
    names.sort();
    assert_eq!(names, ["b", "c"]);
    assert_eq!(source_count(&t.catalog), 2, "`a` and the unrelated `a2`");
    each_file_once(&t);
    for (path, source_id, _) in file_rows(&t) {
        let expected = if path.starts_with("a2/") {
            &other
        } else {
            &outer
        };
        assert_eq!(&source_id, expected, "{path}");
    }

    let second = bootstrap::combine_overlapping_folders(&t.catalog).unwrap();
    assert!(second.is_empty(), "a second run finds nothing: {second:?}");
    assert_eq!(source_count(&t.catalog), 2);
    each_file_once(&t);
}

/// §2.5, through startup: the first start says so, the second says nothing.
#[test]
fn the_first_start_says_folders_were_combined_and_the_second_is_silent() {
    use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
    use orbok_ui::notice::UserNotice;
    let (dir, root) = seeded_dir();
    let data = root.join("data");
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data.clone().into_os_string())).unwrap(),
        &data,
        PlatformRuntimePaths {
            standard_data_dir: Some(&data),
            standard_settings_dir: Some(&data),
        },
    )
    .unwrap();
    let t = Tree {
        _dir: dir,
        root,
        catalog: bootstrap::open_catalog(&context).unwrap(),
    };
    register_raw(&t, "a");
    register_raw(&t, "a/b");

    let first = bootstrap::load_initial_state(&context).unwrap();
    assert_eq!(
        first.notice,
        Some(UserNotice::FoldersCombined {
            folders: vec!["b".into()],
            parent: "a".into(),
        })
    );
    assert_eq!(first.sources.len(), 1, "the list shows one folder");
    assert_eq!(source_count(&t.catalog), 1);

    let second = bootstrap::load_initial_state(&context).unwrap();
    assert_eq!(second.notice, None, "nothing left to combine, nothing said");
    assert_eq!(second.sources.len(), 1);
    each_file_once(&t);
}

// ── Task 117: the settings-file notice at startup ─────────────────────

fn context_at(data: &Path) -> orbok::runtime_context::RuntimeContext {
    use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data.as_os_str().to_os_string())).unwrap(),
        data,
        PlatformRuntimePaths {
            standard_data_dir: Some(data),
            standard_settings_dir: Some(data),
        },
    )
    .unwrap()
}

/// A damaged settings file: the startup that moves it says so, once. The next
/// start finds a fresh file and says nothing.
#[test]
fn a_damaged_settings_file_is_reported_once() {
    use orbok_ui::notice::UserNotice;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let context = context_at(&data);
    std::fs::write(data.join("settings.json"), br#"{"locale": "ja", "theme":"#).unwrap();

    let first = bootstrap::load_initial_state(&context).unwrap();
    assert_eq!(first.notice, Some(UserNotice::SettingsFileUnreadable));
    assert!(data.join("settings.json.unreadable").exists());

    let second = bootstrap::load_initial_state(&context).unwrap();
    assert_eq!(
        second.notice, None,
        "the file was moved once; nothing to say now"
    );
}

/// No file at all (a first start) and a healthy file raise nothing.
#[test]
fn a_first_start_and_a_healthy_file_raise_no_settings_notice() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let context = context_at(&data);
    assert!(!data.join("settings.json").exists());
    assert_eq!(
        bootstrap::load_initial_state(&context).unwrap().notice,
        None
    );
    assert!(
        data.join("settings.json").exists(),
        "the first start wrote its defaults"
    );
    assert_eq!(
        bootstrap::load_initial_state(&context).unwrap().notice,
        None
    );
    assert!(!data.join("settings.json.unreadable").exists());
}

/// A damaged file and a combine in one start: the user's own settings matter
/// more, so this notice is the one shown.
#[test]
fn the_settings_notice_wins_over_the_combine_notice() {
    use orbok_ui::notice::UserNotice;
    let (dir, root) = seeded_dir();
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let context = context_at(&data);
    let t = Tree {
        _dir: dir,
        root,
        catalog: bootstrap::open_catalog(&context).unwrap(),
    };
    register_raw(&t, "a");
    register_raw(&t, "a/b");
    std::fs::write(data.join("settings.json"), b"{ broken").unwrap();

    let state = bootstrap::load_initial_state(&context).unwrap();

    assert_eq!(state.notice, Some(UserNotice::SettingsFileUnreadable));
    assert_eq!(state.sources.len(), 1, "the combine still happened");
}
