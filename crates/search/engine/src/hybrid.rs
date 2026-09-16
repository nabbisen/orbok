//! Hybrid search service (RFC-009): combines keyword and vector
//! retrieval through RRF fusion. Degrades gracefully when either source
//! is unavailable (RFC-009 §21).

use crate::multilingual::MultilingualKeywordEngine;
use crate::rrf::{FusedCandidate, rrf_fuse};
use crate::service::{MatchBadge, SearchResult};
use crate::snippet::{SnippetSource, chunk_records_for, trust_for};
use crate::vector::ExactVectorSearch;
use orbok_core::{OrbokResult, SearchScope};
use orbok_db::Catalog;
use orbok_models::{EmbeddingModel, l2_normalize};
use std::collections::HashMap;
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
    /// Reduced candidate counts.
    Fast,
}

/// Candidate limits per mode (RFC-009 §17).
struct Limits {
    keyword_k: u32,
    vector_k: u32,
    // RFC-010 §9's reranking headroom: fusion keeps ranks 1-50 so a
    // reranker can promote a candidate from 21-50 into the visible top
    // results. No reranker is wired today (Task 040 deleted the dead
    // `with_reranker` call path -- RFC-010's trait is kept as a seam,
    // `crates/search/models/src/lib.rs::CrossEncoderReranker`), but the
    // headroom this constant reserves is still the right fusion width on
    // its own terms, so it is unchanged.
    fusion_n: usize,
}

impl Limits {
    fn for_mode(mode: SearchMode) -> Self {
        match mode {
            SearchMode::Auto => Limits {
                keyword_k: 100,
                vector_k: 100,
                fusion_n: 50,
            },
            SearchMode::Exact => Limits {
                keyword_k: 100,
                vector_k: 0,
                fusion_n: 50,
            },
            SearchMode::Conceptual => Limits {
                keyword_k: 0,
                vector_k: 100,
                fusion_n: 50,
            },
            SearchMode::Fast => Limits {
                keyword_k: 50,
                vector_k: 50,
                fusion_n: 20,
            },
        }
    }

    fn adjust_for_request(&mut self, requested_limit: u32, has_embedding_model: bool) {
        let requested_limit = requested_limit.max(1);
        if !has_embedding_model {
            // Without a vector source, RRF preserves keyword order. Avoid the
            // fixed 100-candidate query cost for small result sets.
            //
            // RFC-060 §10: that saving assumed one candidate becomes one
            // result, which the per-file cap breaks -- 20 candidates from
            // one file now yield one result. The pool is widened so the
            // visible `limit` can be filled from distinct files. It is
            // headroom, not a guarantee: a query whose candidates come from
            // fewer than `limit` files still returns fewer results, which
            // is honest (there are not that many files matching) rather
            // than padded with repeats of the same file.
            let keyword_cap = requested_limit.saturating_mul(PER_FILE_CAP_HEADROOM);
            self.keyword_k = self.keyword_k.min(keyword_cap);
            self.vector_k = 0;
            self.fusion_n = self.fusion_n.min(keyword_cap as usize);
        }
    }
}

/// Hybrid search service. Optional embedding model degrades gracefully
/// when absent (RFC-009 §21).
pub struct HybridSearchService<'a> {
    catalog: &'a Catalog,
    embedding_model: Option<(&'a dyn EmbeddingModel, String)>,
    /// The extraction cache, when the caller has one (RFC-060 §6): the
    /// only source a page/paragraph/block snippet may be rendered from.
    extraction_cache: Option<&'a orbok_cache::CacheService>,
}

/// How many results one file may occupy (RFC-060 §10). One: a result row
/// names a file, and a second row for the same file spends a slot a
/// different file could have had.
pub(crate) const MAX_RESULTS_PER_FILE: usize = 1;

/// How much wider than the requested limit the keyword candidate pool is
/// fetched when the per-file cap applies (RFC-060 §10). Measured on this
/// repository's own `rfcs/` tree: see `rfc060_duplication_measurement.rs`.
const PER_FILE_CAP_HEADROOM: u32 = 5;

/// One search, with everything that decides which results come back
/// (RFC-060 §7). `run_search(catalog, model, query, limit)` had no
/// parameter surface for trust, filters or folder scope, which is why
/// RFC-058 §6's rows 3 and 4 could not be written against it.
#[derive(Debug, Clone)]
pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub mode: SearchMode,
    pub limit: u32,
    /// Kind filter and folder restriction, applied at the query.
    pub scope: SearchScope,
}

impl<'a> SearchRequest<'a> {
    /// An unrestricted request, the shape the old positional call had.
    pub fn new(query: &'a str, mode: SearchMode, limit: u32) -> Self {
        Self {
            query,
            mode,
            limit,
            scope: SearchScope::default(),
        }
    }

