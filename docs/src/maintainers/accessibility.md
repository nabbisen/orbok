# Accessibility

`orbok` targets **WCAG 2.1 Level AA** for its desktop GUI.

This document records the conformance target, the success-criteria checklist
with orbok's current status per criterion, known renderer limitations, and the
manual QA steps that gate each release at M13.

---

## Conformance target

| Scope | Standard | Level |
|---|---|---|
| orbok desktop GUI | WCAG 2.1 | AA |
| CLI output, log files | Not in scope | — |
| Docs site | Not in scope (future) | — |

---

## Success criteria checklist

### 1.1.1 Non-text Content

**Status: Met.**

Every status badge pairs a lucide icon glyph with a text label. No control
communicates only through an image or icon. Icon-only sidebar navigation items
carry `tooltip` strings sourced from the i18n catalog, which are the accessible
text for those controls.

### 1.4.1 Use of Color

**Status: Met.**

Status is conveyed by three redundant channels: text label + lucide icon/shape +
tone colour. No status depends on colour alone. Verified by the
`status_badge_label_and_icon_invariant` test and documented in
`crates/ui/src/components.rs`.

### 1.4.3 Contrast (Minimum)

**Status: Met (token layer).**

All body and label text renders on token-paired foreground/background roles
whose contrast is verified at the snora palette level and additionally guarded
by `crates/ui/src/a11y.rs`. The `contrast_usage_guard_all_presets` test runs
`a11y::audit` across all four theme presets and asserts AA ratios for every pair
orbok renders. All nine pairs are asserted at 4.5:1; no pair currently uses the
3.0:1 large-text/UI threshold.

**`text_muted`, corrected 2026-08-18.** This entry claimed the role was
"intentionally exempt". Two things were wrong with that:

- The exemption was justified by snora documenting `text_muted` as below-body
  contrast for decorative text. **snora withdrew that in 0.34.0**, stating the
  exemption "was ours, invented" — WCAG's exemptions are incidental, decorative,
  invisible text, logotypes and large text, and a role-level exemption is not
  among them. `text_muted` is now asserted against all three surfaces upstream.
- More simply: **orbok never renders `text_muted`.** Its only occurrence in the
  tree is the comment in `a11y.rs` that claimed the exemption.

So there is nothing to exempt. The correct statement is that the role is unused,
which is stronger than an exemption and needs no WCAG argument. **If it is ever
rendered, it must be added to `RENDERED_PAIRS` like any other text role** — do
not reinstate the exemption.

### 1.4.4 Resize Text

**Status: NOT ASSESSED.** Added 2026-08-17 — this criterion was absent from the
checklist entirely, and adding it should not be read as adding a pass.

WCAG 2.1 AA requires text to be resizable **to 200%** without loss of content or
functionality. orbok's in-app control (`TextScale`, `crates/ui/src/theme.rs`)
offers **1.0× / 1.15× / 1.3× — a 130% maximum.** On the in-app control alone the
criterion is not met.

That is not the end of the question. 1.4.4 says *"without assistive technology"*,
and for a desktop application OS-level display scaling is a legitimate mechanism;
an iced application does respond to it. So whether orbok meets 1.4.4 depends on
whether it **reflows correctly at 200% OS scaling without losing content or
functionality** — text clipped, controls pushed off-screen, or dialogs unable to
show their buttons would all fail it.

**That is a manual check and has never been run.** It is a rendering outcome, not
a code property, so it cannot be settled from the source. Added as a row to Owner
Task 003 Part B's manual QA form.

Recorded as unassessed rather than guessed in either direction: the in-app
control's 130% ceiling makes a bare "Met" wrong, and the OS-scaling path makes a
bare "Not met" premature.

### 1.4.11 Non-text Contrast

**Status: Met — but this entry described a mechanism that does not exist.**
Corrected 2026-08-18, prompted by snora 0.34.0's own border-contrast repair.

It previously read: *"UI component boundaries (borders on surface) are included
in `a11y::RENDERED_PAIRS` at the ≥ 3.0:1 threshold."* **They are not.**
`RENDERED_PAIRS` holds nine pairs, every one of them text-on-background at
`min_ratio: 4.5`. No pair reads `palette.border`, and no pair uses a 3.0
threshold at all.

