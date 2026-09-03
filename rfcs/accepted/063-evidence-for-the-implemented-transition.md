# RFC-063: Evidence for the Implemented Transition

**Project:** orbok\
**RFC:** 063\
**Title:** Evidence for the Implemented Transition\
**Status:** Accepted\
**Accepted:** 2026-09-02 by the project owner\
**Target milestone:** project-record integrity\
**Date:** 2026-09-01\
**Supersedes in draft:** the first version of this RFC, titled *Recording Deferral in the RFC Lifecycle*, which named the wrong defect. See §9.\
**Related RFCs:** RFC-000 RFC Lifecycle Policy (this amends its transition rules); RFC-058 Verifying the Wired Application (§5's criteria rule is a *consequence* of this RFC, not a peer); RFC-062 Migration Integrity (the same unenforced-rule pattern, in the storage layer)

---

## 1. The defect

Nine RFCs in `rfcs/done/` carry `Status: Implemented` while making a false claim
about the product. The question is not *where should those files live*. It is
**how did nine of them get there.**

RFC-000 §"Review and transitions" specifies the operation in full:

> **Accept and ship.** RFC is implemented; the implementer or maintainer moves
> the file from `proposed/` to `done/` and updates the Status field with the
> release tag. Done in the same commit (or commit series) that ships the
> implementation.

That is the entire procedure. **The implementer asserts it and moves the file.**
No evidence is required, no verifier is named, and nothing downstream reads the
acceptance criteria that were supposedly met.

### 1.1 The verification exists. It is optional, and the transition does not ask for it.

An earlier draft of this section said the project had no verifier. That was
wrong, and the owner said so: *"we have confirmed workflow and have had several
review processes."* Correct — and counting them makes the real shape visible.

**194 reviews. 55 RFCs in `done/`. Eleven of those 55 ever had a review of their
own:** RFC-006, RFC-008, and RFC-049 through RFC-057. **Forty-four did not.**

| | Reviewed | Never reviewed |
|---|---:|---:|
| RFCs in `done/` | 11 | 44 |
| of those, `Status: Implemented` is false | **0** | **9** |

**All eleven reviewed RFCs hold. All nine false ones are in the never-reviewed
forty-four.** That is as clean a natural experiment as this project is going to
get, and it inverts the diagnosis: **the process works. It is simply not
required.**

What closing an RFC looks like without it — commit `dd1b29b`, *"Search UX,
Source Lifecycle, Result Trust (RFC 041, 037, 038)"*: three RFCs moved to
`done/` in one commit, creating `source_lifecycle.rs` (194 lines),
`filter.rs` (223) and `result_trust.rs` (193) with full test modules — every one
of which the 2026-09-01 audit found has zero production callers. No review, no
closure record, three `Status: Implemented` fields set at once.

So the defect is not an absent capability. It is that **RFC-000's transition
rule and the project's actual verification practice are not connected to each
other.** Running the work through `tasks/dev-team/` → `review-request/` →
`reviewed/` is a choice someone makes, not a precondition of the folder move.
Where it was chosen, eleven for eleven. Where it was not, nine of forty-four are
false.

That also means the fix is much smaller than inventing a process: **make the
transition require the output of the one that already works.**

### 1.2 The asymmetry that let it drift

**This is still the only consequential claim in this project that a file move
can complete.**

| Claim | What must exist before it is believed |
|---|---|
| This code is correct | `clippy -D warnings` across all targets, six CI jobs, a three-platform matrix, and an adversarial review that verifies by execution |
| This colour is a design token | `check-design-tokens.sh`, which is itself self-tested |
| This string is translated | a compile-time exhaustive `match`; a missing translation **fails the build** |
| This release archive is the reviewed source | an independent script verifying it against `git ls-tree` |
| This advisory does not reach us | a written reachability analysis with a stated REMOVE WHEN condition |
| **This RFC is implemented** | **a file move** |

That asymmetry is the fundamental defect. Everything else in the audit's
"documented but not connected" class is downstream of it.

## 2. Why the existing gate did not catch it

`scripts/check-rfc-lifecycle.sh` exists, is well built, reads the git index
rather than the working tree, and is self-tested. It checks:

- every file's `Status` field matches its folder
- every README entry links to a file that exists, with a matching id
- no id is duplicated
- filenames match `NNN-slug.md`

Every one of those is a check on **internal consistency of the filing**. Not one
of them can check **correspondence to the product**. The gate verifies that the
label matches the drawer; it has no way to look inside the drawer.

So a green RFC-lifecycle gate reads as *"the RFC record is in good order"* and
means *"the RFC record is internally consistent."* It would have passed on
`dd1b29b` — and did.

**That is this project's recurring defect class, arriving in the governance
layer:** a check that passes while verifying less than its name implies. It is
the same shape as 289 tests that had never run off Linux, as RFC-008 §23's
"chunks *can be* embedded locally", and as the acceptance criteria in §3.

## 3. First consequence — why the acceptance criteria decayed

RFC-010's criteria are *"Reranker is optional"*, *"Rerank top-N limit exists"*,
*"Rerank status is exposed to UI"*. RFC-037's are *"Startup check exists"*,
*"Manual refresh exists"*. RFC-041's is *"Active filters are visible and
individually removable"*.

These were not written carelessly. **They were written under no pressure to be
falsifiable, because on those RFCs nothing was ever going to run them** — all
five are in §1.1's never-reviewed forty-four. Compare RFC-056, which went
through review: its criterion 5 was found to have verified one instance and
asserted the class, and was relocated rather than waved through. The difference
is not the author. It is whether anyone had to produce an observation.

A criterion becomes falsifiable only when somebody must produce an observation
against it. Absent that moment, criteria drift toward whatever is easy to write
— and "X exists" is the easiest thing to write about software you have just
finished building. The vocabulary of capability is the natural output of a
process with no verification step.

RFC-058 §5 requires criteria to be behaviour-phrased. That rule is necessary and
it is **not sufficient on its own**, and it is important to see why: RFC-038's
criteria 4, 5 and 7 *are* behaviour-phrased — *"Missing files do not appear as
normal ready results"* — and they are false. A well-phrased criterion that nobody
executes fails exactly as silently as a badly-phrased one. **RFC-058 §5 is a
consequence of this RFC, not a peer of it.** Phrase criteria so that evidence is
possible; then require the evidence.

## 4. Second consequence — the handoff is a forward instrument, and there is no closure instrument

RFC-037, RFC-038 and RFC-041 each have a full handoff document in
`rfcs/handoffs/`. They were not under-specified. The handoffs say what already
exists, what to build, and what not to rebuild.

They are **forward** documents. Nothing in the corpus is a **closure** document.

The project noticed the absence and patched it by hand in one place: the newer
handoffs (RFC-056, RFC-057) grew a `**Lifecycle stage:**` line recording the
commits and the review numbers that closed them. The older ones — 037, 038, 041
— have no such line at all. That line is the missing artifact, invented ad hoc,
in the one file that happened to have a slot for it.

## 5. Third consequence — the claim ships and its evidence does not

For RFC-056 and RFC-057 the closure evidence genuinely exists. It is in
`.git-exclude/reviewed/`, where each criterion was checked against a running
build.

`.git-exclude/` is not git-tracked and is not in the release archive. `rfcs/` is
both.

**So the assertion ships and the evidence for it does not.** Anyone reading the
shipped RFC corpus — a contributor, a packager, the external auditor of
2026-09-01, or the project's own architect two months later — sees
`Status: Implemented` with no path to what backs it. The one place the project
does verification well is the one place it does not publish.

## 6. The proposal

**An RFC may enter `done/` only with a closure record.**

This does not add a verification process. It requires the output of the one that
already produces eleven-for-eleven results (§1.1), and it moves that output from
`.git-exclude/` into the tree, where the claim it supports already lives (§5).

### 6.1 What a closure record contains

One entry per acceptance criterion, naming three things:

```text
criterion  → what was run (command, test name, or manual step)
           → what was observed (the actual output or state)
           → where it was verified (commit, CI run, or review)
```

Plus, explicitly: **any criterion that was not met, and why the RFC closes
anyway.** RFC-000's granularity clause already permits partial implementation;
it currently permits it silently. A closure record makes "we shipped the main
design decision and deferred §N" a written statement instead of an omission —
which is exactly the case RFC-041 and RFC-045 are in, undocumented.

A criterion that cannot be given an observation is not a criterion. That is
RFC-058 §5's rule arriving from the other direction, and it is the point at
which "Startup check exists" becomes visibly inadequate: there is nothing to
write in the *what was observed* column.

### 6.2 Where it lives — decision needed

Three options. **Recommendation: (B).**

| | Placement | For | Against |
|---|---|---|---|
| **A** | A `## Closure` section appended to the RFC itself | Self-contained; the reader sees claim and evidence together; no new folder, no gate change beyond a section check | RFCs are design documents; a long evidence table at the bottom dilutes that, and the RFC is then edited after it ships |
| **B** | `rfcs/closures/NNN-slug.md`, mirroring `handoffs/` | *Recommended.* Symmetric with the existing structure — forward instrument, closure instrument. Keeps the RFC a design document. Tracked, so it ships with the corpus. Trivially checkable: `done/NNN-*` requires `closures/NNN-*` | One more folder; one more cross-reference to keep correct |
| **C** | A `## Closure` section appended to the existing handoff | No new folder; the newer handoffs already do this informally | Not every RFC has a handoff, so the rule would have exceptions — and a rule with exceptions is how the append-only migration rule was broken (RFC-062) |

### 6.3 What the gate checks

Mechanically checkable, and therefore in `check-rfc-lifecycle.sh`:

- Every file in `done/` has a corresponding closure record.
- The closure record names every acceptance criterion in the RFC, by number.
- Under option B, the record's id and slug match the RFC's.

**Not checkable, and the gate must not pretend otherwise:** whether the observed
outcomes are true. A human does that — which for this project means the review
that closes the RFC. The gate's job is to make it impossible to close an RFC
*without* someone having had to write down what they saw.

This is a deliberate line. A gate that claimed to verify closure would be a new
instance of the §2 defect.

### 6.4 What this costs

Real, and worth stating: closing an RFC becomes slower. For RFC-056 and RFC-057
the cost is near zero — the evidence already exists in the review record and
would be transcribed. For an RFC closed without that discipline, the cost is
discovering that it cannot be closed. **That is the feature.**

## 7. Backfill — the nine files

With §6 in place, these sort themselves, and the sorting is no longer a
judgement call about folders — it is "can a closure record be written?"

**Cannot be written; nothing shipped** — RFC-023 (ANN), RFC-024 (quantization),
RFC-025 (OCR), RFC-028 (plugin extractors). Their own text reads *"This future
RFC **will** decide whether…"*.

**Cannot be written; the central mechanism is unbuilt** — RFC-010 (no real
reranker), RFC-037 (§10.1/§10.2 both marked "Required", neither exists),
RFC-038 (§16.4/5/7 false; every result is hardcoded `Ready`). These move to
`accepted/` — *"review complete; implementer may start; the work has not yet
shipped"* — which is exactly true of all three and preserves the authorization.
RFC-037 matters most: **the audit's single blocking issue already has its full
design there.** It needs wiring, not a new RFC.

**Can be written, with a stated gap** — RFC-041 (filters visible, never applied),
RFC-045 (folder chosen, never scopes the query). These stay in `done/` and get a
closure record that names the unmet criterion and points at RFC-060. RFC-041 is
borderline — *Narrow* is half its title — and the owner may reasonably move it to
the group above. **Flagged, not decided.**

## 8. The deferral question needs no new state — revised 2026-09-02

**An earlier version of this section proposed a sixth folder (`deferred/`). The
owner rejected it, and was right. It is withdrawn.**

The objection: *"I did not get why the current RFC lifecycle policy with the
5-folder variant was insufficient. I doubt whether 6-folder variant can bring
complexity bad on project management."*

### 8.1 Why I thought a sixth state was needed, and why it does not hold

The argument was that neither existing state fits RFC-023/024/025/028:
`archive/` means Withdrawn — *"the work will not happen"* — which is stronger
than the truth; and `proposed/` would trip RFC-000's own **"Silent withdrawal"**
anti-pattern, four files parked there since v0.8.0.

Both halves are weaker than they looked.

**The `proposed/` objection misreads the anti-pattern.** RFC-000 describes it as
*"An RFC that's been abandoned but not formally withdrawn sits in `proposed/`
indefinitely… the maintainer's unspoken 'I'm not going to do this' is
invisible."* The harm it names is **silence** — invisible intent, reviewers
wasting effort on something nobody will do. That does not apply here: these four
RFCs state their own deferral **in their own text** (*"This future RFC is
intentionally deferred"*). Add one explicit parked line and the intent is not
unspoken at all. Duration in `proposed/` is not the defect; silence is.

**And `proposed/` is a genuinely accurate description of them.** It means *"open
for review and discussion; implementer should not yet start work — the design may
change."* Every clause is true of ANN, quantization, OCR and plugin extractors.
Their designs are undecided, which is exactly what they say.

### 8.2 The cost the objection is pointing at is real

A sixth folder is not one directory. It is four edits to
`scripts/check-rfc-lifecycle.sh` (the `require_dir` list, a status loop, the
README-section extraction, the path enumeration), a new section in the index, an
extension to the gate's self-test, and a rule every future contributor must learn
— for **four files**, against a policy whose own text argues that *"the states
are deliberately few"* and warns specifically against formalising states that
small projects route around.

It would also cost RFC-000 its portability, which is a stated design goal
(*"written to be portable — any project starting an `rfcs/` directory can adopt
this policy verbatim"*). The variant framing mitigated that; it did not remove it.

### 8.3 What was actually wrong, and it was never the folder

The defect the four files exhibit is **not that they are in the wrong drawer**.
It is that `Status: Implemented (v0.8.0)` and the index row *"Vector ANN Indexing
— Implemented"* both read as **"orbok has ANN"**, when what shipped was a
decision to defer it.

§6's evidence requirement fixes that at the source: no closure record can be
written for RFC-025, because there is nothing to put in the *what was observed*
column. It can never enter `done/` again. That is the whole repair, and it needs
no new state.

### 8.4 The disposition, then

**Move RFC-023, RFC-024, RFC-025 and RFC-028 to `proposed/`**, `Status: Proposed`,
each carrying one added line naming the parked status and the condition that would
reopen it — e.g. for RFC-023, *"Parked since v0.8.0. Reopen when a corpus exceeds
the exact-scan ceiling; Owner Task 003 Part A measured vector scan at 0.8 % of
search cost, so that condition is not close."*

The index's Status column carries the same, so a reader scanning `proposed/`
distinguishes RFC-047 (genuinely under review) from four parked entries without
opening them.

No policy amendment. No gate change. No new folder. RFC-000 stands as written,
and the 5-folder variant this project adopted on 2026-08-04 is sufficient.

## 9. What the first draft got wrong, recorded deliberately

The first version of this RFC was titled *Recording Deferral in the RFC
Lifecycle*. It correctly identified nine false statuses and three tiers, and then
named the defect as *"the lifecycle cannot express deferral"* — following the
external audit's framing.

That is a symptom. It explains four of the nine files and none of the mechanism.
The owner rejected it on exactly that ground: *"the reason must exist in the
system, the workflow or the rules."* It does, and §1 is it.

Recorded here rather than silently rewritten, because the wrong diagnosis is
instructive: **it is easy to answer "where should this file be?" and hard to
notice that the question presupposes the file got there legitimately.**

The second draft then over-corrected, claiming the project had no verifier at
all. The owner rejected that too — *"we have confirmed workflow and have had
several review processes"* — and counting them produced §1.1, which is a better
statement than either draft: eleven for eleven where the process ran, nine false
out of forty-four where it did not. **The process is not the problem; its
optionality is.**

A third correction followed on 2026-09-02: the owner rejected the sixth folder
this RFC had carried since its first draft, and §8 is now a withdrawal. That one
is the most instructive of the three, because the proposal **survived a full
rewrite** — I changed my mind about the *cause* and kept the *remedy* the wrong
cause had suggested. A remedy does not inherit correctness from a corrected
diagnosis; it has to be re-derived, and I did not re-derive it.

All three corrections came from being told the diagnosis was wrong and going to
look, which is the discipline this RFC is trying to make structural.

## 10. The pattern this shares with RFC-062

Worth stating once, because it is now two of the audit's structural findings:

- `migrations.rs:18` — *"New migrations are appended here and never reordered or
  edited after release."* Written down. Unenforced. Broken once, in `c54e89d`.
- RFC-000 — *"Implemented: the work has shipped."* Written down. Unenforced.
  Broken nine times.

**This project's rules are unusually good and its rule-enforcement is uneven.**
Where a rule has a gate it holds — design tokens, i18n completeness, package
reproducibility, clippy. Where a rule is prose, it is followed until it is
inconvenient, and nothing says when that happened.

The general form of the fix is not more rules, and — after §1.1 — it is not more
process either. It is: **for each written rule, either build the cheap mechanical
check, or connect it to the review practice that already exists and say so.** An
unenforced rule that is *labelled* unenforced is honest and survives; one that
reads as binding while nothing binds it does not.

orbok does not have a verification problem. It has eleven-for-eleven verification
that forty-four RFCs never had to pass through.

---

## 11. Acceptance criteria

Phrased per RFC-058 §5, over the repository.

1. Attempting to place a file in `rfcs/done/` with no corresponding closure
   record causes `check-rfc-lifecycle.sh` to fail — verified by staging such a
   change and running the gate, and by the gate's own self-test failing when the
   check is removed.
2. A closure record that omits one of its RFC's numbered acceptance criteria
   causes the gate to fail.
3. Every file in `rfcs/done/` after the backfill has a closure record, and each
   record names every criterion with what was run and what was observed.
4. For each of the nine files in §7, the claim its `Status` field makes about the
   product is true — demonstrated by naming, per file, the code path or the
   documented absence that makes it so.
5. `rfcs/README.md` lists every RFC at its current path and the gate passes.
6. No cross-reference in `rfcs/` or in any `//!` / `///` comment points at a moved
   file's old path.

## 12. Open questions

1. **§6.2 — where closure records live.** A / B / C. **Owner decision;
   recommendation is B.**
2. **§7 — RFC-041's group.** Judgement about whether "narrow" is central to
   *Search, Narrow and Browse Around*. **Owner decision.**

*(A third question — whether to add a sixth lifecycle state — was withdrawn on
2026-09-02 after the owner rejected it. See §8.)*
4. **How far back does the backfill go?** Writing closure records for all 55
   implemented RFCs is archaeology. Proposal: backfill only the nine in §7, and
   require records prospectively. An RFC closed years ago whose feature demonstrably
   works needs no reconstructed evidence. Stated so it is a decision rather than
   a drift.
