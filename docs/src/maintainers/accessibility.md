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
`a11y::audit` across all four theme presets and asserts AA ratios (≥ 4.5:1
normal, ≥ 3.0:1 large/UI) for every pair orbok renders. `text_muted` is
intentionally exempt (non-essential decorative text only — never used for
essential content).

### 1.4.11 Non-text Contrast

**Status: Met.**

UI component boundaries (borders on surface) are included in `a11y::RENDERED_PAIRS`
at the ≥ 3.0:1 threshold.

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
| `Escape` | Close overlay / dismiss notice / **skip or back out of the wizard** / cancel a confirm dialog / clear a list selection (priority order, first match wins) |
| `Enter` (search input focused) | Submit search |
| `Enter` (not typing) | Confirm whichever dialog/wizard page/list selection is active, if any (§ below) |
| `Arrow Down`/`Up` (Search view, not typing) | Select next/previous result |
| `Arrow Down`/`Up` (Sources view, not typing) | Select next/previous source |

**The wizard — the walkthrough's actual blocker — is fixed.** `Escape` on
the Setup/Checked/DownloadFailed pages performs the same zero-confirmation
fallback the mouse-only Skip button does; on DownloadConsent it mirrors
Cancel. A keyboard-only user reaching first launch is no longer stuck.

**What remains genuinely unbound** — not upstream-blocked, just not built
in this task, and enumerated rather than left silent (per Task 024 §3.4):
Settings' locale/theme/text-scale pickers and its two toggles; Storage's
three entry-point buttons (its confirm/cancel dialog *is* reachable once
open); the recent-searches panel's open/close/per-entry/clear-entry
controls (same partial shape as Storage: the confirm dialog works, the
trigger doesn't); the search-location chip's remove and scope-toggle
buttons; recent-folder chips; Search's advanced-mode buttons; Sources'
"Add folder" button; the wizard's own `WizardBack` and Setup/Checked's
`DownloadModel`/manual-path `Validate` actions specifically (deliberately
*not* bound to the global `Enter` — those two pages each render a
`text_input` with its own competing `on_submit`, and orbok has no way to
tell whether that input genuinely has keyboard focus; see
`shell::confirm_message`'s own comment); and, as of Task 025, the
Downloading page's Cancel button (`Message::CancelDownloadInProgress`) —
added to stop an in-progress model download, mouse-only for the same
reason as the rest of this list: no shortcut was invented for it rather
than reopening this already-reviewed keyboard map mid-task. Two actions
have an equivalent path despite no direct binding: `SubmitSearch` (the
search input's own `on_submit` already covers it) and Search's empty-state
"Add source" CTA (`Ctrl/Cmd+2` reaches the same view).

**Remediation for the remainder:** not scheduled. A per-button shortcut
scheme covering ~20 more disparate actions is a real design question
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

iced 0.14's `button`/`container` `Status` enum exposes `Active | Hovered |
Pressed | Disabled` only; there is no `Focused` variant. A token-driven focus
ring on standard widgets cannot be delivered through the snora style bridge in
this iced version.

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

Tracked upstream: snora-team issue for focus-ring support when iced exposes
focus state. Until then this criterion is "met where renderer allows."

### 2.5.8 Target Size (Minimum)

**Status: Met.**

Primary action buttons use `Padding::from([tokens.spacing.md, tokens.spacing.lg])`
= `[12, 16]` at the default (Comfortable) density, producing targets well above
the WCAG 2.5.8 AA minimum of 24 × 24 px. orbok's house rule is 44 px for
primary actions (WCAG 2.5.5 AAA guideline), verified by the
`primary_action_target_size` test.

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
| `result_navigation_bounds` | `tests/a11y.rs` | Arrow keys move result selection, clamp at bounds |
| `source_navigation_bounds` | `tests/a11y.rs` | Arrow keys move source selection, clamp at bounds (Task 024) |
| `primary_action_target_size` | `tests/a11y.rs` | Primary buttons ≥ 44 px at default tokens |
| `ctrl_digit_reaches_each_view_by_keyboard` | `tests/keyboard_reachability.rs` | `Ctrl/Cmd+1..6` reaches each view *through the rendered app*, not just the Message map (Task 024) |
| `escape_dismisses_wizard_and_reveals_the_view_behind_it` | `tests/keyboard_reachability.rs` | The walkthrough-blocking fix, end to end (Task 024) |
| `select_and_activate_a_source_by_keyboard` | `tests/keyboard_reachability.rs` | Arrow-select + Enter-remove a source, through the rendered app (Task 024) |
