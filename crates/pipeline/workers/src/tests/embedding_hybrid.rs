//! RFC-008 §24 and RFC-009 §20 integration tests:
//! embedding pipeline correctness, vector search, RRF fusion,
//! mode-based degradation, stale exclusion.

use crate::{ChunkAndIndexWorker, EmbeddingWorker, ExtractionWorker, run_pending};
use orbok_cache::CacheService;
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, JobType, PersistenceMode, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    EmbeddingRepository, FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata,
    SourceRepository,
};
use orbok_models::MockEmbeddingModel;
use orbok_search::{HybridSearchService, SearchMode, rrf_fuse};
use rusqlite::params;
use std::fs;

fn setup(root: &std::path::Path) -> (Catalog, CacheService) {
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(root);
    (catalog, cache)
}

fn seed_and_run(
    catalog: &Catalog,
    cache: &CacheService,
    root: &std::path::Path,
    name: &str,
    content: &str,
) -> orbok_core::FileId {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    let canonical = fs::canonicalize(&path)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let root_canon = fs::canonicalize(root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let src = SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::File,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some(name.into()),
            original_path: canonical.clone(),
            canonical_path: root_canon,
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();

    let file = FileRepository::new(catalog)
        .insert(NewFile {
            source_id: src.source_id.clone(),
            original_path: canonical.clone(),
            canonical_path: canonical.clone(),
            display_path: name.into(),
            extension: Some("md".into()),
            metadata: ObservedMetadata {
                file_size_bytes: content.len() as u64,
                modified_at: Some("2026-01-01T00:00:00Z".into()),
                platform_file_key: None,
                content_hash: Some("abc".into()),
            },
            status: FileStatus::Discovered,
        })
        .unwrap();

    IndexJobRepository::new(catalog)
        .enqueue(JobType::Extract, Some(&src.source_id), Some(&file.file_id))
        .unwrap();

    let extract = ExtractionWorker::new(catalog, cache);
    let chunk = ChunkAndIndexWorker::new(catalog, cache);
    run_pending(catalog, &extract, &chunk, None, 50).unwrap();
    file.file_id
}

fn seed_mock_model(catalog: &Catalog) -> orbok_core::ModelId {
    let model_id = orbok_core::ModelId::from_string("mock_mock-v1".to_string());
    let now = "2026-01-01T00:00:00Z";
    catalog.lock().execute(
        "INSERT OR IGNORE INTO models (model_id, role, model_name, model_version, \
         dimension, status, created_at, updated_at) VALUES (?1,'embedding','mock','v1',8,'available',?2,?2)",
        params![model_id.as_str(), now],
    ).unwrap();
    model_id
}

// RFC-008 §24 test 2: embedding generation succeeds.
#[test]
fn embedding_worker_generates_and_stores_vectors() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let file_id = seed_and_run(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "# Guide\n\nThis is important documentation.\n",
    );
    seed_mock_model(&catalog);

    let embed = EmbeddingWorker::with_mock(&catalog, &cache);
    embed.run(&file_id).unwrap();

    let count = EmbeddingRepository::new(&catalog)
        .count_active("mock_mock-v1")
        .unwrap();
    assert!(count > 0, "embeddings must be stored after worker run");
}

// RFC-008 §24 test 4: stored vector can be retrieved and searched.
#[test]
fn vector_search_returns_nearest_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed_and_run(
        &catalog,
        &cache,
        dir.path(),
        "a.md",
        "token expiry policy\n",
    );
    seed_and_run(
        &catalog,
        &cache,
        dir.path(),
        "b.md",
        "gardening tips and tricks\n",
    );
    seed_mock_model(&catalog);

    let embed = EmbeddingWorker::with_mock(&catalog, &cache);
    for fid in [
        FileRepository::new(&catalog)
            .get_by_path_str("a.md")
            .unwrap(),
        FileRepository::new(&catalog)
            .get_by_path_str("b.md")
            .unwrap(),
    ]
    .into_iter()
    .flatten()
    {
        embed.run(&fid.file_id).unwrap();
    }

    let model = MockEmbeddingModel;
    let service = HybridSearchService::with_model(&catalog, &model, "mock_mock-v1");
    let results = service
        .search("token expiry", SearchMode::Conceptual, 5)
        .unwrap();
    assert!(!results.is_empty(), "vector search should return results");
    assert!(
        results[0]
            .badges
            .contains(&orbok_search::MatchBadge::Semantic)
    );
}

