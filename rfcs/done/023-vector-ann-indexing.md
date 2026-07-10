# RFC-023: Vector ANN Indexing

**Project:** orbok  
**RFC:** 023  
**Title:** Vector ANN Indexing  
**Status:** Implemented (v0.8.0)
**Target Timing:** After exact vector search benchmark shows unacceptable latency  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will decide whether `orbok` needs approximate nearest neighbor indexing for vector search.

The initial implementation should use exact vector search or a simple `VectorStore` abstraction until benchmarks demonstrate that exact search is too slow.

## 2. Motivation

ANN can improve vector retrieval latency on large collections, but it adds complexity: index construction, persistence, compaction, deletion handling, recall/latency tradeoffs, platform dependencies, recovery, and additional storage.

## 3. Activation Conditions

Reconsider this RFC when:

1. Exact vector search exists.
2. A realistic benchmark corpus exists.
3. Vector search latency exceeds acceptable thresholds.
4. Candidate count and chunk count are measured.
5. Storage and memory profiles are available.

## 4. Candidate Algorithms / Engines

Candidates may include HNSW, IVF, product quantization-based indexes, memory-mapped exact scan with SIMD, a dedicated Rust vector search crate, or an external segment-based index.

## 5. Evaluation Criteria

| Criterion | Why It Matters |
|---|---|
| Recall@K | Must preserve quality |
| Query latency | Main reason for ANN |
| Build time | Initial indexing cost |
| Incremental update | Local files change |
| Delete handling | Stale chunks must be removed |
| Disk size | Storage efficiency |
| Memory use | Desktop usability |
| Crash recovery | Index rebuild/repair |
| Rust integration | Maintenance |

## 6. Required Benchmarks

Compare exact scan, optimized exact scan, and ANN candidates.

Metrics:

- recall@10 vs exact;
- latency p50/p95;
- build time;
- memory use;
- disk size;
- stale deletion cost;
- rebuild time.

## 7. Non-Goals

This future RFC should not replace keyword search, require ANN before vector MVP, select quantization automatically, or introduce a server process.

## 8. Expected Decision Output

The activated RFC should produce:

```text
selected ANN strategy or decision to keep exact search
index file format
rebuild policy
delete/stale policy
benchmark evidence
storage impact
recovery plan
```

## 9. Acceptance Criteria

- Exact vector baseline measured.
- ANN recall compared to exact.
- Latency gain justifies complexity.
- Storage overhead measured.
- Recovery/rebuild plan defined.
- Stale chunk deletion behavior defined.

## 10. Deferred Decision

Do not implement ANN until exact search becomes a measured bottleneck.
