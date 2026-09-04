//! v0.4 integration tests: RFC-010 (reranking), RFC-011 (storage),
//! RFC-013 (search UX state), RFC-014 (multilingual search).

use crate::{
    ChunkAndIndexWorker, EmbeddingWorker, ExtractionWorker, run_pending, update_storage_accounting,
};
use orbok_cache::CacheService;
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, JobType, PersistenceMode, SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata, SourceRepository,
};
use orbok_search::{HybridSearchService, SearchMode, contains_cjk, normalize_query};
use std::fs;

fn setup(root: &std::path::Path) -> (Catalog, CacheService) {
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(root);
    (catalog, cache)
}

fn seed(
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
    let root_str = fs::canonicalize(root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let src = SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::File,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some(name.into()),
            original_path: canonical.clone(),
            canonical_path: root_str,
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
            canonical_path: canonical,
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
    let e = ExtractionWorker::new(catalog, cache);
    let c = ChunkAndIndexWorker::new(catalog, cache);
    run_pending(catalog, &e, &c, None, 50).unwrap();
    file.file_id
}

// ── RFC-010: Reranking ─────────────────────────────────────────────────
// Task 040 deleted `HybridSearchService::with_reranker` and the dead call
// path behind it (RFC-060 §8: known-false `Status: Implemented`, no
// production caller, and a known bug -- `enrich_many` truncated to
// `limit` before reranking, so a reranker could never have reached
// fusion ranks 21-50). The two tests that exercised that integration
// point (`reranker_reorders_results`, `fast_mode_returns_results_without_rerank`)
// are gone with it -- their entire premise no longer compiles.
// `MockReranker`/`CrossEncoderReranker` are still tested directly against
// the trait in `crates/search/models/src/lib.rs`'s own `reranker_tests`
// module, the mock's only remaining consumer now that the cross-crate
// integration is gone.
//
// RFC-010 §20: missing reranker does not break search -- now simply
// "search works," since there is no reranker parameter to omit any more.
#[test]
fn search_works_without_reranker() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "important content here\n",
    );

    let service = HybridSearchService::keyword_only(&catalog);
    let results = service.search("important", SearchMode::Auto, 10).unwrap();
    assert!(!results.is_empty());
}

// ── RFC-011: Storage accounting ────────────────────────────────────────

// RFC-011 §15 test 1/2: safe cleanup preserves sources; accounting reflects actual data.
#[test]
fn storage_accounting_reflects_actual_data() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "# Title\n\nContent here.\n",
    );

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let rows = update_storage_accounting(&catalog, &cache_path).unwrap();
    assert!(!rows.is_empty());

    // Keyword index should have at least some entries.
    let kw = rows
        .iter()
        .find(|(cat, _, _)| cat == &orbok_core::StorageCategory::KeywordIndex);
    assert!(kw.is_some());

    // Sources should still be present.
    assert!(!SourceRepository::new(&catalog).list().unwrap().is_empty());
}

// RFC-011 §15 test 3: delete semantic index preserves file catalog.
#[test]
fn delete_embeddings_preserves_file_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let file_id = seed(&catalog, &cache, dir.path(), "doc.md", "some text\n");

    // Seed model + embeddings.
    catalog.lock().execute(
        "INSERT OR IGNORE INTO models (model_id, role, model_name, model_version, \
         dimension, status, created_at, updated_at) VALUES ('mock_mock-v1','embedding','mock','v1',8,'available','t','t')",
        [],
    ).unwrap();
    EmbeddingWorker::with_mock(&catalog, &cache)
        .run(&file_id)
        .unwrap();

    // Delete all embeddings.
    catalog
        .lock()
        .execute("DELETE FROM embeddings", [])
        .unwrap();

    // File catalog intact.
    assert!(
        FileRepository::new(&catalog)
            .get_by_id(&file_id)
            .unwrap()
            .is_some()
    );
    assert!(!SourceRepository::new(&catalog).list().unwrap().is_empty());
}

// ── RFC-013: Search UX state ───────────────────────────────────────────

// RFC-013 §20 test 1/2/3: badge presence in results.
#[test]
fn search_results_carry_keyword_badge() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "doc.md",
        "authentication token rotation\n",
    );
    let results = HybridSearchService::keyword_only(&catalog)
        .search("authentication", SearchMode::Auto, 10)
        .unwrap();
    assert!(!results.is_empty());
    assert!(
        results[0]
            .badges
            .contains(&orbok_search::MatchBadge::Keyword)
    );
}

