# Implementation Handoff — RFC-058: Verifying the Wired Application

**Project:** orbok\
**RFC:** 058\
**Lifecycle stage:** Accepted 2026-09-02. Partially built: the harness and one of §6's eight assertions landed with Task 035 (`60f8602`). §7 and §8 are untouched.\
**Primary owner:** `crates/app/src/wired_application_tests.rs`; `crates/bench`; `.github/workflows/ci.yml`\
**RFC:** [`../accepted/058-verifying-the-wired-application.md`](../accepted/058-verifying-the-wired-application.md)

> **Scope rule:** This builds the *instrument*, not the fixes. Five of §6's
> eight assertions describe defects that belong to RFC-060. **Do not fix them
> here.** If an assertion tempts you into `bootstrap/search.rs`, stop — you have
> left this RFC.

---

## 1. What already exists — do not rebuild it

**The harness is built.** `crates/app/src/wired_application_tests.rs` (Task 035)
already has `test_context()`, `drain_scheduler_until_idle()`, `write_markdown()`,
and five passing tests. It is declared `mod wired_application_tests;` in
`main.rs`, so it lands in the **`--bin orbok`** target — which `ci.yml:277` runs
in the `cross` job on **all three platforms**.

That answers RFC-058 §11's open question 1 and it answers §8 better than §8
asked for: §8 proposed the Linux-only `release` job; the tests already run on
Linux, macOS and Windows. **§8 is therefore already satisfied. Do not add a
second invocation** — verify the existing one covers the file (it does) and say
so.

**One assertion of the eight exists:** §6 row 1, as
`restarting_orbok_picks_up_a_file_edited_while_closed`.

## 2. The board has changed since the RFC was written — read this before §6

RFC-058 §6 was drafted on 2026-09-01, when all eight assertions described live
defects and the instruction *"each assertion is written and observed to fail"*
was straightforwardly executable. **Three months of task work has moved three of
them.** Current state:

| §6 row | Assertion | Status now |
|---|---|---|
| 1 | Edit while closed → restart → found | **Built and passing** (Task 035) |
| 2 | Deleted file → non-`Ready` trust state | Defect live (F-06) — RFC-060 §11.3 |
| 3 | Kind filter removes non-matching | Defect live (F-05) — RFC-060 §11.4 |
| 4 | Folder scope excludes the other folder | Defect live (F-05) — RFC-060 §11.5 |
| 5 | Paused source contributes no results | Defect live (F-07, **source-level**) — RFC-060 §11.6 |
| 6 | PDF snippet is document text, not `1 0 obj` | Defect live (F-03) — RFC-060 §11.1 |
| 7 | Two identical searches, identical order | **Already fixed** (Task 034, `cfdf3c3`) |
| 8 | Japanese query ranks the relevant chunk first | **Already fixed** (Task 034, `cfdf3c3`) |

Note row 5: Task 035 closed the **file**-level half (a *missing file*'s content
no longer surfaces). The **source**-level half — a *paused source* still
contributing — is untouched, because no retrieval query joins `sources`. Assert
the source-level behaviour; do not assume Task 035 covered it.

## 3. What to build, in three groups

### Group A — rows 7 and 8: the defect is fixed, so mutate instead

You cannot observe these failing; the fix landed in `cfdf3c3`. **The rule's
intent is that the assertion can detect the defect**, and for an already-fixed
defect the way to show that is a mutation:

1. Write the assertion. Confirm green.
2. In a scratch tree, revert the specific fix — for row 8, flip
   `rrf_fuse_keyword_lists`'s comparator to ascending; for row 7, delete the
   `chunk_id` tie-break in `rrf_fuse`.
3. Confirm the assertion goes red, **and that it names the right thing**.
4. Restore; confirm green.

Report the red output for both. An assertion added green and never mutated
proves the code works today and nothing about whether it will notice a
regression — which is the distinction this whole RFC is about.

### Group B — rows 2 to 6: write them, watch them fail, and leave them failing

These are RFC-060's defects. **Write the assertions now anyway**, because that is
what makes RFC-060's work checkable rather than self-reported.

**They must not turn CI red in the meantime**, and `#[ignore]` is not the answer
— `ROADMAP.md`'s own debt register records that `#[ignore]`d durability helpers
are exactly how RFC-050's guarantees stopped being verified.

**Use `#[should_panic]` with the expected message**, one per assertion, each
carrying a comment naming the RFC-060 criterion it is waiting on. That gives
three properties `#[ignore]` does not:

- the test **runs**, on all three platforms, every push;
- it fails loudly if the defect is fixed and nobody removed the attribute — so
  the wrapper cannot outlive the defect silently;
- the expected-message string documents what the failure currently looks like.

When RFC-060 §7 lands, the implementer deletes the attribute and the assertion
becomes a plain test. **Say in each comment that this is the removal condition.**

If `#[should_panic]` proves awkward for an async `#[tokio::test]`, report that
rather than falling back to `#[ignore]` — I will scope an alternative.

### Group C — §7, the benchmark

Three concrete defects, all confirmed in the tree today:

1. **`metrics.rs:132` builds `search_service()` before the timing loop at
   `:139`** (and again at `:211`). The application constructs the model *inside*
   every search. **Move service construction inside the timed region**, or time
   `bootstrap::run_search_with` directly — the latter is preferable, since it is
   literally the production entry point.
2. **`queries.rs` holds 9 queries, not the 10 the RFC says.** 9 × 3 runs = **27
   samples**; a "p99" over 27 is the maximum observation. Raise to **≥ 100
   samples** before any p99 is reported. The RFC's "10" came from the audit and
   is wrong by one — correct the RFC text in the same commit.
3. **`latency_metrics` still panics on an empty set.** The `.min(len.saturating_sub(1))`
   guard does not save it: with an empty vec that yields index 0 and
   `latencies_ms[0]` panics. Return an error or an empty summary, with a test.

Add the `model_construction_ms` field §7 asks for.

**Consequence to state in your report, not to act on:** every performance number
this project holds — including RFC-048's failing p99 of 843.88 ms — was measured
with the harness excluding model construction. They are not wrong, but they
attribute the cost to the wrong stage. **Do not re-run RFC-048's benchmark as
part of this task**; fixing the instrument is the whole job here.

## 4. What is not in scope

- Fixing rows 2–6. RFC-060.
- Re-measuring RFC-048.
- §5's acceptance-criteria phrasing rule — that governs future RFCs and needs no code.
- A second CI invocation for §8 (see §1).

## 5. Definition of done

1. Rows 2–8 exist in `wired_application_tests.rs`; row 1 already does.
2. Rows 7 and 8 pass, and each has a recorded mutation showing it goes red when
   its own fix is reverted.
3. Rows 2–6 run on every push, fail for the stated reason, and each names the
   RFC-060 criterion that removes its wrapper.
4. `cargo test -p orbok --bin orbok --locked` is green on all three `cross` legs
   with rows 2–6 in their waiting state.
5. The benchmark times the production entry point, reports ≥ 100 samples, emits
   `model_construction_ms`, and does not panic on an empty query set.
6. RFC-058 §7's "10 queries" is corrected to 9.

## 6. Stop conditions

Stop and report if:

- An assertion cannot be written without changing production code. That means it
  is not an instrument change and belongs to RFC-060.
- `#[should_panic]` does not work cleanly for the async tests (§3 Group B).
- Row 5's source-level assertion turns out to be already satisfied — that would
  mean Task 035 covered more than its review established, and I want to know.
- Fixing the benchmark changes a *keyword-only* number materially. Keyword-only
  has no model to construct, so it should be unaffected; if it moves, the
  harness had a second problem nobody has named.
