# RFC-058: Verifying the Wired Application

**Project:** orbok\
**RFC:** 058\
**Title:** Verifying the Wired Application\
**Status:** Accepted\
**Accepted:** 2026-09-02 by the project owner\
**Target milestone:** verification integrity\
**Date:** 2026-09-01\
**Related RFCs:** RFC-000 (acceptance-criteria convention lives in each RFC, but this one constrains how they may be phrased); RFC-016 Benchmark and Retrieval Evaluation Plan (§7 amends its measurement boundary); RFC-019 Test Matrix and Release Readiness (§6 adds a gate); RFC-056 Hosting the Indexing Scheduler (its handoff §4 states this rule for one subsystem; this generalises it)

---

## 1. Summary

Six shipped features do not run. An external audit (2026-09-01) found that
result trust, result filtering, folder-scoped search, source-status filtering,
reranking, and re-scanning are each implemented in a crate, unit-tested there,
and never called by the application. All six have green test suites. Five of the
RFCs that specified them sit in `rfcs/done/` marked `Implemented`.

This RFC does not fix any of the six. It fixes the two mechanisms that let all
six pass review:

```text
1. Acceptance criteria phrased as capability ("X exists", "X is available")
   are satisfied by a library type and cannot detect an unwired feature.
2. No test in this project runs the application's own entry points against a
   real corpus, so nothing observes whether the application calls anything.
```

The proposal is one rule for how criteria are written, one end-to-end test that
becomes a release gate, and one correction to the benchmark harness so that
performance measurement covers the path the product actually runs.

---

## 2. Motivation

### 2.1 The specification half

Read these criteria from RFCs currently marked Implemented:

| RFC | Criterion | Satisfied by |
|---|---|---|
| 010 §20 | "Reranker is optional." "Rerank top-N limit exists." "Rerank status is exposed to UI." | `MockReranker`, which scores by passage length and has no production caller. **No criterion in §20 asks that a reranker reorder anything.** |
| 037 §21.3, §21.4 | "Startup check exists." "Manual refresh exists." | The `source_lifecycle` module existing. Neither behaviour exists; the module has zero production callers. |
| 041 §25.3 | "Active filters are visible and individually removable." | Chips rendered in the UI. Nothing asks that a filter filter. |
| 045 §22.7 | "Default search scope is 'This folder and subfolders.'" | A default value in UI state. Nothing asks that the folder scope the query. |
| 008 §23 | "chunks *can be* embedded locally" | The capability. Recorded in a prior review: truthfully checkable while nothing was embedded. |

Every one of these is *true*. Every one was checkable at review time. None of
them could fail while the feature was unreachable.

RFC-038 is the counter-example that proves the point. Its §16 criteria 4, 5 and
7 — "Missing files do not appear as normal ready results", "Changed files show
Needs update", "Recovery actions are available" — **are** behaviour-phrased, and
they are false: `bootstrap/search.rs:79` hardcodes `ResultTrustDisplay::default()`,
which is `state: Ready, recovery_actions: []`, for every result. Those criteria
were capable of failing. They were simply never executed.

So the specification defect and the testing defect are the same defect seen
twice: **a claim nobody ran.**

### 2.2 The measurement half

`crates/bench/src/metrics.rs:130` constructs `HybridSearchService` **outside**
the timing loop, with an already-loaded model handed in. The application
constructs the model **inside** every search (`bootstrap/search.rs:43` →
`tract_backend.rs:96-104`: a 17 MB tokenizer parse, a ~470 MB ONNX protobuf
parse, `into_optimized()`, `into_runnable()`).

The harness therefore cannot observe the largest single cost in the product.
This is not a small inaccuracy. It explains an open puzzle this project has
carried since v0.20.0 — keyword-only p99 green, real-model p99 failing by 4× —
and it means RFC-048's measurement-first sequence has been measuring a region
that excludes its own answer since the day it opened.

### 2.3 Why this RFC comes before the fixes

The audit's remediation roadmap lists six features to wire. Wiring them without
this RFC fixes six symptoms. The same review process that passed all six is the
process that will review the fixes.

---

## 3. Goals

- Make it impossible for an RFC to be marked Implemented on criteria that a
  library type satisfies without the application calling it.
- Give the project one test that fails when a feature stops being reachable.
- Make the performance harness measure the path the product runs.
- Re-state the nine currently-false RFC statuses honestly (execution belongs to
  RFC-063; this RFC supplies the criterion by which they were judged false).

