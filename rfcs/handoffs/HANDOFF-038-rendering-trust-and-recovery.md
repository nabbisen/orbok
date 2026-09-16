# Implementation Handoff — RFC-038: show the trust state, and make recovery actions do something

**Project:** orbok\
**RFC:** 038 — Result Freshness, Trust Badges and Recovery Actions\
**Lifecycle stage:** Accepted. The trust state is computed for every result since RFC-060 Slice 4a (`2d90e1c`); nothing renders it.\
**Primary owner:** `crates/ui/src/views.rs` (result rows), `crates/ui/src/components.rs`, `crates/app/src/main.rs` (handlers)\
**RFC:** [`../accepted/038-result-freshness-trust-badges-and-recovery-actions.md`](../accepted/038-result-freshness-trust-badges-and-recovery-actions.md)

Owner priority 2 of 4, 2026-09-16.

---

## 0. Where things stand — from Task 052's sweep, confirmed by the architect

- Every `SearchResultDisplay` carries `trust: ResultTrustDisplay`
  (`crates/ui/src/state.rs:111-112`), filled from the catalog.
- **`result_trust_badge` (`crates/ui/src/components.rs:441`) has no caller.**
  The result row (`views.rs` around `:389-410`) renders the match badges
  (`result.badges`) and never the trust badge.
- **`Message::TrustRecoveryAction { .. }` is a no-op** in the reducer
  (`state.rs:808`, `=> {} // handled by orbok`), and `main.rs` has no
  handler.
- `ResultRecoveryAction` (`crates/search/engine/src/result_trust.rs:76-89`)
  has six variants: `PrepareAgain`, `CheckFolder`, `RemoveFromResults`,
  `OpenAnyway`, `ShowInFolder`, `ViewDetails`.

RFC-038 §16, criterion by criterion: **1** and **10** are met. **3, 5, 6**
hold at unit level with no end-to-end test. **4** holds in data but is never
shown. **2, 7, 8, 9** need this handoff.

## 1. Slice 1 — render the badge (criteria 2, 4, 8)

Call `result_trust_badge` in the result row, beside the match badges. It
already returns `None` for `Ready`, which is criterion 2 ("badges only when
useful"). Criterion 8 (not colour alone): the badge must carry its text label
and icon; follow the CVD pattern in `crates/ui/src/tests/a11y.rs`
(`cvd_icon_pairs_are_distinct`, `cvd_greyscale_status_distinguishable`) and
extend it to trust states.

**Tests, written first and observed failing by mutation:**

- A view test: a result whose trust is `FileNotFound` renders the badge text;
  a `Ready` result renders no trust badge.
- End to end, **no product change needed**, in `wired_application_tests.rs`:
  - **3/6** — a file whose cached extraction carries `PossiblyScannedPdf` or
    `SizeLimitReached` is returned by `run_search` with `PartlyPrepared`.
  - **5** — index a file, edit it, run `check_and_refresh_source`, search:
    `NeedsUpdate`, with `PrepareAgain` among its actions.

## 2. Slice 2 — the recovery actions that stay inside orbok (criteria 7, 9)

Render each result's primary action (the first of `trust.actions`) as a
button, and handle `TrustRecoveryAction` in `main.rs`:

| Action | Behaviour |
|---|---|
| `PrepareAgain` | enqueue the file for re-preparation, through the same job path a source refresh uses |
| `CheckFolder` | run the existing source check for that result's source |
| `RemoveFromResults` | remove the row from the current result list; state only, nothing on disk |
| `ViewDetails` | show the trust detail in Advanced view — the `Trust*Detail` messages exist and no view uses them (criterion 9) |

Each handler gets a test that observes its effect: a job enqueued, a check
run, a row gone, the detail rendered.

## 3. Held back — `OpenAnyway` and `ShowInFolder`

**Do not build these in this handoff.** Both open something through the
operating system, and **orbok currently has no way to open a file at all**:
pressing a result only selects it (`Message::SelectResult` sets
`selected_result`), and no file-opening dependency or message exists. Whether
and how results are opened is waiting on an owner decision. Until then,
these two actions are **not rendered** — a button that cannot work must not
be shown.

## 4. Definition of done

- The trust badge renders for every non-`Ready` result, with a text label.
- The four in-orbok recovery actions work and each has an effect test.
- Criteria 3, 5, 6 have end-to-end tests; every new test was observed failing
  first.
- `OpenAnyway` and `ShowInFolder` are not rendered anywhere.
- A closure record when criterion 7 can be fully evidenced — which needs §3
  resolved. Until then, a record is not written and RFC-038 stays open.

## 5. Stop conditions

- `trust.actions` is ordered in a way where the "primary" action is
  `OpenAnyway` or `ShowInFolder` for common states, leaving those results
  with no usable action once §3's two are hidden. Report which states.
- Re-preparing a file needs a job type the scheduler does not have.
