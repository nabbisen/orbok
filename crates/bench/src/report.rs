//! Benchmark report writer (RFC-016 §12): JSON and Markdown output.

use crate::metrics::{LatencyMetrics, RecallMetrics, SearchTimingMetrics};
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
    use crate::metrics::{LatencyMetrics, RecallMetrics, SearchTimingMetrics};

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

    fn timing_breakdown() -> BenchmarkTimingBreakdown {
        BenchmarkTimingBreakdown {
            corpus_generation_ms: 10,
            extraction_chunking_keyword_index_ms: 20,
            model_load_ms: 15,
            document_embedding_ms: 25,
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
