//! RFC-060 §5, §11 criteria 6 and 9: a non-searchable source contributes no
//! candidates, and nothing under it is opened.
//!
//! Both halves matter. "No results" alone would pass even if the snippet
//! path still opened the file, which is exactly the hole the criterion
//! names: `snippet.rs` opened the catalog's stored path directly, with no
//! `PathGuard` anywhere in the module, while `path_guard.rs`'s own doc
//! requires a `ValidatedPath` before any backend read.

use crate::snippet::{chunk_records_for, searchable_path_guard};
use crate::{
    Fts5KeywordEngine, KeywordDocument, KeywordSearchEngine, MultilingualKeywordEngine,
    SearchService,
};
use orbok_core::{ChunkId, ModelId, OrbokError, SourceId, SourceStatus};
use orbok_db::Catalog;
use orbok_db::repo::{EmbeddingRepository, NewEmbedding, SourceRepository};
use rusqlite::params;

/// The model id the seeded embedding is written under; the vector scan is
/// keyed by it.
const MODEL_ID: &str = "model-test";

/// One source rooted at a real directory (the guard canonicalises, so the
/// path must exist), one indexed file inside it, one chunk.
fn seed(catalog: &Catalog, root: &std::path::Path) -> (ChunkId, String) {
    let file_path = root.join("notes.md");
    std::fs::write(&file_path, "alpha beta gamma\nsecond line\n").unwrap();
    let canonical_root = std::fs::canonicalize(root).unwrap();
    let canonical_file = std::fs::canonicalize(&file_path)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let chunk_id = ChunkId::generate();
    let conn = catalog.lock();
    let t = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at) VALUES ('s1','directory','persistent',?1,?1,'active','balanced',\
         'exclude','ignore',?2,?2)",
        params![canonical_root.to_string_lossy(), t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
         file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('f1','s1',?1,?1,'notes.md',1,'indexed',?2,?2,?2)",
        params![canonical_file, t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
         extractor_version, normalization_version, status, created_at, updated_at) \
         VALUES ('e1','f1','text','v1','norm-v1','succeeded',?1,?1)",
        params![t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
         chunk_status, created_at, updated_at) VALUES (?1,'f1','e1','paragraph',0,'active',?2,?2)",
        params![chunk_id.as_str(), t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chunk_locations (chunk_id, line_start, line_end, location_quality, \
         created_at, updated_at) VALUES (?1, 1, 1, 'exact', ?2, ?2)",
        params![chunk_id.as_str(), t],
    )
    .unwrap();
    drop(conn);

    Fts5KeywordEngine::new(catalog)
        .index(&[KeywordDocument {
            chunk_id: chunk_id.clone(),
            title: Some("notes.md".into()),
            heading_path: None,
            normalized_text: "alpha beta gamma".into(),
        }])
        .unwrap();

    // The trigram row `ChunkRepository::insert_bundle` would write (RFC-014
    // §12). Seeded directly, with CJK text so a Japanese query reaches the
    // trigram query and not the unicode61 one -- otherwise the trigram
    // join has no assertion of its own.
    let conn = catalog.lock();
    conn.execute(
        "INSERT INTO chunk_fts_trigram (title, heading_path, normalized_text) \
         VALUES ('notes.md', NULL, '日本語のテキスト')",
        [],
    )
    .unwrap();
    let trigram_rowid = conn.last_insert_rowid();
    conn.execute(
        "UPDATE keyword_index_records SET trigram_fts_rowid = ?2 WHERE chunk_id = ?1",
        params![chunk_id.as_str(), trigram_rowid],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO models (model_id, role, model_name, model_version, dimension, status, \
         created_at, updated_at) VALUES (?1,'embedding','test-model','v1',3,'available',?2,?2)",
        params![MODEL_ID, t],
    )
    .unwrap();
    drop(conn);

    EmbeddingRepository::new(catalog)
        .upsert(&NewEmbedding {
            chunk_id: chunk_id.clone(),
            model_id: ModelId::from_string(MODEL_ID.to_string()),
            dimension: 3,
            vector: vec![1.0, 0.0, 0.0],
        })
        .unwrap();

    (chunk_id, canonical_file)
}

