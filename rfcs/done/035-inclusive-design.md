# RFC-035: Inclusive Design

**Project:** orbok
**RFC:** 035
**Title:** Inclusive Design
**Status:** Implemented (v0.14.0)
**Target Milestone:** M9 (Settings surface), cross-cutting
**Date:** 2026-06-07
**Depends on:** RFC-032 (themes/tokens), RFC-033 (primitives), RFC-031 (i18n)
**Complements:** RFC-034 (accessibility conformance)

---

## 1. Summary

Where RFC-034 targets a measurable conformance bar (WCAG AA), this RFC covers
the broader **inclusive design** preferences that let people adapt `orbok` to
how they read, see, and work — beyond the minimum any one standard mandates.

The decision is:

> `orbok` exposes a small set of inclusivity preferences — theme (incl. dark and
> high-contrast), text scale, reduced motion, and color-vision-safe status
> styling — all persisted, all delivered through Snora Design tokens, and all
> presented in plain language under Settings. Status meaning is conveyed
> redundantly (text + icon/shape + tone) so it survives any color vision.
> Internationalization is treated as an inclusivity concern: locale-aware
> formatting and readiness for future locales/scripts (including RTL) are part
> of the same surface.

---

## 2. Motivation

- **Different bodies, different needs.** Dark mode for light-sensitivity and
  low-light work; larger text for low vision or simply small/hi-DPI screens;
  reduced motion for vestibular sensitivity; color-vision-safe status for the
  ~8% of men and ~0.5% of women with color vision deficiency. None of these are
  edge cases; each is a sizable slice of a developer/professional audience.
- **The tokens make it cheap.** RFC-032 already routes every size and color
  through tokens. A text-scale preference is a multiplier on `typography`; a
  theme is a preset swap; reduced motion gates the (currently none, but coming)
  motion tokens. The infrastructure is paid for; this RFC spends it.
- **Inclusivity ≠ compliance.** RFC-034 makes orbok *conformant*; RFC-035 makes
  it *comfortable and adaptable*. The GUI design's "calm, utilitarian,
  low-noise, high-clarity" tone (§20.1) and "less is more" project principle
  point the same way: give people control without burying them in options.
- **i18n is inclusivity.** RFC-031 set up the catalog and named RTL as a future
  catalog-only extension. Pulling locale-aware number/date/size formatting and
  RTL-readiness into the inclusivity surface keeps these from being orphaned.

---

## 3. Goals

A focused set of persisted preferences, all under Settings → (Appearance /
Accessibility), all plain-language:

1. **Theme** (from RFC-032): System / Light / Dark / High Contrast Light /
   High Contrast Dark. This RFC owns the *Settings presentation*; RFC-032 owns
   the mechanism.
2. **Text scale:** a small multiplier (e.g. 0.9× / 1.0× / 1.15× / 1.3×, or a
   "Default / Large / Larger" plain-language picker) applied to all
   `typography` roles uniformly, persisted.
3. **Reduced motion:** a boolean that, when set, suppresses non-essential
   animation/transition (and is the default when the OS signals
   reduce-motion). Forward-compatible: gates motion tokens when snora adds
   them; today it is wired and documented with nothing to suppress yet.
4. **Color-vision-safe status:** status badges always pair tone with a distinct
   **icon/shape** and text, so success/warning/danger/info are distinguishable
   without relying on hue (validated against simulated CVD). Always on; not a
   toggle (it is simply correct), but documented here as an inclusivity
   guarantee.
5. **Locale-aware formatting:** numbers, byte sizes, dates use the active
   locale's conventions, centralized in the i18n module (RFC-031 §5.4 already
   centralizes formatting; this RFC ensures grouping/format follow locale).
6. **RTL readiness:** confirm the layout uses snora's direction-aware widgets
   (`LayoutDirection`, already passed to the tab/side bars) so a future RTL
   locale needs catalog work, not layout surgery.

---

## 4. Non-Goals

