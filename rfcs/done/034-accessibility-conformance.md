# RFC-034: Accessibility Conformance (WCAG 2.1 AA)

**Project:** orbok
**RFC:** 034
**Title:** Accessibility Conformance
**Status:** Implemented (v0.13.0)
**Target Milestone:** M9–M13 (cross-cutting), gate at M13
**Date:** 2026-06-07
**Depends on:** RFC-032 (token contrast), RFC-033 (accessible primitives)

---

## 1. Summary

This RFC sets `orbok`'s accessibility target and the concrete, testable
practices that meet it, using the Snora Design system as the delivery
mechanism. The GUI external design already *requires* accessibility (§17, §20)
but states it as goals; this RFC turns those goals into rules with acceptance
criteria and tests.

The decision is:

> `orbok` targets WCAG 2.1 Level AA for its desktop GUI. Color contrast is
> guaranteed at the token layer (Snora Design contrast-tested palettes); status
> is never conveyed by color alone; every interactive control is
> keyboard-reachable with a visible focus indication where the renderer permits;
> dialogs trap and restore focus; and every control carries a screen-reader
> label sourced from the i18n catalog.

---

## 2. Motivation

- The GUI design (§17 Accessibility Specification, §20 Visual Tone) commits to
  keyboard navigation, focus management, screen-reader labels, non-color-only
  status, and readable contrast — but nothing enforces them, and the current
  views partly violate them (color is unused for status, which is *safe*, but
  focus order and SR labels are undefined; some touch targets are below the
  44px guideline).
- The target users include privacy-conscious professionals and developers;
  accessibility is table stakes for a daily-driver desktop tool and a
  precondition for some procurement contexts.
- RFC-032 and RFC-033 have already done most of the heavy lifting: contrast is
  handled at the token layer, and the primitives are keyboard-reachable. This
  RFC ties those together, fills the remaining gaps (focus, labels, target
  size), and makes the guarantee testable.
- Doing this as a named conformance effort (rather than scattered fixes) gives a
  single place to record the **known renderer limitation**: iced 0.14's
  `button`/`container` expose no focused state, so custom focus *rings* are not
  deliverable on standard widgets through the style bridge. We must therefore
  be explicit about what AA we can and cannot mechanically guarantee on iced
  0.14, and how we compensate.

---

## 3. Goals

- A documented conformance target: **WCAG 2.1 AA**, scoped to the desktop GUI.
- **Contrast (1.4.3, 1.4.11):** all body/label text and all status surfaces use
  Snora Design roles whose contrast is verified by snora's automated tests;
  orbok adds a guard test asserting it only ever renders text on token-paired
  backgrounds (e.g. `*_text on *`).
- **Non-color status (1.4.1):** every status is text + (icon or shape) + tone;
  tone is never the sole carrier. (Enforced via RFC-033 `status_badge`.)
- **Keyboard (2.1.1, 2.1.2):** every action reachable and operable by keyboard;
  no keyboard trap except intentional modal focus traps that are escapable with
  `Esc`. The shortcut map from GUI §17.1 (`Ctrl/Cmd+K`, `Enter`, `Esc`,
  `Tab/Shift+Tab`, arrow keys in result list, `Ctrl/Cmd+,`) is implemented.
- **Focus management (2.4.3, 2.4.7):** logical focus order; dialogs trap focus
  and restore it to the trigger on close; the focus indicator is visible where
  the renderer supports it, with a documented fallback where it does not.
- **Labels/name-role-value (4.1.2, 1.1.1):** every icon-only control has a
  text label for assistive tech, sourced from the i18n catalog (RFC-031); no
  control communicates only through an icon glyph.
- **Target size (2.5.8 AA, 24px min; orbok house rule 44px):** all primary
  controls meet the house 44px minimum touch/click target via token padding.

---

## 4. Non-Goals

- WCAG AAA. We target AA; AAA enhancements (e.g. 7:1 contrast everywhere) are
  served opportunistically by the high-contrast themes (RFC-032) but not
  required.
