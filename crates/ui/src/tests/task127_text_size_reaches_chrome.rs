//! Task 127: Text size reaches snora's own chrome. snora 0.52's design-styled
//! sidebar and tab bar read `Typography`, so orbok now builds a scaled copy of
//! its tokens (`theme::chrome_tokens`) for them, while `AppState::tokens`
//! itself stays unscaled -- orbok's own text still goes through `body_s` and
//! its siblings exactly as before, applying the scale once.

use crate::shell::OrbokApp;
use crate::state::AppState;
use crate::tests::iced_test_guard;
use crate::theme::{TextScale, Theme, body_s, chrome_tokens};
use iced_test::simulator;

fn app_at(scale: TextScale) -> OrbokApp {
    OrbokApp::with_state(AppState {
        text_scale: scale,
        ..AppState::default()
    })
}

/// §2 test 1: the tab bar follows Text size. The one "Search" tab's rendered
/// height at Larger is taller than at Default, by roughly the scale factor
/// (some fixed chrome around the label keeps it from being exact).
#[test]
fn the_tab_bar_follows_text_size() {
    let _guard = iced_test_guard();
    let default_app = app_at(TextScale::Default);
    let larger_app = app_at(TextScale::Larger);
    let mut default = simulator(default_app.view());
    let mut larger = simulator(larger_app.view());
    let default_height = default.find("Search").unwrap().bounds().height;
    let larger_height = larger.find("Search").unwrap().bounds().height;
    assert!(
        larger_height > default_height,
        "Larger ({larger_height}) must be taller than Default ({default_height})"
    );
    let ratio = larger_height / default_height;
    assert!(
        (1.05..=TextScale::Larger.factor() + 0.05).contains(&ratio),
        "the tab label grew by {ratio:.3}x, expected roughly {}x",
        TextScale::Larger.factor()
    );
}

/// §2 test 2: one scale, applied once. `chrome_tokens` never reaches
/// `body_s`/`title_s`/`heading_s`: orbok's own body text at Larger is exactly
/// 1.3x Default (not 1.3 * 1.3).
#[test]
fn orbok_s_own_text_is_scaled_once_not_twice() {
    let tokens = Theme::Light.tokens();
    let default = body_s(&tokens, TextScale::Default).0;
    let larger = body_s(&tokens, TextScale::Larger).0;
    assert!(
        (larger / default - TextScale::Larger.factor()).abs() < 0.001,
        "body_s at Larger must be {}x Default, got {}x",
        TextScale::Larger.factor(),
        larger / default
    );

    // The scaled chrome copy is a *different* Tokens value: reading `body_s`
    // through it would double the scale, and it must never happen in the app
    // (chrome_tokens's own doc comment says so; this proves the arithmetic
    // itself, independent of whether some future call site is added by
    // mistake).
    let chrome = chrome_tokens(&tokens, TextScale::Larger);
    let scaled_once = body_s(&tokens, TextScale::Larger).0;
    let would_double = body_s(&chrome, TextScale::Larger).0;
    assert!(
        (would_double / scaled_once - TextScale::Larger.factor()).abs() < 0.001,
        "reading body_s through the chrome copy doubles the scale -- {scaled_once} vs \
         {would_double}; nothing in the app may do this"
    );
}

/// `chrome_tokens` scales every role's size and leaves line heights alone.
#[test]
fn chrome_tokens_scales_size_only() {
    let tokens = Theme::Light.tokens();
    let chrome = chrome_tokens(&tokens, TextScale::Larger);
    let f = TextScale::Larger.factor();
    for (base, scaled) in [
        (tokens.typography.body, chrome.typography.body),
        (tokens.typography.body_small, chrome.typography.body_small),
        (tokens.typography.label, chrome.typography.label),
        (tokens.typography.title, chrome.typography.title),
        (tokens.typography.heading, chrome.typography.heading),
        (tokens.typography.display, chrome.typography.display),
    ] {
        assert!(
            (scaled.size / base.size - f).abs() < 0.001,
            "{} -> {} is not scaled by {f}",
            base.size,
            scaled.size
        );
        assert_eq!(
            scaled.line_height, base.line_height,
            "line height must not scale"
        );
    }
}

/// §1.4: the tab bar grows at Larger; Task 106's narrow-window rule (nothing
/// pushed off-screen) still holds for it at 450 px / Larger, in both locales
/// -- checked through the real shell (`OrbokApp::view`), which Task 106's own
/// tests do not render (they call the page views directly, never the sidebar
/// or tab bar).
#[test]
fn the_grown_tab_bar_stays_inside_a_narrow_window() {
    use crate::i18n::{Locale, MessageKey, tr};
    use crate::state::{NavGroup, ViewId};
    use iced::Size;
    use iced_test::Simulator;

    const WIDTH: f32 = 450.0;
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        // AI: the widest tab bar (three labels), the case most likely to overflow.
        let app = OrbokApp::with_state(AppState {
            locale,
            active_view: ViewId::Sources,
            text_scale: TextScale::Larger,
            ..AppState::default()
        });
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            Size::new(WIDTH, 900.0),
            app.view(),
        );
        for view in ViewId::pages(NavGroup::Ai) {
            let label = tr(locale, view.label_key());
            let found = ui
                .find(label)
                .unwrap_or_else(|_| panic!("{locale:?}: tab {label:?} is not on the page"));
            let bounds = found.bounds();
            assert!(
                bounds.x >= -0.5 && bounds.x + bounds.width <= WIDTH + 0.5,
                "{locale:?}: tab {label:?} is pushed outside the {WIDTH} px window: x {} .. {}",
                bounds.x,
                bounds.x + bounds.width
            );
        }
        // The sidebar's own tooltip label, over Settings (Task 121's fix).
        let settings = OrbokApp::with_state(AppState {
            locale,
            active_view: ViewId::Settings,
            text_scale: TextScale::Larger,
            ..AppState::default()
        });
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            Size::new(WIDTH, 900.0),
            settings.view(),
        );
        let settings_label = tr(locale, MessageKey::NavSettings);
        assert!(
            ui.find(settings_label).is_ok(),
            "{locale:?}: the Settings tab is reachable at 450 px / Larger"
        );
    }
}
