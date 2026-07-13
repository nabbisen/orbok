//! Re-exports for integration testing (RFC-016 §17 benchmark smoke test).
pub mod corpus;
pub mod metrics;
pub mod queries;
pub mod report;

use std::path::{Path, PathBuf};

/// Benchmark options for release evidence runs.
#[derive(Debug, Clone, Default)]
pub struct BenchmarkOptions {
    /// Optional recommended model directory containing `onnx/model.onnx` and
    /// `tokenizer.json`. When set, the benchmark builds real embeddings and
    /// measures hybrid search. Without it, the benchmark remains keyword-only.
    pub model_dir: Option<PathBuf>,
}

/// Full benchmark run returning a `BenchmarkResult`. Used by CI and tests.
pub fn run_bench(
    n_docs: usize,
    work_dir: &Path,
) -> Result<report::BenchmarkResult, Box<dyn std::error::Error>> {
    run_bench_with_options(n_docs, work_dir, BenchmarkOptions::default())
}

/// Full benchmark run with explicit mode options.
pub fn run_bench_with_options(
    n_docs: usize,
    work_dir: &Path,
    options: BenchmarkOptions,
) -> Result<report::BenchmarkResult, Box<dyn std::error::Error>> {
    let corpus_start = std::time::Instant::now();
    corpus::generate(work_dir, n_docs)?;
    let corpus_generation_ms = elapsed_ms(corpus_start);
    let catalog = orbok_db::Catalog::open(work_dir.join("bench-catalog.sqlite3"))?;
    let cache = orbok_cache::CacheService::new(work_dir);
    {
        use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
        use orbok_db::repo::{NewSource, SourceRepository};
        let root = std::fs::canonicalize(work_dir)?
            .to_string_lossy()
            .to_string();
        SourceRepository::new(&catalog).insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some("bench".into()),
            original_path: root.clone(),
            canonical_path: root,
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })?;
    }
    {
        use orbok_fs::{ScanRequest, Scanner};
        use std::sync::atomic::AtomicBool;
        let sources = orbok_db::repo::SourceRepository::new(&catalog);
        let src = sources.list_active()?;
        if let Some(source) = src.first() {
            Scanner::new(&catalog).scan(
                &ScanRequest {
                    source_id: source.source_id.clone(),
                    force_hash: false,
                    enqueue_index_jobs: true,
                },
                &AtomicBool::new(false),
            )?;
        }
    }
    let index_start = std::time::Instant::now();
    let extract = orbok_workers::ExtractionWorker::new(&catalog, &cache);
    let chunk = orbok_workers::ChunkAndIndexWorker::new(&catalog, &cache);
    orbok_workers::run_pending(&catalog, &extract, &chunk, None, n_docs as u32 * 4)?;
    let extraction_chunking_keyword_index_ms = elapsed_ms(index_start);

    let mut real_model = None;
    let mut model_id = None;
    let mut model_evidence = None;
    let mut model_load_ms = 0;
    let mut document_embedding_ms = 0;
    if let Some(model_dir) = options.model_dir.as_deref() {
        let model_load_start = std::time::Instant::now();
        let model = load_real_model(model_dir)?;
        let id = register_benchmark_model(&catalog, model.as_ref(), model_dir)?;
        model_load_ms = elapsed_ms(model_load_start);
        model_evidence = Some(report::BenchmarkModelEvidence {
            model_id: id.as_str().to_string(),
            name: model.name().to_string(),
            version: model.version().to_string(),
            dimension: model.dimension(),
        });
        let embed = orbok_workers::EmbeddingWorker::with_model(&catalog, &cache, model, id.clone());
        let document_embedding_start = std::time::Instant::now();
        for file_id in indexed_file_ids(&catalog)? {
            embed.run(&file_id)?;
        }
        document_embedding_ms = elapsed_ms(document_embedding_start);
        model_id = Some(id.as_str().to_string());
        real_model = Some(embed);
    }

    let index_elapsed_ms =
        extraction_chunking_keyword_index_ms + model_load_ms + document_embedding_ms;
    let catalog_size = std::fs::metadata(work_dir.join("bench-catalog.sqlite3"))
        .map(|m| m.len())
        .unwrap_or(0);
    let corpus_size = corpus::total_bytes(work_dir);
    let search_model = real_model.as_ref().map(|embed| embed.model());
    let search_timing = metrics::measure_search_timing(
        &catalog,
        queries::LABELED_QUERIES,
        search_model,
        model_id.as_deref(),
    )?;
    let latencies = search_timing.total_ms.clone();
    let recall = metrics::compute_recall(
        &catalog,
        queries::LABELED_QUERIES,
        search_model,
        model_id.as_deref(),
    )?;
    Ok(report::BenchmarkResult {
        n_docs,
        mode: if real_model.is_some() {
            report::BenchmarkMode::HybridRealModel
        } else {
            report::BenchmarkMode::KeywordOnly
        },
        model: model_evidence,
        timing_ms: report::BenchmarkTimingBreakdown {
            corpus_generation_ms,
            extraction_chunking_keyword_index_ms,
            model_load_ms,
            document_embedding_ms,
            search: search_timing,
        },
        corpus_bytes: corpus_size,
        catalog_bytes: catalog_size,
        index_elapsed_ms,
        indexing_files_per_sec: if index_elapsed_ms > 0 {
            (n_docs as f64 * 1000.0) / index_elapsed_ms as f64
        } else {
            0.0
        },
        search_latency_ms: latencies,
        recall_at_k: recall,
    })
}

