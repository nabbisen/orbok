//! Benchmark report writer (RFC-016 §12): JSON and Markdown output.

use crate::metrics::{EmbeddingBatchMetrics, LatencyMetrics, RecallMetrics, SearchTimingMetrics};
use std::fs;
use std::path::Path;

/// Complete benchmark result.
#[derive(Debug, serde::Serialize)]
pub struct BenchmarkResult {
    pub n_docs: usize,
    pub mode: BenchmarkMode,
    pub model: Option<BenchmarkModelEvidence>,
    pub timing_ms: BenchmarkTimingBreakdown,
    pub corpus_bytes: u64,
    pub catalog_bytes: u64,
    pub index_elapsed_ms: u64,
    pub indexing_files_per_sec: f64,
    pub search_latency_ms: LatencyMetrics,
    pub recall_at_k: RecallMetrics,
}

/// Machine-readable timing breakdown for benchmark diagnostics.
#[derive(Debug, serde::Serialize)]
pub struct BenchmarkTimingBreakdown {
    pub corpus_generation_ms: u64,
    pub extraction_chunking_keyword_index_ms: u64,
    pub model_load_ms: u64,
    pub document_embedding_ms: u64,
    pub search: SearchTimingMetrics,
    /// RFC-048 Task 011: batch/padding-shape statistics for the document
    /// embedding phase above. `None` in keyword-only mode, where no real
    /// embedding backend runs.
    pub embedding_batches: Option<EmbeddingBatchMetrics>,
}

/// Non-secret model identity recorded for release evidence.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchmarkModelEvidence {
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub dimension: u32,
}

/// Benchmark search mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BenchmarkMode {
    KeywordOnly,
    HybridRealModel,
}

impl BenchmarkMode {
    pub fn label(self) -> &'static str {
        match self {
            BenchmarkMode::KeywordOnly => "keyword-only",
            BenchmarkMode::HybridRealModel => "hybrid-real-model",
        }
    }
}

impl BenchmarkResult {
    pub fn write_json(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }

    /// RFC-048 Task 011 §3/§7: projected indexing throughput if padding
    /// skew were eliminated -- i.e. only real (non-padding) token
    /// positions were ever processed -- holding the measured
    /// cost-per-padded-token-position constant, as the task specifies.
    /// `None` in keyword-only mode, or if there is nothing to project
    /// from (zero padded positions or zero documents).
    pub fn projected_files_per_sec_without_padding_skew(&self) -> Option<f64> {
        let batches = self.timing_ms.embedding_batches.as_ref()?;
        if batches.padded_token_positions_total == 0 || self.n_docs == 0 {
            return None;
        }
        let ms_per_padded_position = self.timing_ms.document_embedding_ms as f64
            / batches.padded_token_positions_total as f64;
        let projected_document_embedding_ms =
            batches.real_token_positions_total as f64 * ms_per_padded_position;
        let projected_index_elapsed_ms = self.timing_ms.corpus_generation_ms as f64
            + self.timing_ms.extraction_chunking_keyword_index_ms as f64
            + self.timing_ms.model_load_ms as f64
            + projected_document_embedding_ms;
        if projected_index_elapsed_ms <= 0.0 {
            return None;
        }
        Some((self.n_docs as f64 * 1000.0) / projected_index_elapsed_ms)
    }

    /// RFC-048 Task 011 §3: padded token-positions per document (the
    /// benchmark's whole corpus, not just files that reached embedding).
    /// `None` in keyword-only mode.
    pub fn padded_token_positions_per_doc(&self) -> Option<f64> {
        let batches = self.timing_ms.embedding_batches.as_ref()?;
        if self.n_docs == 0 {
            return None;
        }
        Some(batches.padded_token_positions_total as f64 / self.n_docs as f64)
    }