- A fully custom theme/color editor (RFC-032 non-goal restated).
- Per-widget font-family selection or font uploading (snora selects no font
  family; orbok's font choice is app-level and out of scope here).
- Shipping an RTL locale in v1 (we ensure *readiness*; the locale itself is a
  future RFC-031 extension).
- Dyslexia-specific font bundling (candidate future item; not in this RFC).
- Animations themselves — there are none to add yet; this RFC only ensures the
  *off switch* exists and defaults correctly.
- Re-implementing locale data (we use the platform/locale conventions via the
  i18n module; no ICU dependency mandated by this RFC).

---

## 5. Design

### 5.1. Settings surface (plain language, less-is-more)

Two Settings sections, worded per GUI §23 (no jargon):

```text
Appearance
  Theme:        [ System ▾ ]   (System / Light / Dark /
                                High Contrast Light / High Contrast Dark)
  Text size:    [ Default ▾ ]  (Default / Large / Larger)

Accessibility
  [x] Reduce motion           (fewer animations and transitions)
  Language:     [ English ▾ ]  (moves here / mirrors RFC-031 setting)
  Status colors are always paired with labels and icons, so they remain
  clear for all kinds of color vision.   ← explanatory text, not a toggle
```

These reuse RFC-033 primitives (pickers as token-driven controls) and RFC-031
strings. No advanced jargon; consistent with the existing Advanced toggle
gating deeper detail.

### 5.2. Text scale

A `TextScale` enum with a multiplier:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextScale { #[default] Default, Large, Larger }

impl TextScale {
    pub fn factor(self) -> f32 { match self { Self::Default => 1.0, Self::Large => 1.15, Self::Larger => 1.3 } }
}
```

Applied centrally: the `theme.rs` typography helpers (RFC-032 §5.2) multiply the
role size by `state.text_scale.factor()`. Because every view reads sizes through
those helpers, the scale propagates everywhere with no per-view change. Line
heights are multipliers already, so they scale correctly.

Bounds: the scale is clamped so layouts remain usable (no unbounded growth);
the scrollable page wrappers (already present) absorb reflow.

### 5.3. Reduced motion

A `reduced_motion: bool` in `AppState`, defaulting from an OS signal resolved in
`orbok-app` (like the theme System resolution). It is threaded to any animated
surface. Since snora 0.25 ships no motion tokens and orbok has no animations
today, the wiring is a no-op guard now and a real gate the moment motion is
introduced — which is the point: motion arrives already respecting the
preference rather than being retrofitted.

### 5.4. Color-vision-safe status (always-on guarantee)

Building on RFC-033's `status_badge` and RFC-034's non-color rule: each status
tone is bound to a **distinct lucide icon/shape** in addition to text and color,
so the three signals are redundant:

| Status         | Tone     | Icon (lucide)        | Text label   |
|----------------|----------|----------------------|--------------|
| Current/OK     | Success  | `Check` / `CircleCheck` | "Current"  |
| Stale          | Warning  | `TriangleAlert`      | "Stale"      |
| Missing        | Danger   | `CircleX` / `Unplug` | "Missing"    |
| Keyword match  | Info     | `Type` / `Hash`      | "Keyword"    |
| Semantic match | Accent   | `Sparkles` / `Brain` | "Semantic"   |
| Temporary      | Neutral  | `Clock`              | "Temporary"  |

A simulated-CVD check (deuteranopia/protanopia/tritanopia transform on the tone
colors) confirms that even with hue collapsed, the icon+text distinguishes the
statuses. This is validated once as a test fixture over the token palette.

### 5.5. Locale-aware formatting

Move all number/byte/date formatting behind the i18n module's parameterized
functions (RFC-031 already mandates this for new formatting). This RFC closes
remaining ad-hoc `format!("{gib:.3} GiB")` / `format!("Query: {last}")` style
sites in views, routing them through `i18n` so digit grouping and unit
presentation follow locale. (English `1,234` vs un-grouped, JA conventions, etc.
— resolves RFC-031's open question for the cases orbok actually renders.)

### 5.6. RTL readiness

No code change expected beyond an audit: confirm `LayoutDirection` is plumbed
(it already is, to `app_side_bar`/`app_tab_bar`) and that no view hard-codes
left/right where start/end is meant. Record the result so a future RTL locale
(RFC-031 extension) is catalog-only. Any hard-coded directional layout found is
fixed to direction-aware form.

### 5.7. Persistence

All preferences persist alongside locale/theme in `app_settings`:
`ui.text_scale`, `ui.reduced_motion` (theme/locale already covered by RFC-032 /
RFC-031). Read at startup and written on change by `orbok-app`; `orbok-ui` emits
typed `Set*` messages.

---

## 6. Rules

1. Every inclusivity preference is persisted and restored across restarts.
2. Text size is applied only through the central typography helpers; no view
   reintroduces a fixed size that bypasses the scale.
3. Status meaning is always conveyed by text **and** icon/shape **and** tone —
   three redundant channels — so it survives any color vision and grayscale.
4. New animated UI must read `reduced_motion` before animating; motion that
   cannot be reduced is not added.
5. User-facing formatting (numbers, sizes, dates) goes through the i18n module,
   not ad-hoc `format!` in views.
6. Layout uses start/end (direction-aware) semantics, never hard-coded
   left/right, so RTL stays catalog-only.

---

## 7. Acceptance Criteria

- Settings exposes Theme, Text size, Reduce motion, and Language in plain
  language; all persist across restart.
- Choosing "Large"/"Larger" visibly scales all text uniformly without breaking
  layout (content reflows within scrollable wrappers).
- Reduce motion defaults on when the OS signals it; the flag is threaded to any
  future animation gate.
- Every status badge renders text + a distinct icon + tone; a grayscale render
  still distinguishes all statuses.
- Numbers/sizes/dates in views are produced by the i18n module and follow the
  active locale.
- A direction audit confirms no hard-coded left/right; RTL is catalog-only.
- Build warning-free; suite green.

---

## 8. Testing Requirements

1. `TextScale::factor` and the scaled typography helpers produce the expected
   sizes; round-trip through `ui.text_scale`.
2. Persistence round-trips for `ui.text_scale` and `ui.reduced_motion`.
3. Reduced-motion OS default: mocked "reduce motion" environment yields
   `reduced_motion = true`.
4. **CVD distinguishability fixture:** apply deuteranopia/protanopia/tritanopia
   simulation to the status tone colors and assert each status remains
   distinguishable by its (icon, label) pair (and that no two statuses collide
   under simulated hue collapse).
5. Locale formatting: byte-size and count formatting match expected
   locale-specific output for `en` and `ja`.
6. Direction audit test/lint: grep for suspect hard-coded `Left`/`Right` layout
   in views (heuristic), and assert direction-aware widgets receive a
   `LayoutDirection`.

---

## 9. Unresolved Questions

- Should text scale be a continuous slider or the 3–4 discrete steps proposed?
  (Leaning: discrete, plain-language steps — simpler, "less is more", and
  predictable for layout testing.)
- Bundle a dyslexia-friendly font option later? (Out of scope now; candidate
  future RFC, depends on app-level font strategy.)
- Should "System" theme and "reduce motion" gain live OS-change subscriptions
  (shared with RFC-032's open question), or stay startup-resolved? (Shared
  decision; leaning startup-only for v1.)
- Do we add a one-line in-app preview ("The quick brown fox…") under Text size
  so users see the effect before applying? (Nice-to-have; defer to handoff.)

---

## 10. Decision

Expose a small, persisted, plain-language set of inclusivity preferences —
theme, text scale, reduced motion — all delivered through Snora Design tokens;
guarantee color-vision-safe status via redundant text+icon+tone; route
user-facing formatting through the i18n module for locale-awareness; and audit
the layout for RTL readiness so future locales are catalog-only. This completes
the design-system program begun in RFC-032 and complements the AA conformance of
RFC-034 with broader adaptability.
