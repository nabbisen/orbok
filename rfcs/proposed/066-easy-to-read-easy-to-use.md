# RFC-066: Easy to Read, Easy to Use

**Project:** orbok\
**RFC:** 066\
**Title:** Easy to Read, Easy to Use\
**Status:** Proposed\
**Target milestone:** accessibility for older users (after the AUR publication and Task 139)\
**Date:** 2026-11-04\
**Related RFCs:** RFC-032 (tokens and themes); RFC-034 (accessibility conformance); RFC-035 (inclusive design, text size); RFC-041 (plain language)\
**Related tasks:** Task 106 (rows that hold controls wrap); Task 118 (one standard per kind of control); Task 127 and Task 136 (text size reaches every word); Task 138 (the theme follows the system)

---

## 1. Summary

Make orbok comfortable for older users, and so for everyone, by one
standard rather than piecemeal fixes:

1. **Every place in the sidebar is named in words,** not only by an icon
   and a hover tooltip.
2. **Text can grow to twice its size,** and the first screen offers the
   choice.
3. **Every piece of text orbok shows reaches 7:1 contrast** (WCAG AAA) in
   the Light and Dark themes, placeholders included. An instruction is
   never only a placeholder.
4. **Every control is at least 44 × 44 px to hit,** and a link looks like a
   link.
5. **Headings are heavier than body text,** so a page can be scanned.

## 2. Motivation: what was measured (2026-11-04)

From snora 0.53's tokens and orbok's own screenshots:

| Area | Today | Why it matters for older users |
|---|---|---|
| Sidebar | Icons only (about 14 px glyphs); names appear on hover | Icons must be guessed, and the AI icon can't be. Hovering is hard with an unsteady hand. The tooltip also covers the tab label beside it |
| Text sizes (Default) | body 16 px; button labels, paths, snippets and hints 14 px | 14 px is small for ageing eyes |
| Text size setting | Default ×1.0, Large ×1.15, Larger ×1.3 (body at most about 21 px) | WCAG 1.4.4 expects text to work at 200 %. The setting is found only in Settings |
| Placeholder text, Light theme | **2.92 : 1** (measured from a screenshot: `#979798` on white) | Below even AA (4.5 : 1). Placeholders carry instructions ("Or type a path manually…") |
| Blue (accent) text on the background (outlined buttons, choices, links) | Light 6.70 : 1, Dark 6.78 : 1 | AA, not AAA. **Not in `a11y.rs`'s audited pairs at all** |
| Body and secondary text | 8.1–17.8 : 1 | Already AAA |
| Controls | Buttons and choices about 30–37 px tall; the folder chip's × about 24 px; links are bare text | Small targets are missed; unmarked links aren't recognized as links |
| Headings | Differ from body only by size | Weight helps scanning |

**Already right, and kept:**
- little is shown by default (Advanced view off, recent searches behind one
  button, one filled main action per page);
- no notice disappears on a timer;
- status is colour, a word and an icon;
- the high-contrast themes (10–21 : 1);
- reduced motion;
- the theme follows the system.

## 3. Design

### 3.1 The sidebar says where each place is

- Each sidebar item shows its **icon with its name beneath it**: Search,
  AI, Settings / 検索, AI, 設定 (the existing `NavSearch`, `NavAi`,
  `NavSettings` copy). The hover tooltip then has nothing to add and goes
  away, along with its overlap with the tab bar.
- **The names follow the text size.** The rail widens to fit the longest
  name at the current size in the current language. It never truncates.
- **This is snora's widget.** orbok asks snora for a labelled variant of
  its sidebar (§5). orbok does not draw its own: two sidebars would be two
  standards.

### 3.2 Text can grow to twice its size, and the first screen offers it

- **Four steps:**

  | Setting | Factor | Body text |
  |---|---|---|
  | Default / 標準 | ×1.0 | 16 px |
  | Large / 大 | ×1.25 | 20 px |
  | Larger / 特大 | ×1.5 | 24 px |
  | Largest / 最大 | ×2.0 | 32 px |

  "Largest" / 「最大」 is **new copy, for the owner's approval.** The saved
  values `large` and `larger` keep their names. Their factors grow (1.15 →
  1.25, 1.3 → 1.5), in the direction the user chose. The CHANGELOG says
  so.
- **Every page works at ×2.0:**
  - rows that hold controls wrap (Task 106);
  - nothing clips or overlaps at the narrowest window orbok allows, in
    both languages;
  - pages scroll rather than squeeze.