    pub fn write_markdown(&self, path: &Path) -> std::io::Result<()> {
        let recall_status = if self.recall_at_k.recall >= 0.75 {
            "PASS"
        } else {
            "FAIL"
        };
        let p99_status = if self.search_latency_ms.p99_ms <= 200.0 {
            "PASS"
        } else {
            "FAIL"
        };
        let indexing_status = if self.indexing_files_per_sec >= 10.0 {
            "PASS"
        } else {
            "FAIL"
        };
        let model = self
            .model
            .as_ref()
            .map(|model| {
                format!(
                    "{} ({} {}, {} dims)",
                    model.model_id, model.name, model.version, model.dimension
                )
            })
            .unwrap_or_else(|| "none".to_string());
        let embedding_batch_section = self
            .timing_ms
            .embedding_batches
            .as_ref()
            .map(|batches| {
                format!(
                    "## Embedding Batch Shape (RFC-048 Task 011)\n\n\
                     | Metric | Value |\n|---|---|\n\
                     | `embed_batch` calls | {} |\n\
                     | Batch size (= chunks/doc) — min/mean/p50/max | {:.0} / {:.1} / {:.0} / {:.0} |\n\
                     | Padded seq len — min/mean/p50/max | {:.0} / {:.1} / {:.0} / {:.0} |\n\
                     | Real token positions (total) | {} |\n\
                     | Padded token positions (total) | {} |\n\
                     | Real / padded ratio | {:.4} |\n\
                     | Padded token positions / doc | {:.1} |\n\
                     | Projected throughput without padding skew | {} |\n\n",
                    batches.call_count,
                    batches.batch_size.min,
                    batches.batch_size.mean,
                    batches.batch_size.p50,
                    batches.batch_size.max,
                    batches.padded_seq_len.min,
                    batches.padded_seq_len.mean,
                    batches.padded_seq_len.p50,
                    batches.padded_seq_len.max,
                    batches.real_token_positions_total,
                    batches.padded_token_positions_total,
                    batches.real_to_padded_ratio,
                    self.padded_token_positions_per_doc().unwrap_or(0.0),
                    self.projected_files_per_sec_without_padding_skew()
                        .map(|value| format!("{value:.1} files/s"))
                        .unwrap_or_else(|| "n/a".to_string()),
                )
            })
            .unwrap_or_default();
        let md = format!(
            "# orbok Benchmark Report\n\n\
             ## Corpus\n\n\
             | Metric | Value |\n|---|---|\n\
             | Mode | {} |\n\
             | Embedding model | {} |\n\
             | Documents | {} |\n\
             | Corpus size | {:.1} KiB |\n\
             | Catalog size | {:.1} KiB |\n\
             | Bytes per doc | {:.0} |\n\n\
             ## Indexing\n\n\
             | Metric | Value |\n|---|---|\n\
             | Total time | {} ms |\n\
             | Throughput | {:.1} files/s |\n\
             | Corpus generation | {} ms |\n\
             | Extraction/chunking/keyword indexing | {} ms |\n\
             | Model load | {} ms |\n\
             | Document embedding | {} ms |\n\n\
             {embedding_batch_section}\
             ## Search Latency\n\n\
             | Percentile | Latency |\n|---|---|\n\
             | p50 | {:.2} ms |\n\
             | p95 | {:.2} ms |\n\
             | p99 | {:.2} ms |\n\
             | min | {:.2} ms |\n\
             | max | {:.2} ms |\n\n\
             ## Search Timing Breakdown (p99)\n\n\
             | Component | p99 |\n|---|---:|\n\
             | Total | {:.2} ms |\n\
             | Keyword retrieval | {:.2} ms |\n\
             | Query embedding | {:.2} ms |\n\
             | Vector scan | {:.2} ms |\n\
             | Fusion | {:.2} ms |\n\
             | Result enrichment | {:.2} ms |\n\
             | Rerank | {:.2} ms |\n\n\
             ## Retrieval Quality\n\n\
             | Metric | Value |\n|---|---|\n\
             | Recall@{} | {:.1}% |\n\
             | Queries evaluated | {} |\n\
             | Queries with hit | {} |\n\n\
             ## Release Gate Check\n\n\
             | Gate | Target | Observed | Status |\n|---|---:|---:|---|\n\
             | Recall@{} | >= 75.0% | {:.1}% | {} |\n\
             | Search p99 | <= 200.00 ms | {:.2} ms | {} |\n\
             | Indexing throughput | >= 10.0 files/s | {:.1} files/s | {} |\n",
            self.mode.label(),
            model,
            self.n_docs,
            self.corpus_bytes as f64 / 1024.0,
            self.catalog_bytes as f64 / 1024.0,
            if self.n_docs > 0 {
                self.catalog_bytes as f64 / self.n_docs as f64
            } else {
                0.0
            },
            self.index_elapsed_ms,
            self.indexing_files_per_sec,
            self.timing_ms.corpus_generation_ms,
            self.timing_ms.extraction_chunking_keyword_index_ms,
            self.timing_ms.model_load_ms,
            self.timing_ms.document_embedding_ms,
            self.search_latency_ms.p50_ms,
            self.search_latency_ms.p95_ms,
            self.search_latency_ms.p99_ms,
            self.search_latency_ms.min_ms,
            self.search_latency_ms.max_ms,
            self.timing_ms.search.total_ms.p99_ms,
            self.timing_ms.search.keyword_ms.p99_ms,
            self.timing_ms.search.query_embedding_ms.p99_ms,
            self.timing_ms.search.vector_scan_ms.p99_ms,
            self.timing_ms.search.fusion_ms.p99_ms,
            self.timing_ms.search.enrichment_ms.p99_ms,
            self.timing_ms.search.rerank_ms.p99_ms,
            self.recall_at_k.k,
            self.recall_at_k.recall * 100.0,
            self.recall_at_k.queries_evaluated,
            self.recall_at_k.queries_with_any_hit,
            self.recall_at_k.k,
            self.recall_at_k.recall * 100.0,
            recall_status,
            self.search_latency_ms.p99_ms,
            p99_status,
            self.indexing_files_per_sec,
            indexing_status,
        );
        fs::write(path, md)
    }

