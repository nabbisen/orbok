//! Hybrid search service (RFC-009): combines keyword and vector
//! retrieval through RRF fusion. Degrades gracefully when either source
//! is unavailable (RFC-009 §21).

use crate::KeywordSearchEngine;
use crate::multilingual::MultilingualKeywordEngine;
use crate::rrf::{FusedCandidate, rrf_fuse};
use crate::service::{MatchBadge, SearchResult};
use crate::snippet::{chunk_records_for, load_snippet};
use crate::vector::ExactVectorSearch;
use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_models::{CrossEncoderReranker, EmbeddingModel, RerankCandidate, l2_normalize};
use std::path::Path;
use std::time::Instant;

/// Search mode selector (RFC-009 §8, GUI design §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Keyword + vector, RRF fused.
    #[default]
    Auto,
    /// Keyword-first; vector disabled.
    Exact,
    /// Vector-first; keyword disabled.
    Conceptual,
    /// Reduced candidate counts; no reranking.
    Fast,
}

/// Candidate limits per mode (RFC-009 §17).
struct Limits {
    keyword_k: u32,
    vector_k: u32,
    fusion_n: usize,
    rerank: bool,
}

impl Limits {
    fn for_mode(mode: SearchMode) -> Self {
        match mode {
            SearchMode::Auto => Limits {
                keyword_k: 100,
                vector_k: 100,
                fusion_n: 50,
                rerank: true,
            },
            SearchMode::Exact => Limits {
                keyword_k: 100,
                vector_k: 0,
                fusion_n: 50,
                rerank: false,
            },
            SearchMode::Conceptual => Limits {
                keyword_k: 0,
                vector_k: 100,
                fusion_n: 50,
                rerank: true,
            },
            SearchMode::Fast => Limits {
                keyword_k: 50,
                vector_k: 50,
                fusion_n: 20,
                rerank: false,
            },
        }
    }

    fn adjust_for_request(
        &mut self,
        requested_limit: u32,
        has_embedding_model: bool,
        has_reranker: bool,
    ) {
        let requested_limit = requested_limit.max(1);
        if !has_embedding_model {
            // Without a vector source, RRF preserves keyword order. Avoid the
            // fixed 100-candidate query cost for small result sets.
            let keyword_cap = requested_limit;
            self.keyword_k = self.keyword_k.min(keyword_cap);
            self.vector_k = 0;
            self.fusion_n = self.fusion_n.min(keyword_cap as usize);
            self.rerank &= has_reranker;
        }
    }
}

/// Hybrid search service. Optional embedding model and reranker both
/// degrade gracefully when absent (RFC-009 §21, RFC-010 §20).
pub struct HybridSearchService<'a> {
    catalog: &'a Catalog,
    embedding_model: Option<(&'a dyn EmbeddingModel, String)>,
    reranker: Option<&'a dyn CrossEncoderReranker>,
}

/// Timing breakdown for one search execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchTiming {
    pub total_ms: f64,
    pub keyword_ms: f64,
    pub query_embedding_ms: f64,
    pub vector_scan_ms: f64,
    pub fusion_ms: f64,
    pub enrichment_ms: f64,
    pub rerank_ms: f64,
}

/// Search results plus timing evidence for benchmark diagnostics.
#[derive(Debug)]
pub struct SearchProfile {
    pub results: Vec<SearchResult>,
    pub timing: SearchTiming,
}

impl<'a> HybridSearchService<'a> {
    /// Keyword-only mode (no embedding model).
    pub fn keyword_only(catalog: &'a Catalog) -> Self {
        Self {
            catalog,
            embedding_model: None,
            reranker: None,
        }
    }

    /// Hybrid mode with an embedding model.
    pub fn with_model(catalog: &'a Catalog, model: &'a dyn EmbeddingModel, model_id: &str) -> Self {
        Self {
            catalog,
            embedding_model: Some((model, model_id.to_string())),
            reranker: None,
        }
    }

    /// Add optional local reranker (RFC-010).
    pub fn with_reranker(mut self, reranker: &'a dyn CrossEncoderReranker) -> Self {
        self.reranker = Some(reranker);
        self
    }

    pub fn is_hybrid(&self) -> bool {
        self.embedding_model.is_some()
    }

    pub fn has_reranker(&self) -> bool {
        self.reranker.is_some()
    }

    /// Execute a search and return enriched, optionally reranked results.
    pub fn search(
        &self,
        query: &str,
        mode: SearchMode,
        limit: u32,
    ) -> OrbokResult<Vec<SearchResult>> {
        Ok(self.search_profile(query, mode, limit)?.results)
    }

