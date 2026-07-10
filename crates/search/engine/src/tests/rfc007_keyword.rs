//! Keyword engine tests, validating RFC-007 acceptance criteria:
//! exact-term retrieval, identifiers, replace-on-reindex, deletion,
//! contentless privacy, and query-injection neutralization.

use crate::query::build_match_pair_expression;
use crate::{Fts5KeywordEngine, KeywordDocument, KeywordSearchEngine, build_match_expression};
use orbok_core::ChunkId;
use orbok_db::Catalog;
use rusqlite::params;

/// Insert the FK chain (source → file → extraction → chunk) directly;
/// the chunking stage that will do this for real lands in M5.
fn seed_chunk(catalog: &Catalog, ordinal: i64) -> ChunkId {
    let chunk_id = ChunkId::generate();
    let conn = catalog.lock();
    let t = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT OR IGNORE INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at) VALUES ('s1','directory','persistent','/d','/d','active','balanced',\
         'exclude','ignore',?1,?1)",
        params![t],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO files (file_id, source_id, original_path, canonical_path, \
         display_path, file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES ('f1','s1','/d/a.md','/d/a.md','a.md',1,'indexed',?1,?1,?1)",
        params![t],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO extraction_records (extraction_id, file_id, extractor_name, \
         extractor_version, normalization_version, status, created_at, updated_at) \
         VALUES ('e1','f1','text','v1','norm-v1','succeeded',?1,?1)",
        params![t],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
         chunk_status, created_at, updated_at) \
         VALUES (?1,'f1','e1','paragraph',?2,'active',?3,?3)",
        params![chunk_id.as_str(), ordinal, t],
    )
    .unwrap();
    chunk_id
}

fn doc(chunk_id: &ChunkId, text: &str) -> KeywordDocument {
    KeywordDocument {
        chunk_id: chunk_id.clone(),
        title: Some("a.md".into()),
        heading_path: None,
        normalized_text: text.into(),
    }
}

// RFC-007: exact terms and identifiers are searchable.
#[test]
fn exact_terms_and_identifiers_match() {
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    let c2 = seed_chunk(&catalog, 1);
    engine
        .index(&[
            doc(&c1, "refresh tokens should expire before ERR4042 occurs"),
            doc(&c2, "unrelated text about gardening"),
        ])
        .unwrap();

    let hits = engine.search("ERR4042", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].chunk_id, c1);
    assert_eq!(hits[0].rank, 1);
    assert_eq!(hits[0].file_id.as_str(), "f1");

    // Multi-term query is AND-ed.
    assert_eq!(engine.search("refresh tokens", 10).unwrap().len(), 1);
    assert!(engine.search("refresh gardening", 10).unwrap().is_empty());
}

// RFC-007 §8.1: the index is contentless — no stored text is readable.
#[test]
fn index_is_contentless() {
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    engine.index(&[doc(&c1, "confidential body text")]).unwrap();

    let conn = catalog.lock();
    // Retrieving column values from a contentless table yields NULL (or
    // errors); either way the text must not come back.
    let got: Option<String> = conn
        .query_row("SELECT normalized_text FROM chunk_fts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(None);
    assert!(
        got.is_none(),
        "contentless index must not return stored text"
    );
}

// Replace-on-reindex: the old token set stops matching.
#[test]
fn reindex_replaces_previous_tokens() {
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    engine.index(&[doc(&c1, "alpha bravo")]).unwrap();
    engine.index(&[doc(&c1, "charlie delta")]).unwrap();

    assert!(engine.search("alpha", 10).unwrap().is_empty());
    assert_eq!(engine.search("charlie", 10).unwrap().len(), 1);
}

// Deletion removes a chunk from retrieval (contentless_delete path).
#[test]
fn delete_removes_from_retrieval() {
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    engine.index(&[doc(&c1, "ephemeral text")]).unwrap();
    assert_eq!(engine.search("ephemeral", 10).unwrap().len(), 1);

    engine.delete(&[c1]).unwrap();
    assert!(engine.search("ephemeral", 10).unwrap().is_empty());
}

// RFC-015 §13: FTS5 operators in user input are neutralized.
#[test]
fn query_syntax_is_neutralized() {
    assert_eq!(
        build_match_expression("a OR b"),
        Some("\"a\" \"OR\" \"b\"".into())
    );
    assert_eq!(build_match_expression("  "), None);
    assert_eq!(
        build_match_expression("say \"hi\""),
        Some("\"say\" \"\"\"hi\"\"\"".into())
    );

    // And operators must not crash or widen retrieval at runtime.
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    engine.index(&[doc(&c1, "plain text")]).unwrap();
    assert!(engine.search("plain OR missing", 10).unwrap().is_empty());
    assert!(
        engine
            .search("title: plain NEAR(x y)", 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn long_auto_query_uses_safe_anchor_phrase() {
    assert_eq!(
        build_match_pair_expression("embedding model cosine similarity"),
        Some("\"embedding model\"".into())
    );
    assert_eq!(
        build_match_pair_expression("say \"hi\" then continue"),
        Some("\"say \"\"hi\"\"\"".into())
    );
    assert_eq!(
        build_match_pair_expression("source allowlist path"),
        build_match_expression("source allowlist path")
    );
}

// Stale chunks are filtered out of results (RFC-007 freshness rule).
#[test]
fn stale_chunks_are_excluded() {
    let catalog = Catalog::open_in_memory().unwrap();
    let engine = Fts5KeywordEngine::new(&catalog);
    let c1 = seed_chunk(&catalog, 0);
    engine.index(&[doc(&c1, "soon stale")]).unwrap();
    {
        let conn = catalog.lock();
        conn.execute(
            "UPDATE chunks SET chunk_status = 'stale' WHERE chunk_id = ?1",
            params![c1.as_str()],
        )
        .unwrap();
    }
    assert!(engine.search("stale", 10).unwrap().is_empty());
}
