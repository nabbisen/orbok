# RFC-052: UI Localization and Design-Gate Compliance

**Project:** orbok  
**RFC:** 052  
**Title:** UI Localization and Design-Gate Compliance  
**Status:** Proposed  
**Target milestone:** v1.0.0 UI compliance  
**Date:** 2026-07-14  
**Related RFCs:** RFC-031 GUI Internationalization; RFC-032 Design Tokens; RFC-034 Accessibility; RFC-035 Inclusive Design  
**Handoff:** [`HANDOFF-052-ui-localization-and-design-gate-compliance.md`](../handoffs/HANDOFF-052-ui-localization-and-design-gate-compliance.md)

---

## 1. Summary

This RFC restores enforcement of already-decided UI rules: every user-visible
string must use the typed English/Japanese catalog, and visual values in views
and components must use Snora Design tokens. Both checks become mandatory CI
and release gates.

No new visual design or translation system is introduced.

## 2. Triggering Evidence

The architecture preparation review found literal English copy in search,
source, indexing, model, and wizard views plus native folder-picker titles in
`orbok-app`. The repository's design-token script also fails on literal
padding, and CI does not execute that script. Japanese mode therefore cannot
localize all visible interactions, while an implemented RFC-032 rule is not
enforced.

## 3. Localization Boundary

- Every visible label, placeholder, status, help text, dialog title, progress
  phrase, separator with semantic meaning, and formatted message maps through a
  typed `MessageKey` or typed parameterized formatter.
- `orbok-ui` owns locale-neutral keys and En/Ja translations.
- `orbok-app` platform integrations request already-localized strings from the
  UI/i18n boundary; they do not embed English display copy.
- Backend errors remain typed. Raw backend/debug strings are not shown directly
  to users.
- Dynamic technical identifiers such as filenames may remain data, but all
  surrounding prose and units are localized and safely formatted.

## 4. Catalog and Formatting Rules

1. Every new key has En and Ja entries in the same change.
2. Compile-time/exhaustive tests cover every `(Locale, MessageKey)` pair.
3. Counts, byte sizes, progress, model metadata, and file-position messages use
   centralized parameterized formatting rather than English concatenation.
4. Accessibility labels and native-dialog titles follow the same catalog rule.
5. Translation quality receives manual Japanese review; key parity alone is
   necessary but not sufficient.

## 5. Design-Token Rules

RFC-032 remains authoritative: view/component modules contain no literal font
size, padding, spacing, radius, or color. The current zero padding is redundant
and must be removed. A future genuine structural zero may use a narrowly named,
documented central helper outside the banned view/component locations; a helper
that merely hides an arbitrary literal is non-compliant.

The existing checker may be improved for precision, but any change must retain
planted-violation tests for each forbidden category.

## 6. Mandatory Automation

CI fast and release gates must run:

- the design-token checker;
- an i18n literal-copy checker over designated UI and platform-integration
  files;
- exhaustive En/Ja catalog tests.

The checker discovers tracked files under designated UI and platform-integration
directories and compares that set exactly with a classified allowlist. Every
data/technical exception records its reason and classification; broad file or
line-pattern exclusions are forbidden. A new unclassified tracked file fails
the gate. Checker scripts must have self-tests or fixtures proving both clean
and planted-violation behavior. Release documentation lists the commands.

The complete Phase 1 literal inventory, display/data classifications, and
exception reasons require a separate review and acceptance before bulk catalog
migration begins.

## 7. Scope

In scope:

- localize current literal UI copy identified by the review and a complete
  repository scan;
- add any required parameterized message API;
- remove current token violations;
- wire both policy checks into CI and release documentation;
- correct nearby stale user documentation revealed by the same terminology
  review.

Out of scope:

- Translating the mdBook.
- Adding languages beyond English and Japanese.
- Replacing the typed catalog with Fluent.
- Redesigning layouts or changing product workflows.
- General file-size refactoring except where needed to keep the catalog/view
  changes reviewable.

## 8. Testing Requirements

1. Exhaustive message parity/non-empty tests remain green.
2. Parameterized messages render meaningful En and Ja output.
3. Locale switching updates every affected view without restart.
4. Native folder-picker titles use the active locale.
5. Literal-copy checker fails on a planted English label/placeholder.
6. Token checker fails on planted font, padding, spacing, radius, and color
   literals and passes the production tree.
7. Manual Japanese QA covers wizard, search, sources, indexing, models,
   settings, dialogs, progress, empty states, and accessibility labels.
8. Discovery/allowlist equality fails on an unclassified tracked UI file and
   accepts only individually reasoned data/technical exceptions.

## 9. Acceptance Criteria

This RFC is accepted when the user-visible-string boundary, checker scope, and
zero-literal token policy are approved.

It is implemented when the production UI contains no uncatalogued visible
copy, En/Ja behavior is manually reviewed, both policy scripts pass and run in
CI/release gates, documentation matches the commands, and accessibility QA
finds no English-only control in Japanese mode.
