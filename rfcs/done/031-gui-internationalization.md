# RFC-031: GUI Internationalization (i18n)

**Project:** orbok
**RFC:** 031
**Title:** GUI Internationalization
**Status:** Implemented (v0.1.0)
**Target Milestone:** M0 (catalog skeleton), M9 (full Search UI coverage)
**Date:** 2026-06-06

---

## 1. Summary

This RFC defines how the `orbok` GUI supports multiple languages.

The decision is:

> All user-facing GUI strings are resolved through a compile-time-checked
> message catalog in `orbok-ui`. English (`en`) is the source locale and
> Japanese (`ja`) is the first additional locale. Views never contain
> hard-coded user-facing string literals.

This RFC implements the project-instruction requirement "The GUI must
support multiple languages (i18n)", which no earlier RFC covered.

---

## 2. Motivation

- The target users (requirements §6.1) explicitly include "professionals
  with mixed Japanese and English local document collections". A
  Japanese-capable search pipeline (RFC-014) with an English-only UI
  would be incoherent.
- Copywriting rules (GUI/UX design §23) define careful plain-language
  wording; that wording must be translatable without hunting string
  literals across view code.
- Retrofitting i18n after M9 would touch every view; establishing the
  catalog at M0 costs little.

---

## 3. Goals

- Single mechanism for all user-facing strings.
- Compile-time detection of missing translations.
- English fallback when a key has no translation in the active locale.
- Locale selection in Settings, persisted in `app_settings`
  (`ui.locale = "en" | "ja" | "auto"`).
- `auto` resolves from the OS locale at startup, falling back to `en`.
- Parameterized messages (counts, sizes, paths) without format-string
  injection risks.
- No network access for translations; catalogs are compiled in.

---

## 4. Non-Goals

- Translating documentation (`docs/`) in v1.
- Translating log messages or `app_events` (operational data stays
  English for diagnosability; RFC-018).
