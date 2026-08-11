//! Benchmark metrics (RFC-016 §10–§11).

use crate::queries::LabeledQuery;
use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_models::EmbeddingModel;
use orbok_search::{HybridSearchService, SearchTiming};
use std::time::Instant;

/// Latency percentiles in milliseconds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

/// Component latency percentiles for measured search runs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchTimingMetrics {
    pub total_ms: LatencyMetrics,
    pub keyword_ms: LatencyMetrics,
    pub query_embedding_ms: LatencyMetrics,
    pub vector_scan_ms: LatencyMetrics,
    pub fusion_ms: LatencyMetrics,
    pub enrichment_ms: LatencyMetrics,
    pub rerank_ms: LatencyMetrics,
}

/// Distribution summary (min/mean/p50/max) for a batch-level measurement
/// (RFC-048 Task 011).
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DistributionMetrics {
    pub min: f64,
    pub mean: f64,
    pub p50: f64,
    pub max: f64,
}

fn distribution_metrics(mut values: Vec<f64>) -> DistributionMetrics {
    if values.is_empty() {
        return DistributionMetrics {
            min: 0.0,
            mean: 0.0,
            p50: 0.0,
            max: 0.0,
        };
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let p50_index = (0.50 * values.len() as f64) as usize;
    DistributionMetrics {
        min: values[0],
        mean,
        p50: values[p50_index.min(values.len() - 1)],
        max: values[values.len() - 1],
    }
}

/// Aggregated embedding-batch statistics across an entire indexing run
/// (RFC-048 Task 011): how much of the embedding cost is padding skew
/// versus real token positions, and the batch/sequence-length shape that
/// produced it. `batch_size`'s distribution is also chunks-per-document,
/// under the current per-file batching (`EmbeddingWorker::run` embeds one
/// file's active chunks in a single call) -- reported once rather than
/// duplicated as two identical distributions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingBatchMetrics {
    pub call_count: usize,
    pub batch_size: DistributionMetrics,
    pub padded_seq_len: DistributionMetrics,
    pub real_token_positions_total: u64,
    pub padded_token_positions_total: u64,
    /// `real_token_positions_total / padded_token_positions_total`. 1.0
    /// would mean no padding skew at all; the RFC-048 baseline's
    /// `Fixed(512)` padding made this ratio roughly `avg_tokens / 512`.
    pub real_to_padded_ratio: f64,
}

pub fn embedding_batch_metrics(
    stats: &[orbok_models::EmbeddingBatchStats],
) -> EmbeddingBatchMetrics {
    let batch_sizes: Vec<f64> = stats.iter().map(|s| s.batch_size as f64).collect();
    let seq_lens: Vec<f64> = stats.iter().map(|s| s.padded_seq_len as f64).collect();
    let real_token_positions_total: u64 = stats.iter().map(|s| s.real_token_positions).sum();
    let padded_token_positions_total: u64 = stats
        .iter()
        .map(|s| s.batch_size as u64 * s.padded_seq_len as u64)
        .sum();
    EmbeddingBatchMetrics {
        call_count: stats.len(),
        batch_size: distribution_metrics(batch_sizes),
        padded_seq_len: distribution_metrics(seq_lens),
        real_token_positions_total,
        padded_token_positions_total,
        real_to_padded_ratio: if padded_token_positions_total > 0 {
            real_token_positions_total as f64 / padded_token_positions_total as f64
        } else {
            0.0
        },
    }
}

/// Recall@k result.
#[derive(Debug, serde::Serialize)]
pub struct RecallMetrics {
    pub k: usize,
    pub recall: f64,
    pub queries_evaluated: usize,
    pub queries_with_any_hit: usize,
}

/// Measure search latency by running each labeled query multiple times.
pub fn measure_search_latency(
    catalog: &Catalog,
    queries: &[LabeledQuery],
    model: Option<&dyn EmbeddingModel>,
    model_id: Option<&str>,
) -> OrbokResult<LatencyMetrics> {
    Ok(measure_search_timing(catalog, queries, model, model_id)?.total_ms)
}

