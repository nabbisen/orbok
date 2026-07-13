# RFC-048: Real-Model Benchmark Performance Recovery

**Project:** orbok  
**RFC:** 048  
**Title:** Real-Model Benchmark Performance Recovery  
**Status:** Proposed  
**Target milestone:** v1.0.0 benchmark readiness  
**Date:** 2026-07-13  
**Related RFCs:** RFC-016 Benchmark and Retrieval Evaluation Plan; RFC-021 Default Embedding Model Selection; RFC-023 Vector ANN Indexing; RFC-047 v1.0.0 RC Evidence Collection and Review  
**Handoff:** [`HANDOFF-048-real-model-performance-recovery.md`](../handoffs/HANDOFF-048-real-model-performance-recovery.md)

---

## 1. Summary

This RFC defines the follow-up work required after real-model benchmark evidence
failed the v1.0.0 performance thresholds.

The first accepted action is measurement: split benchmark timing into model
load, extraction/chunking/keyword indexing, embedding generation, query
embedding, vector scan, enrichment, and total search latency. Optimization work
must follow the measured bottleneck rather than guessing.

## 2. Triggering Evidence

RFC-047 evidence review recorded structurally valid real-model evidence:

- mode: `hybrid-real-model`;
- model: `multilingual-e5-small`, v1, 384 dimensions;
- recall@5: 100%.

It failed two release thresholds:

- p99 search latency: observed 843.88 ms, required <= 200 ms;
- indexing throughput: observed 0.3659 files/s, required >= 10 files/s.

The evidence review package is
`.git-exclude/review-request/048-real-model-benchmark-evidence-review.md`.

## 3. Motivation

The current result is too slow for v1.0.0, but the evidence does not yet prove
which subsystem dominates the cost.

Likely contributors include:

- one real ONNX query embedding per measured search;
- serial document embedding during benchmark indexing;
- per-file rather than cross-file embedding batches;
- exact vector scan and result enrichment;
- benchmark timing that currently reports a single indexing duration and a
  single end-to-end search latency distribution.

The project needs a measured recovery path before deciding whether to optimize
batching, search latency, model selection, benchmark methodology, or release
threshold policy.

## 4. Scope

In scope:

- Add benchmark timing breakdowns needed to identify the bottleneck.
- Preserve existing end-to-end benchmark metrics.
- Investigate query-time and indexing-time model inference costs.
- Propose and implement targeted optimizations only after timings identify the
  bottleneck.
- Rerun guarded real-model benchmark evidence after changes.

Out of scope:

- Lowering RFC-016 thresholds in this RFC.
- Treating keyword-only benchmark evidence as a substitute for real-model
  evidence.
- Replacing `multilingual-e5-small` without a follow-up model-selection
  decision.
- Introducing ANN/HNSW before measurement shows exact scan is the bottleneck.
- Proceeding to v1.0.0 release-candidate review while thresholds fail.

## 5. Required Measurement

The benchmark report must keep the existing top-level metrics and add enough
timing detail to distinguish at least:

- corpus generation;
- extraction/chunking/keyword indexing;
- model load;
- document embedding generation;
- query embedding;
- vector scan;
- result enrichment;
- total search latency.

The JSON report should include machine-readable timing fields. Markdown should
summarize them for release review.

## 6. Recovery Decision Rules

After measurement:

- If document embedding dominates indexing, optimize batching/concurrency or
  document text preparation before changing thresholds.
- If query embedding dominates p99, investigate query embedding caching,
  backend/runtime configuration, or model/runtime selection.
- If exact vector scan dominates p99 at 1,000 documents, revisit RFC-023's exact
  scan decision with measured evidence.
- If enrichment dominates p99, optimize result loading/snippet enrichment.
- If the selected real model/runtime cannot plausibly meet thresholds on target
  hardware, open a follow-up decision RFC rather than silently relaxing gates.

## 7. Acceptance Criteria

This RFC is accepted when:

- the RFC and `HANDOFF-048` are reviewed;
- the measurement-first recovery sequence is approved;
- review agrees not to proceed to RC review until real-model p99 and indexing
  throughput meet the documented thresholds or a later RFC changes them.

It is implemented when:

- benchmark timing breakdowns exist;
- targeted performance changes, if needed, are implemented and reviewed;
- guarded real-model benchmark evidence passes RFC-047 thresholds, or a later
  accepted RFC changes the benchmark policy.
