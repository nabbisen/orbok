//! Task 047 §1: `add_source` refuses a folder whose canonical path is
//! already registered, and the RFC-045 search-in-folder lookup still reuses
//! it.

use crate::bootstrap::{self, AddSourceOutcome};
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;

fn added(outcome: AddSourceOutcome) -> orbok_ui::state::SourceCard {
    match outcome {
        AddSourceOutcome::Added { card, .. } => card,
        AddSourceOutcome::AlreadyRegistered { card } => {
            panic!(
                "expected a new source, got AlreadyRegistered({})",
                card.source_id
            )
        }
    }
}

#[test]
fn adding_an_already_registered_folder_inserts_nothing_and_returns_the_existing_source() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();

    let first = added(bootstrap::add_source(&catalog, &folder.to_string_lossy()).unwrap());

    // The same folder, written three ways; canonicalisation must make them equal.
    let spellings = [
        folder.to_string_lossy().to_string(),
        format!("{}/", folder.to_string_lossy()),
        folder.join(".").to_string_lossy().to_string(),
    ];
    for spelling in &spellings {
        let outcome = bootstrap::add_source(&catalog, spelling).unwrap();
        assert_eq!(
            SourceRepository::new(&catalog).list().unwrap().len(),
            1,
            "adding {spelling:?} again must not register a second source for the same folder"
        );
        match outcome {
            AddSourceOutcome::AlreadyRegistered { card } => assert_eq!(
                card.source_id, first.source_id,
                "{spelling:?} must return the source that is already registered"
            ),
            AddSourceOutcome::Added { card, .. } => {
                panic!(
                    "{spelling:?} was reported as newly added ({})",
                    card.source_id
                )
            }
        }
    }
}

#[test]
fn search_in_folder_lookup_reuses_the_registered_source() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();

    let first = added(bootstrap::add_source(&catalog, &folder.to_string_lossy()).unwrap());

    let found = bootstrap::find_source_by_canonical_path(&catalog, &first.display_path)
        .expect("the registered folder must be found by its canonical path");
    assert_eq!(found.source_id, first.source_id);

    let elsewhere = dir.path().join("elsewhere");
    assert!(
        bootstrap::find_source_by_canonical_path(&catalog, &elsewhere.to_string_lossy()).is_none(),
        "an unregistered path must not match"
    );
}
