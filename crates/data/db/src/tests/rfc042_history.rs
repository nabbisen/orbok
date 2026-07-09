//! RFC-042 §17.1 — search history repository unit tests.
//!
//! Validates the storage policy (§8): dedup, max-entry eviction, empty
//! rejection, clear, and folder-filter validity for restore (§9).

use crate::Catalog;
use crate::repo::{NewSource, SearchHistoryRepository, SourceRepository};
use orbok_core::{
    HiddenFilePolicy, IndexMode, PersistenceMode, SearchHistorySettings, SourceType,
    StoredKindFilter, StoredSearchFilter, SymlinkPolicy,
};

fn settings() -> SearchHistorySettings {
    SearchHistorySettings::default()
}

fn kind(label: &str) -> StoredSearchFilter {
    StoredSearchFilter::Kind {
        value: StoredKindFilter::Pdfs,
        label: label.to_string(),
    }
}

// RFC-042 §17.1: creates a history entry after a successful search.
#[test]
fn creates_entry() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    repo.upsert("renewal policy", &[], Some(12), "en", &settings())
        .unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].search_text, "renewal policy");
    assert_eq!(list[0].previous_result_count, Some(12));
}

// RFC-042 §17.1 / §8.5: empty searches are not stored.
#[test]
fn rejects_empty_search() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    assert!(repo.upsert("   ", &[], None, "en", &settings()).is_err());
    assert_eq!(repo.count().unwrap(), 0);
}

// RFC-042 §8.4: same text + same filters dedupes (update, not duplicate).
#[test]
fn deduplicates_same_search_and_filters() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    let filters = vec![kind("PDFs")];
    let id1 = repo
        .upsert("audit log", &filters, Some(3), "en", &settings())
        .unwrap();
    let id2 = repo
        .upsert("audit log", &filters, Some(5), "en", &settings())
        .unwrap();
    assert_eq!(id1, id2, "same search+filters reuses the entry");
    assert_eq!(repo.count().unwrap(), 1);
    // result count is refreshed on the dedup update.
    assert_eq!(repo.list().unwrap()[0].previous_result_count, Some(5));
}

// RFC-042 §8.4: same text + different filters are kept separately.
#[test]
fn different_filters_kept_separately() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    repo.upsert("audit log", &[kind("PDFs")], None, "en", &settings())
        .unwrap();
    repo.upsert("audit log", &[], None, "en", &settings())
        .unwrap();
    assert_eq!(repo.count().unwrap(), 2);
}

// RFC-042 §8.3: max entry count is enforced (oldest evicted).
#[test]
fn enforces_max_entries() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    let small = SearchHistorySettings {
        max_entries: 3,
        ..SearchHistorySettings::default()
    };
    for i in 0..6 {
        repo.upsert(&format!("search {i}"), &[], None, "en", &small)
            .unwrap();
    }
    assert_eq!(repo.count().unwrap(), 3, "kept only the newest 3");
    // Newest survives, oldest evicted.
    let texts: Vec<String> = repo
        .list()
        .unwrap()
        .into_iter()
        .map(|e| e.search_text)
        .collect();
    assert!(texts.contains(&"search 5".to_string()));
    assert!(!texts.contains(&"search 0".to_string()));
}

// RFC-042 §17.1: clears all entries.
#[test]
fn clears_all_entries() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    repo.upsert("a", &[], None, "en", &settings()).unwrap();
    repo.upsert("b", &[], None, "en", &settings()).unwrap();
    assert_eq!(repo.count().unwrap(), 2);
    repo.clear().unwrap();
    assert_eq!(repo.count().unwrap(), 0);
}

// RFC-042 §17.2: remove a single entry.
#[test]
fn removes_single_entry() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    let id = repo.upsert("keep", &[], None, "en", &settings()).unwrap();
    repo.upsert("drop", &[], None, "en", &settings()).unwrap();
    repo.remove(&id).unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].search_text, "drop");
}

// RFC-042 §17.2: get fetches a stored entry with its filters intact.
#[test]
fn get_round_trips_filters() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    let filters = vec![
        StoredSearchFilter::Folder {
            id: "src_1".into(),
            label: "Documents".into(),
        },
        kind("PDFs"),
    ];
    let id = repo
        .upsert("contract", &filters, Some(7), "ja", &settings())
        .unwrap();
    let got = repo.get(&id).unwrap().expect("entry exists");
    assert_eq!(got.locale, "ja");
    assert_eq!(got.filters.len(), 2);
    assert_eq!(got.filters[0].folder_id(), Some("src_1"));
    assert_eq!(got.filters[1].label(), "PDFs");
}

fn new_source(path: &str) -> NewSource {
    NewSource {
        source_type: SourceType::Directory,
        persistence_mode: PersistenceMode::Persistent,
        display_name: Some("Docs".into()),
        original_path: path.into(),
        canonical_path: path.into(),
        index_mode: IndexMode::Balanced,
        include_patterns: vec![],
        exclude_patterns: vec![],
        hidden_file_policy: HiddenFilePolicy::Exclude,
        symlink_policy: SymlinkPolicy::Ignore,
        max_file_size_bytes: None,
    }
}

// RFC-042 §9 step 3: a folder filter whose source still exists is a valid
// restore target; one whose source is gone is detectable via folder_id.
#[test]
fn folder_filter_validity_is_detectable() {
    let catalog = Catalog::open_in_memory().unwrap();
    let src = SourceRepository::new(&catalog)
        .insert(new_source("/docs"))
        .unwrap();

    let present = StoredSearchFilter::Folder {
        id: src.source_id.as_str().to_string(),
        label: "Docs".into(),
    };
    let missing = StoredSearchFilter::Folder {
        id: "src_gone".into(),
        label: "Old".into(),
    };

    let sources = SourceRepository::new(&catalog);
    // The present source resolves; the missing one does not.
    assert!(
        sources
            .get(&orbok_core::SourceId::from_string(
                present.folder_id().unwrap().to_string()
            ))
            .unwrap()
            .is_some()
    );
    assert!(
        sources
            .get(&orbok_core::SourceId::from_string(
                missing.folder_id().unwrap().to_string()
            ))
            .unwrap()
            .is_none()
    );
}

// RFC-042 §8.2: stored filters survive a serde round-trip (JSON storage).
#[test]
fn stored_filter_serde_round_trips() {
    let filters = vec![
        StoredSearchFilter::Folder {
            id: "src_1".into(),
            label: "Documents".into(),
        },
        kind("PDFs"),
    ];
    let json = serde_json::to_string(&filters).unwrap();
    let back: Vec<StoredSearchFilter> = serde_json::from_str(&json).unwrap();
    assert_eq!(filters, back);
}

// RFC-042 §15: accessible label includes search text and filters.
#[test]
fn accessible_label_includes_text_and_filters() {
    let catalog = Catalog::open_in_memory().unwrap();
    let repo = SearchHistoryRepository::new(&catalog);
    let id = repo
        .upsert(
            "token rotation",
            &[kind("PDFs")],
            Some(4),
            "en",
            &settings(),
        )
        .unwrap();
    let entry = repo.get(&id).unwrap().unwrap();
    let label = entry.accessible_label();
    assert!(label.contains("token rotation"));
    assert!(label.contains("PDFs"));
}
