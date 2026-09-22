//! Task 088: a toggle's own button says what it toggles, not another
//! control's name.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::AppState;
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

/// Every combination of the two independent toggles, in both locales: the
/// Remember-recent-searches button must read exactly "On"/"Off", never the
/// Advanced-view label -- and vice versa, so a fix that merely swaps which
/// toggle owns the wrong text does not pass by accident.
#[test]
fn each_toggle_shows_only_its_own_on_off_label() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for remember in [true, false] {
            for advanced in [true, false] {
                let state = AppState {
                    locale,
                    remember_recent_searches: remember,
                    show_advanced: advanced,
                    ..AppState::default()
                };
                let mut ui = simulator(views::settings_view(&state));
                let on = tr(locale, MessageKey::SettingsToggleOn);
                let off = tr(locale, MessageKey::SettingsToggleOff);
                assert!(
                    ui.find(if remember { on } else { off }).is_ok(),
                    "{locale:?} remember={remember}: the Remember-recent-searches \
                     toggle must read its own On/Off"
                );
                assert!(
                    ui.find(if advanced { on } else { off }).is_ok(),
                    "{locale:?} advanced={advanced}: the Advanced-view toggle \
                     must read its own On/Off"
                );
            }
        }
    }
}
