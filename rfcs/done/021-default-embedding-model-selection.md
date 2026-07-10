# RFC-021: Default Embedding Model Selection

**Project:** orbok  
**RFC:** 021  
**Title:** Default Embedding Model Selection  
**Status:** Implemented (v0.7.0)
**Target Timing:** After basic embedding pipeline and benchmark corpus are complete  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

> **Annotation (2026-06-30, RFC-046):** This RFC mentions "candle" once, as a backend-*feasibility evaluation criterion* (§4), during model selection. It did **not** scope a candle inference backend as a deliverable, and no candle backend was implemented. RFC-046 removed the never-implemented candle backend wiring from `orbok-embed` (the `CandleCpu`/`CandleCuda` enum variants remain, routed to a not-supported error). The historical text below is unchanged.

---


## 1. Summary

This future RFC will select the first recommended default embedding model for `orbok`.

The initial implementation should support an embedding model abstraction without overcommitting to a specific model. This RFC should be activated only when `orbok` has stable chunking, local embedding backend abstraction, vector storage, benchmark corpus, retrieval quality metrics, and model registry workflow.

## 2. Motivation

The default embedding model strongly affects semantic search quality, Japanese and mixed-language support, model file size, CPU latency, GPU compatibility, vector dimension, storage size, license obligations, and model installation UX.

Choosing too early risks optimizing for an unmeasured assumption.

## 3. Activation Conditions

Reconsider this RFC when:

1. RFC-006 chunking is implemented.
2. RFC-008 embedding pipeline is implemented.
3. RFC-016 benchmark harness exists.
4. At least two candidate models can be tested.
5. Japanese/mixed-language test queries exist.
6. Storage impact can be measured.

## 4. Candidate Evaluation Criteria

| Criterion | Why It Matters |
|---|---|
| Retrieval quality | Core semantic search value |
| Japanese support | Required for mixed-language use |
| Model size | Download/storage impact |
| Embedding dimension | Vector storage cost |
| CPU latency | Local-first usability |
| GPU backend compatibility | Future performance path |
| License | Redistribution and packaging |
| Runtime support | candle/ONNX/local backend feasibility |
| Stability | Reproducible outputs |

## 5. Required Benchmark Tasks

Benchmark each candidate on:

- English conceptual queries;
- Japanese natural-language queries;
- mixed Japanese-English technical queries;
- source-code-related queries;
- identifier fallback behavior through hybrid search;
- long-document chunk retrieval;
- storage per 100k chunks;
- embedding throughput on CPU.

## 6. Non-Goals

This future RFC should not add hosted embedding APIs, force one model forever, require model bundling, replace keyword search, or solve reranking selection.

## 7. Expected Decision Output

The activated RFC should produce:

```text
recommended default embedding model
fallback model if any
supported model format
backend requirement
dimension
license summary
download/install policy
benchmark report
known limitations
```

## 8. Acceptance Criteria

- At least two candidate models compared.
- Benchmark report exists.
- Japanese/mixed-language performance considered.
- Storage impact measured.
- License reviewed.
- Model installation UX impact reviewed.
- Reindexing implication documented.

## 9. Deferred Decision

No default embedding model is selected by this future RFC draft.

Implement model abstraction first.