    /// Restrict this request to a scope.
    pub fn with_scope(mut self, scope: SearchScope) -> Self {
        self.scope = scope;
        self
    }
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
            extraction_cache: None,
        }
    }

    /// Give the service the extraction cache, so results whose positions
    /// are pages, paragraphs or blocks can render a snippet from the
    /// cached segments (RFC-060 §6). Without it those render none.
    pub fn with_extraction_cache(mut self, cache: &'a orbok_cache::CacheService) -> Self {
        self.extraction_cache = Some(cache);
        self
    }

    /// Hybrid mode with an embedding model.
    pub fn with_model(catalog: &'a Catalog, model: &'a dyn EmbeddingModel, model_id: &str) -> Self {
        Self {
            catalog,
            embedding_model: Some((model, model_id.to_string())),
            extraction_cache: None,
        }
    }

    pub fn is_hybrid(&self) -> bool {
        self.embedding_model.is_some()
    }

    /// Execute a search and return enriched results.
    pub fn search(
        &self,
        query: &str,
        mode: SearchMode,
        limit: u32,
    ) -> OrbokResult<Vec<SearchResult>> {
        Ok(self.search_profile(query, mode, limit)?.results)
    }

    /// Execute one [`SearchRequest`] -- the parameter surface RFC-060 §7
    /// asks for, carrying the kind filter and folder scope the plain
    /// `search` above cannot express.
    pub fn search_request(&self, request: &SearchRequest<'_>) -> OrbokResult<Vec<SearchResult>> {
        Ok(self
            .search_profile_scoped(request.query, request.mode, request.limit, &request.scope)?
            .results)
    }

    /// Execute a search and return timing evidence for benchmark diagnostics.
    pub fn search_profile(
        &self,
        query: &str,
        mode: SearchMode,
        limit: u32,
    ) -> OrbokResult<SearchProfile> {
        self.search_profile_scoped(query, mode, limit, &SearchScope::default())
    }

    /// [`Self::search_profile`], restricted to a scope. Every candidate
    /// source applies it, so fusion cannot reintroduce what one of them
    /// excluded.
    pub fn search_profile_scoped(
        &self,
        query: &str,
        mode: SearchMode,
        limit: u32,
        scope: &SearchScope,
    ) -> OrbokResult<SearchProfile> {
        let total_start = Instant::now();
        let mut timing = SearchTiming::default();
        let mut limits = Limits::for_mode(mode);
        limits.adjust_for_request(limit, self.embedding_model.is_some());

        // Keyword candidates — use multilingual engine (RFC-014).
        let keyword_start = Instant::now();
        let kw_candidates = if limits.keyword_k > 0 {
            let keyword_engine = MultilingualKeywordEngine::new(self.catalog);
            if mode == SearchMode::Auto && query.split_whitespace().count() >= 4 {
                keyword_engine.search_pairs_scoped(query, limits.keyword_k, scope)?
            } else {
                keyword_engine.search_scoped(query, limits.keyword_k, scope)?
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
                    scope: scope.clone(),
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
        let results = self.enrich_many(&fused, limit as usize)?;
        timing.enrichment_ms = elapsed_ms(enrichment_start);

        // No reranker is wired (RFC-010, Task 040): `rerank_ms` stays 0 --
        // kept as a field, not deleted, since `orbok-bench`'s
        // `SearchTiming`/`SearchProfile` consumers (crates/bench/src/report.rs,
        // metrics.rs) still read it for p99 latency reporting.
        timing.total_ms = elapsed_ms(total_start);

        Ok(SearchProfile { results, timing })
    }

    fn enrich_many(
        &self,
        candidates: &[FusedCandidate],
        limit: usize,
    ) -> OrbokResult<Vec<SearchResult>> {
        // RFC-060 §10 / HANDOFF-060 §4: at most one result per file, taken
        // in rank order. Measured on this repository's own `rfcs/` tree
        // (113 files, 15 queries): 174 of 300 returned slots were a repeat
        // of a file already shown, 14 of 15 queries repeated at least one
        // file, and the worst single file took 18 of 20 slots. Excluding
        // the whole-file `"document"` chunk -- the handoff's other option --
        // would have reclaimed 11 of those 300 slots, because the
        // duplication is mostly *section* chunks of one file competing with
        // each other, not the document chunk.
        //
        // Later candidates fill the slots the skipped ones would have
        // taken, so a query with enough distinct files still returns
        // `limit` results: this narrows what one file may occupy, it does
        // not shrink the result set (RFC-041 §25.5).
        let mut per_file: HashMap<String, usize> = HashMap::new();
        let top_candidates: Vec<&FusedCandidate> = candidates
            .iter()
            .filter(|candidate| {
                let seen = per_file
                    .entry(candidate.file_id.as_str().to_string())
                    .or_default();
                *seen += 1;
                *seen <= MAX_RESULTS_PER_FILE
            })
            .take(limit)
            .collect();
        let chunk_ids: Vec<_> = top_candidates
            .iter()
            .map(|candidate| candidate.chunk_id.clone())
            .collect();
        let records = chunk_records_for(self.catalog, &chunk_ids)?;
        let snippets = SnippetSource::new(self.catalog, self.extraction_cache)?;

        let mut results = Vec::with_capacity(top_candidates.len());
        for candidate in top_candidates {
            let Some(lookup) = records.get(candidate.chunk_id.as_str()) else {
                continue;
            };
            let (chunk, canonical_path) = (&lookup.record, &lookup.canonical_path);
            let rendered = snippets.render(chunk, canonical_path);
            let trust = trust_for(canonical_path, &lookup.file_status, &rendered.warnings);
            let snippet = rendered.snippet;
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
                trust,
            });
        }
        Ok(results)
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
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
