# RFC-032: Design Token Foundation and Theming

**Project:** orbok
**RFC:** 032
**Title:** Design Token Foundation and Theming
**Status:** Implemented (v0.12.0)
**Target Milestone:** M9 (Search UI), retroactive across all views
**Date:** 2026-06-07
**Depends on:** RFC-027 (GUI framework), RFC-031 (i18n)
**Enables:** RFC-033 (components), RFC-034 (accessibility), RFC-035 (inclusive design)

---

## 1. Summary

This RFC makes the Snora Design token system (`snora::design`, snora 0.25)
the single source of truth for every visual property in the `orbok` GUI, and
adds user-selectable theming on top of it.

The decision is:

> No view in `orbok-ui` hard-codes a font size, a padding, a gap, a corner
> radius, or a color. Every such value is read from a `snora::design::Tokens`
> bundle that is threaded through the view layer from `AppState`. The active
> token bundle is chosen by a persisted theme setting (`Light`, `Dark`,
> `High Contrast Light`, `High Contrast Dark`, or `System`).

Today the token bundle exists in `AppState` but only drives the notice
primitive; everything else uses literal magic numbers (`text(..).size(15)`,
`.padding(10)`, ad-hoc colors). This RFC closes that gap. It is the
foundation the three following design-system RFCs build on.

---

## 2. Motivation

The migration to snora 0.25 introduced Snora Design but, by the project's own
note, "we have not used it fully yet." The current view code shows the cost of
that:

- **Inconsistency.** Body text is variously `size(15)`, `size(14)`,
  `size(13)`, `size(12)` for what is semantically the same role. Padding is
  `10`, `8`, `[12,16]`, `[12,18]` with no system. The same "card" concept is
  rebuilt by hand in `search_view`, `sources_view`, and `indexing_view` with
  different numbers.
- **No themes.** There is a `high_contrast: bool` that flips between
  `Tokens::light()` and `Tokens::high_contrast_light()`, but it is not
  persisted, there is no dark theme, and the four built-in presets snora ships
  (`light`, `dark`, `high_contrast_light`, `high_contrast_dark`) are not all
  reachable. Dark mode is one of the most-requested accessibility and comfort
  features for a developer-facing tool and is one preset constructor away.
- **Accessibility is unverifiable.** Contrast cannot be reasoned about when
  colors are scattered literals. Snora Design's palettes are contrast-tested at
  the token level; an app that bypasses them forfeits that guarantee. RFC-034
  cannot land on hard-coded colors.
- **Future-proofing.** `Tokens` is `#[non_exhaustive]`; new token groups (e.g.
  elevation, motion) arrive without breaking changes. Code that reads tokens
  inherits those improvements; code with magic numbers does not.

This is the same lesson as RFC-031 (i18n): centralize a cross-cutting concern
behind one typed mechanism so the compiler and the test suite can enforce it,
rather than letting literals proliferate across every view.

---

## 3. Goals

- A single `Tokens` bundle in `AppState`, passed to **every** view function.
- All font sizes drawn from `tokens.typography` text roles (`body`,
  `body_small`, `label`, `title`, `heading`, `display`).
- All spacing/padding/gap values drawn from `tokens.spacing`
  (`xs`/`sm`/`md`/`lg`/`xl`/`xxl`).
- All colors drawn from `tokens.palette` semantic roles via the
  `snora::design` style bridge (no literal `iced::Color`).
- All corner radii drawn from `tokens.radius`.
- A persisted `Theme` setting with five values: `Light`, `Dark`,
  `HighContrastLight`, `HighContrastDark`, `System`.
- A clean migration path: a small set of token-reading helper functions in
  `orbok-ui` so that view call sites stay readable.
- Zero behavioral regressions: the test suite and zero-warning build stay green.

---

## 4. Non-Goals

- Defining new design tokens or palettes of our own. We consume snora's
  vocabulary; we do not fork it. (If orbok needs a token snora lacks, that is an
  upstream change to snora — see §9.)
- A custom/user-authored theme editor. Users pick from the built-in presets;
  arbitrary color customization is out of scope.
- Replacing raw iced widgets with snora widget primitives — that is RFC-033.
  This RFC is about the *values* (tokens); RFC-033 is about the *widgets* that
  consume them.
- Live OS theme-change subscription. `System` resolves once at startup (and on
  manual re-resolve); snora explicitly does not do OS theme detection, so a
  follow-up may add a platform watcher. v1 reads the OS preference once.
