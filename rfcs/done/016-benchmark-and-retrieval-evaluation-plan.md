# RFC-016: Benchmark and Retrieval Evaluation Plan

**Project:** orbok  
**RFC:** 016  
**Title:** Benchmark and Retrieval Evaluation Plan  
**Status:** Implemented (v0.5.0)
**Target Milestone:** M13  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the benchmark and retrieval evaluation plan for `orbok`.

The central decision is:

> `orbok` must be evaluated across four dimensions: retrieval quality, latency, storage growth, and indexing cost. Storage efficiency and search quality must be measured together rather than optimized independently.

---

## 2. Motivation

`orbok` combines exact search, semantic search, hybrid RRF, and optional reranking. It also aims to minimize storage by avoiding full source duplication and controlling derived indexes.

Without benchmarks, the project cannot safely decide:

- chunk size;
- embedding model;
- vector format;
- keyword search backend;
- reranker default;
- cache retention policy;
- Japanese tokenizer strategy;
- storage mode defaults.

The benchmark plan must be introduced before release hardening, not after performance regressions are discovered.

---

## 3. Goals

- Define retrieval quality metrics.
- Define performance metrics.
- Define storage metrics.
- Define indexing metrics.
- Define benchmark corpora.
- Define repeatable benchmark execution.
- Support regression tracking in CI or release validation.
- Provide evidence for future decisions such as quantization, ANN, and Japanese tokenization.

---

## 4. Non-Goals

- This RFC does not select the final embedding model.
- This RFC does not require a massive public benchmark corpus.
- This RFC does not require cloud evaluation.
- This RFC does not implement telemetry.
- This RFC does not require benchmarking user-private documents.

---

## 5. Benchmark Categories

`orbok` should benchmark:

1. **Retrieval Quality**
2. **Search Latency**
3. **Indexing Throughput**
4. **Storage Growth**
5. **Memory Use**
6. **Cache Effectiveness**
7. **Failure and Recovery Cost**

---

## 6. Retrieval Quality Metrics

Required metrics:

| Metric | Meaning |
|---|---|
| Top-k recall | Whether relevant result appears in top K |
| MRR | Mean reciprocal rank of first relevant result |
| nDCG | Ranking quality across top results |
| Exact identifier success rate | Whether exact IDs, codes, versions are found |
| No-result false negative rate | Query should match but returns nothing |
| Stale-result rate | How often stale results appear when fresh exists |
| Rerank improvement rate | Whether reranking improves ranking |

Recommended K values:

```text
top 1
top 3
top 5
top 10
top 20
```

---

## 7. Performance Metrics

Required metrics:

| Metric | Unit |
|---|---|
| cold startup time | ms |
| warm startup time | ms |
| keyword search latency | ms |
| vector search latency | ms |
| hybrid fusion latency | ms |
| rerank latency | ms |
| snippet loading latency | ms |
| first-result latency | ms |
| full-result latency | ms |

Measure separately for:

- model missing;
- model loaded;
- CPU-only;
- GPU-enabled where available;
- rerank disabled;
- rerank enabled.

---

## 8. Indexing Metrics

Required metrics:

| Metric | Unit |
|---|---|
| file scan rate | files/sec |
| extraction throughput | MB/sec or chars/sec |
| chunking throughput | chunks/sec |
| keyword index throughput | chunks/sec |
| embedding throughput | chunks/sec |
| full initial indexing time | seconds/minutes |
| incremental indexing time | seconds |
| failed-file isolation | pass/fail |

---

## 9. Storage Metrics

Required metrics:

| Category | Metric |
|---|---|
| persistent catalog | size per file/chunk |
| exact search index | bytes per chunk |
| semantic search index | bytes per vector/chunk |
| localcache extraction cache | bytes per source MB |
| snippet cache | bytes per snippet |
| model files | total model size |
| logs | size after normal usage |

Storage must be reported by lifecycle category, matching RFC-011.

---

## 10. Benchmark Corpora

## 10.1. Synthetic Corpus

Create deterministic generated documents for regression testing.

