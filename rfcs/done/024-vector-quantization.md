# RFC-024: Vector Quantization

**Project:** orbok  
**RFC:** 024  
**Title:** Vector Quantization  
**Status:** Implemented (v0.8.0)
**Target Timing:** After vector storage size is measured on realistic corpora  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will decide whether and how `orbok` should quantize embedding vectors.

The initial implementation should prioritize correctness and benchmark baseline quality before quantization.

## 2. Motivation

Vector storage may become large. Quantization can reduce storage and memory use, but it may reduce retrieval quality and complicate debugging.

Possible formats include FP16, INT8 scalar quantization, binary quantization, product quantization, and model-specific compressed formats.

## 3. Activation Conditions

Reconsider this RFC when:

1. Embedding pipeline is implemented.
2. Vector storage size is measured.
3. Retrieval benchmark exists.
4. At least one large realistic corpus has been indexed.
5. Storage pressure is confirmed.

## 4. Candidate Strategies

| Strategy | Expected Benefit | Risk |
|---|---|---|
| FP16 | Moderate size reduction | Small precision impact |
| INT8 scalar | Large size reduction | Relevance loss |
| Binary | Very small vectors | Major relevance risk |
| Product quantization | Large-scale compression | Complexity |
| Keep FP32 | Highest quality | Storage cost |

## 5. Evaluation Metrics

Measure top-k recall vs FP32, MRR/nDCG vs FP32, Japanese query impact, exact identifier hybrid impact, vector storage size, vector search latency, quantization time, and rebuild cost.

## 6. Storage Mode Interaction

| Mode | Vector Format |
|---|---|
| High Accuracy | FP32 or FP16 |
| Balanced | FP16 or FP32 |
| Space Saving | INT8 if validated |

Do not enable aggressive quantization by default without benchmark evidence.

## 7. localcache Interaction

Embedding bundle namespace must include vector format:

```text
embedding-bundle:<model_id>:<vector_format>:v1
```

Changing vector format invalidates relevant embedding/vector cache payloads.

## 8. Non-Goals

This future RFC should not change embedding model selection, introduce ANN automatically, sacrifice exact keyword search, or quantize before baseline measurement.

## 9. Expected Decision Output

The activated RFC should produce:

```text
selected vector formats
default per storage mode
conversion/rebuild policy
quality impact
storage impact
benchmark report
migration plan
```

## 10. Acceptance Criteria

- FP32 baseline exists.
- Quantized formats compared against FP32.
- Storage savings measured.
- Quality loss measured.
- Vector format migration defined.
- User-visible storage mode impact defined.

## 11. Deferred Decision

Do not implement vector quantization as default behavior until measured storage pressure and quality impact justify it.