- Animation/motion tokens (RFC-035 covers reduced-motion as a preference;
  snora 0.25 ships no motion tokens to consume yet).

---

## 5. Design

### 5.1. Token threading

`AppState` already owns `tokens: snora::design::Tokens`. This RFC promotes that
field from "notice-only" to "the styling context for the whole view tree."

Every public view function gains access to the active tokens. Two equivalent
shapes are acceptable; we choose (a) for minimal churn:

```text
(a) view fns take &AppState (already do); read state.tokens internally.
(b) view fns take an explicit &Tokens param.
```

We keep (a). Helper functions that build sub-elements (cards, badges, buttons)
take `&Tokens` explicitly so they are unit-testable without a full `AppState`.

### 5.2. Typography: replace every `.size(N)`

A small mapping table is defined once (in a new `orbok-ui/src/theme.rs`) and
every call site uses it:

| Old literal(s)        | Semantic role          | Token source                         |
|-----------------------|------------------------|--------------------------------------|
| `26`                  | page heading           | `tokens.typography.heading.size`     |
| `22`, `20`            | section title / metric | `tokens.typography.title.size`       |
| `18`                  | card/empty-state title | `tokens.typography.title.size`       |
| `15`, `14`, `13`      | body                   | `tokens.typography.body.size`        |
| `12`, `11`            | secondary metadata     | `tokens.typography.body_small.size`  |
| button labels         | label                  | `tokens.typography.label.size`       |

The snora style bridge already exposes `style::text::{body_size, body_small_size,
label_size, title_size, heading_size, display_size}` returning `iced::Pixels`;
orbok wraps these in `theme.rs` helpers so views never touch raw numbers:

```rust
// orbok-ui/src/theme.rs
use snora::design::Tokens;
use iced::Pixels;

pub fn body(tokens: &Tokens) -> Pixels { snora::design::style::text::body_size(tokens) }
pub fn meta(tokens: &Tokens) -> Pixels { snora::design::style::text::body_small_size(tokens) }
pub fn label(tokens: &Tokens) -> Pixels { snora::design::style::text::label_size(tokens) }
pub fn title(tokens: &Tokens) -> Pixels { snora::design::style::text::title_size(tokens) }
pub fn heading(tokens: &Tokens) -> Pixels { snora::design::style::text::heading_size(tokens) }
```

### 5.3. Spacing: replace every `.padding(N)` / `.spacing(N)`

| Old literal     | Semantic step | Token              |
|-----------------|---------------|--------------------|
| `2`             | xs            | `tokens.spacing.xs`|
| `4`, `6`, `8`   | sm            | `tokens.spacing.sm`|
| `10`, `12`      | md            | `tokens.spacing.md`|
| `16`            | lg            | `tokens.spacing.lg`|
| `[28, 40]` page | xl/xxl        | `tokens.spacing.xl`/`xxl` |

Page-region padding (`page()` helper, currently `Padding::from([28.0, 40.0])`)
becomes `[tokens.spacing.xl, tokens.spacing.xxl]`.

### 5.4. Color: no literal `iced::Color`

Colors come from `tokens.palette` roles, converted at the iced boundary with
`snora::design::style::color::to_iced_color`. The relevant roles:
`background`, `surface`, `surface_raised`, `text_primary`, `text_secondary`,
`text_muted`, `border`, `accent`/`accent_text`, the four status pairs
(`success`/`warning`/`danger`/`info` each with `*_text`), and `focus`.

A grep gate (CI heuristic, §6) forbids `iced::Color::` and `Color::from_rgb`
in `orbok-ui` view modules; the only sanctioned color path is the token bridge.

### 5.5. Theme model and selection

A new typed enum in `orbok-ui`:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,            // resolve from OS once at startup
    Light,
    Dark,
    HighContrastLight,
    HighContrastDark,
}

