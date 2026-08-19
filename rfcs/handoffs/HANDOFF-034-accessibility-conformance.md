# HANDOFF-034 — Accessibility Conformance (WCAG 2.1 AA)

**RFC:** `rfcs/done/034-accessibility-conformance.md`
**Owner crate(s):** `orbok-ui` (keyboard map, labels, contrast guard),
`orbok-app` (shortcut subscription wiring), `docs/`
**Prereqs:** HANDOFF-032 (token contrast) + HANDOFF-033 (accessible primitives).
**Release:** current version — do **not** bump.

---

## 0. Orientation

Tokens (032) and primitives (033) already give us contrast and keyboard-reachable
controls. This step adds the keyboard **map**, focus **trap/restore** for
dialogs, i18n **labels** for icon-only controls, the 44px **target** assertion,
the **contrast usage-guard** test, and the maintainer **conformance doc** — then
gates it in M13 QA. It also records, in one place, the iced-0.14 focus-ring
limitation so it is an owned decision, not a silent gap.

---

## 1. Files

| File | Action |
|---|---|
| `crates/ui/src/state.rs` | add focus-target field for dialogs; ensure messages exist for shortcut actions |
| `crates/ui/src/shell.rs` | map keyboard events → `Message` (or expose a `key_to_message` fn for app) |
| `crates/app/src/main.rs` | iced keyboard subscription → `key_to_message` (text-input-safe) |
| `crates/ui/src/components.rs` | `status_badge` label invariant; icon buttons carry i18n label |
| `crates/ui/src/i18n/{en,ja}.rs` | add label keys for every icon-only control |
| `crates/ui/src/a11y.rs` | **new** — contrast usage-guard pairs + helper |
| `crates/ui/src/tests.rs` | keyboard-map, focus-restore, contrast-guard, label-coverage, target-size tests |
| `docs/src/maintainers/accessibility.md` | **new** — AA checklist, focus-ring limitation, manual a11y QA |
| `docs/src/maintainers/release_readiness.md` | reference the a11y QA section (M13 gate) |

---

## 2. Keyboard map

Implement GUI §17.1. Keep the *mapping* in `orbok-ui` (pure, testable); keep the
*subscription* in `orbok-app` (iced runtime / I/O boundary, RFC-027).

```rust
// orbok-ui/src/shell.rs  (or a small keyboard.rs)
use iced::keyboard::{Key, Modifiers, key::Named};

/// Map a key event to a Message, or None to let iced handle it normally.
/// MUST return None for ordinary text entry so inputs keep working.
pub fn key_to_message(key: &Key, mods: Modifiers, text_input_focused: bool) -> Option<Message> {
    use Message::*;
    match (key, mods) {
        // Ctrl/Cmd+K — focus global search
        (Key::Character(c), m) if c == "k" && m.command() => Some(FocusSearch),
        // Ctrl/Cmd+, — open Settings
        (Key::Character(c), m) if c == "," && m.command() => Some(Switch(ViewId::Settings)),
        // Esc — close active dialog/drawer
        (Key::Named(Named::Escape), _) => Some(DismissOverlay),
        // Enter — submit search only when search input focused (else None)
        (Key::Named(Named::Enter), _) if text_input_focused => Some(SubmitSearch),
        // Arrow keys move result selection only when not typing
        (Key::Named(Named::ArrowDown), _) if !text_input_focused => Some(SelectNextResult),
        (Key::Named(Named::ArrowUp),   _) if !text_input_focused => Some(SelectPrevResult),
        _ => None, // printable keys & everything else: let iced handle
    }
}
```

New messages to add (state.rs): `FocusSearch`, `DismissOverlay`,
`SelectNextResult`, `SelectPrevResult`. `FocusSearch` sets focus to the search
`text_input` (use an `iced::widget::text_input::Id` and
`iced::widget::operate`/`focus` op in the app update return). `DismissOverlay`
closes whichever overlay is open (reset confirm, add/remove dialog) — reuse
existing `CancelResetCatalog` semantics generically.

`orbok-app` wires `iced::keyboard::on_key_press(...)` (or subscription) →
`key_to_message`, passing whether a text input is focused (track via a
`search_focused: bool` set by `text_input` focus/blur, or approximate with the
active view + input state).

---

## 3. Focus trap / restore for dialogs

iced 0.14 has no built-in focus trap. Practical approach:

- Track `focus_return: Option<text_input::Id /* or control id */>` in state: set
  it to the triggering control when a dialog opens; on `DismissOverlay`/confirm,
  issue a focus op back to it.
- For the reset-catalog confirm (the one true modal today), ensure Tab cycles
  only its controls while open. In iced 0.14 this is approximated by rendering
  *only* the dialog's interactive controls while `confirm_reset` is true (the
  page behind is non-interactive), which is already close to current behavior —
  verify and make explicit.
- Document this as the iced-0.14 approximation in `accessibility.md`.

---

## 4. Labels for icon-only controls

Rule (RFC-034 §5.5): **no control is operable as icon-only.** For each icon
button, ensure an i18n label exists and is either shown (wide layouts) or
attached as the control's accessible string. Inventory of current icon-only
controls to fix:

- Search submit (Search icon) — label `SearchButton` (exists).
- Add folder (FolderPlus) — label `SourcesAddFolder` (verify exists).
- Remove source (Trash2) — **add** `SourceRemoveLabel` (en/ja).
- Collapsed sidebar nav icons — already have `tooltip` per item; ensure those
  strings are the i18n source and present in both locales.