Includes:

- short text files;
- long Markdown files;
- code-like files;
- CSV files;
- repeated identifiers;
- changed/deleted files;
- mixed English/Japanese strings.

Purpose:

- reproducible CI tests;
- no licensing issue;
- easy expected-result labels.

## 10.2. Fixture Corpus

Small curated fixtures:

- sample PDF;
- sample DOCX;
- sample HTML;
- Markdown technical notes;
- source code with comments;
- Japanese/mixed-language documents.

Purpose:

- extraction quality;
- location metadata;
- snippet correctness.

## 10.3. Local Private Corpus

Optional developer-only benchmark.

Rules:

- not committed to repository;
- no telemetry upload;
- results may be recorded only as aggregate metrics;
- paths/content redacted.

---

## 11. Query Set Design

Query types:

| Query Type | Example |
|---|---|
| exact identifier | `ABC-1234` |
| version | `v0.19.0` |
| code symbol | `refresh_token` |
| natural-language English | `how to rotate client secrets` |
| natural-language Japanese | `認証トークンの有効期限` |
| mixed-language | `OAuth クライアント 設定` |
| filename/path | `auth.md` |
| long conceptual query | `policy for expiring long-lived credentials` |

Each query should have expected relevant document/chunk IDs.

---

## 12. Benchmark Execution

Recommended command:

```text
cargo run -p orbok-bench --release -- 1000 target/orbok-bench/results
```

Recommended output:

```text
target/orbok-bench/results/orbok-bench-results.json
target/orbok-bench/results/orbok-bench-report.md
```

JSON is for tooling. Markdown is for human review.

---

## 13. Regression Thresholds

Initial thresholds should be loose and tightened later.

Examples:

| Metric | Initial Gate |
|---|---|
| exact identifier top-5 recall | must not regress |
| keyword-only search | must work |
| no document upload | must pass |
| safe cleanup preserves sources | must pass |
| indexing crash recovery | must pass |
| startup smoke test | must pass |

Do not set aggressive latency gates before realistic hardware data exists.

---

## 14. Hardware Profiles

At minimum, record:

```text
OS
CPU
RAM
GPU
storage type
embedding backend
model name/version
build profile
```

Suggested profiles:

- low-end CPU-only laptop;
- normal developer laptop;
- GPU-enabled workstation;
- Windows laptop;
- macOS laptop;
- Linux desktop.

---

## 15. Reporting

Benchmark report should include:

```text
environment
app version
schema version
model version
corpus version
query set version
metrics table
regressions
warnings
```

Avoid including document content.

---

## 16. Interaction with RFC Decisions

Benchmark results should inform:

- RFC-006 chunk size;
- RFC-007 keyword backend;
- RFC-008 vector format;
- RFC-009 candidate limits;
- RFC-010 rerank default;
- RFC-014 Japanese strategy;
- RFC-011 storage defaults.

---

## 17. Acceptance Criteria

- Benchmark harness exists.
- Synthetic corpus exists.
- Fixture corpus exists.
- Query labels exist.
- Retrieval quality metrics are computed.
- Indexing and storage metrics are computed.
- Benchmark output is JSON and Markdown.
- No benchmark requires uploading documents.
- Benchmark does not log document content by default.
- Release candidate includes benchmark report.

---

## 18. Testing Requirements

Required tests:

1. Benchmark command runs on synthetic corpus.
2. Metrics output is parseable JSON.
3. Markdown report is generated.
4. Exact identifier query has expected top-k result.
5. Japanese fixture query is included.
6. Storage categories are measured.
7. Benchmark redacts file paths when configured.
8. Benchmark failure exits nonzero for release-gated suites.

---

## 19. Unresolved Questions

- Should benchmark harness be a separate crate?
- Which public fixture documents can be redistributed?
- Should benchmark report be attached to release artifacts?
- Should CI run only synthetic benchmarks?
- What hardware should define release baseline?

---

## 20. Decision

Introduce benchmarking as a first-class engineering artifact.

Do not finalize storage/search defaults without measured evidence.
