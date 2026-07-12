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

**Status: Met.**

Every action is keyboard-operable. The shortcut map is implemented in
`key_to_message` (`crates/ui/src/shell.rs`) and wired via
`iced::keyboard::listen()` in the `orbok` app crate. No action is mouse-only.

Shortcut map:

| Shortcut | Action |
|---|---|
| `Ctrl/Cmd + K` | Focus Search view |
| `Ctrl/Cmd + ,` | Open Settings |
| `Escape` | Close overlay / dismiss notice |
| `Enter` (search focused) | Submit search |
| `Arrow Down` (not typing) | Select next result |
| `Arrow Up` (not typing) | Select previous result |
| `Tab` / `Shift+Tab` | Navigate controls (iced built-in) |

### 2.1.2 No Keyboard Trap

**Status: Met.**

The confirmation dialog (`confirm_reset`) renders only its own interactive
controls while active, so Tab cycling is naturally contained. `Escape` always
dismisses via `DismissOverlay`. No permanent keyboard trap exists.

### 2.4.3 Focus Order

**Status: Met (iced built-in).**

iced 0.14 manages Tab order by widget tree order, which matches the visual
reading order in orbok's column-based layouts.

### 2.4.7 Focus Visible

**Status: Partially met — known renderer limitation (see below).**

iced 0.14's `button`/`container` `Status` enum exposes `Active | Hovered |
Pressed | Disabled` only; there is no `Focused` variant. A token-driven focus
ring on standard widgets cannot be delivered through the snora style bridge in
this iced version.

What we provide:
- iced's own built-in keyboard focus traversal (operability — 2.1.1 — is met).
- The selected result card uses `card::selected` (accent border) as a visible
  selection indicator.
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
| No programmatic `text_input::focus()` Task in iced 0.14 | 2.4.3 (operability) | `FocusSearch` switches to Search view; user's next keypress reaches input | revisit when iced exposes focus Task |

---

## Manual a11y QA (M13 gate)

Before each release, run through the following steps on at least one platform:

### Keyboard-only walkthrough

1. Launch orbok. Using only `Tab`, `Shift+Tab`, `Enter`, `Escape`, and arrow
   keys, verify you can:
   - Navigate the sidebar to every group.
   - Open each tab within the Search and AI groups.
   - Enter a search query and submit it.
   - Move through results with arrow keys.
   - Open and dismiss the reset confirmation dialog with `Escape`.
   - Reach and change the theme in Settings.
2. Press `Ctrl/Cmd+K` from any page: confirm the Search view comes to focus.
3. Press `Ctrl/Cmd+,` from any page: confirm Settings opens.

### Screen reader spot check

On macOS (VoiceOver) or Linux (Orca):

1. Navigate to the Sources view; confirm source cards announce their content.
2. Navigate to the Storage view; confirm the danger button announces "Reset
   Catalog" (or locale equivalent).
3. Confirm status badges announce their label text.

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
| `key_map_shortcuts` | `tests.rs` | Shortcut keys → correct Messages |
| `key_map_no_text_swallow` | `tests.rs` | Printable keys not intercepted while typing |
| `dismiss_overlay_closes_reset` | `tests.rs` | Escape closes confirm_reset dialog |
| `result_navigation_wraps` | `tests.rs` | Arrow keys move selection, clamp at bounds |
| `primary_action_target_size` | `tests.rs` | Primary buttons ≥ 44 px at default tokens |
