//! Benchmark metrics (RFC-016 §10–§11).

use crate::queries::LabeledQuery;
use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_models::EmbeddingModel;
use orbok_search::HybridSearchService;
use std::time::Instant;

/// Latency percentiles in milliseconds.
#[derive(Debug, serde::Serialize)]
pub struct LatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
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
    let service = search_service(catalog, model, model_id);
    let mut latencies_ms: Vec<f64> = Vec::new();
    // 3 warm-up runs.
    for q in queries.iter().take(3) {
        let _ = service.search(q.query, orbok_search::SearchMode::Auto, 10)?;
    }
    // Measured runs.
    for _ in 0..3 {
        for q in queries {
            let start = Instant::now();
            let _ = service.search(q.query, orbok_search::SearchMode::Auto, 10)?;
            latencies_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |pct: f64| -> f64 {
        let idx = (pct / 100.0 * latencies_ms.len() as f64) as usize;
        latencies_ms[idx.min(latencies_ms.len().saturating_sub(1))]
    };
    Ok(LatencyMetrics {
        p50_ms: p(50.0),
        p95_ms: p(95.0),
        p99_ms: p(99.0),
        min_ms: latencies_ms.first().copied().unwrap_or(0.0),
        max_ms: latencies_ms.last().copied().unwrap_or(0.0),
    })
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
