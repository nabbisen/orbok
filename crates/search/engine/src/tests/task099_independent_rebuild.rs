//! Task 099 §2.7: deleting one index leaves the other working. Real
//! chunk/keyword/vector data through the real repositories -- not
//! assumed from the deletion SQL alone.

use crate::vector::ExactVectorSearch;
use crate::{KeywordSearchEngine, MultilingualKeywordEngine};
use orbok_core::{CleanupAction, CleanupPlan, ExtractionId, FileId, ModelId, SearchScope};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRepository, ChunkSpec, CleanupExecutor};
use rusqlite::params;

const DIMENSION: u32 = 4;

fn seed_source_and_file(catalog: &Catalog, file_id: &str) -> (FileId, ExtractionId) {
    let conn = catalog.lock();
    let t = "2026-09-23T00:00:00Z";
    conn.execute(
        "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at) VALUES ('s1','directory','persistent','/d','/d','active','balanced',\
         'exclude','ignore',?1,?1)",
        params![t],
    )
    .unwrap();
    let path = format!("/d/{file_id}.md");
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
         display_path, file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES (?1,'s1',?2,?2,?2,1,'indexed',?3,?3,?3)",
        params![file_id, path, t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO models (model_id, role, model_name, model_version, dimension, status, \
         created_at, updated_at) VALUES ('m','embedding','mock','v1',?1,'available',?2,?2)",
        params![DIMENSION, t],
    )
    .unwrap();
    let extraction_id = format!("e-{file_id}");
    conn.execute(
        "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
         extractor_version, normalization_version, status, created_at, updated_at) \
         VALUES (?1,?2,'text','v1','norm-v1','succeeded',?3,?3)",
        params![extraction_id, file_id, t],
    )
    .unwrap();
    (
        FileId::from_string(file_id.to_string()),
        ExtractionId::from_string(extraction_id),
    )
}

fn seed_embedding(catalog: &Catalog, chunk_id: &str, vector: &[f32]) {
    let conn = catalog.lock();
    let t = "2026-09-23T00:00:00Z";
    let blob: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
    conn.execute(
        "INSERT INTO embeddings (embedding_id, chunk_id, model_id, vector_format, dimension, \
         norm, storage_location, vector_blob, status, created_at, updated_at) \
         VALUES (?1,?2,'m','fp32',?3,'none','sqlite_blob',?4,'active',?5,?5)",
        params![format!("emb-{chunk_id}"), chunk_id, DIMENSION, blob, t],
    )
    .unwrap();
}

fn vector_search(catalog: &Catalog, query_vec: &[f32]) -> usize {
    ExactVectorSearch {
        catalog,
        model_id: "m".into(),
        dimension: DIMENSION,
        scope: SearchScope::default(),
    }
    .search(query_vec, 10)
    .unwrap()
    .len()
}

fn keyword_search(catalog: &Catalog, query: &str) -> usize {
    MultilingualKeywordEngine::new(catalog)
        .search(query, 10)
        .unwrap()
        .len()
}

/// §2.7: deleting the vector index leaves keyword search working, and the
/// vector search genuinely returns nothing afterward (not just "the test
/// didn't check").
#[test]
fn deleting_the_vector_index_leaves_keyword_search_working() {
    let catalog = Catalog::open_in_memory().unwrap();
    let (file_id, extraction_id) = seed_source_and_file(&catalog, "f1");
    let spec = ChunkSpec {
        chunk_kind: "paragraph",
        chunk_ordinal: 0,
        heading_path: None,
        title: None,
        normalized_text: "orbok searches your documents locally".to_string(),
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "exact",
        location_kind: "lines",
        parent_idx: None,
    };
    let inserted = ChunkRepository::new(&catalog)
        .insert_bundle(&file_id, &extraction_id, &[spec])
        .unwrap();
    let real_chunk_id = inserted[0].chunk_id.as_str().to_string();
    let query_vec = [1.0, 0.0, 0.0, 0.0];
    seed_embedding(&catalog, &real_chunk_id, &query_vec);

    assert_eq!(
        keyword_search(&catalog, "orbok"),
        1,
        "keyword search must find the seeded chunk"
    );
    assert_eq!(
        vector_search(&catalog, &query_vec),
        1,
        "vector search must find the seeded chunk"
    );

    let model_id = ModelId::from_string("m".to_string());
    let plan = CleanupPlan::for_action(CleanupAction::DeleteVectorIndex, 0).with_model(model_id);
    CleanupExecutor::new(&catalog).run_safe(&plan).unwrap();

    assert_eq!(
        keyword_search(&catalog, "orbok"),
        1,
        "deleting the vector index must not break keyword search"
    );
    assert_eq!(
        vector_search(&catalog, &query_vec),
        0,
        "the vector index really is gone"
    );
}

/// §2.7, the other direction: deleting the keyword index leaves vector
/// search working, and the keyword search genuinely returns nothing
/// afterward.
#[test]
fn deleting_the_keyword_index_leaves_vector_search_working() {
    let catalog = Catalog::open_in_memory().unwrap();
    let (file_id, extraction_id) = seed_source_and_file(&catalog, "f1");
    let spec = ChunkSpec {
        chunk_kind: "paragraph",
        chunk_ordinal: 0,
        heading_path: None,
        title: None,
        normalized_text: "orbok searches your documents locally".to_string(),
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "exact",
        location_kind: "lines",
        parent_idx: None,
    };
    let inserted = ChunkRepository::new(&catalog)
        .insert_bundle(&file_id, &extraction_id, &[spec])
        .unwrap();
    let real_chunk_id = inserted[0].chunk_id.as_str().to_string();
    let query_vec = [1.0, 0.0, 0.0, 0.0];
    seed_embedding(&catalog, &real_chunk_id, &query_vec);

    assert_eq!(keyword_search(&catalog, "orbok"), 1);
    assert_eq!(vector_search(&catalog, &query_vec), 1);

    let plan = CleanupPlan::for_action(CleanupAction::DeleteKeywordIndex, 0);
    CleanupExecutor::new(&catalog).run_safe(&plan).unwrap();

    assert_eq!(
        keyword_search(&catalog, "orbok"),
        0,
        "the keyword index really is gone"
    );
    assert_eq!(
        vector_search(&catalog, &query_vec),
        1,
        "deleting the keyword index must not break vector search"
    );
}