/// What each of the four retrieval sites returns for the seeded chunk:
/// unicode61 keyword, trigram keyword, vector scan, enrichment lookup.
fn candidates_at_every_site(catalog: &Catalog, chunk_id: &ChunkId) -> (usize, usize, usize, usize) {
    let keyword = Fts5KeywordEngine::new(catalog).search("alpha", 10).unwrap();
    let trigram = MultilingualKeywordEngine::new(catalog)
        .search("日本語", 10)
        .unwrap();
    let vectors = EmbeddingRepository::new(catalog)
        .list_active_for_scan(MODEL_ID, 3, &orbok_core::SearchScope::default())
        .unwrap();
    let records = chunk_records_for(catalog, std::slice::from_ref(chunk_id)).unwrap();
    (keyword.len(), trigram.len(), vectors.len(), records.len())
}

/// Criterion 6: paused source, no results **and** no file under it opened.
#[test]
fn a_paused_source_yields_no_results_and_no_file_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let (chunk_id, canonical_file) = seed(&catalog, dir.path());
    let sources = SourceRepository::new(&catalog);
    let source_id = SourceId::from_string("s1".to_string());

    // Active: the control. Without it the test could pass by never matching
    // anything at all. Each site is asserted separately -- asserting only
    // through `SearchService` cannot tell them apart, and the first draft of
    // this test did exactly that: removing the keyword join left it green,
    // because the enrichment join dropped the record instead. That is the
    // post-filtering RFC-041 §25.5 forbids, passing itself off as a fix.
    let service = SearchService::new(&catalog);
    assert_eq!(
        candidates_at_every_site(&catalog, &chunk_id),
        (1, 1, 1, 1),
        "while active, every retrieval site must see the chunk \
         (keyword, trigram, vector, enrichment)"
    );
    assert_eq!(
        service.search("alpha", 10).unwrap().len(),
        1,
        "the active source must be searchable, or this test proves nothing"
    );
    assert!(
        searchable_path_guard(&catalog)
            .unwrap()
            .validate(std::path::Path::new(&canonical_file))
            .is_ok(),
        "while active, the file is inside the boundary"
    );

    sources
        .set_status(&source_id, SourceStatus::Paused)
        .unwrap();

    assert_eq!(
        candidates_at_every_site(&catalog, &chunk_id),
        (0, 0, 0, 0),
        "a paused source must contribute nothing at any retrieval site \
         (keyword, trigram, vector, enrichment)"
    );
    assert!(
        service.search("alpha", 10).unwrap().is_empty(),
        "a paused source must contribute no results (RFC-060 criterion 6)"
    );

    // The other half: even if a candidate reached the snippet path, the
    // boundary refuses to open anything under a paused source.
    let guard = searchable_path_guard(&catalog).unwrap();
    assert!(
        matches!(
            guard.validate(std::path::Path::new(&canonical_file)),
            Err(OrbokError::PathOutsideSources)
        ),
        "no file under a paused source may be opened during a search"
    );
}

/// Criterion 9: a path outside every registered source is an error, not
/// file contents.
#[test]
fn load_snippet_refuses_a_path_outside_every_registered_source() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog, dir.path());

    let stray = outside.path().join("elsewhere.md");
    std::fs::write(&stray, "secret text outside every source\n").unwrap();

    let snippets = crate::snippet::SnippetSource::new(&catalog, None).unwrap();
    let record = orbok_db::repo::ChunkRecord {
        chunk_id: ChunkId::generate(),
        file_id: orbok_core::FileId::generate(),
        chunk_ordinal: 0,
        heading_path: None,
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "exact".to_string(),
        location_kind: "lines".to_string(),
    };

    let result = snippets.load(&record, stray.to_str().unwrap());
    assert!(
        matches!(result, Err(OrbokError::PathOutsideSources)),
        "a path outside every source must be an error, never file contents -- got {result:?}"
    );
}