`crates/ui/src/a11y.rs` excludes `palette.border` **deliberately and with a
stated reason**, which is why the status is still Met: 1.4.11 applies to a
boundary only when the border is the sole means of conveying the component's
extent, and orbok's cards are defined by their `card::surface` fill.

That argument holds only while two things remain true, and both were re-checked
on 2026-08-18:

1. **orbok's cards are fill-defined.** Only `card::surface` and `card::selected`
   are used — both fill styles.
2. **orbok does not render snora's dialog card.** orbok's confirmation dialog is
   bespoke (`components.rs`: *"no snora primitive yet"*), and orbok calls
   `snora::render`, not `snora::design::render`. This matters because RFC-039
   made snora's dialog card **border-defined rather than shadow-defined** — for
   that surface the border *is* the boundary, and the exclusion would not apply.

**If either changes — adopting `snora::design::render` (upgrade-plan Phase 3), or
a border-defined surface of our own — this criterion must be re-assessed and a
border pair added to `RENDERED_PAIRS`.**

For the record: the border value orbok was rendering measured **1.28:1 (light)**
and **1.19:1 (dark)** until snora repaired it to ~3.1:1 in 0.34.0. Under the
exclusion above that was not an orbok conformance failure — but it is a
demonstration of why an untested role is worth naming as untested rather than
describing as covered.

### 2.1.1 Keyboard

