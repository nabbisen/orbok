# Implementation Handoff — RFC-047: v1.0.0 RC Evidence Collection and Review

**Project:** orbok  
**RFC:** 047  
**Lifecycle stage:** Design + handoff  
**Target milestone:** v1.0.0 release candidate readiness  
**Primary owners:** project owner for manual evidence; Codex/maintainer for evidence review packaging  
**RFC:** [`../proposed/047-v1-rc-evidence-collection.md`](../proposed/047-v1-rc-evidence-collection.md)

> **Scope rule:** This handoff coordinates evidence collection and review. It
> does not authorize new product implementation, change release gates, or create
> a release candidate by itself.

## 1. Inputs

- [`docs/src/maintainers/v1_0_readiness.md`](../../docs/src/maintainers/v1_0_readiness.md)
- [`docs/src/maintainers/release_readiness.md`](../../docs/src/maintainers/release_readiness.md)
- [`docs/src/maintainers/benchmark_report.md`](../../docs/src/maintainers/benchmark_report.md)
- [`docs/src/maintainers/accessibility.md`](../../docs/src/maintainers/accessibility.md)

## 2. Owner / Manual Evidence Tasks

1. Run the guarded real-model benchmark command from the v1.0.0 readiness
   ledger.
2. Preserve the generated benchmark artifacts:
   - `orbok-bench-results.json`
   - `orbok-bench-report.md`
3. Fill the real-model benchmark evidence template.
4. Run manual QA on Linux.
5. Fill the Linux manual QA evidence template.
6. Run manual QA on macOS.
7. Fill the macOS manual QA evidence template.
8. Run manual QA on Windows.
9. Fill the Windows manual QA evidence template.
10. Record any blocking issues found during benchmark or manual QA work.

## 3. Codex / Maintainer Evidence Review Tasks

After owner/manual evidence exists:

1. Create one evidence review request package under `.git-exclude/review-request/`.
2. Review the real-model benchmark JSON and Markdown.
3. Confirm the JSON evidence includes:
   - `"mode": "hybrid-real-model"`;
   - a non-null `model` object;
   - model id, name, version, and dimension.
4. Confirm benchmark thresholds:
   - recall@5 >= 0.75;
   - p99 search latency <= 200 ms;
   - indexing throughput >= 10 files/s.
5. Review manual QA evidence for Linux, macOS, and Windows.
6. Report blocking issues first, if any.
7. If evidence passes, recommend proceeding to release-candidate rehearsal.

## 4. Release-Candidate Review Tasks

Only after evidence review passes:

1. Rerun the release gates required by
   [`release_readiness.md`](../../docs/src/maintainers/release_readiness.md).
2. Produce release archive and checksum evidence.
3. Confirm changelog and version metadata state.
4. Prepare one release-candidate review request package.
5. Do not tag, publish, or call v1.0.0 released without explicit project-owner
   confirmation.

## 5. Stop Conditions

Stop and return to design or implementation planning if:

- real-model benchmark evidence fails a threshold;
- benchmark JSON is not `hybrid-real-model`;
- any manual QA platform has a blocking issue;
- release gates fail during RC rehearsal;
- the owner decides to defer v1.0.0.

## 6. Non-goals

- Do not add new product features.
- Do not alter benchmark thresholds.
- Do not promote advisory `cargo deny` policy.
- Do not treat keyword-only benchmark evidence as a substitute for real-model
  validation.
- Do not create additional readiness-doc implementation packages unless an
  accepted RFC/handoff explicitly calls for them.

## 7. Definition of Done

The handoff is complete when either:

- the v1.0.0 evidence review and RC review are completed under this process; or
- the project owner withdraws or supersedes RFC-047.
