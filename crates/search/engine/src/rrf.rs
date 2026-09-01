//! Reciprocal Rank Fusion (RFC-009 §7).
//!
//! Standard RRF formula: score(d) = Σ 1 / (k + rank_i(d))
//! Default k = 60 (validated in information-retrieval literature).

use orbok_core::{ChunkId, FileId};
use orbok_models::VectorCandidate;

use crate::KeywordCandidate;

/// One fused candidate carrying per-source ranks and the combined score.
#[derive(Debug, Clone)]
pub struct FusedCandidate {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub rrf_score: f64,
    pub keyword_rank: Option<u32>,
    pub vector_rank: Option<u32>,
}

/// RRF k constant (RFC-009 §7).
pub const RRF_K: f64 = 60.0;

/// Fuse keyword and vector candidates using Reciprocal Rank Fusion.
/// Returns candidates in descending RRF score order, deduplicated by
/// chunk_id (RFC-009 §9 deduplication).
pub fn rrf_fuse(
    keyword: &[KeywordCandidate],
    vector: &[VectorCandidate],
    limit: usize,
) -> Vec<FusedCandidate> {
    use std::collections::HashMap;
    let mut scores: HashMap<String, FusedCandidate> = HashMap::new();

    for kw in keyword {
        let key = kw.chunk_id.as_str().to_string();
        let entry = scores.entry(key).or_insert_with(|| FusedCandidate {
            chunk_id: kw.chunk_id.clone(),
            file_id: kw.file_id.clone(),
            rrf_score: 0.0,
            keyword_rank: None,
            vector_rank: None,
        });
        entry.rrf_score += 1.0 / (RRF_K + kw.rank as f64);
        entry.keyword_rank = Some(kw.rank);
    }

    for vc in vector {
        let key = vc.chunk_id.as_str().to_string();
        let entry = scores.entry(key).or_insert_with(|| FusedCandidate {
            chunk_id: vc.chunk_id.clone(),
            file_id: vc.file_id.clone(),
            rrf_score: 0.0,
            keyword_rank: None,
            vector_rank: None,
        });
        entry.rrf_score += 1.0 / (RRF_K + vc.rank as f64);
        entry.vector_rank = Some(vc.rank);
    }

    let mut fused: Vec<FusedCandidate> = scores.into_values().collect();
    // Task 034 §2 (F-10): `scores.into_values()` iterates a `HashMap`
    // (randomly seeded per instance), and a tied `rrf_score` is structural,
    // not rare -- rank 1/5 and rank 5/1 fuse to the same score. Without this
    // tie-break, `sort_by`'s stability only preserves whatever order the map
    // happened to yield, which changes between calls. `chunk_id` is a total
    // order already available on every candidate.
    fused.sort_by(|a, b| {
        b.rrf_score
            .partial_cmp(&a.rrf_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.as_str().cmp(b.chunk_id.as_str()))
    });
    fused.truncate(limit);
    fused
}

/// Fuse two ranked [`KeywordCandidate`] lists using the same Reciprocal Rank
/// Fusion as [`rrf_fuse`] (RFC-009 §7), for combining two keyword indexes
/// whose native scores are not on a comparable scale (Task 034 §1, audit
/// F-02b) -- e.g. FTS5 `bm25()` from `chunk_fts` (unicode61) against
/// `chunk_fts_trigram` (trigram): different indexes, vastly different term
/// counts and lengths, so their raw `bm25` values cannot be sorted against
/// each other meaningfully. RRF only consumes each list's own rank, never
/// the score, which is exactly what makes it valid across incomparable
/// scorers.
///
/// Returns candidates deduplicated by `chunk_id`, `.score` set to the fused
/// RRF score (not a `bm25` value -- callers must not treat it as one), and
/// `.rank` re-assigned 1-based in descending fused-score order.
pub fn rrf_fuse_keyword_lists(
    primary: &[KeywordCandidate],
    secondary: &[KeywordCandidate],
    limit: usize,
) -> Vec<KeywordCandidate> {
    use std::collections::HashMap;
    let mut scores: HashMap<String, (KeywordCandidate, f64)> = HashMap::new();

    for kw in primary.iter().chain(secondary.iter()) {
        let key = kw.chunk_id.as_str().to_string();
        let entry = scores.entry(key).or_insert_with(|| (kw.clone(), 0.0));
        entry.1 += 1.0 / (RRF_K + kw.rank as f64);
    }

    let mut fused: Vec<(KeywordCandidate, f64)> = scores.into_values().collect();
    fused.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .partial_cmp(a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.as_str().cmp(b.chunk_id.as_str()))
    });
    fused.truncate(limit);
    fused
        .into_iter()
        .enumerate()
        .map(|(i, (mut c, score))| {
            c.score = score;
            c.rank = (i + 1) as u32;
            c
        })
        .collect()
}