- **The first screen** (the setup wizard's first page) shows **Language
  and Text size** at its top, with the same choice controls and copy as
  Settings. Someone who needs larger text gets it before reading anything
  else. The theme already follows the system (Task 138), so it isn't
  asked.

### 3.3 Every piece of text reaches 7 : 1

- **The rule:** every foreground/background pair orbok renders as text
  reaches **7 : 1 in Light and Dark** (WCAG 1.4.6, AAA). It already does
  in both high-contrast themes. Non-text boundaries keep 3 : 1.
- **The audit covers what is rendered** (`a11y.rs`):
  - its threshold for text pairs rises from 4.5 to 7;
  - **two missing pairs are added:** accent on background (outlined
    labels and links), and the text-input placeholder on the input's
    background.
- **The placeholder:**
  - every `text_input` gets an explicit style whose placeholder uses the
    secondary text colour (8.1–9.2 : 1). The design-token gate requires a
    style on every `text_input`, as it does for buttons;
  - **an instruction is never only a placeholder.** Where a placeholder
    says what to do ("Or type a path manually…"), the same words move to
    a visible label above the field, and the placeholder goes. The search
    box keeps its placeholder, because the Search button under it already
    says what the field is for.
- **The accent** (6.70 and 6.78 : 1) is snora's palette. orbok asks snora
  for an accent that reaches 7 : 1 against the background and surfaces in
  Light and Dark (§5). Until then, the audit lists those two pairs as
  **known AA-only, with the snora request as the reason**: visible and
  named, not hidden.

### 3.4 Every control is at least 44 × 44 px, and a link looks like one

- **44 × 44 px minimum hit area** (WCAG 2.5.5, AAA) at Default, for every
  button, choice, switch, chip, the chip's ×, tab and sidebar item. Larger
  text sizes only grow it.
  - orbok's own controls (`icon_primary`, `icon_secondary`, the result
    card, the folder card's actions) set it directly;
  - snora's controls (`button::*`, `chip`, the tab bar) need a minimum
    target-size token they honour (§5).
- **A link is underlined** ("Choose another folder", "Choose a folder"):
  snora's ghost/link style gains an underline (§5).
- **Measured, not eyeballed:** a test renders each kind of control through
  `iced_test`'s simulator at Default and checks its bounds are at least
  44 × 44.

### 3.5 Headings are heavier

- Page headings and card titles use **semibold**. Body text stays
  regular. One weight step: no italics, no shadows, no other emphasis.
- **Both languages:** confirm the system's Japanese font has a semibold
  (or bold) face. If it doesn't, headings fall back to regular, and the
  manual pass records it.

## 4. What does not change

- **What is shown by default.** "Less is more" stands. Nothing in this RFC
  adds a control to a page, except the first screen's two choices, which
  replace a trip to Settings.
- **The high-contrast themes,** already 10–21 : 1.
- **Copy,** except "Largest" / 「最大」 and the placeholder instruction that
  becomes a label (same words).

## 5. What orbok asks of snora (one letter)

1. **A labelled sidebar:** icon with its name beneath. The name follows
   the typography scale, and the rail widens to fit it.
2. **A minimum target size,** honoured by `design::button`, `chip`
   (including its ×), the tab bar and the sidebar. For example,
   `tokens.target_min = 44.0`.
3. **An underline** for the ghost/link button style, or a separate `link`
   variant.
4. **An accent reaching 7 : 1** against background and surfaces in the
   Light and Dark presets, or guidance on an AAA preset.

orbok implements its own part (§3.2, §3.3 apart from the accent, §3.4 for
its own controls, §3.5) without waiting. Each snora item lands when snora
releases it.

## 6. Acceptance criteria

1. The sidebar shows every name in words at every text size and in both
   languages, and has no hover-only names.
2. Text size has four steps up to ×2.0. Every page at ×2.0, at the
   narrowest window and in both languages, has no clipping or overlap
   (`iced_test` checks plus a manual pass under the GUI rule file).
3. The first screen offers Language and Text size.
4. `a11y.rs` audits every rendered text pair at 7 : 1, the accent and the
   placeholder included. The only exceptions are listed with their snora
   reason, and removed when snora delivers.
5. No instruction lives only in a placeholder.
6. Every control kind measures at least 44 × 44 px at Default.
7. Links are underlined.
8. Headings are semibold in both languages, or the fallback is recorded.