    /// Execute a search and return timing evidence for benchmark diagnostics.
    pub fn search_profile(
        &self,
        query: &str,
        mode: SearchMode,
        limit: u32,
    ) -> OrbokResult<SearchProfile> {
        let total_start = Instant::now();
        let mut timing = SearchTiming::default();
        let mut limits = Limits::for_mode(mode);
        limits.adjust_for_request(
            limit,
            self.embedding_model.is_some(),
            self.reranker.is_some(),
        );

        // Keyword candidates — use multilingual engine (RFC-014).
        let keyword_start = Instant::now();
        let kw_candidates = if limits.keyword_k > 0 {
            let keyword_engine = MultilingualKeywordEngine::new(self.catalog);
            if mode == SearchMode::Auto && query.split_whitespace().count() >= 4 {
                keyword_engine.search_pairs(query, limits.keyword_k)?
            } else {
                keyword_engine.search(query, limits.keyword_k)?
            }
        } else {
            Vec::new()
        };
        timing.keyword_ms = elapsed_ms(keyword_start);

        // Vector candidates.
        let vec_candidates = if limits.vector_k > 0 {
            if let Some((model, model_id)) = &self.embedding_model {
                let query_embedding_start = Instant::now();
                let mut query_vec = model.embed_batch(&[query])?.remove(0);
                l2_normalize(&mut query_vec);
                timing.query_embedding_ms = elapsed_ms(query_embedding_start);

                let vector_scan_start = Instant::now();
                ExactVectorSearch {
                    catalog: self.catalog,
                    model_id: model_id.clone(),
                    dimension: model.dimension(),
                }
                .search(&query_vec, limits.vector_k)
                .inspect(|_| {
                    timing.vector_scan_ms = elapsed_ms(vector_scan_start);
                })?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Fuse.
        let fusion_start = Instant::now();
        let fused = rrf_fuse(&kw_candidates, &vec_candidates, limits.fusion_n);
        timing.fusion_ms = elapsed_ms(fusion_start);

        // Enrich with snippets.
        let enrichment_start = Instant::now();
        let mut results = self.enrich_many(&fused, limit as usize)?;
        timing.enrichment_ms = elapsed_ms(enrichment_start);

        // Optional reranking (RFC-010): reorder using passage scores.
        let rerank_start = Instant::now();
        if limits.rerank {
            if let Some(reranker) = self.reranker {
                results = rerank_results(reranker, query, results)?;
            }
        }
        timing.rerank_ms = elapsed_ms(rerank_start);
        timing.total_ms = elapsed_ms(total_start);

        Ok(SearchProfile { results, timing })
    }

    fn enrich_many(
        &self,
        candidates: &[FusedCandidate],
        limit: usize,
    ) -> OrbokResult<Vec<SearchResult>> {
        let top_candidates: Vec<&FusedCandidate> = candidates.iter().take(limit).collect();
        let chunk_ids: Vec<_> = top_candidates
            .iter()
            .map(|candidate| candidate.chunk_id.clone())
            .collect();
        let records = chunk_records_for(self.catalog, &chunk_ids)?;

        let mut results = Vec::with_capacity(top_candidates.len());
        for candidate in top_candidates {
            let Some((chunk, canonical_path)) = records.get(candidate.chunk_id.as_str()) else {
                continue;
            };
            let snippet = load_snippet(chunk, canonical_path);
            let display_path = short_display_path(canonical_path);
            let title = chunk.heading_path.clone().or_else(|| {
                Path::new(canonical_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            });
            let mut badges = Vec::new();
            if candidate.keyword_rank.is_some() {
                badges.push(MatchBadge::Keyword);
            }
            if candidate.vector_rank.is_some() {
                badges.push(MatchBadge::Semantic);
            }
            results.push(SearchResult {
                chunk_id: candidate.chunk_id.clone(),
                file_id: candidate.file_id.clone(),
                canonical_path: canonical_path.clone(),
                display_path,
                title,
                heading_path: chunk.heading_path.clone(),
                snippet,
                keyword_rank: candidate.keyword_rank.unwrap_or(0),
                keyword_score: 0.0,
                badges,
            });
        }
        Ok(results)
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Rerank enriched results using the reranker model (RFC-010 §8).
fn rerank_results(
    reranker: &dyn CrossEncoderReranker,
    query: &str,
    mut results: Vec<SearchResult>,
) -> OrbokResult<Vec<SearchResult>> {
    let top_n = reranker.max_candidates() as usize;
    let to_rerank = results.len().min(top_n);
    let candidates: Vec<RerankCandidate> = results[..to_rerank]
        .iter()
        .map(|r| RerankCandidate {
            chunk_id: r.chunk_id.clone(),
            passage_text: r.snippet.clone().unwrap_or_default(),
        })
        .collect();
    let scores = reranker.rerank(query, &candidates)?;
    // Map scores back to results by chunk_id.
    for result in results[..to_rerank].iter_mut() {
        if let Some(score) = scores.iter().find(|s| s.chunk_id == result.chunk_id) {
            result.keyword_score = score.score as f64;
        }
    }
    results[..to_rerank].sort_by(|a, b| {
        b.keyword_score
            .partial_cmp(&a.keyword_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

fn short_display_path(path: &str) -> String {
    let p = Path::new(path);
    let parts: Vec<_> = p.components().collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    let tail: std::path::PathBuf = parts[parts.len() - 2..].iter().collect();
    format!("…/{}", tail.display())
}
