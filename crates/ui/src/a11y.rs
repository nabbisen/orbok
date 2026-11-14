//! Accessibility usage guards (RFC-034 §5.1).
//!
//! We don't re-derive contrast math; we assert the foreground/background role
//! pairs orbok *actually renders* meet WCAG AA across all four token presets,
//! using `snora::design::contrast` (available via the facade since snora 0.25.1).
//!
//! ## Why this exists
//!
//! Snora Design's built-in palettes are already contrast-tested at the preset
//! level. This module guards orbok's *usage* — e.g. it catches the case where
//! the app renders `text_secondary on background` while snora only guarantees
//! `text_secondary on surface`. The test suite calls [`audit`] for every
//! theme preset and asserts every pair meets its AA threshold.
//!
//! ## Contrast thresholds (Task 140 §2.1, RFC-066 §3.3, RFC-118)
//!
//! - **Text, any size: WCAG AAA, ≥ 7.0 : 1.** Task 140 moved every text
//!   pair here from AA (4.5:1) to AAA, matching `theme::Theme::tokens`'s
//!   own move to `Tokens::light_aaa`/`dark_aaa` for Light/Dark (the
//!   high-contrast presets already clear AAA by a wide margin). **No
//!   exceptions:** every pair below must reach 7.0 in every preset: see
//!   the pasted table in Review Request 320.
//! - **Non-text UI boundaries** (a control's outline, not used alone to
//!   convey its shape): WCAG 1.4.11, ≥ 3.0 : 1. None are in
//!   [`RENDERED_PAIRS`] yet -- see `palette.border`'s own module comment
//!   below for why, and for the measured numbers that would already
//!   clear 3:1 if that ever changes.
//!
//! `text_muted` is **not rendered by orbok at all** — this comment is its only
//! occurrence in the tree. It is therefore absent from the pairs below because
//! there is nothing to audit, not because it is exempt.
//!
//! This previously claimed an exemption on the grounds that snora documented
//! the role as below-body contrast for decorative text. snora **withdrew** that
//! in 0.34.0 ("was ours, invented"); WCAG grants no role-level exemption. If
//! `text_muted` is ever rendered here, add it to [`RENDERED_PAIRS`] like any
//! other text role rather than reinstating the exemption.
//!
//! **Scope of this list, settled by snora 2026-08-18.** It covers the roles and
//! surfaces *orbok* renders, not snora's palette as a whole. snora runs its own
//! derived-pair enforcement (their RFC-063) over all 35 of its pairs, and its
//! token surface is contractually frozen — so a role we do not render is not
//! ours to audit. Do not widen this list to mirror snora's.
//!
//! `palette.border` is also excluded: snora's border role is a visual
//! separator between surfaces, not a foreground element rendered over text.
//! WCAG 1.4.11 (non-text contrast) applies to UI component *boundaries*, but
//! only when the border itself is the sole means of conveying the component's
//! bounding box. orbok's cards use `card::surface` (background fill) to
//! define their extent, so the border is supplementary decoration and does not
//! need to meet the 3:1 threshold independently.
//!
//! **This holds only while orbok's surfaces stay fill-defined.** snora's own
//! dialog card is border-defined (RFC-039), so adopting `snora::design::render`
//! would make the border load-bearing and require a pair here. orbok renders
//! `snora::render` and a bespoke confirm dialog today, so it does not.
//! Re-checked 2026-08-18 against snora 0.34.0's border repair.
//!
//! **If that day comes, the pair would pass.** Measured on snora 0.37.1
//! (independently, twice), `border` against every surface: `light` 3.12–3.38,
//! `dark` 3.17–3.81, both high-contrast presets 19.80–21.00 — all clear SC
//! 1.4.11's 3:1. So adding the pair is bookkeeping, not a blocked change
//! waiting on an upstream value.

use snora::design::contrast::contrast_ratio;
use snora::design::{Color, Tokens};