**Status: Partially met.** Updated 2026-08-16/17 (Task 024) after the first
keyboard-only walkthrough (Owner Task 003 Part B: *"nothing worked at
all"*) found this entry's prior "Met" claim false. Task 024 closed the
walkthrough-blocking defect and a defined set of others; a real remainder
is recorded below rather than folded into "Met."

**The three facts that caused this remain true** and explain why the fix
is a binding map, not a framework change:

1. **iced 0.14 performs no Tab traversal of its own.** `Named::Tab`
   appears nowhere in `iced-0.14.0` or `iced_runtime-0.14.0`; focus
   movement requires the application to call
   `focus_next()`/`focus_previous()` (`iced_runtime::widget::operation`).
2. **orbok now calls them.** `Tab`/`Shift+Tab` map to
   `Message::FocusNext`/`FocusPrevious` (`crates/ui/src/shell.rs`), turned
   into that `Task` in `crates/app/src/main.rs`. The stale comment
   claiming iced handles Tab itself is gone.
3. **Only `text_input` and `text_editor` implement `Focusable`** in
   `iced_widget-0.14.2`. `button` does not, so Tab still reaches only
   orbok's 4 text inputs — 2.1.1 does not require Tab traversal, and the
   criterion is met for everything below through direct-access shortcuts
   and the selection-model pattern instead, not by waiting on iced to make
   buttons focusable (a non-goal, confirmed correct in Task 024's review).

**Why the original tests did not catch the defect:**
`crates/ui/src/tests/a11y.rs` asserted that `key_to_message` maps each
shortcut to the right `Message` — true, and irrelevant to whether the
resulting action is *reachable*. `crates/ui/src/tests/keyboard_reachability.rs`
now drives the real chain (`key_to_message` → `AppState::update` →
re-render) and asserts on the rendered result via `iced_test::Simulator`'s
`find()`, with each test's binding broken and confirmed to fail before
being restored.

**What is bound**, covering the walkthrough-blocking case and the fixed
navigation/list/dialog surface:

| Shortcut | Action |
|---|---|
| `Ctrl/Cmd + K` | Focus Search view |
| `Ctrl/Cmd + ,` | Open Settings |
| `Ctrl/Cmd + 1`..`6` | Jump directly to each of the six views |
| `Tab` / `Shift+Tab` | Move focus among the 4 text inputs |
| `Escape` | Close overlay / dismiss notice / **skip or back out of the wizard** / **cancel an in-progress model download** / cancel a confirm dialog / clear a list selection (priority order, first match wins) |
| `Enter` (search input focused) | Submit search |
| `Enter` (not typing) | Confirm whichever dialog/wizard page/list selection is active, if any (§ below) — on the Setup page this downloads the reviewed model |
| `Arrow Down`/`Up` (Search view, not typing) | Select next/previous result |
| `Arrow Down`/`Up` (Sources view, not typing) | Select next/previous source |

**The wizard — the walkthrough's actual blocker — is fixed.** `Escape` on
the Setup/Checked/DownloadFailed pages performs the same zero-confirmation
fallback the mouse-only Skip button does; on DownloadConsent it mirrors
Cancel; on Downloading it requests cancellation (Task 027), mirroring
Task 025's Cancel button. A keyboard-only user reaching first launch is no
longer stuck.

**Setup's primary action, `DownloadModel`, is bound to `Enter` (Task 027).**
Task 024 left it unbound, reasoning that a global `Enter` binding could
fire alongside the page's own `text_input`'s `on_submit(WizardValidate)`
and double-dispatch. That reasoning does not hold: iced's `text_input`
captures every `Enter` it receives while it genuinely has focus
(`shell.capture_event()`, unconditional on modifiers), and
`iced::keyboard::listen()` — the subscription `key_to_message` runs
through — only ever receives events the widget tree left uncaptured. A
captured `Enter` never reaches `key_to_message` at all, so there is no
double-fire to guard against: with the path input focused, only its own
`WizardValidate` fires; with nothing focused, only `DownloadModel` fires.
Verified against the running application, not just reasoned from source
(see Review 185 §4's original claim and its correction in the Task 027
review request).

**What remains genuinely unbound** — not upstream-blocked, just not built
in this task, and enumerated rather than left silent (per Task 024 §3.4):
Settings' locale/theme/text-scale pickers and its two toggles; Storage's
three entry-point buttons (its confirm/cancel dialog *is* reachable once
open); the recent-searches panel's open/close/per-entry/clear-entry
controls (same partial shape as Storage: the confirm dialog works, the
trigger doesn't); the search-location chip's remove and scope-toggle
buttons; recent-folder chips; Search's advanced-mode buttons; Sources'
"Add folder" button; and the wizard's own `WizardBack` and CheckedNotOk's
manual-path `Validate` action specifically (already reachable through
that page's own `text_input` submit whenever it has focus; not bound to
the global `Enter` too since that would only be redundant, not
conflicting — see `shell::confirm_message`'s own comment). Two actions
have an equivalent path despite no direct binding: `SubmitSearch` (the
search input's own `on_submit` already covers it) and Search's empty-state
"Add source" CTA (`Ctrl/Cmd+2` reaches the same view).

**Remediation for the remainder:** not scheduled. A per-button shortcut
scheme covering ~19 more disparate actions is a real design question
(what keys, how they're discoverable, whether some deserve a different
mechanism entirely), not a mechanical extension of Task 024's map —
recorded here so the next person does not have to rediscover the count.

### 2.1.2 No Keyboard Trap

**Status: Met — re-assessed 2026-08-17 (Task 024).**

Tab traversal is now real (2.1.1), so this criterion moved from "vacuously
true" to actually checkable. `focus_next()`/`focus_previous()` cycle among
orbok's 4 text inputs (`iced_runtime::widget::operation`'s own contract —
these operations wrap, they do not dead-end), and nothing in the map
consumes `Tab` for any other purpose that could strand focus inside a
widget. `Escape` remains a keyboard-only way out of every dialog and the
wizard, independent of the Tab cycle. No trap.

### 2.4.3 Focus Order

**Status: Met, for what Tab actually reaches — corrected 2026-08-16,
confirmed 2026-08-17 (Task 024).**

This previously read "Met (iced built-in) — iced 0.14 manages Tab order by
widget tree order," which was false (iced 0.14 manages no Tab order at
all) and was corrected to NOT MET. Task 024 wired real traversal
(`focus_next()`/`focus_previous()`), so the criterion is checkable again:
orbok's column-based layouts match visual reading order, and at most one
of the 4 text inputs is ever on screen at once (the search query input,
the wizard's path input, and the Sources "type a path manually" input
never coexist), so there is no multi-input ordering to get wrong. **This
is Met for the traversal that exists, not for the whole interface** —
buttons sit outside any order because they are outside Tab's reach
entirely, which is 2.1.1's remaining gap, not this criterion's.

### 2.4.7 Focus Visible

**Status: Partially met — known renderer limitation (see below).**

**The blocking claim here was over-scoped. Re-assessed 2026-08-18** after snora
0.34.0 corrected the same over-statement in its own documentation at five sites.

It read: *"A token-driven focus ring on standard widgets cannot be delivered
through the snora style bridge in this iced version."* The accurate constraint is
narrower:

> iced cannot tell a style closure that a widget **iced** owns is focused.

A `container` style closure is an arbitrary `Fn(&Theme) -> Style`, so anything
**the application** already knows is available inside it — including a focused or
selected boolean driving border colour *and* width. **orbok already does this**:
`result_card` and `source_card` choose `card::selected` over `card::surface` from
orbok's own `is_selected`, which is precisely the mechanism the old text said
could not be delivered.

So the criterion splits, and only one half is blocked:

| Focus owned by | Ring renderable? | Status |
|---|---|---|
| **orbok** — list selection (`selected_result_idx`, `selected_source`) | **Yes, today** | rendered via `card::selected` |
| **iced** — the 4 text inputs reached by `Tab` | No — `Status` has no `Focused` variant | genuinely blocked |

For iced-owned focus the text inputs render their own cursor/caret, which is
visible feedback for the only widgets `Tab` can reach (see 2.1.1). So the
practical gap is narrower than "no focus ring anywhere".

**Available and not yet adopted:** `snora::design::FocusTokens` (`ring_width`,
`ring_offset`, `ring_color`) exists and is reachable under the `design` feature.
orbok's selection indicator is currently an accent border chosen ad hoc rather
than driven by those tokens. Moving to `FocusTokens` would make ring width and
offset token-driven like every other visual value, and is the obvious next step
for this criterion — not a blocked one.

What we provide:
- Corrected 2026-08-17 (Task 024): the line here previously claimed "iced's
  own built-in keyboard focus traversal (operability — 2.1.1 — is met)" —
  false on both halves even before this task (iced 0.14 has no built-in
  traversal; orbok never called `focus_next`/`focus_previous` before Task
  024 wired it) and irrelevant to *this* criterion regardless, since 2.4.7
  is about focus being *visible*, not present.
- `focus_next()`/`focus_previous()` move real iced focus among the 4 text
  inputs (2.1.1), and iced's text inputs render their own cursor/caret as
  visible focus feedback for those — the gap is buttons and cards, which
  have no `Focused` status to render at all.
- Both selection-model list views use `card::selected` (accent border) as
  a visible selection indicator: the search results list, and, since Task
  024, the Sources view.
- High-contrast themes maximise the visibility of affordances we can render.

Tracked upstream: iced exposing focus state for widgets it owns. That remains a
real dependency — but it now bounds a **quarter** of this criterion (4 text
inputs that already show a caret), not all of it. Do not restate it as a blanket
"cannot render a focus ring".

### 2.5.8 Target Size (Minimum)

**Status: Met — evidence corrected 2026-08-18.**

Prompted by snora RFC-061, which found its own `chip` dismiss control at
**15.0 px** against the 24 × 24 minimum. Asking the same question here exposed
two problems with this entry, though not with the targets themselves.

**1. The cited test does not verify the claim.** `primary_action_target_size`
asserts three inequalities on *token values* — `spacing.md >= 10`,
`spacing.lg >= 14`, `2 × spacing.md >= 24`. It never measures a widget. It
verifies the tokens *permit* an adequate target, not that any control is one.

**2. The claim was scoped to primary actions; the criterion is not.** WCAG 2.5.8
applies to every target. orbok's smallest controls are not primary actions — they
are chips and toggles built with bare `button(...)`, which take iced's
`DEFAULT_PADDING` (5 px vertical, 10 px horizontal), not the
`[spacing.md, spacing.lg]` = `[12, 16]` this entry described.

**The smallest target, computed:** a chip renders `meta_s` text — `body_small`,
14.0 px at the default scale — at iced's default `LineHeight::Relative(1.3)`,
inside 5 px vertical padding each side. Height ≈ **14 × 1.3 + 10 ≈ 28 px**,
comfortably above 24. `TextScale` only increases from there (1.0 / 1.15 / 1.3),
so 28 px is the floor. Width is label-driven and far larger.

So the status holds. **What was wrong was the evidence, not the outcome** — and
had a control been undersized, nothing in the suite would have caught it.

orbok's house rule of 44 px for primary actions (WCAG 2.5.5 AAA guideline) is
separate and unaffected.

**Not applicable:** snora's RFC-061 chip repair does not reach orbok. We do not
use `snora::design::widget::chip`; our chips are single `button`s whose label
*contains* the `✕` (`views.rs:196`), so the dismiss target is the whole chip
rather than a glyph-sized control.

### 4.1.2 Name, Role, Value

**Status: Partially met.**

Every interactive control has a text label (name) and uses a native iced widget
(role). Value exposure to the platform accessibility tree depends on iced's
AccessKit integration, which is limited in v0.14. Labels sourced from the i18n
catalog are the authoritative accessible names and will flow to AccessKit when
iced exposes the tree.

---

## Known renderer limitations (iced 0.14)

These are owned, tracked decisions — not silent gaps.

| Limitation | Criterion | Mitigation | Upstream |
|---|---|---|---|
| No `Focused` widget status → no CSS-style focus ring on buttons/cards | 2.4.7 | High-contrast themes; card::selected accent border | snora-team issue; revisit when iced exposes focus state |
| AccessKit integration limited | 4.1.2 | i18n labels as authoritative names; tooltip strings on icon controls | iced roadmap item |
| `FocusSearch` targets a view switch, not the input directly | 2.4.3 (operability) | Switches to Search view; user's next keypress reaches input | Task 024 found `iced_runtime::widget::operation::focus::<T>(id)` genuinely exists in iced 0.14 (used for `focus_next`/`focus_previous` there) — this row's original claim that no such Task exists at all was wrong. Retargeting `FocusSearch` at it directly is a small, separate follow-up (needs an `Id` assigned to the search input), not done here. |

---

## Manual a11y QA (M13 gate)

Before each release, run through the following steps on at least one platform:

### Keyboard-only walkthrough

**Rewritten 2026-08-17 (Task 024)** after the version above turned out to
describe capabilities that did not exist — step 1 asked for sidebar
navigation and a theme change via `Tab`, both still unreachable today (see
2.1.1's remainder list). This version only asks for what is actually
bound; do not extend it to cover 2.1.1's known-open items until they are.

1. **First launch, with no model configured:** confirm the setup wizard
   appears and blocks the rest of the app. Press `Escape`. Confirm the
   wizard closes and the Search view renders behind it — this is the
   walkthrough that originally found *"nothing worked at all."*
2. From the Search view, press `Ctrl/Cmd+1` through `Ctrl/Cmd+6` in turn:
   confirm each lands on Search, Sources, Indexing, Storage, Models,
   Settings respectively.
3. On the Search view, `Tab` into the query input, type a query, press
   `Enter`: confirm it submits. With results showing, use `Arrow Down`/
   `Arrow Up` (not while typing) to move the selection; confirm the
   selected card shows an accent border.
4. On the Sources view with at least one folder added, use `Arrow Down`/
   `Arrow Up` to select a source (confirm the accent border), then press
   `Enter`: confirm it is removed. Press `Escape` after selecting one:
   confirm the selection clears without removing it.
5. Trigger the Storage reset confirmation (mouse is fine to reach it —
   its own trigger button is not yet keyboard-bound, tracked in 2.1.1):
   confirm `Escape` cancels and `Enter` confirms.
6. Press `Ctrl/Cmd+K` from any page: confirm the Search view comes to
   focus. Press `Ctrl/Cmd+,` from any page: confirm Settings opens.

**Do not** attempt to reach the sidebar/tab-bar directly, the theme/
locale/text-scale pickers, or any of the other items 2.1.1 lists as
remaining — they are mouse-only by design record, not a walkthrough
failure to report again.

### Screen reader spot check — BLOCKED, do not attempt

**Verified 2026-08-15:** `accesskit` appears nowhere in iced 0.14's source, and
iced 0.14 declares no accessibility feature to enable. It is absent from orbok's
dependency graph as a result. **orbok exposes no platform accessibility tree on
any platform**, so no screen reader can announce its cards, buttons or badges.

The steps this section previously listed — confirming that source cards announce
their content, that the danger button announces "Reset Catalog", that status
badges announce their labels — could never have passed. They are preserved here
as the specification of what to run *once the block clears*, not as work to do
now:

1. Navigate to the Sources view; confirm source cards announce their content.
2. Navigate to the Storage view; confirm the danger button announces "Reset
   Catalog" (or locale equivalent).
3. Confirm status badges announce their label text.

**This does not widen the conformance gap already recorded.** §4.1.2 above is
marked *Partially met* and the "Known renderer limitations" table names limited
AccessKit integration with i18n labels as the mitigation. What changes here is
only that the QA procedure now matches that position instead of asking for an
outcome the architecture cannot produce.

**Reinstatement trigger:** iced exposing an accessibility tree. The labels are
already in place and are the authoritative accessible names (§4.1.2), so
reinstating this is a QA change rather than a development one. Run it on Linux
with Orca; on Windows use NVDA in preference to Narrator, as it is what blind
Windows users predominantly run.

### High-contrast visual pass

Switch to each of the four non-System themes and verify:
- Body text is legible on all surfaces.
- Status badges (Stale, Missing, Current, Keyword, Semantic) are distinguishable.
- Danger buttons are visually distinct from primary buttons.

### Grayscale status-distinguishability pass

Take a screenshot of the Search view with at least one result showing multiple
badge types, and desaturate it. Confirm each badge type is distinguishable by
its icon and label alone.

---

## Automated coverage

| Test | File | What it checks |
|---|---|---|
| `contrast_usage_guard_all_presets` | `tests.rs` | All `a11y::RENDERED_PAIRS` meet AA across 4 presets |
| `status_badge_label_and_icon_invariant` | `tests.rs` | Every tone maps to a non-null icon; badges build without panic |
| `badge_tone_mapping` | `tests.rs` | Stable label → Tone mapping |
| `key_map_shortcuts` | `tests/a11y.rs` | Shortcut keys → correct Messages |
| `key_map_view_shortcuts` | `tests/a11y.rs` | `Ctrl/Cmd+1..6` → correct view (Task 024) |
| `key_map_tab_focus` | `tests/a11y.rs` | `Tab`/`Shift+Tab` → `FocusNext`/`FocusPrevious` (Task 024) |
| `key_map_source_arrows_scoped_to_sources_view` | `tests/a11y.rs` | Arrow keys route to Sources' own selection, not Search's (Task 024) |
| `key_map_enter_confirms_by_context` | `tests/a11y.rs` | `Enter` (not typing) dispatches the right concrete Message per dialog/wizard/list context (Task 024) |
| `key_map_no_text_swallow` | `tests/a11y.rs` | Printable keys not intercepted while typing |
| `dismiss_overlay_closes_reset` | `tests/a11y.rs` | Escape closes confirm_reset dialog |
| `dismiss_overlay_closes_clear_history_confirm` | `tests/a11y.rs` | Escape closes confirm_clear_history dialog (Task 024) |
| `dismiss_overlay_skips_wizard_on_setup` | `tests/a11y.rs` | Escape performs the wizard's zero-confirmation Skip fallback (Task 024) |
| `dismiss_overlay_cancels_download_consent` | `tests/a11y.rs` | Escape on DownloadConsent mirrors its own Cancel button (Task 024) |
| `dismiss_overlay_clears_list_selection` | `tests/a11y.rs` | Escape clears the active view's list selection (Task 024) |
| `key_map_escape_cancels_download_in_progress` | `tests/a11y.rs` | Escape on Downloading → `CancelDownloadInProgress`, not `DismissOverlay`; every other wizard kind still falls through (Task 027) |
| `result_navigation_bounds` | `tests/a11y.rs` | Arrow keys move result selection, clamp at bounds |
| `source_navigation_bounds` | `tests/a11y.rs` | Arrow keys move source selection, clamp at bounds (Task 024) |
| `primary_action_target_size` | `tests/a11y.rs` | Primary buttons ≥ 44 px at default tokens |
| `ctrl_digit_reaches_each_view_by_keyboard` | `tests/keyboard_reachability.rs` | `Ctrl/Cmd+1..6` reaches each view *through the rendered app*, not just the Message map (Task 024) |
| `escape_dismisses_wizard_and_reveals_the_view_behind_it` | `tests/keyboard_reachability.rs` | The walkthrough-blocking fix, end to end (Task 024) |
| `select_and_activate_a_source_by_keyboard` | `tests/keyboard_reachability.rs` | Arrow-select + Enter-remove a source, through the rendered app (Task 024) |
| `enter_on_setup_reaches_the_download_consent_page` | `tests/keyboard_reachability.rs` | `Enter` on Setup reaches and renders `DownloadConsent`, through the rendered app (Task 027) |
| `escape_on_downloading_requests_cancellation_through_the_real_chain` | `crates/app/src/model_flow.rs` | `Escape`'s Message reaches `model_flow::reduce` and actually sets `cancelling` — the chain `keyboard_reachability.rs` cannot cover, since `CancelDownloadInProgress` is model_flow-owned, not `AppState`-owned (Task 027) |
