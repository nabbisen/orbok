# RFC-010: Optional Local Reranking

**Project:** orbok  
**RFC:** 010  
**Title:** Optional Local Reranking  
**Status:** Implemented (v0.4.0)
**Target Milestone:** M11  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines optional local reranking for `orbok`.

The central decision is:

> Reranking must be optional, local-only, bounded by candidate limits, and never required for the search pipeline to return results.

Reranking refines the top candidates produced by hybrid search. It improves result quality but can be expensive on local machines.

---

## 2. Motivation

Hybrid keyword/vector search produces good candidates, but rank fusion is still coarse. A cross-encoder or reranker can evaluate query-document relevance more precisely.

However:

- rerankers are slower than embedding search;
- CPU-only execution may be too slow;
- model files add storage;
- long chunks may exceed model limits;
- reranker availability should not block search.

Therefore reranking must be a quality enhancement, not a core dependency.

---

## 3. Goals

- Support optional local reranking.
- Keep search usable without reranker.
- Bound reranker cost by top-N limit.
- Track reranker model/version.
- Invalidate rerank cache when model changes.
- Provide clear UI explanation.
- Avoid sending query or document text externally.
- Allow Fast mode to disable reranking.

---

## 4. Non-Goals

- This RFC does not choose a final reranker model.
- This RFC does not implement hosted reranking.
- This RFC does not require reranking on every search.
- This RFC does not replace RRF.
- This RFC does not define model installation UX.

---

## 5. Reranking Position in Pipeline

```text
keyword retrieval
  + vector retrieval
  -> RRF fusion
  -> top N candidates
  -> optional reranker
  -> final rank
  -> snippet loading/display
```

Reranker input should be limited to top fused candidates.

---

## 6. Reranker Model Requirements

A reranker model candidate should be evaluated by:

- local inference feasibility;
- multilingual/Japanese support;
- model size;
- CPU latency;
- GPU support;
- license;
- maximum input length;
- relevance quality.

The architecture should support replacing the model.

---

## 7. Backend Abstraction

```rust
pub trait RerankBackend {
    fn backend_name(&self) -> &'static str;
    fn load_model(&self, model: &ModelRecord) -> Result<Box<dyn RerankModel>>;
}

pub trait RerankModel {
    fn model_id(&self) -> ModelId;
    fn rerank(&self, input: RerankRequest) -> Result<Vec<RerankScore>>;
}
```

## 7.1. RerankRequest

```rust
pub struct RerankRequest {
    pub query: String,
    pub candidates: Vec<RerankCandidateInput>,
}
```

## 7.2. RerankCandidateInput

```rust
pub struct RerankCandidateInput {
    pub chunk_id: ChunkId,
    pub text: String,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    pub source_type: Option<String>,
}
```

## 7.3. RerankScore

```rust
pub struct RerankScore {
    pub chunk_id: ChunkId,
    pub score: f32,
}
```

---

## 8. Rerank Input Text

Reranker input should be compact.

Recommended construction:

```text
Title: <title>
Section: <heading path>
Text:
<chunk text or compact context>
```

Rules:

- keep within model token limit;
- prefer child chunk text;
- include heading/title context;
- avoid loading entire parent section if too long;
- do not include unrelated file contents.

Input construction must be versioned:

```text
rerank_text_builder_version = "rerank-text-v1"
```

---

## 9. Candidate Limits

Recommended defaults:

| Search Mode | Rerank Top N |
|---|---:|
| Fast | 0 |
| Exact | 0 |
| Auto | 20 if enabled |
| Conceptual | 20 if enabled |
| Deep | 50 if enabled |

User settings may allow:

```text
disabled
top 20
top 50
top 100
```

Top 100 should be considered advanced and may be slow.

---

## 10. Fallback Behavior

Search must return results if:

- reranker model is missing;
- reranker backend fails to load;
- reranking times out;
- reranking is canceled;
- candidate text cannot be loaded.

Fallback:

```text
return RRF-fused ranking
show warning or subtle notice
```

Do not fail the whole search.

---

## 11. Timeout and Cancellation

Reranking should support:

- cancellation when user changes query;
- timeout policy;
- background progress;
- partial failure handling.

