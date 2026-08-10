//! Task 009: RFC-031's `auto` locale never reached OS detection because
//! `Option::or_else` only evaluates its closure on `None`, and
//! `OrbokSettings::default().locale` was `"en"` -- a value
//! `Locale::parse` accepts, so the priority chain stopped at the first
//! step on every fresh profile. Existing unit tests on
//! `Locale::from_env_values` (crates/ui/src/tests/i18n.rs) proved OS
//! detection parses correctly; they could not fail when the startup
//! chain never called it, which is exactly what shipped.
//!
//! These tests exercise the priority chain itself
//! (`bootstrap::startup::resolve_locale`), not the OS-detection parsing
//! alone, and drive the OS-detected value through an injected parameter
//! rather than `std::env` -- mutating process environment variables is
//! `unsafe` in this edition and races the parallel test harness
//! (HANDOFF-055 §5).

use crate::bootstrap::startup::resolve_locale;
use crate::settings::OrbokSettings;
use orbok_ui::i18n::Locale;

/// The test that would have caught the original defect: a
/// default-constructed `OrbokSettings` -- the exact value a fresh profile
/// starts with -- must resolve through to OS detection rather than
/// stopping at the settings value.
#[test]
fn default_settings_resolve_through_to_os_detection() {
    let settings = OrbokSettings::default();
    let resolved = resolve_locale(&settings.locale, None, Some(Locale::Ja));
    assert_eq!(resolved, Locale::Ja);
}

/// Structural guard on the same fact from the other direction: the
/// default settings value must not itself be a parseable `Locale`, since
/// that is precisely what makes the fall-through in the test above
/// happen. A future edit that makes `OrbokSettings::default().locale`
/// `"en"` again -- or that adds an `"auto" => Some(...)` arm to
/// `Locale::parse` -- fails this without needing to reconstruct why.
#[test]
fn default_settings_locale_is_not_a_parseable_locale() {
    assert_eq!(Locale::parse(&OrbokSettings::default().locale), None);
}

/// The regression this fix could plausibly introduce, and the guard for
/// the scope boundary: every profile that has ever launched orbok already
/// has an explicit `"en"` written to disk (RFC-049 C4), and must keep
/// getting English even on a Japanese-environment machine, not silently
/// switch.
#[test]
fn explicit_en_setting_wins_over_a_japanese_environment() {
    let resolved = resolve_locale("en", None, Some(Locale::Ja));
    assert_eq!(resolved, Locale::En);
}

/// The catalog step still takes priority over the environment when the
/// settings value itself is the "auto" sentinel.
#[test]
fn catalog_locale_wins_over_environment_when_settings_is_auto() {
    let resolved = resolve_locale("auto", Some("ja"), Some(Locale::En));
    assert_eq!(resolved, Locale::Ja);
}

/// When nothing resolves -- "auto" settings, no catalog value, no
/// environment match -- the chain falls back to the documented default.
#[test]
fn falls_back_to_default_when_nothing_resolves() {
    let resolved = resolve_locale("auto", None, None);
    assert_eq!(resolved, Locale::default());
}
