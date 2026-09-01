//! Task 034 §1/§2 (audit F-02, F-02b, F-10): CJK keyword merge must not sort
//! FTS5 `bm25()` descending (lower is better, RFC-007/`fts5.rs`), and must not
//! compare `bm25(chunk_fts)` against `bm25(chunk_fts_trigram)` directly --
//! different indexes, incomparable scales. And RRF fusion must be
//! deterministic under tied scores.

use crate::{KeywordSearchEngine, MultilingualKeywordEngine};
use orbok_core::{ChunkId, ExtractionId, FileId};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRepository, ChunkSpec};
use rusqlite::params;

fn seed_source_and_file(catalog: &Catalog, file_id: &str) -> (FileId, ExtractionId) {
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
    let path = format!("/d/{file_id}.md");
    conn.execute(
        "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
         display_path, file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES (?1,'s1',?2,?2,?2,1,'indexed',?3,?3,?3)",
        params![file_id, path, t],
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

fn spec(text: &str) -> ChunkSpec {
    ChunkSpec {
        chunk_kind: "paragraph",
        chunk_ordinal: 0,
        heading_path: None,
        title: None,
        normalized_text: text.to_string(),
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "exact",
        parent_idx: None,
    }
}

/// Task 034 §1 (F-02/F-02b): a short, dense chunk containing the query term
/// must outrank a long chunk that mentions it once, buried in filler --
/// exactly what bm25 rewards, and exactly what a reversed/incomparable-scale
/// sort inverts or scrambles. Real FTS5 data through the real
/// `ChunkRepository::insert_bundle` path (both `chunk_fts` and
/// `chunk_fts_trigram`), not a synthetic score.
#[test]
fn cjk_merge_ranks_the_dense_relevant_chunk_first() {
    let catalog = Catalog::open_in_memory().unwrap();

    // unicode61 tokenizes a whole punctuation-delimited CJK run as ONE
    // token (RFC-014 §9), so the query phrase must be its own isolated
    // token in both chunks to be found via the same index -- otherwise
    // this would be testing trigram-vs-unicode61 incomparability (F-02b)
    // rather than the sort direction (F-02) in isolation. Verified
    // empirically (sqlite3 CLI against the real chunk_fts DDL) that this
    // exact phrasing produces bm25(short) = -1.3253e-6 (better) and
    // bm25(long) = -8.0292e-7 (worse) -- lower is better, so "short" must
    // rank first once the sort direction and fusion are both correct.
    let (short_file, short_extraction) = seed_source_and_file(&catalog, "f-short");
    ChunkRepository::new(&catalog)
        .insert_bundle(&short_file, &short_extraction, &[spec("認証エラー")])
        .unwrap();

    let filler = "今日は天気がとても良いので散歩に出かけました。".repeat(2);
    let long_text = format!("{filler}認証エラー。{filler}");
    let (long_file, long_extraction) = seed_source_and_file(&catalog, "f-long");
    ChunkRepository::new(&catalog)
        .insert_bundle(&long_file, &long_extraction, &[spec(&long_text)])
        .unwrap();

    let engine = MultilingualKeywordEngine::new(&catalog);
    let hits = engine.search("認証エラー", 10).unwrap();

    assert_eq!(hits.len(), 2, "both chunks contain the query term");
    assert_eq!(
        hits[0].file_id.as_str(),
        "f-short",
        "the short, dense chunk must rank first -- got order {:?}",
        hits.iter().map(|h| h.file_id.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(hits[0].rank, 1);
    assert_eq!(hits[1].rank, 2);
}

/// Task 034 §2 (F-10): `rrf_fuse_keyword_lists` -- the function §1's fix put
/// both `multilingual.rs` merge sites onto -- must not depend on `HashMap`
/// iteration order under a structural tie (chunk_id "a" at primary rank 1 /
/// secondary rank 5 fuses to the same score as "b" at primary rank 5 /
/// secondary rank 1). A same-content-through-the-real-engine version of this
/// test does not actually exercise the defect: two chunks with identical
/// text get *consistent*, not tied, cross-source ranks (unicode61 and
/// trigram agree on row order for identical rows), so it passes whether or
/// not the tie-break exists. This constructs the tie directly instead.
#[test]
fn rrf_fuse_keyword_lists_is_deterministic_under_structural_ties() {
    use crate::rrf::rrf_fuse_keyword_lists;

    let mk = |id: &str, rank: u32| crate::KeywordCandidate {
        chunk_id: ChunkId::from_string(id.to_string()),
        file_id: FileId::from_string(id.to_string()),
        rank,
        score: 0.0,
    };

    let primary = vec![mk("a", 1), mk("b", 5)];
    let secondary = vec![mk("a", 5), mk("b", 1)];

    let mut orderings = std::collections::HashSet::new();
    for _ in 0..20 {
        let fused = rrf_fuse_keyword_lists(&primary, &secondary, 10);
        let order: Vec<String> = fused
            .iter()
            .map(|c| c.chunk_id.as_str().to_string())
            .collect();
        orderings.insert(order);
    }
    assert_eq!(
        orderings.len(),
        1,
        "structurally tied rrf_fuse_keyword_lists output must be deterministic, got {} distinct orderings: {orderings:?}",
        orderings.len()
    );
}

/// Task 034 §2 (F-10): `rrf_fuse` itself, not just the CJK merge path that
/// wraps it -- the audit's own repro shape (keyword+vector fusion with a
/// structural tie: rank 1/5 vs rank 5/1 score identically).
#[test]
fn rrf_fuse_is_deterministic_under_structural_ties() {
    use crate::KeywordCandidate;
    use crate::rrf::rrf_fuse;
    use orbok_models::VectorCandidate;

    let mk_kw = |id: &str, rank: u32| KeywordCandidate {
        chunk_id: ChunkId::from_string(id.to_string()),
        file_id: FileId::from_string(id.to_string()),
        rank,
        score: 0.0,
    };
    let mk_vec = |id: &str, rank: u32| VectorCandidate {
        chunk_id: ChunkId::from_string(id.to_string()),
        file_id: FileId::from_string(id.to_string()),
        rank,
        score: 0.0,
    };

    // "c1" at keyword rank 1 / vector rank 5; "c2" at keyword rank 5 / vector
    // rank 1 -- structurally identical rrf_score (1/(60+1) + 1/(60+5) either
    // way), so only a tie-break can make the order stable.
    let keyword = vec![mk_kw("c1", 1), mk_kw("c2", 5)];
    let vector = vec![mk_vec("c1", 5), mk_vec("c2", 1)];

    let mut orderings = std::collections::HashSet::new();
    for _ in 0..20 {
        let fused = rrf_fuse(&keyword, &vector, 10);
        let order: Vec<String> = fused
            .iter()
            .map(|f| f.chunk_id.as_str().to_string())
            .collect();
        orderings.insert(order);
    }
    assert_eq!(
        orderings.len(),
        1,
        "structurally tied rrf_fuse output must be deterministic, got {} distinct orderings: {orderings:?}",
        orderings.len()
    );
}

/// Task 034 §11 Q1: confirm, rather than assume, that switching the CJK
/// merge to `rrf_fuse_keyword_lists` does not change ordering for pure-ASCII
/// queries. It must not: `contains_cjk` gates the fusion branch entirely, so
/// an ASCII query never leaves `search_with_exact_terms`'s first
/// `Fts5KeywordEngine::search` call -- untouched by this task -- and keeps
/// unicode61's own ascending-bm25 (lower-is-better) order.
#[test]
fn ascii_query_ordering_is_unchanged_by_the_cjk_fusion_fix() {
    let catalog = Catalog::open_in_memory().unwrap();

    let (short_file, short_extraction) = seed_source_and_file(&catalog, "f-short");
    ChunkRepository::new(&catalog)
        .insert_bundle(&short_file, &short_extraction, &[spec("timeout error")])
        .unwrap();

    let filler = "the quick brown fox jumps over the lazy dog ".repeat(20);
    let long_text = format!("{filler} timeout error {filler}");
    let (long_file, long_extraction) = seed_source_and_file(&catalog, "f-long");
    ChunkRepository::new(&catalog)
        .insert_bundle(&long_file, &long_extraction, &[spec(&long_text)])
        .unwrap();

    let engine = MultilingualKeywordEngine::new(&catalog);
    let hits = engine.search("timeout error", 10).unwrap();

    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0].file_id.as_str(),
        "f-short",
        "ASCII query ordering must stay bm25-ascending (lower/better first) -- got {:?}",
        hits.iter().map(|h| h.file_id.as_str()).collect::<Vec<_>>()
    );
}
