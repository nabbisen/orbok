# Implementation Handoff — RFC-048: Real-Model Benchmark Performance Recovery

**Project:** orbok  
**RFC:** 048  
**Lifecycle stage:** Design + handoff  
**Target milestone:** v1.0.0 benchmark readiness  
**Primary owners:** benchmark/search/embed pipeline  
**RFC:** [`../proposed/048-real-model-performance-recovery.md`](../proposed/048-real-model-performance-recovery.md)

> **Scope rule:** Start with measurement. Do not optimize, change thresholds,
> or reroute the release-candidate path until the benchmark identifies the
> dominant real-model bottleneck.

## 1. Trigger

The real-model benchmark evidence from RFC-047 failed:

- p99 search latency: 843.88 ms, target <= 200 ms;
- indexing throughput: 0.3659 files/s, target >= 10 files/s.

Recall@5 passed at 100%, so the immediate blocker is performance, not retrieval
quality.

## 2. Phase 1 — Benchmark Timing Breakdown

Touch likely files:

- `crates/bench/src/lib.rs`
- `crates/bench/src/metrics.rs`
- `crates/bench/src/report.rs`

Tasks:

1. Add structured timing fields to benchmark results.
2. Split indexing timing into:
   - corpus generation;
   - extraction/chunking/keyword indexing;
   - model load;
   - document embedding generation.
3. Split measured search timing where practical into:
   - query embedding;
   - vector scan;
   - result enrichment / total search.
4. Preserve existing top-level JSON fields so existing evidence readers still
   work.
5. Add unit tests for report serialization/Markdown output.
6. Run keyword-only benchmark smoke to prove existing mode still works.

Validation:

- `cargo fmt --all --check`
- `cargo test -p orbok-bench`
- `cargo run -p orbok-bench -- 10 target/orbok-bench/rfc048-smoke --expect-mode keyword-only`
- `git diff --check`

Review point:

- One implementation review package for timing breakdown only.

## 3. Phase 2 — Bottleneck Classification

After owner reruns the guarded real-model benchmark with timing breakdowns:

1. Review the timing evidence.
2. Classify the dominant bottleneck:
   - document embedding;
   - query embedding;
   - vector scan;
   - enrichment;
   - model load;
   - other.
3. Choose the next implementation handoff slice based on measured evidence.

Review point:

- One evidence review package for the timing run.

## 4. Phase 3 — Targeted Recovery

Potential work, selected only after Phase 2:

- Cross-file document embedding batching.
- Bounded embedding concurrency.
- Query embedding cache for repeated measured queries, if product semantics
  justify it.
- Runtime/model configuration adjustment.
- Exact-scan optimization only if scan dominates.
- Enrichment optimization only if enrichment dominates.

Each selected recovery change gets its own implementation review package.

## 5. Stop Conditions

Stop and return to design review if:

- the measured runtime cannot plausibly meet p99 <= 200 ms with the selected
  model/runtime;
- achieving indexing throughput >= 10 files/s requires changing model choice or
  benchmark policy;
- a proposed fix would change user-visible search semantics;
- release thresholds need amendment.

## 6. Non-goals

- Do not lower thresholds.
- Do not proceed to RC review with failing real-model evidence.
- Do not treat keyword-only evidence as passing real-model evidence.
- Do not implement ANN/HNSW unless measurement shows exact scan is the
  bottleneck.
- Do not change model selection in this handoff.

## 7. Definition of Done

RFC-048 is done when:

- timing breakdown implementation is reviewed and committed;
- measured evidence identifies and resolves the real-model performance
  bottleneck, or a later RFC supersedes the performance policy;
- RFC-047 real-model benchmark evidence passes or is formally superseded by a
  later accepted decision.