## 4. Non-Goals

- Wiring any of the six features. That is RFC-060, RFC-061 and Task 035.
- Property-based testing, fuzzing, or golden-file retrieval regression. All are
  worth doing and all are separate.
- Changing RFC-000's lifecycle states. That is RFC-063.
- A general integration-test framework. This RFC asks for one test, deliberately.

---

## 5. Decision 1 — how acceptance criteria are phrased

**Rule.** Every acceptance criterion in every future RFC must be falsifiable by
observing the running application. Concretely, a criterion must name:

```text
a starting state → an action taken through the application's own entry point
                 → an observable outcome
```

**Banned phrasings**, because a library type satisfies them: *"X exists"*,
*"X is available"*, *"X is supported"*, *"X is explicit"*, *"X is exposed to
UI"*, *"X can be done"*, *"X is optional"*.

**Required shape**, by example — the same features as §2.1, rewritten:

| Instead of | Write |
|---|---|
| "Manual refresh exists." | "With a source registered and a file edited on disk, invoking Rescan causes that file's new content to be returned by a subsequent search." |
| "Reranker is optional." | "With reranking enabled and a corpus where fusion rank 1 is a weaker match than fusion rank 4, the returned order differs from the fusion order." (And, separately: "with reranking disabled, search returns fusion order and does not error.") |
| "Active filters are visible and individually removable." | "With a `.pdf` and a `.md` both matching a query, applying the Documents filter returns only the `.pdf`." |
| "Result trust states are explicit." | "With an indexed file deleted from disk, its result carries a non-Ready trust state and at least one recovery action." |

Criteria about *appearance* (labels, colour independence, copy) stay as they
are; they are already observations of the running UI. This rule targets criteria
about *capability*.

**Applies to:** every RFC written after this one is accepted. Existing RFCs are
not rewritten wholesale — but §6's gate applies to all of them, and RFC-063
decides what happens to the nine whose status is currently false.

**Cost:** criteria get longer. That is the point; a criterion short enough to be
unfalsifiable is not doing work.

## 6. Decision 2 — the end-to-end reachability test

**One test module, one job: prove the application's own entry points produce
correct results against a real corpus on disk.**

Placement: `crates/app`, because that is where the entry points live and because
a test in `orbok-search` can only ever prove that `orbok-search` works — which
is already proven, and is exactly what went wrong.

Shape:

```rust
// A real temp data dir, a real corpus on disk, real migrations.
// No hand-constructed HybridSearchService, no directly-invoked worker.
let ctx = test_runtime_context();          // real RuntimeContext
let catalog = bootstrap::open_catalog(&ctx)?;
bootstrap::add_source_and_scan(&ctx, &catalog, corpus_dir)?;
drain_scheduler_until_idle(&ctx, &catalog)?;   // the hosted scheduler, not a worker

let results = bootstrap::run_search(&ctx, &catalog, "…", 20)?;
```

Required assertions, one per currently-unreachable capability. Each must be
written so it **fails today**:

| # | Assertion | Fails today because |
|---|---|---|
| 1 | Editing a file on disk and re-running the corpus refresh returns the new content | F-01 — nothing re-scans |
| 2 | A result for a deleted file carries a non-`Ready` trust state | F-06 — hardcoded `Ready` |
| 3 | Applying a kind filter removes non-matching results | F-05 — filters never applied |
| 4 | A search scoped to folder A does not return a file from folder B | F-05 — scope never reaches the query |
| 5 | A paused source contributes no results | F-07 — no `sources` join |
| 6 | A PDF result's snippet contains text from the document, not `1 0 obj` | F-03 — page numbers read as line numbers |
| 7 | Two identical searches return identical orderings | F-10 — `HashMap` iteration order |
| 8 | A Japanese query returns the more relevant chunk first | F-02 — descending sort on a lower-is-better score |

**Required practice — and this is the load-bearing part of the whole RFC:**
each assertion is written and observed to **fail** before its fix lands. An
assertion added alongside its fix proves that the fix works today; an assertion
that was watched failing proves that the test can detect the defect coming back.
This project has repeatedly found checks that passed while verifying less than
they claimed. The only reliable defence is to break the thing on purpose.

