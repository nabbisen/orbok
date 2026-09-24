//! Task 114 (RFC-064 §3): a folder covers **this folder and subfolders** or
//! **this folder only**. The scanner honours it, narrowing drops what is below
//! the top level, and only a folder that covers its subfolders covers a folder
//! inside it.

use crate::bootstrap::{self, AddSourceOutcome};
use orbok_core::SourceId;
use orbok_db::Catalog;
use orbok_db::repo::{FileRepository, SourceRepository};
use orbok_fs::{ScanRequest, Scanner};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// `root/f/x.md`, `root/f/sub/y.md`, `root/f/sub/deep/z.md`.
struct Tree {
    _dir: tempfile::TempDir,
    root: PathBuf,
    catalog: Catalog,
}

fn native(base: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn tree() -> Tree {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for rel in ["f/x.md", "f/sub/y.md", "f/sub/deep/z.md"] {
        let path = native(&root, rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("# {rel}\n\ntext\n")).unwrap();
    }
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    Tree {
        _dir: dir,
        root,
        catalog,
    }
}

fn add(t: &Tree, rel: &str) -> AddSourceOutcome {
    bootstrap::add_source(&t.catalog, &native(&t.root, rel).to_string_lossy()).unwrap()
}

fn added(outcome: AddSourceOutcome) -> orbok_ui::state::SourceCard {
    match outcome {
        AddSourceOutcome::Added { card, .. } => card,
        other => panic!("expected a new folder: {other:?}"),
    }
}

fn id(card: &orbok_ui::state::SourceCard) -> SourceId {
    SourceId::from_string(card.source_id.clone())
}

fn scan(t: &Tree, card: &orbok_ui::state::SourceCard) -> orbok_fs::ScanSummary {
    Scanner::new(&t.catalog)
        .scan(
            &ScanRequest {
                source_id: id(card),
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn file_names(t: &Tree) -> Vec<String> {
    let conn = t.catalog.lock();
    let mut stmt = conn
        .prepare("SELECT display_path FROM files ORDER BY display_path")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|p| p.unwrap().replace('\\', "/"))
        .collect()
}

/// Narrow with no cache involved: the catalog half, for setting a folder up.
fn set_only(t: &Tree, card: &orbok_ui::state::SourceCard) {
    SourceRepository::new(&t.catalog)
        .narrow_to_top_level(&id(card))
        .unwrap();
}

fn covers(t: &Tree, card: &orbok_ui::state::SourceCard) -> bool {
    SourceRepository::new(&t.catalog)
        .get(&id(card))
        .unwrap()
        .unwrap()
        .covers_subfolders
}

/// §2.1: a new folder covers its subfolders; **this folder only** reads the
/// direct entries and does not descend.
#[test]
fn this_folder_only_prepares_only_the_top_level() {
    let t = tree();
    let card = added(add(&t, "f"));
    assert!(card.covers_subfolders, "a new folder covers its subfolders");
    scan(&t, &card);
    assert_eq!(file_names(&t), ["sub/deep/z.md", "sub/y.md", "x.md"]);

    let only = tree();
    let card = added(add(&only, "f"));
    set_only(&only, &card);
    assert!(!covers(&only, &card));
    let summary = scan(&only, &card);
    assert_eq!(
        file_names(&only),
        ["x.md"],
        "the top level is prepared and `sub/` has no rows"
    );
    // The scanner does not even walk `sub/` (the insert's own check, which
    // would refuse those rows, is a second line of defence, not the first).
    assert_eq!(summary.seen_files, 1, "only the direct entries are read");
}

/// The card reads the choice back from the record.
#[test]
fn the_card_says_what_the_folder_covers() {
    let t = tree();
    let card = added(add(&t, "f"));
    set_only(&t, &card);
    let cards = bootstrap::get_sources(&t.catalog).unwrap();
    assert!(!cards[0].covers_subfolders);
    bootstrap::widen_source(&t.catalog, &card.source_id).unwrap();
    assert!(bootstrap::get_sources(&t.catalog).unwrap()[0].covers_subfolders);
}

/// The counted line's number: files below the top level, by path components.
#[test]
fn the_count_is_the_files_below_the_top_level() {
    let t = tree();
    let card = added(add(&t, "f"));
    scan(&t, &card);
    assert_eq!(
        bootstrap::narrow_file_count(&t.catalog, &card.source_id).unwrap(),
        2,
        "`sub/y.md` and `sub/deep/z.md`; `x.md` stays"
    );
    // A folder with nothing below its top level: a genuine zero.
    let flat = tree();
    let card = added(add(&flat, "f/sub/deep"));
    scan(&flat, &card);
    assert_eq!(
        bootstrap::narrow_file_count(&flat.catalog, &card.source_id).unwrap(),
        0
    );
}

/// §1.6 / RFC-064 §3.3 row 3: a subfolder of a **this folder only** folder is
/// added -- the files do not overlap -- and a folder that covers its
/// subfolders still refuses it.
#[test]
fn a_subfolder_of_a_this_folder_only_folder_is_added() {
    let t = tree();
    let f = added(add(&t, "f"));
    set_only(&t, &f);

    let sub = add(&t, "f/sub");
    assert!(
        matches!(sub, AddSourceOutcome::Added { ref combined, .. } if combined.is_empty()),
        "{sub:?}"
    );
    assert_eq!(SourceRepository::new(&t.catalog).count().unwrap(), 2);
    // ... and nothing covers it for a search either.
    assert!(
        bootstrap::covering_source(&t.catalog, &native(&t.root, "f/sub/deep").to_string_lossy())
            .is_some(),
        "`f/sub` covers `f/sub/deep` (it covers its subfolders)"
    );
    assert!(
        bootstrap::covering_source(&t.catalog, &native(&t.root, "f").to_string_lossy())
            .is_some_and(|c| c.limit_path.is_none()),
        "`f` itself is `f`"
    );

    // The control: with subfolders covered, the same add is refused.
    let refused = tree();
    added(add(&refused, "f"));
    assert!(matches!(
        add(&refused, "f/sub"),
        AddSourceOutcome::AlreadyIncluded { .. }
    ));
}

/// Search in a subfolder of a **this folder only** folder registers it: the
/// folder does not cover it.
#[test]
fn a_subfolder_of_a_this_folder_only_folder_is_not_covered_for_search() {
    let t = tree();
    let f = added(add(&t, "f"));
    set_only(&t, &f);
    assert!(
        bootstrap::covering_source(&t.catalog, &native(&t.root, "f/sub").to_string_lossy())
            .is_none()
    );
}

/// §1.5: widening over an added subfolder combines them, in the same
/// transaction as the setting, and queues the scan.
#[test]
fn widening_over_an_added_subfolder_combines_them() {
    let t = tree();
    let f = added(add(&t, "f"));
    set_only(&t, &f);
    let sub = added(add(&t, "f/sub"));
    scan(&t, &f);
    scan(&t, &sub);
    assert_eq!(file_names(&t).len(), 3, "x under f; y and z under sub");

    let combined = bootstrap::widen_source(&t.catalog, &f.source_id).unwrap();

    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].source_id, sub.source_id);
    assert_eq!(SourceRepository::new(&t.catalog).count().unwrap(), 1);
    assert!(covers(&t, &f));
    let files = FileRepository::new(&t.catalog);
    for name in ["sub/y.md", "sub/deep/z.md"] {
        let path = native(&t.root, &format!("f/{name}"))
            .to_string_lossy()
            .to_string();
        let record = files.find_by_canonical_path(&path).unwrap().unwrap();
        assert_eq!(
            record.source_id.as_str(),
            f.source_id,
            "{name} belongs to `f` now"
        );
    }
    let scans: i64 = t
        .catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = 'scan' AND source_id = ?1 \
             AND status = 'queued'",
            [&f.source_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(scans >= 1, "widening queued a scan of the folder");
}

/// The startup combine agrees: an overlapping pair is combined only when the
/// outer folder covers its subfolders.
#[test]
fn the_startup_combine_leaves_a_subfolder_of_a_this_folder_only_folder_alone() {
    let t = tree();
    let f = added(add(&t, "f"));
    set_only(&t, &f);
    added(add(&t, "f/sub"));

    assert!(
        bootstrap::combine_overlapping_folders(&t.catalog)
            .unwrap()
            .is_empty()
    );
    assert_eq!(SourceRepository::new(&t.catalog).count().unwrap(), 2);
}

/// Stop condition §4.2: a scan that read the folder **before** it was narrowed
/// writes nothing below the top level afterwards -- the insert itself checks.
#[test]
fn a_scan_that_started_before_narrowing_cannot_write_below_the_top_level() {
    use orbok_core::FileStatus;
    use orbok_db::repo::{NewFile, ObservedMetadata};
    let t = tree();
    let f = added(add(&t, "f"));
    let new_file = |rel: &str| {
        let path = native(&t.root, rel).to_string_lossy().to_string();
        NewFile {
            source_id: id(&f),
            original_path: path.clone(),
            canonical_path: path,
            display_path: rel.to_string(),
            extension: Some("md".into()),
            metadata: ObservedMetadata::default(),
            status: FileStatus::Discovered,
        }
    };
    let files = FileRepository::new(&t.catalog);

    assert!(
        files
            .insert_below_top_level(new_file("f/sub/y.md"))
            .unwrap()
            .is_some(),
        "while the folder covers its subfolders"
    );
    set_only(&t, &f);
    assert!(
        files
            .insert_below_top_level(new_file("f/sub/deep/z.md"))
            .unwrap()
            .is_none(),
        "once it does not, the insert is refused"
    );
    assert_eq!(
        file_names(&t).len(),
        0,
        "and the narrowing erased the first"
    );
}