- A full AccessKit / platform accessibility-tree integration. iced 0.14's
  accessibility surface is limited; we deliver labels and keyboard operability
  and record AccessKit integration as a forward-looking item (it is gated on
  iced/snora support).
- Mobile/touch-specific gestures (orbok is desktop; §16 responsive rules are
  honored but touch gesture a11y is out of scope).
- Re-deriving contrast math — we rely on snora's contrast utilities
  (`snora_design::contrast`; not currently on the `snora::design` facade — see
  the handoff's import note) and snora's palette tests, and only add orbok-level
  *usage* guards.

---

## 5. Design

### 5.1. Contrast — guaranteed at the token layer

Because RFC-032 forbids literal colors and RFC-033 routes all status through
tone-paired primitives, contrast is structurally correct: snora's palettes pair
each status background with a contrast-tested `*_text` foreground, and the
built-in presets are AA-tested for body text on primary surfaces. orbok adds one
**usage guard test**: enumerate the foreground/background pairs orbok actually
renders and assert each meets the AA ratio using `snora_design::contrast`'s
`contrast_ratio` (4.5:1 normal text, 3:1 large text and
UI components). `text_muted` is exempt (snora documents it as below-body and for
non-essential text only) and must therefore never be used for essential text — a
rule we encode in the component layer.

### 5.2. Non-color status

Delivered by RFC-033's `status_badge(tokens, label, tone)`: the label text is
mandatory, the tone is supplementary. The §17.4 examples (`[Stale]`,
`[Missing Source]`, `[Failed]`, `[Current]`, `[Temporary]`) are realized as
text badges; an optional lucide icon (already the sole-gateway icon system)
reinforces shape. A test asserts no status path produces a tone without a label.

### 5.3. Keyboard map and focus

Implement the GUI §17.1 shortcut table at the shell level (`shell.rs` /
`orbok-app` subscription):

| Shortcut          | Action                                   |
|-------------------|------------------------------------------|
| `Ctrl/Cmd+K`      | focus global search input                |
| `Enter`           | submit search / activate focused primary |
| `Esc`             | close dialog/drawer, restore focus       |
| `Tab`/`Shift+Tab` | move through controls in logical order   |
| Arrow keys        | move through result list when focused     |
| `Ctrl/Cmd+,`      | open Settings                            |

Shortcuts must not intercept normal text entry (§17.1). Dialogs (reset
confirmation, add/remove source, model install) trap focus while open and
restore focus to the triggering control on close (§17.2).

### 5.4. Focus visibility — the iced 0.14 limitation (recorded)

`snora-widgets` documents that iced 0.14's `button`/`container` `Status` has no
`Focused` variant, so a token-driven focus *ring* cannot be drawn on standard
widgets through the style bridge. `FocusTokens` remain valid vocabulary for
future iced versions / custom widgets.

orbok's stance, recorded here so it is a known, owned limitation rather than a
silent gap:

1. We rely on iced's built-in keyboard focus traversal for operability (2.1.1)
   — which works — and on `Hovered`/`Pressed` styling for pointer feedback.
2. Where a visible focus indicator is mechanically achievable (custom widgets,
   or list-selection highlight like the existing result `▶` marker), we provide
   one.
3. We file/track the upstream need (snora-team influence) for focus-ring support
   when iced exposes focus state; this RFC is the reference.
4. The high-contrast themes (RFC-032) maximize the visibility of the affordances
   we *can* render.

This is an honest AA-for-iced-0.14 posture: keyboard operability is met; the
*visible focus indicator* success criterion (2.4.7) is met where the renderer
allows and explicitly tracked where it does not.

### 5.5. Screen-reader labels

Every icon-only control (trash/remove, folder-add, search submit icon button,
nav icons in collapsed sidebar) gets an accessible text label from the i18n
catalog. Since iced 0.14's a11y-tree exposure is limited, the practical rule is:
**no control is icon-only in its operable form** — icon controls either include
a visible text label (wide layouts, GUI §20.3 "always pair icons with text") or
provide a tooltip/label string that is also the i18n source for future AccessKit
exposure. The sidebar already carries `tooltip` strings per item; this RFC
extends the same discipline to every icon button.

### 5.6. Target size

The existing `icon_btn` aims for ~44px via `[12,16]` padding. Under RFC-032 this
becomes token-driven (`[spacing.md, spacing.lg]`) and is asserted: a test (or
documented measurement) confirms primary controls meet the 44px house minimum
at the default density. Chips/badges may be smaller (non-essential, and AA's
24px applies to them) but actionable chips meet 24px.

### 5.7. Conformance record

A new doc `docs/src/maintainers/accessibility.md` records: the AA target, the
success-criteria checklist with orbok's status per criterion, the iced-0.14
focus-ring limitation, and the manual a11y QA steps (keyboard-only walkthrough,
screen-reader spot check, high-contrast visual pass). This complements RFC-019's
manual QA checklist with an accessibility section gated at M13.

---

## 6. Rules

1. Essential text and all status surfaces render only on token-paired
   foreground/background roles; `text_muted` is never used for essential text.
2. Every status carries a text label; tone/color is supplementary, never sole.
3. Every action is keyboard-operable; the §5.3 shortcut map is implemented and
   does not break text entry.
4. Dialogs trap focus and restore it to the trigger on close.
5. No control is operable as icon-only; icon controls carry an i18n label
   (visible in wide layouts, tooltip otherwise).
6. Primary controls meet the 44px house target via tokens.
7. The iced-0.14 focus-ring limitation is documented, not silently ignored; new
   custom widgets that *can* show focus, must.

---

## 7. Acceptance Criteria

- The usage-guard contrast test passes for every foreground/background pair
  orbok renders (AA thresholds).
- No status badge is produced without a text label (test).
- A keyboard-only user can: focus search (`Ctrl/Cmd+K`), run a search, move
  through results (arrows), open and `Esc`-close every dialog with focus
  restored, and reach Settings (`Ctrl/Cmd+,`).
- Every icon-only control has an associated i18n label string.
- Primary controls measure ≥44px at default density.
- `docs/src/maintainers/accessibility.md` exists with the AA checklist, the
  focus-ring limitation, and the manual a11y QA steps; M13 QA includes it.
- Build warning-free; suite green.

---

## 8. Testing Requirements

1. **Contrast usage guard:** table of orbok's rendered (fg, bg) role pairs;
   assert `contrast_ratio ≥ 4.5` (normal) / `≥ 3.0` (large/UI) for each, across
   all four theme presets.
2. **Status-label invariant:** property/table test that `status_badge` always
   includes non-empty label text.
3. **Keyboard map:** unit tests that key events map to the correct `Message`
   (and that printable keys in a focused text input are *not* swallowed by
   shortcuts).
4. **Focus restore:** dialog open→close returns focus target to the trigger
   (via `iced_test` where feasible; else a state-machine assertion on the
   focus-target field).
5. **Label coverage:** test enumerating icon controls asserts each has a
   non-empty i18n label in both locales.
6. **Target size:** assert primary action padding resolves to ≥44px at default
   tokens.

---

## 9. Unresolved Questions

- When iced/snora expose widget focus state, do we adopt AccessKit for a full
  platform a11y tree, and on what timeline? (Tracked; gated on upstream.)
- Should we ship an in-app "keyboard shortcuts" help overlay (discoverability),
  or document shortcuts only? (Leaning: small overlay; cheap and high value.)
- Do any badges that are *actionable* (e.g. "Reindex File" on a stale card)
  need the 44px target rather than 24px? (Likely yes for the action, no for the
  badge label; clarify in handoff.)

---

## 10. Decision

Adopt WCAG 2.1 AA as `orbok`'s GUI accessibility target, delivered through the
Snora Design token contrast guarantee (RFC-032), the accessible primitives
(RFC-033), an implemented keyboard map with focus trap/restore, i18n-sourced
labels for all icon controls, and a 44px target rule — with the iced-0.14
focus-ring limitation documented and tracked rather than hidden. Conformance is
recorded in a maintainer doc and gated in M13 QA.
