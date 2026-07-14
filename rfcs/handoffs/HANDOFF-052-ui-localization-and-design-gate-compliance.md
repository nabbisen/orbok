# Implementation Handoff — RFC-052: UI Localization and Design-Gate Compliance

**Project:** orbok  
**RFC:** 052  
**Lifecycle stage:** Design + handoff  
**Primary owners:** `orbok-ui`, platform UI integration, CI  
**RFC:** [`../proposed/052-ui-localization-and-design-gate-compliance.md`](../proposed/052-ui-localization-and-design-gate-compliance.md)

> **Scope rule:** Enforce existing RFC-031/032 decisions. Do not redesign the
> UI or create broad exceptions to make heuristic checks pass.

## 1. Expected Change Surface

- `crates/ui/src/i18n.rs` and `i18n/{en,ja}.rs`
- parameterized i18n formatting helpers/tests
- `crates/ui/src/views.rs` and `views/wizard.rs`
- `crates/ui/src/components.rs`
- platform dialog calls in `crates/app/src/main.rs`
- `scripts/check-design-tokens.sh`
- a new literal-copy/i18n policy checker and fixtures
- `.github/workflows/ci.yml`
- testing/release/accessibility documentation

## 2. Phase 1 — Inventory and Checker Contract

1. Produce a complete inventory of visible literals in UI/platform integration
   files and classify display copy versus technical/data strings.
2. Discover tracked files under the designated UI/platform-integration
   directories and compare them exactly with a classified allowlist.
3. Give every technical/data exception an individual reason; prohibit broad
   file or line-pattern exclusions.
4. Add clean and planted-violation fixtures for the token and literal-copy
   checkers.
5. Make checkers fail closed when expected paths are missing or a new tracked
   file is unclassified.

Mandatory review point: approve the complete inventory, classifications,
exception reasons, discovery roots, and fixtures before bulk catalog edits.

## 3. Phase 2 — Catalog and View Migration

1. Add typed keys and En/Ja translations in coherent screen-sized slices.
2. Add parameterized formatters for counts, progress, bytes, missing-file
   notes, model metadata, and similar dynamic copy.
3. Replace literals in wizard, search, sources, indexing, models, settings,
   dialogs, empty states, and accessibility labels.
4. Pass localized native-dialog titles from the active locale into `orbok-app`.
5. Remove the current redundant zero padding. Add no helper for it. A future
   genuine structural-zero helper requires documented semantics outside the
   banned view/component locations and separate review.

Keep each slice small enough for English/Japanese copy review. Splitting the
oversized view/i18n files by stable responsibility is encouraged when directly
adjacent, but unrelated refactoring is deferred.

## 4. Phase 3 — Mandatory Gates and Manual QA

1. Add both scripts plus exhaustive catalog tests to fast and release CI.
2. Document the exact local commands.
3. Run manual Japanese QA over every screen and native dialog.
4. Check keyboard/screen-reader labels in both locales.
5. Correct stale nearby docs (`README` Japanese-search wording and model setup
   instructions) where they conflict with observed behavior.

## 5. Validation

- exhaustive i18n unit tests
- parameterized En/Ja formatter tests
- design-token checker and its planted violations
- literal-copy checker and its planted violations
- `cargo test -p orbok-ui --lib`
- `cargo test --workspace --lib`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `mdbook build docs`
- manual En/Ja UI checklist
- `git diff --check`

## 6. Stop Conditions

Return to design review if a backend API must become locale-aware, translation
requires replacing the catalog technology, token compliance requires a new
Snora token, or a UI workflow/layout change becomes necessary.

## 7. Definition of Done

All visible copy and accessibility labels switch with locale, token and literal
policy checks pass production plus planted tests, CI/release gates run them,
manual Japanese QA is recorded, and documentation matches the final UI.
