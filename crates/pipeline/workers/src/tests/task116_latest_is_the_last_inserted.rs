//! Task 116 §2.4: "the newest extraction" is the one inserted last, not the one
//! whose `completed_at` sorts greatest -- two readings can sort the wrong way,
//! and which extraction came after which is an event.

use crate::chunk_and_index::ChunkAndIndexWorker;
use orbok_cache::CacheService;
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{FileRepository, NewFile, NewSource, ObservedMetadata, SourceRepository};

fn insert_extraction(catalog: &Catalog, id: &str, file_id: &str, completed_at: &str) {
    catalog
        .lock()
        .execute(
            "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
             extractor_version, normalization_version, status, completed_at, created_at, \
             updated_at) VALUES (?1, ?2, 'x', '1', '1', 'succeeded', ?3, ?3, ?3)",
            rusqlite::params![id, file_id, completed_at],
        )
        .unwrap();
}

#[test]
fn the_latest_extraction_is_the_last_one_inserted() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let source = SourceRepository::new(&catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: None,
            original_path: path.clone(),
            canonical_path: path.clone(),
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
            original_path: format!("{path}/a.md"),
            canonical_path: format!("{path}/a.md"),
            display_path: "a.md".into(),
            extension: Some("md".into()),
            metadata: ObservedMetadata::default(),
            status: FileStatus::Discovered,
        })
        .unwrap();

    // `first` came first, and is spelled so that it sorts *after* `second`
    // although `second` is the later moment: `.1234Z` > `.123456Z` as text.
    insert_extraction(
        &catalog,
        "ext-first",
        file.file_id.as_str(),
        "2026-09-24T15:26:29.1234Z",
    );
    insert_extraction(
        &catalog,
        "ext-second",
        file.file_id.as_str(),
        "2026-09-24T15:26:29.123456Z",
    );

    let cache = CacheService::new(dir.path());
    let worker = ChunkAndIndexWorker::new(&catalog, &cache);
    assert_eq!(
        worker.latest_extraction_id(&file.file_id).unwrap().as_str(),
        "ext-second",
        "the extraction inserted last is the latest, whatever its completed_at sorts as"
    );
}