/// Measure total and component search latency.
pub fn measure_search_timing(
    catalog: &Catalog,
    queries: &[LabeledQuery],
    model: Option<&dyn EmbeddingModel>,
    model_id: Option<&str>,
) -> OrbokResult<SearchTimingMetrics> {
    let service = search_service(catalog, model, model_id);
    let mut timings = TimingSamples::default();
    // 3 warm-up runs.
    for q in queries.iter().take(3) {
        let _ = service.search(q.query, orbok_search::SearchMode::Auto, 10)?;
    }
    // Measured runs.
    for _ in 0..3 {
        for q in queries {
            let start = Instant::now();
            let profile = service.search_profile(q.query, orbok_search::SearchMode::Auto, 10)?;
            let mut timing = profile.timing;
            // Keep total latency comparable with the historical outer timing.
            timing.total_ms = start.elapsed().as_secs_f64() * 1000.0;
            timings.push(timing);
        }
    }
    Ok(timings.into_metrics())
}

#[derive(Default)]
struct TimingSamples {
    total_ms: Vec<f64>,
    keyword_ms: Vec<f64>,
    query_embedding_ms: Vec<f64>,
    vector_scan_ms: Vec<f64>,
    fusion_ms: Vec<f64>,
    enrichment_ms: Vec<f64>,
    rerank_ms: Vec<f64>,
}

impl TimingSamples {
    fn push(&mut self, timing: SearchTiming) {
        self.total_ms.push(timing.total_ms);
        self.keyword_ms.push(timing.keyword_ms);
        self.query_embedding_ms.push(timing.query_embedding_ms);
        self.vector_scan_ms.push(timing.vector_scan_ms);
        self.fusion_ms.push(timing.fusion_ms);
        self.enrichment_ms.push(timing.enrichment_ms);
        self.rerank_ms.push(timing.rerank_ms);
    }

    fn into_metrics(self) -> SearchTimingMetrics {
        SearchTimingMetrics {
            total_ms: latency_metrics(self.total_ms),
            keyword_ms: latency_metrics(self.keyword_ms),
            query_embedding_ms: latency_metrics(self.query_embedding_ms),
            vector_scan_ms: latency_metrics(self.vector_scan_ms),
            fusion_ms: latency_metrics(self.fusion_ms),
            enrichment_ms: latency_metrics(self.enrichment_ms),
            rerank_ms: latency_metrics(self.rerank_ms),
        }
    }
}

fn latency_metrics(mut latencies_ms: Vec<f64>) -> LatencyMetrics {
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |pct: f64| -> f64 {
        let idx = (pct / 100.0 * latencies_ms.len() as f64) as usize;
        latencies_ms[idx.min(latencies_ms.len().saturating_sub(1))]
    };
    LatencyMetrics {
        p50_ms: p(50.0),
        p95_ms: p(95.0),
        p99_ms: p(99.0),
        min_ms: latencies_ms.first().copied().unwrap_or(0.0),
        max_ms: latencies_ms.last().copied().unwrap_or(0.0),
    }
}

/// Compute recall@k: for each labeled query, check whether any of the
/// top-k results matches an expected document pattern.
pub fn compute_recall(
    catalog: &Catalog,
    queries: &[LabeledQuery],
    model: Option<&dyn EmbeddingModel>,
    model_id: Option<&str>,
) -> OrbokResult<RecallMetrics> {
    const K: usize = 5;
    let service = search_service(catalog, model, model_id);
    let mut hits = 0usize;
    let evaluated = queries.len();
    for q in queries {
        let results = service.search(q.query, orbok_search::SearchMode::Auto, K as u32)?;
        let hit = results.iter().any(|r| {
            let path = r.display_path.to_lowercase();
            q.relevant_patterns.iter().any(|pat| path.contains(pat))
        });
        if hit {
            hits += 1;
        }
    }
    Ok(RecallMetrics {
        k: K,
        recall: if evaluated > 0 {
            hits as f64 / evaluated as f64
        } else {
            0.0
        },
        queries_evaluated: evaluated,
        queries_with_any_hit: hits,
    })
}

fn search_service<'a>(
    catalog: &'a Catalog,
    model: Option<&'a dyn EmbeddingModel>,
    model_id: Option<&str>,
) -> HybridSearchService<'a> {
    match (model, model_id) {
        (Some(model), Some(model_id)) => HybridSearchService::with_model(catalog, model, model_id),
        _ => HybridSearchService::keyword_only(catalog),
    }
}