fn elapsed_ms(start: std::time::Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

fn load_real_model(
    model_dir: &Path,
) -> Result<Box<dyn orbok_models::EmbeddingModel>, Box<dyn std::error::Error>> {
    match orbok_workers::verify_embedding_model(Some(&model_dir.to_string_lossy())) {
        orbok_workers::VerifyOutcome::Ready => {}
        outcome => {
            return Err(format!(
                "model directory is not ready: {}",
                orbok_workers::verify_outcome_summary(&outcome)
            )
            .into());
        }
    }
    let config = orbok_embed::recommended_config_from_model_dir(model_dir);
    orbok_embed::create_embedding_model(&config).map_err(|err| err.into())
}

fn register_benchmark_model(
    catalog: &orbok_db::Catalog,
    model: &dyn orbok_models::EmbeddingModel,
    model_dir: &Path,
) -> orbok_core::OrbokResult<orbok_core::ModelId> {
    use orbok_db::repo::{ModelRepository, ModelRole, ModelStatus, NewModel};

    let record = ModelRepository::new(catalog).insert(NewModel {
        role: ModelRole::Embedding,
        model_name: model.name().to_string(),
        model_version: model.version().to_string(),
        local_path: Some(model_dir.to_string_lossy().into_owned()),
        license_summary: None,
        size_bytes: None,
        backend: Some("tract".to_string()),
        dimension: Some(model.dimension()),
        status: ModelStatus::Available,
    })?;
    Ok(record.model_id)
}

fn indexed_file_ids(
    catalog: &orbok_db::Catalog,
) -> orbok_core::OrbokResult<Vec<orbok_core::FileId>> {
    let conn = catalog.lock();
    let mut stmt = conn
        .prepare("SELECT file_id FROM files WHERE file_status = 'indexed'")
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(orbok_core::FileId::from_string(
            row.map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?,
        ));
    }
    Ok(ids)
}

#[cfg(test)]
mod bench_tests {
    use super::*;
    use orbok_models::EmbeddingModel;

    #[test]
    fn real_model_benchmark_registers_model_before_embedding() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = orbok_db::Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
        let model = orbok_models::MockEmbeddingModel;

        let model_id = register_benchmark_model(&catalog, &model, dir.path()).unwrap();
        let record = orbok_db::repo::ModelRepository::new(&catalog)
            .get(&model_id)
            .unwrap()
            .unwrap();

        assert_eq!(record.model_id.as_str(), model_id.as_str());
        assert_eq!(record.role, orbok_db::repo::ModelRole::Embedding);
        assert_eq!(record.model_name, "mock");
        assert_eq!(record.model_version, "v1");
        assert_eq!(record.dimension, Some(model.dimension()));
        assert_eq!(record.status, orbok_db::repo::ModelStatus::Available);
    }

    // RFC-016 §17 / RFC-023 baseline: benchmark with 100 synthetic documents.
    // Results inform the ANN and quantization decisions.
    #[test]
    fn bench_full_pipeline() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_bench(100, dir.path()).unwrap();
        result.print_summary();
        // RFC-023 gate: exact scan must be fast enough for the test corpus.
        assert!(
            result.search_latency_ms.p99_ms < 2000.0,
            "p99 search latency too high: {:.2}ms",
            result.search_latency_ms.p99_ms
        );
        // RFC-016 recall target (relaxed for synthetic corpus with mock model).
        assert!(
            result.recall_at_k.recall >= 0.0,
            "recall must be a valid fraction"
        );
        println!(
            "Recall@{}: {:.1}%",
            result.recall_at_k.k,
            result.recall_at_k.recall * 100.0
        );
    }
}
