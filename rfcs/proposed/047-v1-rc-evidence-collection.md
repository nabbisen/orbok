# RFC-047: v1.0.0 RC Evidence Collection and Review

**Project:** orbok  
**RFC:** 047  
**Title:** v1.0.0 RC Evidence Collection and Review  
**Status:** Proposed  
**Target milestone:** v1.0.0 release candidate readiness  
**Date:** 2026-07-13  
**Related RFCs:** RFC-016 Benchmark and Retrieval Evaluation Plan; RFC-019 Test Matrix and Release Readiness; RFC-000 RFC lifecycle policy  
**Handoff:** [`HANDOFF-047-v1-rc-evidence-collection.md`](../handoffs/HANDOFF-047-v1-rc-evidence-collection.md)

---

## 1. Summary

This RFC defines the evidence collection and review process required before
orbok can enter a v1.0.0 release-candidate review.

It does not add product behavior, change benchmark thresholds, create a release
candidate, or replace the release readiness gates. It formalizes how the
remaining owner-provided evidence is collected and reviewed.

## 2. Motivation

After v0.23.0, the repository-verifiable readiness story is largely stable:
release gates are documented, CI covers the automatable checks, the `tract`
feature builds, and keyword-only benchmark evidence is green.

The remaining v1.0.0 blockers are evidence-driven:

1. real-model benchmark validation with a local model artifact;
2. manual QA sign-off on Linux, macOS, and Windows;
3. release publication evidence;
4. explicit project-owner confirmation.

These items cannot be completed by code changes alone. The project needs a
durable RFC-level process so review work does not drift into ad hoc cleanup or
implementation.

## 3. Scope

In scope:

- Define the required v1.0.0 evidence collection sequence.
- Define review boundaries for evidence review and release-candidate review.
- Tie the process to existing maintainer docs and readiness ledgers.
- Preserve owner/manual responsibilities separately from repository-verifiable
  checks.

Out of scope:

- Changing RFC-016 benchmark thresholds.
- Changing RFC-019 release readiness gates.
- Promoting `cargo deny` from advisory to blocking.
- Adding product functionality.
- Creating, tagging, or publishing v1.0.0.

## 4. Required Evidence

Before a v1.0.0 release-candidate review can start, the project must have:

1. a real-model benchmark evidence entry;
2. one manual QA evidence entry for Linux;
3. one manual QA evidence entry for macOS;
4. one manual QA evidence entry for Windows.

The evidence templates live in
[`docs/src/maintainers/v1_0_readiness.md`](../../docs/src/maintainers/v1_0_readiness.md).

## 5. Real-Model Benchmark Evidence

The real-model benchmark must use the guarded command documented in the
readiness ledger:

```sh
cargo run -p orbok-bench --release --features orbok-embed/tract -- \
  1000 target/orbok-bench/results-real-model \
  --model-dir /path/to/multilingual-e5-small \
  --expect-mode hybrid-real-model
```

The evidence must include both generated artifacts:

- `orbok-bench-results.json`
- `orbok-bench-report.md`

The JSON evidence must show:

- `"mode": "hybrid-real-model"`;
- a non-null `model` object;
- model id, name, version, and dimension;
- recall@5 >= 0.75;
- p99 search latency <= 200 ms;
- indexing throughput >= 10 files/s.

## 6. Manual QA Evidence

Manual QA evidence must be collected separately for:

- Linux;
- macOS;
- Windows.

Each platform entry must use the evidence template in the v1.0.0 readiness
ledger and must reference the checklist sources:

- [`docs/src/maintainers/release_readiness.md`](../../docs/src/maintainers/release_readiness.md);
- [`docs/src/maintainers/accessibility.md`](../../docs/src/maintainers/accessibility.md).

If a platform passes with notes, the notes must not hide blocking release
issues. Blocking issues must be resolved or explicitly defer the release
candidate.

## 7. Review Boundaries

This RFC defines three boundaries:

1. **Design + handoff review**: review this RFC and its handoff before evidence
   collection is treated as the active process.
2. **Evidence review**: after owner/manual evidence exists, review only the
   supplied evidence and any blocking findings from that evidence.
3. **Release-candidate review**: after evidence review passes and release gates
   are rerun, review the final RC package.

Implementation work should not be started from this RFC unless the accepted
handoff calls for it. Evidence review may produce follow-up implementation work
only when evidence reveals a concrete blocker.

## 8. Acceptance Criteria

This RFC is accepted when:

- the RFC is reviewed as Proposed;
- `HANDOFF-047` is reviewed with it;
- `rfcs/README.md` lists RFC-047 under Proposed;
- future v1.0.0 evidence review packages reference this RFC and handoff.

This RFC is implemented only after the v1.0.0 evidence collection and review
process has been used for the release-candidate path or deliberately withdrawn
by project-owner decision.