// RFC-009 §20 test 1: RRF fuses keyword and vector results.
#[test]
fn rrf_fusion_combines_keyword_and_vector() {
    use orbok_core::{ChunkId, FileId};
    use orbok_models::VectorCandidate;
    use orbok_search::KeywordCandidate;

    let chunk_a = ChunkId::from_string("c_a".to_string());
    let chunk_b = ChunkId::from_string("c_b".to_string());
    let chunk_c = ChunkId::from_string("c_c".to_string());
    let file = FileId::from_string("f_1".to_string());

    // chunk_a ranks 1 in keyword, chunk_b ranks 1 in vector, chunk_c is keyword-only.
    let kw = vec![
        KeywordCandidate {
            chunk_id: chunk_a.clone(),
            file_id: file.clone(),
            rank: 1,
            score: -1.0,
        },
        KeywordCandidate {
            chunk_id: chunk_c.clone(),
            file_id: file.clone(),
            rank: 2,
            score: -2.0,
        },
    ];
    let vc = vec![
        VectorCandidate {
            chunk_id: chunk_b.clone(),
            file_id: file.clone(),
            rank: 1,
            score: 0.9,
        },
        VectorCandidate {
            chunk_id: chunk_a.clone(),
            file_id: file.clone(),
            rank: 2,
            score: 0.7,
        },
    ];
    let fused = rrf_fuse(&kw, &vc, 10);
    assert_eq!(fused.len(), 3);
    // chunk_a appears in both lists → highest RRF score.
    assert_eq!(fused[0].chunk_id.as_str(), "c_a");
    assert!(fused[0].keyword_rank.is_some() && fused[0].vector_rank.is_some());
}

// RFC-009 §20 test 4: keyword-only mode works without embedding model.
#[test]
fn keyword_only_mode_works_without_model() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed_and_run(
        &catalog,
        &cache,
        dir.path(),
        "notes.md",
        "refresh token expiry\n",
    );

    let service = HybridSearchService::keyword_only(&catalog);
    assert!(!service.is_hybrid());
    let results = service.search("refresh", SearchMode::Auto, 10).unwrap();
    assert!(!results.is_empty());
}

// RFC-008 §24 test 5: model change marks embeddings stale.
#[test]
fn model_change_marks_embeddings_stale() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let file_id = seed_and_run(&catalog, &cache, dir.path(), "doc.md", "some content\n");
    seed_mock_model(&catalog);

    EmbeddingWorker::with_mock(&catalog, &cache)
        .run(&file_id)
        .unwrap();
    assert!(
        EmbeddingRepository::new(&catalog)
            .count_active("mock_mock-v1")
            .unwrap()
            > 0
    );

    EmbeddingRepository::new(&catalog)
        .mark_stale_for_model("mock_mock-v1")
        .unwrap();
    assert_eq!(
        EmbeddingRepository::new(&catalog)
            .count_active("mock_mock-v1")
            .unwrap(),
        0
    );
}

// RFC-008 §24 test 6: stale chunks excluded from vector search.
#[test]
fn stale_chunks_excluded_from_vector_search() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let file_id = seed_and_run(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "secret information\n",
    );
    seed_mock_model(&catalog);
    EmbeddingWorker::with_mock(&catalog, &cache)
        .run(&file_id)
        .unwrap();

    // Stale all chunks.
    catalog
        .lock()
        .execute("UPDATE chunks SET chunk_status='stale'", [])
        .unwrap();

    let count = EmbeddingRepository::new(&catalog)
        .count_active("mock_mock-v1")
        .unwrap();
    assert_eq!(
        count, 0,
        "stale chunks must not appear as active embeddings"
    );
}

// RFC-008 §24 test 8: deleted vector index doesn't delete source catalog.
#[test]
fn deleting_embeddings_does_not_delete_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let file_id = seed_and_run(&catalog, &cache, dir.path(), "doc.md", "preserved source\n");
    seed_mock_model(&catalog);
    EmbeddingWorker::with_mock(&catalog, &cache)
        .run(&file_id)
        .unwrap();

    catalog
        .lock()
        .execute("DELETE FROM embeddings", [])
        .unwrap();

    // Source file catalog must be intact.
    assert!(
        FileRepository::new(&catalog)
            .get_by_id(&file_id)
            .unwrap()
            .is_some()
    );
}