    pub fn print_summary(&self) {
        println!(
            "Docs: {}  |  Index: {} ms ({:.1} files/s)  |  \
             p50: {:.2}ms  p99: {:.2}ms  |  Recall@{}: {:.0}%  |  Mode: {}",
            self.n_docs,
            self.index_elapsed_ms,
            self.indexing_files_per_sec,
            self.search_latency_ms.p50_ms,
            self.search_latency_ms.p99_ms,
            self.recall_at_k.k,
            self.recall_at_k.recall * 100.0,
            self.mode.label(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{DistributionMetrics, LatencyMetrics, RecallMetrics, SearchTimingMetrics};

    #[test]
    fn markdown_records_model_evidence_without_paths() {
        let dir = tempfile::tempdir().unwrap();
        let result = BenchmarkResult {
            n_docs: 10,
            mode: BenchmarkMode::HybridRealModel,
            model: Some(BenchmarkModelEvidence {
                model_id: "embedding_multilingual-e5-small-v1".to_string(),
                name: "multilingual-e5-small".to_string(),
                version: "v1".to_string(),
                dimension: 384,
            }),
            timing_ms: timing_breakdown(),
            corpus_bytes: 1024,
            catalog_bytes: 2048,
            index_elapsed_ms: 100,
            indexing_files_per_sec: 100.0,
            search_latency_ms: LatencyMetrics {
                p50_ms: 1.0,
                p95_ms: 2.0,
                p99_ms: 3.0,
                min_ms: 0.5,
                max_ms: 4.0,
            },
            recall_at_k: RecallMetrics {
                k: 5,
                recall: 1.0,
                queries_evaluated: 1,
                queries_with_any_hit: 1,
            },
        };

        let markdown_path = dir.path().join("report.md");
        result.write_markdown(&markdown_path).unwrap();
        let markdown = std::fs::read_to_string(markdown_path).unwrap();

        assert!(markdown.contains("| Mode | hybrid-real-model |"));
        assert!(markdown.contains("## Search Timing Breakdown (p99)"));
        assert!(markdown.contains("| Query embedding | 3.00 ms |"));
        assert!(markdown.contains(
            "| Embedding model | embedding_multilingual-e5-small-v1 \
             (multilingual-e5-small v1, 384 dims) |"
        ));
        assert!(!markdown.contains("tokenizer.json"));
        assert!(!markdown.contains("onnx/model.onnx"));

        let json_path = dir.path().join("report.json");
        result.write_json(&json_path).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(json_path).unwrap()).unwrap();
        assert_eq!(json["mode"], "hybrid-real-model");
        assert_eq!(json["timing_ms"]["document_embedding_ms"], 25);
        assert_eq!(
            json["timing_ms"]["search"]["query_embedding_ms"]["p99_ms"],
            3.0
        );
        assert_eq!(
            json["model"]["model_id"],
            "embedding_multilingual-e5-small-v1"
        );
        assert_eq!(json["model"]["dimension"], 384);
    }

    // RFC-048 Task 011: the embedding-batch section only renders, and the
    // derived figures only compute, when embedding_batches is Some (a
    // hybrid-real-model run) -- confirmed absent above for a run with
    // embedding_batches: None (timing_breakdown()'s default), confirmed
    // present and numerically correct here.
    #[test]
    fn embedding_batch_section_reports_measured_and_derived_figures() {
        let dir = tempfile::tempdir().unwrap();
        let mut timing = timing_breakdown();
        timing.document_embedding_ms = 1000;
        timing.embedding_batches = Some(EmbeddingBatchMetrics {
            call_count: 10,
            batch_size: DistributionMetrics {
                min: 1.0,
                mean: 2.0,
                p50: 2.0,
                max: 4.0,
            },
            padded_seq_len: DistributionMetrics {
                min: 10.0,
                mean: 50.0,
                p50: 40.0,
                max: 100.0,
            },
            real_token_positions_total: 500,
            padded_token_positions_total: 1000,
            real_to_padded_ratio: 0.5,
        });
        let result = BenchmarkResult {
            n_docs: 10,
            mode: BenchmarkMode::HybridRealModel,
            model: None,
            timing_ms: timing,
            corpus_bytes: 1024,
            catalog_bytes: 2048,
            index_elapsed_ms: 1100,
            indexing_files_per_sec: 9.0,
            search_latency_ms: latency(),
            recall_at_k: RecallMetrics {
                k: 5,
                recall: 1.0,
                queries_evaluated: 1,
                queries_with_any_hit: 1,
            },
        };

        // padded_token_positions_per_doc = 1000 / 10 docs = 100.0.
        assert_eq!(result.padded_token_positions_per_doc(), Some(100.0));
        // ms_per_padded_position = 1000ms / 1000 positions = 1.0.
        // projected_document_embedding_ms = 500 real positions * 1.0 = 500.
        // projected_index_elapsed_ms = 10 + 20 + 15 + 500 = 545.
        // projected files/s = 10 docs * 1000 / 545 ms.
        let projected = result
            .projected_files_per_sec_without_padding_skew()
            .unwrap();
        assert!(
            (projected - (10.0 * 1000.0 / 545.0)).abs() < 1e-9,
            "projected throughput {projected} did not match the hand-computed figure"
        );

        let markdown_path = dir.path().join("report.md");
        result.write_markdown(&markdown_path).unwrap();
        let markdown = std::fs::read_to_string(markdown_path).unwrap();
        assert!(markdown.contains("## Embedding Batch Shape (RFC-048 Task 011)"));
        assert!(markdown.contains("| `embed_batch` calls | 10 |"));
        assert!(markdown.contains("| Real / padded ratio | 0.5000 |"));
        assert!(markdown.contains("| Padded token positions / doc | 100.0 |"));
    }

    fn timing_breakdown() -> BenchmarkTimingBreakdown {
        BenchmarkTimingBreakdown {
            corpus_generation_ms: 10,
            extraction_chunking_keyword_index_ms: 20,
            model_load_ms: 15,
            document_embedding_ms: 25,
            embedding_batches: None,
            search: SearchTimingMetrics {
                total_ms: latency(),
                keyword_ms: latency(),
                query_embedding_ms: latency(),
                vector_scan_ms: latency(),
                fusion_ms: latency(),
                enrichment_ms: latency(),
                rerank_ms: latency(),
            },
        }
    }

    fn latency() -> LatencyMetrics {
        LatencyMetrics {
            p50_ms: 1.0,
            p95_ms: 2.0,
            p99_ms: 3.0,
            min_ms: 0.5,
            max_ms: 4.0,
        }
    }
}