**Runtime budget.** A ~20-file corpus, keyword path only where possible. If the
suite exceeds ~30 s it is doing too much; split the model-dependent assertions
behind the existing feature gating rather than growing the corpus.

## 7. Decision 3 — the benchmark measures the production entry point

`crates/bench` calls `bootstrap::run_search` (or a shared function that
`bootstrap::run_search` also calls), inside the timed region, so that model
resolution is measured.

Three further corrections, all small:

1. **Sample count.** `queries.rs` defines 10 queries × 3 runs = 30 samples. A
   p99 over 30 samples is the maximum observation. The v1.0 gate is stated as a
   p99. Raise to ≥ 100 samples before any p99 is reported.
2. **`latency_metrics(vec![])`** indexes `latencies_ms[0]` and panics on an
   empty query set (`metrics.rs:191`). Return an error or an empty summary.
3. **A `model_construction_ms` field** in the timing breakdown, so the cost this
   harness has been blind to is visible rather than merely included.

**Consequence, stated plainly so it is not discovered later:** every performance
number this project currently holds excludes model construction. The failing
real-model p99 of 843.88 ms and the 0.3659 files/s in RFC-048's evidence were
measured with the harness as it stands. They are not wrong, but they attribute
the cost to the wrong stage. **No further RFC-048 measurement should be
commissioned until this lands.**

## 8. Decision 4 — the release gate

Add to RFC-019's release-readiness matrix, and to the `release` CI job:

```
cargo test -p orbok --test wired_application --locked
```

Rationale for making it a gate rather than a suggestion: the six defects it
catches are exactly the class that produced no red anywhere for four releases.
A test that is not a gate would not have caught them either.

Platform scope: the `release` job (Linux) initially. It has no GUI dependency,
so extending it to the `cross` job's three legs is cheap and should follow once
it is stable — consistent with Task 032's finding that a Linux-only suite is a
gap, not a scope.

---

## 9. Alternatives considered

**Fix the six features and move on.** Rejected: it treats the audit's finding as
six bugs. The audit's own conclusion is that they share one cause, and the cause
is in the process, not the code.

**A full integration-test framework.** Rejected as premature. One test file with
eight assertions is small enough to be written this week and is sufficient to
catch every instance found. Grow it when something escapes it.

**Enforce §5's phrasing with a script.** Rejected for now. A grep for banned
phrases would produce false positives on prose and would not catch a
capability-phrased criterion written in different words. The rule is for human
review; if it erodes, revisit with tooling then.

**Retro-fit §5 to all 55 implemented RFCs.** Rejected. The nine that make false
claims are handled by RFC-063; rewriting the criteria of RFCs whose work
genuinely shipped is archaeology with no reader.

---

## 10. Acceptance criteria

Written in this RFC's own required shape, since anything else would be
self-refuting.

1. Running `cargo test -p orbok --test wired_application` on today's `main`
   fails at least six of §6's eight assertions, and the failure output names
   which capability is unreachable.
2. Each of §6's eight assertions has been observed to fail, and the failure
   recorded in the implementing task's report, before its corresponding fix is
   merged.
3. After the fixes for a given assertion land, that assertion passes, and
   reverting the fix in a scratch commit makes it fail again.
4. `cargo run -p orbok-bench --release …` produces a report whose
   `timing_ms` breakdown contains a non-zero `model_construction_ms` when run
   with a real model directory, and whose latency summary is computed from
   ≥ 100 samples.
5. `latency_metrics` called with an empty query set returns an error rather
   than panicking, demonstrated by a test.
6. The `release` CI job fails when §6's test file is deleted or made to return
   early — verified by pushing that change to a scratch branch, not by
   inspection.
7. No RFC accepted after this one contains a criterion matching §5's banned
   phrasings.

---

## 11. Open questions

1. **Does the wired-application test belong in `--test` (an integration target)
   or in `--bin orbok`'s own module?** `--test` is conventional and gives a
   separate binary; `--bin orbok` is where the existing RFC-049 boundary tests
   live and is already in the `cross` job. Implementer's call, recorded in the
   handoff.
2. **Model-dependent assertions.** Assertion 6 (PDF snippets) and the Japanese
   ranking assertion 8 need no model. Nothing in §6 currently does. If a future
   assertion does, it must be gated the way the existing model tests are, and
   must not silently skip — a skipped assertion is a capability-phrased
   criterion in another costume.
