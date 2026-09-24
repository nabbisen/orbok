//! Task 108 §2.3/§2.4: every path that shows a folder builds its card in one
//! place, from the record's own status and counts, and a folder with no name
//! is shown by its path, not by an English word of ours.

use crate::bootstrap::{self, AddSourceOutcome};
use orbok_core::{
    HiddenFilePolicy, IndexMode, PersistenceMode, SourceStatus, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{NewSource, SourceRepository};
use orbok_ui::state::SourceCard;

fn register(catalog: &Catalog, path: &str, display_name: Option<&str>) -> orbok_core::SourceId {
    SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: display_name.map(String::from),
            original_path: path.into(),
            canonical_path: path.into(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap()
        .source_id
}

fn listed(catalog: &Catalog, id: &orbok_core::SourceId) -> SourceCard {
    bootstrap::get_sources(catalog)
        .unwrap()
        .into_iter()
        .find(|c| c.source_id == id.as_str())
        .unwrap()
}

/// Test 3: the three callers -- the startup list, the search-in-folder
/// lookup and `add_source`'s already-registered answer -- produce the same
/// card for the same record, and a Missing record says so through all of
/// them (`source_card` used to hard-code Active).
#[test]
fn every_caller_builds_the_same_card_from_the_records_own_status() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();

    let AddSourceOutcome::Added { card: added, .. } =
        bootstrap::add_source(&catalog, &folder.to_string_lossy()).unwrap()
    else {
        panic!("expected a new folder");
    };
    let id = orbok_core::SourceId::from_string(added.source_id.clone());
    assert_eq!(
        listed(&catalog, &id),
        added,
        "add_source and the list agree"
    );

    SourceRepository::new(&catalog)
        .set_status(&id, SourceStatus::Missing)
        .unwrap();
    let canonical = folder.canonicalize().unwrap().to_string_lossy().to_string();
    let from_list = listed(&catalog, &id);
    let from_search_folder = bootstrap::covering_source(&catalog, &canonical)
        .unwrap()
        .card;
    let from_add = match bootstrap::add_source(&catalog, &canonical).unwrap() {
        AddSourceOutcome::AlreadyRegistered { card } => card,
        AddSourceOutcome::Added { .. } | AddSourceOutcome::AlreadyIncluded { .. } => {
            panic!("the folder is already registered")
        }
    };
    assert_eq!(from_list.status, SourceStatus::Missing);
    assert_eq!(from_search_folder, from_list, "search-in-folder path");
    assert_eq!(from_add, from_list, "add_source's already-registered path");
}

/// Test 4: a record with no display name shows its last path component; a
/// path with none (a root) shows whole. Never "source" or "folder".
#[test]
fn a_folder_with_no_name_is_shown_by_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    let nameless = register(&catalog, "/home/user/Projects", None);
    let empty_name = register(&catalog, "/data/Reports", Some(""));
    let root = register(&catalog, "/", None);

    assert_eq!(listed(&catalog, &nameless).display_name, "Projects");
    assert_eq!(listed(&catalog, &empty_name).display_name, "Reports");
    assert_eq!(listed(&catalog, &root).display_name, "/");
}
