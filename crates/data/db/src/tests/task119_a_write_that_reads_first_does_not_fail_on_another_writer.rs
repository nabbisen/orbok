//! A transaction that reads and then writes must not fail because another
//! connection committed in between. SQLite's default (deferred) transaction
//! takes a read snapshot at its first read and cannot upgrade it to a write once
//! another connection has committed: it fails at once with "database is locked",
//! and no busy timeout helps. Every catalog write transaction begins immediate
//! (it takes the write lock first, and waits for it like any other write).
//!
//! The scheduler and the UI each hold their own connection, so this is a real
//! interleaving, not a hypothetical one.

use crate::Catalog;
use crate::repo::{
    FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata, SourceRepository,
};
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy,
};

const FILES: usize = 300;

fn new_file(source: &orbok_core::SourceId, name: &str) -> NewFile {
    NewFile {
        source_id: source.clone(),
        original_path: format!("/d/{name}"),
        canonical_path: format!("/d/{name}"),
        display_path: name.into(),
        extension: Some("md".into()),
        metadata: ObservedMetadata::default(),
        status: FileStatus::Discovered,
    }
}

#[test]
fn enqueueing_while_another_connection_writes_does_not_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    let mine = Catalog::open(&path).unwrap();
    let other = Catalog::open(&path).unwrap();

    let source = SourceRepository::new(&mine)
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
        .unwrap()
        .source_id;
    let ids: Vec<_> = (0..FILES)
        .map(|i| {
            FileRepository::new(&mine)
                .insert(new_file(&source, &format!("mine-{i}.md")))
                .unwrap()
                .file_id
        })
        .collect();

    let writer_source = source.clone();
    let writer = std::thread::spawn(move || {
        let files = FileRepository::new(&other);
        for i in 0..FILES * 2 {
            files
                .insert(new_file(&writer_source, &format!("other-{i}.md")))
                .expect("the other connection's write");
        }
    });

    let jobs = IndexJobRepository::new(&mine);
    for id in &ids {
        jobs.enqueue_extraction_if_idle(id).expect(
            "an enqueue must wait for the other writer, not fail with 'database is locked'",
        );
    }
    writer.join().unwrap();
}
