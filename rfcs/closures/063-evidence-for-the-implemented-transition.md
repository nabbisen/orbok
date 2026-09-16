# Closure Record — RFC-063: Evidence for the Implemented Transition

**RFC:** [063](../done/063-evidence-for-the-implemented-transition.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `2787630` (RFC opened from the 2026-09-01 audit),
`868ac75` (accepted; §7's dispositions applied — 023/024/025/028 to
`proposed/`, 037/038 to `accepted/`), `4ae22f4`, `d3f8b93`, `4a22f64`
(RFC-037's closure record, the first written; 037 back to `done/`),
`60985a6` (the closure-record gate in `scripts/check-rfc-lifecycle.sh`, the
shrink-only `LEGACY-ALLOWLIST.txt`, RFC-041 returned to `accepted/`, RFC-045's
record), `6497863` (RFC-010 to `proposed/`, allowlist shrinks by one),
`13d9edf` (how the allowlist shrinks), `cd0d445` (the shrink-only check
compares HEAD against its parent, not the index against HEAD).

**This record is recursive.** It is the first closure record written for
RFC-063 itself, and the mechanism that requires it is the one RFC-063
defines: `check-rfc-lifecycle.sh`'s closure-record enforcement, landed in
`60985a6`. RFC-063 cannot enter `done/` without passing its own gate, so the
gate accepting this record is part of the evidence for criterion 1, not a
formality around it. Records for 037, 045, 059 and 060 were written before
this one under the same gate.

**Transcribed, not re-derived**: each row names what was run in the
Task 052 closure sweep (2026-09-16) and what was observed.

---

## §11 acceptance criteria

### 1. Attempting to place a file in `rfcs/done/` with no corresponding closure record causes `check-rfc-lifecycle.sh` to fail — verified by staging such a change and running the gate, and by the gate's own self-test failing when the check is removed.

→ what was run: `bash scripts/check-rfc-lifecycle.test.sh`, which builds its
own temporary repository and stages the change there.
→ what was observed: PASS, all 29 assertions. The ones for this criterion:
`removing the allowlist entry exposes the missing closure record` (fail, as
expected), `failure names the missing closure record`, and `restoring the id
to the legacy allowlist exempts it again` (pass).
→ second half, by mutation in this sweep: the line flagging a missing
record (`flag "$done_file has no closure record ..."`) replaced with `:`,
then `bash scripts/check-rfc-lifecycle.test.sh` run.
→ what was observed: exit 1, `FAIL: removing the allowlist entry exposes
the missing closure record (expected fail, got pass)`,
`check-rfc-lifecycle.test.sh: FAILED`. Script restored byte-identical
(`cmp`).

### 2. A closure record that omits one of its RFC's numbered acceptance criteria causes the gate to fail.

→ what was run: the same self-test.
→ what was observed: PASS — `a closure record missing a criterion is caught`
(fail, as expected), `failure names the missing criterion number`, and `a
complete closure record (mixed prose + table criteria) makes the gate pass`.
`60985a6` also records a bug this case found: `comm` needs byte-sorted input,
and `sort -n` broke coverage for any RFC with 10 or more criteria. The bug
was caught by mutation-testing against RFC-045's 13 criteria.
→ where verified: `bash scripts/check-rfc-lifecycle.test.sh`.

### 3. Every file in `rfcs/done/` after the backfill has a closure record, and each record names every criterion with what was run and what was observed.

→ what was run: `bash scripts/check-rfc-lifecycle.sh` against the index,
plus an enumeration of `git ls-files 'rfcs/done/*.md'` against
`rfcs/closures/` and the allowlist.
→ what was observed: gate `rfc lifecycle gate: ok`. There are 50 files in
`done/`. Four have closure records (037, 045, 059, 060), and the gate
confirms each names every criterion. The other 46 are on
`LEGACY-ALLOWLIST.txt`, and **no `done/` file is in neither set**. Of §7's
nine files, only 037 and 045 are still in `done/`, and both have records.
→ **Scope:** the literal "every file in `rfcs/done/`" is not true: 46 files
are exempt. The exemption is RFC-063 §12 Q4's own proposal: "backfill only
the nine in §7, and require records prospectively". It is enforced as
shrink-only, so no new `done/` RFC can be exempt. See the not-met section
below.
→ where verified: `bash scripts/check-rfc-lifecycle.sh`.

### 4. For each of the nine files in §7, the claim its `Status` field makes about the product is true — demonstrated by naming, per file, the code path or the documented absence that makes it so.

→ what was run: `git grep` over tracked `crates/`, per file, as listed.
→ what was observed:

| RFC | Now | `Status` | Why it is true |
|---|---|---|---|
| 010 | `proposed/` | Proposed | The only `CrossEncoderReranker` impl is `MockReranker`, `#[cfg(test)]` (`crates/search/models/src/lib.rs:371-375`). `git grep '\.rerank('` finds only that module's own test. |
| 023 | `proposed/` | Proposed | `git grep -i 'hnsw\|usearch\|faiss'` over `crates/` and every `Cargo.toml`: no matches. No ANN index exists. |
| 024 | `proposed/` | Proposed | `quantize_to_i8` / `upsert_i8` are called only from `crates/pipeline/workers/src/tests/v08_features.rs` and `crates/search/models/src/lib.rs`'s own tests. There is no production caller. |
| 025 | `proposed/` | Proposed | `git grep -i 'tesseract\|\bocr\b'` finds only a test comment (`v08_features.rs:231`). No OCR code exists. |
| 028 | `proposed/` | Proposed | `crates/pipeline/extract/src/plugin.rs` is the interface only; its own doc says loading "is not yet implemented". `PluginRegistry` is constructed only in `v08_features.rs` and `v09_rc.rs` tests. |
| 037 | `done/` | Implemented (0.25.0) | Its closure record `rfcs/closures/037-…` passes the gate (criterion 3). |
| 038 | `accepted/` | Accepted | Work not shipped. The Task 052 sweep found that `result_trust_badge` (`crates/ui/src/components.rs:441`) has no caller, and no view reads `SearchResultDisplay.trust`. |
| 041 | `accepted/` | Accepted | Work not shipped. `git grep -i 'suggested_filter\|active_filters\|browse_around'` over `crates/ui/src/views*` finds no match. Narrow and Browse Around do not render. |
| 045 | `done/` | Implemented (v0.20.0) | Its closure record `rfcs/closures/045-…` passes the gate and names its unmet criteria. |

→ where verified: the commands above, run 2026-09-16 at `82c1415`.

### 5. `rfcs/README.md` lists every RFC at its current path and the gate passes.

→ what was run: `bash scripts/check-rfc-lifecycle.sh`. The gate checks
index↔folder equality both ways and resolves every link, reading the git
index.
→ what was observed: `rfc lifecycle gate: ok`, exit 0.
→ where verified: `bash scripts/check-rfc-lifecycle.sh`; the self-test's
link and mismatch assertions all pass (criterion 1's run).

### 6. No cross-reference in `rfcs/` or in any `//!` / `///` comment points at a moved file's old path.

→ what was run: two checks.
- The gate's link integrity. It covers Markdown links inside `rfcs/` and
  links into `rfcs/` from any tracked file.
- A plain-text search, since a `//!` path is not a link. For each of the
  nine, `git grep -E "(done|accepted|proposed|archive)/<slug>"` over the
  whole tracked tree, excluding hits in the file's current folder.
→ what was observed: gate ok. The plain-text search found **0 stale hits
for each of the nine** (010, 023, 024, 025, 028, 037, 038, 041, 045).
→ where verified: `bash scripts/check-rfc-lifecycle.sh`, and the `git grep`
above.

---

## Criteria not met, and why RFC-063 closes anyway

- **3, literal scope.** 46 `done/` files have no record and are exempt
  through `LEGACY-ALLOWLIST.txt`. **Why this closes:** RFC-063 §12 Q4
  proposes exactly this scope. The gate enforces it as shrink-only, so the
  exemption cannot grow. Every RFC that entered `done/` after RFC-063 was
  accepted has a record. §12 still words Q1, Q2 and Q4 as open. They
  were decided in practice (Q1: option B, the format every record uses; Q2:
  RFC-041 returned to `accepted/` in `60985a6`; Q4: backfill limited to the
  nine files, the rest allowlisted). Amending §12 to say so is the
  architect's edit and was not made in this sweep.