Add a `components::icon_button(tokens, glyph, label, on)` that *always* takes a
label and renders icon+label in wide layouts / icon+tooltip in narrow, so
icon-only construction is impossible by API shape.

---

## 5. Contrast usage guard (`a11y.rs`)

> **Import-path note (verified against snora 0.25):** the `snora::design`
> facade re-exports the token *types* and the iced style bridge, but **not** the
> `contrast` module. Two options:
> (a) read contrast via the underlying crate — add `snora-design` to
> `orbok-ui/Cargo.toml` dev-or-normal deps and `use snora_design::contrast`; or
> (b) file a one-line upstream ask to re-export `contrast` under
> `snora::design::contrast` (we have snora-team influence; this is the cleaner
> fix and benefits every snora app). Recommended: do (a) now, file (b). `Color`
> and `Tokens` *are* on the facade (`snora::design::{Color, Tokens}`).

```rust
//! Accessibility usage guards (RFC-034 §5.1). We don't re-derive contrast math;
//! we assert the (fg,bg) role pairs orbok actually renders meet WCAG AA across
//! all theme presets, using snora's contrast utilities.

use snora::design::{Tokens, Color};
use snora_design::contrast::contrast_ratio; // facade gap: see import-path note above

/// The foreground/background role pairs orbok renders, by purpose.
fn rendered_pairs(t: &Tokens) -> Vec<(&'static str, Color, Color, f32 /*min*/)> {
    let p = &t.palette;
    vec![
        ("body on background",        p.text_primary,   p.background,    4.5),
        ("body on surface",           p.text_primary,   p.surface,       4.5),
        ("secondary on surface",      p.text_secondary, p.surface,       4.5),
        ("accent_text on accent",     p.accent_text,    p.accent,        4.5),
        ("danger_text on danger",     p.danger_text,    p.danger,        4.5),
        ("warning_text on warning",   p.warning_text,   p.warning,       4.5),
        ("success_text on success",   p.success_text,   p.success,       4.5),
        ("info_text on info",         p.info_text,      p.info,          4.5),
        ("border on surface (UI)",    p.border,         p.surface,       3.0),
        // NOTE: text_muted is excluded because orbok never renders it.
        // (This line once read "intentionally excluded — never used for
        // essential text", citing a snora exemption withdrawn in 0.34.0 as
        // "ours, invented". Corrected 2026-08-19; see RFC-034 §5.1.)
    ]
}

pub fn audit(t: &Tokens) -> Vec<(&'static str, f32, f32)> {
    rendered_pairs(t).into_iter()
        .map(|(name, fg, bg, min)| (name, contrast_ratio(fg, bg), min))
        .collect()
}
```

Test asserts every pair meets its `min` for `Tokens::light()`, `dark()`,
`high_contrast_light()`, `high_contrast_dark()`. (snora already tests its own
palettes; this guards orbok's *usage* — e.g. catches if we ever render
`text_secondary` on `background` where contrast is thinner.)

---

## 6. Target size

Under HANDOFF-032 the icon/action padding is `[spacing.md, spacing.lg]` =
`[12, 16]`. With label text (~14px) the control clears 44px tall. Add a test (or
documented measurement) asserting primary action vertical extent ≥ 44px at
default tokens. Actionable chips meet ≥24px (AA 2.5.8); decorative badges exempt.

---

## 7. `docs/src/maintainers/accessibility.md`

Sections:
1. **Target:** WCAG 2.1 AA, desktop GUI scope.
2. **Criteria checklist:** 1.1.1, 1.4.1, 1.4.3, 1.4.11, 2.1.1, 2.1.2, 2.4.3,
   2.4.7, 2.5.8, 4.1.2 — each with orbok's status (Met / Met-with-note /
   Tracked) and the mechanism (token / primitive / keyboard map / doc).
3. **Known limitation:** iced 0.14 exposes no widget focus state → no
   token-driven focus ring on standard buttons/containers; keyboard operability
   is met; visible-focus (2.4.7) met where renderer allows; upstream tracked.
4. **Manual a11y QA (M13):** keyboard-only walkthrough script; screen-reader
   spot check; high-contrast visual pass; grayscale status-distinguishability
   pass (shared with RFC-035).

Add a pointer from `release_readiness.md` so M13 sign-off includes a11y.

---

## 8. Tests

1. `key_map_*`: each shortcut maps to the right `Message`; printable char while
   `text_input_focused=true` → `None` (no swallow).
2. `focus_restores_on_dialog_close`: open→close sets `focus_return` back to
   trigger.
3. `contrast_guard_all_presets`: `a11y::audit` pairs meet AA for all four themes.
4. `icon_controls_have_labels`: enumerate icon controls; each has non-empty
   en+ja label.
5. `status_badge_label_invariant`: (shared with 033) no tone-only badge.
6. `primary_target_min_size`: primary action ≥ 44px at default tokens.

---

## 9. Definition of done

- [ ] Keyboard map implemented; subscription wired in app; text entry unaffected.
- [ ] Dialogs trap (approximated) and restore focus to trigger.
- [ ] Every icon control has an i18n label (en+ja); `icon_button` API forbids
      icon-only construction.
- [ ] `a11y.rs` contrast guard passes for all four presets.
- [ ] Primary controls ≥44px; actionable chips ≥24px.
- [ ] `accessibility.md` written with AA checklist + focus-ring limitation +
      manual QA; referenced from release_readiness.md.
- [ ] Tests pass; build + tests warning-free and green.
- [ ] CHANGELOG entry under the current version (no version bump).
