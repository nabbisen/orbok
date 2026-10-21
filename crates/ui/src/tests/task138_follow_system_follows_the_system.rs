//! Task 138: "Follow system" follows the system. Before this, `Theme::System`
//! always resolved to Light, at startup, with no way to react to a real OS
//! light/dark preference -- `Message::SystemThemeMode` now carries it in,
//! both once at boot (`iced::system::theme()`) and continuously while
//! running (`iced::system::theme_changes()`), and `Theme::resolved` is the
//! one place that turns it, plus the stored setting, into a concrete theme.

use crate::state::{AppState, Message};
use crate::theme::Theme;
use iced::theme::Mode;

/// A fresh app whose stored theme setting is `System` (the default).
fn system_themed_app() -> AppState {
    AppState::default()
}

/// §2: red first against the pre-fix code -- `Theme::System` ignored
/// `Message::SystemThemeMode` entirely and always rendered Light.
#[test]
fn follow_system_with_a_reported_dark_mode_yields_the_dark_tokens() {
    let mut state = system_themed_app();
    assert_eq!(state.theme, Theme::System);
    state.update(&Message::SystemThemeMode(Mode::Dark));
    assert_eq!(
        state.tokens,
        Theme::Dark.tokens(),
        "Follow system with Mode::Dark reported must render the dark tokens"
    );
}

/// The same, the other way: a dark desktop switching to light is followed,
/// not stuck at whichever mode arrived first.
#[test]
fn follow_system_switches_back_to_light_when_the_mode_changes_again() {
    let mut state = system_themed_app();
    state.update(&Message::SystemThemeMode(Mode::Dark));
    assert_eq!(state.tokens, Theme::Dark.tokens());
    state.update(&Message::SystemThemeMode(Mode::Light));
    assert_eq!(
        state.tokens,
        Theme::Light.tokens(),
        "Follow system must track a later mode change, not only the first one"
    );
}

/// `Mode::None` (no preference reported) gives Light -- one documented
/// fallback (`Theme::resolved`'s own doc comment), not a panic or a stale
/// value left over from whatever `tokens` held before.
#[test]
fn mode_none_gives_light() {
    let mut state = system_themed_app();
    state.update(&Message::SystemThemeMode(Mode::Dark));
    state.update(&Message::SystemThemeMode(Mode::None));
    assert_eq!(
        state.tokens,
        Theme::Light.tokens(),
        "Mode::None must resolve Follow system to Light"
    );
}

/// A concrete theme ignores the system, both at startup and while running
/// (the task's own §1 item 3): setting Dark explicitly, then reporting a
/// light desktop, must change nothing.
#[test]
fn a_concrete_theme_ignores_a_system_mode_change() {
    let mut state = system_themed_app();
    state.update(&Message::SetTheme(Theme::Dark));
    assert_eq!(state.tokens, Theme::Dark.tokens());
    state.update(&Message::SystemThemeMode(Mode::Light));
    assert_eq!(
        state.tokens,
        Theme::Dark.tokens(),
        "an explicitly chosen Dark theme must not react to the system mode"
    );
}

/// The same, for Light: a `SystemThemeMode(Dark)` report while the stored
/// setting is Light must not darken the app.
#[test]
fn a_concrete_light_theme_also_ignores_a_system_mode_change() {
    let mut state = system_themed_app();
    state.update(&Message::SetTheme(Theme::Light));
    state.update(&Message::SystemThemeMode(Mode::Dark));
    assert_eq!(
        state.tokens,
        Theme::Light.tokens(),
        "an explicitly chosen Light theme must not react to the system mode"
    );
}

/// Switching the stored setting to `System` after the OS mode is already
/// known picks up that mode immediately, from `system_theme_mode`, not a
/// hard-coded Light -- the setting and the OS mode are two independent
/// pieces of state, and `SetTheme` must read the one already stored.
#[test]
fn choosing_follow_system_after_the_mode_is_already_known_uses_it_at_once() {
    let mut state = system_themed_app();
    state.update(&Message::SetTheme(Theme::Light));
    state.update(&Message::SystemThemeMode(Mode::Dark));
    assert_eq!(state.tokens, Theme::Light.tokens());
    state.update(&Message::SetTheme(Theme::System));
    assert_eq!(
        state.tokens,
        Theme::Dark.tokens(),
        "choosing Follow system must apply the already-known dark mode at once"
    );
}

/// §2: `snora_tokens` is rebuilt with the theme, so text size is not lost
/// when the system mode changes -- it must stay derived from the *current*
/// `text_scale`, never silently reset to the default scale.
#[test]
fn a_system_mode_change_keeps_the_current_text_scale() {
    use crate::theme::TextScale;

    let mut state = system_themed_app();
    state.update(&Message::SetTextScale(TextScale::Larger));
    let scaled_label_before = state.snora_tokens.for_snora_widget().typography.label.size;

    state.update(&Message::SystemThemeMode(Mode::Dark));

    let scaled_label_after = state.snora_tokens.for_snora_widget().typography.label.size;
    assert_eq!(
        scaled_label_after, scaled_label_before,
        "a system theme-mode change must not reset the text scale"
    );
    let unscaled_label = Theme::Dark.tokens().typography.label.size;
    assert!(
        (scaled_label_after / unscaled_label - TextScale::Larger.factor()).abs() < 0.001,
        "snora_tokens must still be scaled by Larger after the mode change, got {scaled_label_after} vs unscaled {unscaled_label}"
    );
}
