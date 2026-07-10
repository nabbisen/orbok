# RFC-033: Component Primitive Migration

**Project:** orbok
**RFC:** 033
**Title:** Component Primitive Migration
**Status:** Implemented (v0.12.0)
**Target Milestone:** M9 (Search UI), M10 (Storage), M12 (Models)
**Date:** 2026-06-07
**Depends on:** RFC-032 (tokens must be threaded first)
**Enables:** RFC-034 (accessibility rides on accessible primitives)

---

## 1. Summary

This RFC replaces `orbok-ui`'s hand-built iced widgets with the Snora Design
component primitives (`snora::design::{button, card, chip, progress, notice}`),
and establishes snora as the **sole gateway for UI component primitives** —
the same rule already in force for lucide icons and (per RFC-032) design tokens.

The decision is:

> Where a Snora Design primitive exists for a UI element, `orbok-ui` uses it
> rather than re-deriving the element from raw `iced::widget` building blocks.
> Buttons, cards, status chips/badges, progress rows, and notices all flow
> through `snora::design`. Bespoke iced composition is reserved for elements
> snora has no primitive for, and such cases are documented.

Today only `notice` is used. `button`, `card`, `chip`, and `progress` ship in
snora 0.25 and are unused; orbok rebuilds cards as `container(..).padding(..)`,
buttons as raw `button(text(..))`, and badges as space-joined strings.

---

## 2. Motivation

- **Consistency.** Three views build a "card" three different ways. A single
  `card::surface(tokens, content)` call gives every card identical padding,
  radius, border, and surface color — and they all restyle together when the
  theme changes.
