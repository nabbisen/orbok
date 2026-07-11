//! Benchmark report writer (RFC-016 §12): JSON and Markdown output.

use crate::metrics::{LatencyMetrics, RecallMetrics};
use std::fs;
use std::path::Path;

/// Complete benchmark result.
#[derive(Debug, serde::Serialize)]
pub struct BenchmarkResult {
    pub n_docs: usize,
    pub corpus_bytes: u64,
    pub catalog_bytes: u64,
    pub index_elapsed_ms: u64,
    pub indexing_files_per_sec: f64,
    pub search_latency_ms: LatencyMetrics,
    pub recall_at_k: RecallMetrics,
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
        let md = format!(
            "# orbok Benchmark Report\n\n\
             ## Corpus\n\n\
             | Metric | Value |\n|---|---|\n\
             | Documents | {} |\n\
             | Corpus size | {:.1} KiB |\n\
             | Catalog size | {:.1} KiB |\n\
             | Bytes per doc | {:.0} |\n\n\
             ## Indexing\n\n\
             | Metric | Value |\n|---|---|\n\
             | Total time | {} ms |\n\
             | Throughput | {:.1} files/s |\n\n\
             ## Search Latency\n\n\
             | Percentile | Latency |\n|---|---|\n\
             | p50 | {:.2} ms |\n\
             | p95 | {:.2} ms |\n\
             | p99 | {:.2} ms |\n\
             | min | {:.2} ms |\n\
             | max | {:.2} ms |\n\n\
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
            self.search_latency_ms.p50_ms,
            self.search_latency_ms.p95_ms,
            self.search_latency_ms.p99_ms,
            self.search_latency_ms.min_ms,
            self.search_latency_ms.max_ms,
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
             p50: {:.2}ms  p99: {:.2}ms  |  Recall@{}: {:.0}%",
            self.n_docs,
            self.index_elapsed_ms,
            self.indexing_files_per_sec,
            self.search_latency_ms.p50_ms,
            self.search_latency_ms.p99_ms,
            self.recall_at_k.k,
            self.recall_at_k.recall * 100.0,
        );
    }
}
