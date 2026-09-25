//! Task 116 follow-up (Review 294 §3): "seen" means exactly one thing -- the
//! scanner saw the file during a scan. A pipeline write (the chunk worker
//! marking `no_text_found`) is not the scanner seeing it.

use crate::Catalog;
use crate::repo::{FileRepository, NewFile, NewSource, ObservedMetadata, SourceRepository};
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy,
};

fn folder_with_a_file() -> (Catalog, orbok_core::SourceId, orbok_core::FileId) {
    let catalog = Catalog::open_in_memory().unwrap();
    let source = SourceRepository::new(&catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: None,
            original_path: "/d".into(),
            canonical_path: "/d".into(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();
    let file = FileRepository::new(&catalog)
        .insert(NewFile {
            source_id: source.source_id.clone(),
            original_path: "/d/a.pdf".into(),
            canonical_path: "/d/a.pdf".into(),
            display_path: "a.pdf".into(),
            extension: Some("pdf".into()),
            metadata: ObservedMetadata::default(),
            status: FileStatus::Discovered,
        })
        .unwrap();
    (catalog, source.source_id, file.file_id)
}

fn status(catalog: &Catalog, id: &orbok_core::FileId) -> FileStatus {
    FileRepository::new(catalog)
        .get_by_id(id)
        .unwrap()
        .unwrap()
        .file_status
}

/// A file whose chunk job sets `no_text_found` **during** a scan that did not
/// see it is marked missing at that scan's end: the pipeline's status write does
/// not count as the scanner having seen the file.
#[test]
fn a_pipeline_status_write_does_not_count_as_seen() {
    let (catalog, source_id, file_id) = folder_with_a_file();
    let files = FileRepository::new(&catalog);
    let generation = SourceRepository::new(&catalog)
        .begin_scan(&source_id)
        .unwrap();

    // The chunk worker finishes for this file while the scan is running.
    files.set_status(&file_id, FileStatus::NoTextFound).unwrap();
    // The scan ends without having seen the file (it is gone from the disk).
    let missing = files.mark_missing_unseen(&source_id, generation).unwrap();

    assert_eq!(missing, 1);
    assert_eq!(status(&catalog, &file_id), FileStatus::Missing);
}

/// The scanner's own status-only path does count: a file it saw and could not
/// read (`permission_denied`) is not marked missing by its own scan.
#[test]
fn the_scanners_status_only_write_counts_as_seen() {
    let (catalog, source_id, file_id) = folder_with_a_file();
    let files = FileRepository::new(&catalog);
    let generation = SourceRepository::new(&catalog)
        .begin_scan(&source_id)
        .unwrap();

    files
        .set_status_seen(&file_id, FileStatus::PermissionDenied)
        .unwrap();

    assert_eq!(
        files.mark_missing_unseen(&source_id, generation).unwrap(),
        0
    );
    assert_eq!(status(&catalog, &file_id), FileStatus::PermissionDenied);
}