- **Correct semantics for free.** `button::danger` uses the contrast-tested
  `danger_text on danger` pair (mandatory and verified in snora's suite); the
  Reset Catalog and Delete actions currently use an undifferentiated raw button
  that gives the user no visual signal that the action is destructive. The GUI
  external design (§2.5) explicitly requires destructive actions to be visually
  distinct — the primitive delivers that; the raw button does not.
- **Accessibility baseline.** snora's chip and notice controls are documented as
  keyboard-reachable native buttons; `button::*_maybe(None)` yields a properly
  *disabled* control instead of a clickable-but-inert one. Building on these
  primitives gives RFC-034 a conformant starting point instead of a retrofit.
- **Less code, fewer bugs.** Result cards, source cards, model cards, cleanup
  action cards, and indexing job rows are all the same two or three primitives
  parameterized differently. Centralizing them shrinks `views.rs` (currently
  460 ELOC, the second-largest file) and removes the per-card numeric drift.
- **Badges that mean something.** Status is currently `text(shown.join("  "))`
  — color-free, structureless strings. `chip`/tone-driven badges carry semantic
  `Tone` (Stale→Warning, Missing→Danger, Keyword/Semantic→Info/Accent) while
  keeping the text label, satisfying the "never color alone" rule (§17.4).

---

## 3. Goals

- A thin orbok component layer (`orbok-ui/src/components.rs`) mapping each
  orbok view-model element to a snora primitive:
  - `result_card`, `source_card`, `model_card`, `cleanup_action_card` →
    `card::surface` / `card::selected`.
  - `status_badge(tone, label)` → tone-styled `chip`-like pill (text + tone).
  - primary/secondary/ghost/danger actions → `button::{primary, secondary,
    ghost, danger}` (+ `_maybe` for disabled).
  - indexing job progress → `progress::row`.
  - notices → existing `notice` primitive (already done; folded in here for
    completeness).
- A documented **primitive inventory**: for every interactive/structural element
  in the GUI external design, either the snora primitive used, or an explicit
  "no primitive; bespoke" note with rationale.
- Destructive actions (Reset Catalog, Delete Vector/Keyword Index, Remove
  Source) rendered with `button::danger`.
- Disabled states (e.g. search submit while running, actions requiring a model)
  rendered with `*_maybe(None)`.

---

## 4. Non-Goals

- Introducing tokens — RFC-032 owns that and is a prerequisite.
- New accessibility behaviors beyond what primitives provide by default
  (keyboard map, focus order, SR labels) — RFC-034 owns those.
- Building primitives snora lacks (e.g. a two-pane split, a modal/dialog
  container, a data table). Those stay bespoke; this RFC only *documents* them
  as bespoke and keeps them token-driven.
- Changing information architecture or workflows. This is a like-for-like
  visual/behavioral substitution, not a redesign.
- Forking or vendoring snora widgets.

---

## 5. Design

### 5.1. The component layer

A new `orbok-ui/src/components.rs` holds orbok's domain-to-primitive adapters.
View functions call these; they do not call `snora::design::*` directly (one
indirection so a future primitive swap touches one file). Example:

```rust
// orbok-ui/src/components.rs
use snora::design::{Tokens, card, button, variants::Tone};

/// A search result card. Selected state uses the accent-bordered card.
pub fn result_card<'a>(
    tokens: &Tokens,
    vm: &ResultCardVm,
    selected: bool,
    on_select: Message,
) -> Element<'a, Message> {
    let body = /* title/path/heading/snippet/badges column, token-sized */;
    let inner = if selected { card::selected(tokens, body) }
                else        { card::surface(tokens, body) };
    button(inner).on_press(on_select).style(/* ghost wrapper */).into()
}

/// A status badge: text label + semantic tone, never color alone.
pub fn status_badge<'a>(tokens: &Tokens, label: &str, tone: Tone) -> Element<'a, Message> {
    // tone-tinted pill via the chip/notice style bridge; label always present.
}
```

### 5.2. Primitive inventory (GUI external design → snora 0.25)

| orbok element (GUI design ref)          | snora 0.25 primitive            | Notes |
|-----------------------------------------|---------------------------------|-------|
| SearchResultCard (§7.3)                  | `card::surface` / `card::selected` | wrapped in ghost button for selection |
| SourceCard (§8.1)                        | `card::surface`                 | danger action in DangerZone |
| ModelCard (§11.1)                        | `card::surface`                 | status via `status_badge` |
| CleanupActionCard (§10.1)               | `card::surface` + `button::*`   | destructive → `button::danger` |
| StatusBadge (§5.3, §17.4)               | tone-styled pill (chip bridge)  | text + Tone; "never color alone" |
| Match badges Keyword/Semantic/Reranked  | `status_badge` (Info/Accent)    | advanced-only per RFC-013 stays |
| Stale / Missing badges                   | `status_badge` (Warning/Danger) | always shown (trust signal) |
| IndexJobRow progress (§9.1)             | `progress::row`                 | indeterminate → `None` |
| Primary actions (Search, Add, Install)   | `button::primary`               | one per surface |
| Secondary actions (Rescan, Locate)       | `button::secondary`             | |
| Tertiary actions (Details, Copy Path)    | `button::ghost`                 | |
| Destructive (Reset, Delete, Remove)      | `button::danger`                | + confirmation (existing) |
| Disabled actions (submit-while-running)  | `*_maybe(None)`                 | true disabled state |
| Notices / banners (§15.3, errors)        | `notice::Notice` (done)         | tone-driven |
| Sidebar / tab bar (§4)                   | `snora::widget::app_side_bar` / `app_tab_bar` (done) | already snora |
| Two-pane Search layout (§7.1)            | **bespoke** (no snora split)    | token-driven `row!`; documented |
| Confirmation dialogs (§10.3)             | **bespoke** (no snora modal)    | token-driven; candidate upstream ask |
| Wizard stepper (§6.1)                    | **bespoke** (no snora stepper)  | token-driven column |

### 5.3. Bespoke elements

Where no snora primitive exists, the element stays hand-built but **must** be
token-driven (RFC-032) and is listed in the inventory's bespoke rows. Each
bespoke row is a candidate upstream request to snora (we have snora-team
influence; a modal/dialog primitive and a split-pane primitive are the two
strongest candidates and should be filed as snora issues referencing this RFC).

### 5.4. Migration order (per view, lowest risk first)

1. Storage view — most destructive actions; biggest safety win from
   `button::danger`. (M10)
2. Search view — result cards → `card::*`, badges → `status_badge`. (M9)
3. Sources / Models views — cards + danger actions. (M2/M12 surfaces)
4. Indexing view — `progress::row`. (M9)
5. Wizard — token-driven bespoke; buttons → `button::*`.

Each step is independently shippable and independently tested.

### 5.5. Gateway rule

`orbok-ui` view modules import UI primitives only from `snora::design` /
`snora::widget` (via the `components.rs` adapter). Direct construction of a
styled `iced::widget::button`/`container` in a view module — where a snora
primitive exists for that role — is forbidden by the same CI heuristic family
as RFC-032. This mirrors the established "snora is the sole lucide gateway"
principle.

---

## 6. Rules

1. If a Snora Design primitive exists for a role (button, card, chip/badge,
   progress, notice), `orbok-ui` uses it via `components.rs`. Re-deriving that
   role from raw iced widgets in a view module is not mergeable.
2. Destructive actions use `button::danger`. No destructive action uses a
   neutral/primary button.
3. Disabled actions use `*_maybe(None)`, never an enabled button with a no-op
   handler.
4. Status badges always pair a text label with a `Tone`; tone is never the only
   signal.
5. Bespoke elements (no snora primitive) are listed in the §5.2 inventory, stay
   token-driven, and each carries a one-line rationale.

---

## 7. Acceptance Criteria

- Every element in the §5.2 inventory is rendered by its listed snora primitive
  (or documented bespoke).
- Reset Catalog, Delete Vector Index, Delete Keyword Index, and Remove Source
  render with `button::danger`.
- Search submit (while running) and model-dependent actions render visibly
  disabled via `*_maybe(None)`.
- No view module constructs a styled button/card/badge from raw iced widgets for
  a role that has a snora primitive (CI heuristic gate).
- `views.rs` shrinks (cards/badges/buttons moved to `components.rs`); both files
  stay under the 500-ELOC strong-split threshold.
- Build warning-free; full suite green.

---

## 8. Testing Requirements

1. `components.rs` smoke tests: each adapter (`result_card`, `source_card`,
   `model_card`, `cleanup_action_card`, `status_badge`, action buttons,
   `progress::row` wrapper) builds an `Element` for both a normal and an edge
   case (empty fields, indeterminate progress, disabled).
2. Tone mapping test: Stale→Warning, Missing→Danger, Keyword→Info,
   Semantic→Accent, Reranked→Accent (table-driven).
3. Disabled-state test: the disabled action builders produce a button with no
   active press handler (via `iced_test` where feasible, else a builder-level
   assertion).
4. Inventory guard: a doc/test listing the inventory rows, so adding a new view
   element without classifying it is caught in review.
5. Existing `smoke_views` tests continue to pass against migrated views.

---

## 9. Unresolved Questions

- Should `status_badge` become a true upstream `snora::design::badge` primitive
  rather than a chip-bridge composition in orbok? (We have influence; filing it
  upstream would benefit other snora apps. Leaning: build in orbok now, propose
  upstream with this RFC as the use case.)
- Modal/dialog and split-pane: build minimal bespoke now and file snora feature
  requests, or block on upstream? (Leaning: bespoke now, file requests; do not
  block orbok on upstream.)
- Result-card selection: snora's `card::selected` is non-interactive in 0.25
  (caller controls visual state). We wrap it in a ghost button. Revisit if snora
  adds interactive selection (snora RFC-027 referenced in its card docs).

---

## 10. Decision

Migrate `orbok-ui` to Snora Design component primitives behind a thin
`components.rs` adapter, establish snora as the sole gateway for UI primitives,
render destructive actions with `button::danger` and disabled actions with
`*_maybe`, and document every bespoke element with a rationale and an upstream
candidacy note. Prerequisite: RFC-032. Prerequisite for: RFC-034.