impl Theme {
    /// The concrete token bundle for this theme. `System` must be
    /// resolved to a concrete variant first via `resolve_system`.
    pub fn tokens(self) -> snora::design::Tokens { /* match → Tokens::light() etc. */ }
}
```

`AppState` replaces `high_contrast: bool` with `theme: Theme` plus the derived
`tokens`. On `Message::SetTheme(theme)`, the state recomputes `tokens` and
emits a persistence message. The existing `ToggleHighContrast` message is
removed in favor of explicit selection; the Settings view (RFC-035 refines it)
gains a theme picker.

`System` resolution: read the OS color-scheme preference once at startup in
`orbok-app` (platform best-effort: `dark`/`light`; default `Light` if unknown),
producing a concrete `Theme` for token construction while the stored setting
stays `System`. snora performs no OS detection, so this small resolver lives in
`orbok-app` (it is platform I/O, which `orbok-ui` must not do per RFC-027).

### 5.6. Persistence

The theme is persisted like the locale (RFC-031): in `app_settings` under
`ui.theme = "system" | "light" | "dark" | "high_contrast_light" |
"high_contrast_dark"`, read at startup by `orbok-app`, written on change. The
boundary stays clean: `orbok-ui` produces a `PersistTheme(Theme)` message;
`orbok-app` performs the catalog write.

### 5.7. Boundary note

Like the lucide gateway rule, this RFC establishes a **token gateway rule**:
`orbok-ui` reads design values only from `snora::design` (tokens + style
bridge). It never invents sizes/colors and never depends on raw palette
constants from elsewhere. snora remains the sole gateway to the design
vocabulary, consistent with how it is already the sole gateway to lucide icons.

---

## 6. Rules

1. No `orbok-ui` view or component module contains a literal font size,
   padding, spacing, radius, or color. CI greps view modules for `.size(<int>)`,
   `.padding(<int>)`, `iced::Color`, and `from_rgb` as a heuristic gate
   (mirrors the RFC-031 string-literal gate).
2. Design values are read from `state.tokens` (views) or a `&Tokens` parameter
   (helpers). No global/static token singleton.
3. Adding a theme means adding a `Theme` variant and its `tokens()` arm — a
   compile-checked exhaustive `match`, exactly like locales in RFC-031.
4. New UI features land already token-driven; a feature that introduces a magic
   number is not mergeable.
5. The platform OS-preference resolver lives in `orbok-app`, never in
   `orbok-ui` (RFC-027 boundary).

---

## 7. Acceptance Criteria

- Every `orbok-ui` view renders using values read from `state.tokens`; no magic
  numbers remain in view/component modules (verified by the CI grep gate).
- A user can choose Light, Dark, High Contrast Light, High Contrast Dark, or
  System from Settings; the choice persists across restarts.
- Switching theme at runtime restyles the whole app without restart.
- `System` resolves to a dark bundle on a dark-preference OS and a light bundle
  otherwise, with `Light` as the safe fallback.
- The four-preset coverage is reachable: each `Theme` variant maps to its snora
  preset constructor.
- Build stays warning-free; the full test suite stays green.

---

## 8. Testing Requirements

1. `Theme::tokens()` returns the expected snora preset for each variant
   (exhaustive over `Theme`, excluding `System` which is pre-resolved).
2. `SetTheme` updates `AppState.tokens` to the matching preset; round-trips
   through the `ui.theme` setting string.
3. OS-preference resolver maps a mocked "dark" environment to a dark theme and
   unknown to `Light`.
4. A token-coverage test asserts the `theme.rs` helpers return the same
   `Pixels`/role values as the underlying snora bridge (guards against drift if
   the helper layer is edited).
5. Grep-gate test: a unit/integration check (or `xtask`) scanning view modules
   for forbidden literals fails on a planted violation.

---

## 9. Unresolved Questions

- Should `System` gain a live OS theme-change subscription (re-resolve when the
  user flips OS dark mode while orbok is open), or is startup-time resolution
  enough for v1? (Leaning: startup-only now; subscription as a follow-up once a
  cross-platform watcher is chosen.)
- If orbok ever needs a token snora lacks (e.g. a "selection" background before
  snora adds `Palette::selection`), do we petition upstream (we have snora-team
  influence) or carry a temporary local extension? (Leaning: upstream first;
  snora already documents `selection`/`overlay`/`separator` as planned roles.)
- Should density (`Comfortable`/`Compact`) be user-selectable now? snora 0.25
  resolves only `Comfortable`; defer to when `Compact` is resolvable.

---

## 10. Decision

Adopt the Snora Design token system as the single source of truth for all
visual values in `orbok-ui`, threaded from `AppState.tokens`, with a persisted
five-value `Theme` setting (`System` resolved at startup in `orbok-app`). This
is the foundation for RFC-033 (component primitives), RFC-034 (accessibility
conformance), and RFC-035 (inclusive design).