Recommended initial timeout:

```text
configurable; conservative default
```

Avoid hardcoding a universal timeout until benchmarked.

---

## 12. Rerank Cache

Rerank cache may store query/candidate/model score tuples.

Cache key should include:

```text
query hash
chunk_id
chunk content hash
reranker model id
rerank text builder version
```

Cache value:

```text
score
created_at
expires_at
```

Privacy policy:

- if search history is disabled, avoid storing raw query text;
- use query hash where possible;
- cache may be disabled in privacy-strict mode.

Use the `orbok` cache tables or a dedicated ephemeral cache. Do not store rerank cache in `localcache` because it is query-derived rather than file-derived.

---

## 13. Model Change Handling

When reranker model changes:

- invalidate rerank cache for old model;
- no need to reindex files;
- no need to regenerate embeddings.

This is different from embedding model changes.

---

## 14. Source Change Handling

If chunk content hash changes:

- invalidate rerank cache for that chunk;
- rerank using new text after reindexing.

If source file is stale:

- either avoid reranking stale chunks or mark result stale;
- never present reranked stale result as fresh.

---

## 15. UI Requirements

## 15.1. Settings

Reranking settings:

```text
Reranking: disabled | default | enabled
Rerank candidate count: 20 | 50 | 100
```

User-facing label:

```text
Deep result refinement
```

Avoid default UI term:

```text
Cross-Encoder
```

## 15.2. Search Status

When reranking is active:

```text
Refining top results locally...
```

When reranker missing:

```text
Deep result refinement is unavailable. Showing fused search results.
```

When reranking times out:

```text
Reranking took too long. Showing initial search results.
```

## 15.3. Result Badge

```text
[Refined]
```

or:

```text
[Reranked]
```

Prefer plain language:

```text
[Refined]
```

---

## 16. Performance Considerations

Reranking is expensive because it evaluates query-candidate pairs.

Strategies:

- top-N limit;
- Fast mode disables it;
- lazy model loading;
- batch inference if backend supports it;
- cache scores;
- cancel stale rerank jobs;
- show fused results first in UI if rerank is slow.

---

## 17. Security and Privacy

Reranking uses query text and chunk text.

Rules:

- run locally by default;
- do not upload query or candidate text;
- do not log candidate text;
- do not store raw query text if privacy setting disables it;
- treat rerank cache as sensitive ephemeral data.

---

## 18. API Impact

Search request:

```json
{
  "query": "token rotation policy",
  "mode": "auto",
  "rerank": "default",
  "rerank_top_n": 20
}
```

Search response:

```json
{
  "results": [],
  "rerank_status": {
    "used": true,
    "model_id": "model_...",
    "candidate_count": 20,
    "duration_ms": 420
  }
}
```

Fallback response:

```json
{
  "rerank_status": {
    "used": false,
    "reason": "model_missing"
  }
}
```

---

## 19. Testing Requirements

Required tests:

1. Search works with reranking disabled.
2. Search falls back when reranker missing.
3. Rerank top N limit is enforced.
4. Reranker changes final order when scores differ.
5. Timeout returns fused results.
6. Query change cancels previous rerank job.
7. Rerank cache invalidates on model change.
8. Rerank cache invalidates on chunk content hash change.
9. Raw candidate text is not logged.
10. Fast mode disables reranking.

---

## 20. Acceptance Criteria

- Reranker is optional.
- Missing reranker does not break search.
- Rerank model is represented in model registry.
- Rerank top-N limit exists.
- Rerank input text is bounded.
- Rerank status is exposed to UI.
- Rerank cache is ephemeral and privacy-aware.
- Rerank cache invalidates correctly.
- Search remains usable on CPU-only systems.

---

## 21. Unresolved Questions

- Which reranker model should be recommended first?
- Should reranking stream partial updates to UI?
- Should rerank cache be enabled by default?
- What should the default timeout be?
- Should stale results be reranked or excluded?
- Should reranking use parent context for short sections?

---

## 22. Decision

Implement optional local reranking after hybrid search is stable.

Keep reranking disabled or conservative by default until performance and model quality are validated.