/// A (foreground, background, minimum-ratio, description) entry.
pub struct ContrastPair {
    pub description: &'static str,
    pub fg_role: fn(&Tokens) -> Color,
    pub bg_role: fn(&Tokens) -> Color,
    /// WCAG AAA minimum for text: 7.0. Non-text UI boundaries: 3.0
    /// (WCAG 1.4.11) -- none rendered here yet (Task 140 §2.1).
    pub min_ratio: f32,
}

/// All foreground/background role pairs that orbok renders.
///
/// Add a new entry here whenever a new color combination is introduced in
/// `views.rs`, `components.rs`, or `notice.rs`. The test suite will then
/// automatically verify it across all presets.
pub const RENDERED_PAIRS: &[ContrastPair] = &[
    ContrastPair {
        description: "body text on background",
        fg_role: |t| t.palette.text_primary,
        bg_role: |t| t.palette.background,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "body text on surface (cards)",
        fg_role: |t| t.palette.text_primary,
        bg_role: |t| t.palette.surface,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "secondary text on surface",
        fg_role: |t| t.palette.text_secondary,
        bg_role: |t| t.palette.surface,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "accent_text on accent (primary button)",
        fg_role: |t| t.palette.accent_text,
        bg_role: |t| t.palette.accent,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "danger_text on danger (danger button / badge)",
        fg_role: |t| t.palette.danger_text,
        bg_role: |t| t.palette.danger,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "warning_text on warning (stale badge surface)",
        fg_role: |t| t.palette.warning_text,
        bg_role: |t| t.palette.warning,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "success_text on success (current badge surface)",
        fg_role: |t| t.palette.success_text,
        bg_role: |t| t.palette.success,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "info_text on info (keyword badge surface)",
        fg_role: |t| t.palette.info_text,
        bg_role: |t| t.palette.info,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "text_primary on surface_raised (raised cards)",
        fg_role: |t| t.palette.text_primary,
        bg_role: |t| t.palette.surface_raised,
        min_ratio: 7.0,
    },
    // Task 140 §1a.3/§2.1: the two pairs `components::link` adds --
    // outlined labels already read `accent` on their own background
    // (the fill is accent only when chosen/filled; a link's text is
    // always accent, directly on background or surface).
    ContrastPair {
        description: "accent on background (link text)",
        fg_role: |t| t.palette.accent,
        bg_role: |t| t.palette.background,
        min_ratio: 7.0,
    },
    ContrastPair {
        description: "accent on surface (link text, on a card)",
        fg_role: |t| t.palette.accent,
        bg_role: |t| t.palette.surface,
        min_ratio: 7.0,
    },
    // Task 140 §2.1/§2.2: every `text_input`'s placeholder is now
    // explicitly `text_secondary` on the input's own `background`
    // (`components::text_input_style`), not iced's derived
    // approximation -- so this is the pair that actually renders.
    ContrastPair {
        description: "text_secondary on background (text_input placeholder)",
        fg_role: |t| t.palette.text_secondary,
        bg_role: |t| t.palette.background,
        min_ratio: 7.0,
    },
];

/// Result of a single contrast check.
pub struct PairResult<'a> {
    pub description: &'a str,
    pub ratio: f32,
    pub min_ratio: f32,
    pub passes: bool,
}

/// Audit all [`RENDERED_PAIRS`] against a token bundle.
///
/// Returns one [`PairResult`] per pair. The test suite iterates every preset
/// and asserts `result.passes` for each entry.
pub fn audit(tokens: &Tokens) -> Vec<PairResult<'_>> {
    RENDERED_PAIRS
        .iter()
        .map(|pair| {
            let fg = (pair.fg_role)(tokens);
            let bg = (pair.bg_role)(tokens);
            let ratio = contrast_ratio(fg, bg);
            PairResult {
                description: pair.description,
                ratio,
                min_ratio: pair.min_ratio,
                passes: ratio >= pair.min_ratio,
            }
        })
        .collect()
}