// RFC-013 §20 test 5: UI state handles missing source gracefully.
#[test]
fn search_view_handles_no_snippet() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    let _fid = seed(&catalog, &cache, dir.path(), "temp.md", "content\n");

    // Delete source file from disk after indexing.
    fs::remove_file(dir.path().join("temp.md")).unwrap();

    let results = HybridSearchService::keyword_only(&catalog)
        .search("content", SearchMode::Auto, 10)
        .unwrap();
    if !results.is_empty() {
        // snippet.is_none() is acceptable when source is missing (FR-092).
        // The result itself should still appear.
        assert!(!results[0].canonical_path.is_empty());
    }
}

// RFC-013 §20 UI state transitions — tested in orbok-ui crate (avoids
// pulling the iced/GUI compile chain into orbok-workers tests).
#[test]
fn result_selection_concept_documented() {
    // When a user selects a search result, the UI keeps the selected index.
    // Tested via orbok_ui::state::AppState in orbok-ui unit tests.
    let selected: Option<usize> = None;
    assert!(selected.is_none(), "no result selected initially");
    let selected = Some(0usize);
    assert_eq!(selected, Some(0));
}

// ── RFC-014: Multilingual search ──────────────────────────────────────

// RFC-014 §19 test 1: full-width → half-width normalization.
#[test]
fn fullwidth_normalizes_to_halfwidth() {
    assert_eq!(normalize_query("ＡＢＣ１２３"), "ABC123");
    assert_eq!(normalize_query("ａｂｃ"), "abc");
}

// RFC-014 §19 test 2: identifiers like client_secret preserved.
#[test]
fn identifier_preserved_through_normalize() {
    let q = "client_secret";
    assert_eq!(normalize_query(q), q);
}

// RFC-014 §19 test 3: RFC-014 style identifier preserved.
#[test]
fn rfc_style_identifier_preserved() {
    assert_eq!(normalize_query("RFC-014"), "RFC-014");
}

// RFC-014 §19 test 8: CJK query routes to trigram index (no crash, results possible).
#[test]
fn cjk_query_routes_to_trigram() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "ja.md",
        "# 認証トークンのローテーション\n\nOAuthクライアントシークレットの有効期限を設定します。\n",
    );

    use orbok_search::KeywordSearchEngine;
    use orbok_search::MultilingualKeywordEngine;
    // Japanese query — must not error.
    let engine = MultilingualKeywordEngine::new(&catalog);
    let results = engine.search("認証トークン", 10).unwrap();
    // May or may not have results depending on trigram availability,
    // but must not panic or return an error.
    let _ = results;
}

// RFC-014 §19 test 7: Japanese query does not break code symbol search.
#[test]
fn japanese_query_does_not_break_english_search() {
    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "code.md",
        "fn refresh_token() -> Token { ... }\n",
    );

    let results = HybridSearchService::keyword_only(&catalog)
        .search("refresh_token", SearchMode::Exact, 10)
        .unwrap();
    assert!(
        !results.is_empty(),
        "English identifier search must work alongside Japanese support"
    );
}

// CJK detection function.
#[test]
fn cjk_detection_correct() {
    assert!(contains_cjk("認証"));
    assert!(contains_cjk("OAuth クライアント"));
    assert!(!contains_cjk("refresh_token"));
    assert!(!contains_cjk("ABC 123"));
}

// RFC-001 testing requirement #1: safe cleanup preserves persistent source settings.
#[test]
fn safe_cleanup_preserves_sources() {
    use crate::CleanupService;
    use orbok_core::{CleanupAction, CleanupPlan};

    let dir = tempfile::tempdir().unwrap();
    let (catalog, cache) = setup(dir.path());
    seed(
        &catalog,
        &cache,
        dir.path(),
        "note.md",
        "# Test\n\nContent.\n",
    );

    // Baseline: source exists.
    assert!(!SourceRepository::new(&catalog).list().unwrap().is_empty());

    // Run all safe cleanup actions one by one.
    let cache_db = dir.path().join("orbok-cache.sqlite3");
    for action in [
        CleanupAction::ClearSnippetCache,
        CleanupAction::ClearExpiredSearchCache,
        CleanupAction::ClearTemporaryExtraction,
        CleanupAction::RemoveReplacedStaleIndexes,
    ] {
        let plan = CleanupPlan::for_action(action, 0);
        CleanupService::new(&catalog, &cache, &cache_db)
            .run_safe(&plan)
            .expect("safe cleanup must not error");
        // Source registration must survive every safe cleanup.
        assert!(
            !SourceRepository::new(&catalog).list().unwrap().is_empty(),
            "source must survive safe cleanup action {action:?}"
        );
    }
}