- Locale-aware *search* behavior (that is RFC-014's domain).
- Plural-rules completeness beyond what `en`/`ja` need (Japanese has no
  plural inflection; English needs singular/plural only).
- RTL locales in v1 (`snora` supports RTL layout, so this is a
  catalog-only extension later).

---

## 5. Design

### 5.1. Mechanism

A typed, in-crate catalog rather than a runtime translation framework:

```rust
// orbok-ui/src/i18n/
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Locale {
    #[default]
    En,
    Ja,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageKey {
    NavSearch,
    NavSources,
    NavIndexing,
    NavStorage,
    NavModels,
    NavSettings,
    BadgeLocalOnly,
    // ... one variant per user-facing string
}

pub fn tr(locale: Locale, key: MessageKey) -> &'static str { /* match */ }
```

Rationale for typed keys over string keys or FTL files:

1. A `match` over `MessageKey` is exhaustive: adding a key without
   adding every locale's text is a **compile error**, which is the
   strongest possible "missing translation" check.
2. Zero runtime parsing, zero extra dependencies, trivially testable.
3. Matches the project principle "avoid vague definitions resulting from
   over-pursuing abstraction".

Parameterized messages are functions, not format strings:

```rust
pub fn tr_indexed_files(locale: Locale, n: u64) -> String;
pub fn tr_space_recovered(locale: Locale, bytes: u64) -> String;
```

This avoids positional-placeholder mismatch between locales and keeps
number/byte formatting locale-aware in one place.

### 5.2. Migration path

If the catalog outgrows enum-per-string (e.g. community translations,
many locales), migrate to Fluent (`fluent-rs`) behind the same `tr`
call sites. The enum design intentionally maps 1-to-1 onto Fluent
message IDs. This migration would be a follow-up RFC.

### 5.3. Locale resolution

```text
app_settings ui.locale
├── "en" / "ja"  → use directly
└── "auto"       → read OS locale (e.g. LANG / Windows user locale)
                   → "ja*" → Ja, otherwise → En
```

Resolution happens once at startup and on settings change; the resolved
`Locale` lives in the UI state and is passed to view functions.

### 5.4. Copywriting source of truth

English strings follow GUI/UX design §23 ("Preferred Terms" /
"Plain-Language Examples"). Japanese strings follow the same
plain-language policy; technical loanwords (インデックス, キャッシュ,
モデル) are acceptable where they are common usage, but the §23
avoid-list (BM25, RRF, cross-encoder, …) applies equally to Japanese.

---

## 6. Rules

1. `orbok-ui` view code must not contain user-facing string literals;
   CI may grep view modules for quoted literals as a heuristic gate.
2. Backend crates (`orbok-core`, `orbok-db`, …) return typed errors and
   enums, never display strings; the UI maps them to `MessageKey`s.
   This keeps the backend locale-free and the boundary clean.
3. Date/time/size formatting is centralized in the i18n module.
4. New UI features must land with both `en` and `ja` strings.

---

## 7. Acceptance Criteria

- Locale can be switched at runtime from Settings without restart.
- Every navigation label, badge, dialog, and empty state renders in both
  locales.
- A missing translation is a compile error.
- Backend crates contain no user-facing display strings.
- `auto` locale resolves Japanese OS environments to `ja`.

---

## 8. Testing Requirements

1. `tr` returns non-empty text for every `(Locale, MessageKey)` pair
   (exhaustive iteration test).
2. Parameterized messages format counts/sizes correctly per locale.
3. Locale persistence round-trips through `app_settings`.
4. OS-locale auto-detection unit tests with mocked environment.

---

## 9. Unresolved Questions

- Should number formatting use locale digit grouping (1,234 vs 1234)?
- Should the docs site gain a Japanese translation track later?
- When (if ever) to migrate to Fluent?

---

## 10. Decision

Adopt a compile-time-checked typed message catalog in `orbok-ui` with
`en` (source) and `ja` locales, locale setting persisted in
`app_settings`, and a documented migration path to Fluent.

---

## 11. Amendment (2026-08-08): §7's `auto` criterion was unmet from implementation until `3d2b47d`

§7's acceptance criterion — *"`auto` locale resolves Japanese OS environments to
`ja`"* — was recorded as met when this RFC moved to `done/`. It was not, and it
stayed unmet in every release that shipped afterwards. A user installing orbok
on a Japanese-locale OS got an English interface.

**What was actually built.** Everything §5 describes exists and works:
`Locale::from_env` reads `LANG` then `LANGUAGE`, `Locale::parse` deliberately
does not recognise `"auto"` so that value falls through the priority chain, and
the chain itself resolves settings → catalog → environment → default. No part of
that machinery was broken.

**Why the criterion was unmet anyway.** `OrbokSettings::default().locale` was
`"en"`, not `"auto"` (§5's `ui.locale = "en" | "ja" | "auto"` names three values;
the default was one of the concrete two). `Locale::parse("en")` returns `Some`,
and `Option::or_else` evaluates its closure only on `None` — so on every fresh
profile the chain stopped at its first element and `Locale::from_env` was never
called. The environment branch was reachable in production only from a
`settings.json` holding an unrecognised locale string.

**Why no test caught it.** §8's requirement 4 — *"OS-locale auto-detection unit
tests with mocked environment"* — was satisfied literally. Those tests call the
detection helper directly and pass; they cannot fail when nothing in the startup
path invokes it. The requirement asked for unit tests of the detector and got
them. It did not ask that the detector be reached, and so nothing checked that
it was.

That is the substance worth carrying out of this amendment: **an acceptance
criterion phrased as a behaviour, verified by a test of a component, is not
verified.** The gap held for the entire life of the RFC and was found by the
project owner asking what language a fresh install starts in — not by review and
not by CI.

**The fix** (Task 009, `3d2b47d`) changes the default to `"auto"` and extracts
the priority chain into `bootstrap::startup::resolve_locale`, which takes the
OS-detected value as a parameter so the chain is testable without mutating
process environment variables. Coverage now includes the default resolving
through to OS detection, and an explicit `"en"` still winning over a Japanese
environment.

**Existing profiles are deliberately unchanged.** Every profile that has ever
launched orbok already has `"en"` written to disk (RFC-049's first-run settings
write), and a settings file lacking the field fails to deserialize rather than
adopting the new default — so no installation silently switches language. The
fix reaches new profiles only, which is the population §7's criterion was about.

Nothing in this RFC's design, rules, or criteria changes. §7's criterion is
unchanged and is now met.
