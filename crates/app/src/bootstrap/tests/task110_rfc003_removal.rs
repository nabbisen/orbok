//! Task 110 (RFC-003 closure, criterion 8): removing a folder from orbok
//! removes its registration and what orbok prepared, never the files.

use crate::bootstrap::{self, AddSourceOutcome};
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;

#[test]
fn removing_a_folder_never_deletes_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("keep.md");
    std::fs::write(&file, "# keep\n").unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    let AddSourceOutcome::Added { card, .. } =
        bootstrap::add_source(&catalog, None, &folder.to_string_lossy()).unwrap()
    else {
        panic!("expected a new folder");
    };
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();

    bootstrap::remove_source(&catalog, &card.source_id).unwrap();

    assert_eq!(
        SourceRepository::new(&catalog).list().unwrap().len(),
        0,
        "the registration is gone"
    );
    assert!(file.exists(), "the file on disk is untouched");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# keep\n");
}
