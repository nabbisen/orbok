//! Task 127: Text size reaches snora's own chrome. snora 0.52's `design`
//! widgets read `Typography`, so orbok builds a scaled copy of its tokens
//! (`theme::snora_tokens`) for every surface snora itself draws, while
//! `AppState::tokens` stays unscaled -- orbok's own text still goes through
//! `body_s` and its siblings exactly as before, applying the scale once.
//!
//! Review 305 §2: the rule reaches every snora-drawn surface orbok renders,
//! not only the tab bar -- the sidebar tooltip, the notice, and the search
//! row's folder chip too. Every state here that touches `text_scale` goes
//! through `AppState::update` (`Message::SetTextScale`), never a struct
//! literal that sets the field directly: `snora_tokens` is derived, kept in
//! sync by the reducer, and a literal that bypasses it would silently test
//! against a stale value.

use crate::shell::OrbokApp;
use crate::state::AppState;
use crate::state::location::{SearchFolderScope, SearchLocation};
use crate::tests::iced_test_guard;
use crate::theme::{TextScale, Theme, body_s, snora_tokens};
use iced_test::simulator;

/// A fresh app at `scale`, going through the reducer so `snora_tokens` is
/// correct (see the module doc comment).
fn app_at(scale: TextScale) -> OrbokApp {
    let mut state = AppState::default();
    state.update(&crate::state::Message::SetTextScale(scale));
    OrbokApp::with_state(state)
}

/// `app_at`, on the Search page, with a folder chosen -- so the search row's
/// removable chip renders.
fn app_at_with_chip(scale: TextScale) -> OrbokApp {
    let mut app = app_at(scale);
    app.state
        .update(&crate::state::Message::SearchLocationSelected(
            SearchLocation::remembered(orbok_core::id::SourceId::generate(), "Docs")
                .with_scope(SearchFolderScope::FolderAndSubfolders),
        ));
    app
}

/// `app_at`, with a notice showing.
fn app_at_with_notice(scale: TextScale) -> OrbokApp {
    let mut app = app_at(scale);
    app.state.update(&crate::state::Message::ShowNotice(
        crate::notice::UserNotice::FolderAdded,
    ));
    app
}

/// The height of the first thing `label` is found in, at `scale`, through
/// `build`. `build` gets its own fresh app each call (`Simulator` borrows it).
fn height_at(scale: TextScale, label: &str, build: fn(TextScale) -> OrbokApp) -> f32 {
    let app = build(scale);
    let mut ui = simulator(app.view());
    ui.find(label)
        .unwrap_or_else(|_| panic!("{label:?} not found at {scale:?}"))
        .bounds()
        .height
}

/// Asserts `label`'s rendered height at Larger is roughly `TextScale::Larger`
/// times its height at Default -- "roughly" because fixed chrome around a
/// label (padding, an icon) keeps the ratio from being exact.
fn assert_grows_with_scale(what: &str, label: &str, build: fn(TextScale) -> OrbokApp) {
    let default = height_at(TextScale::Default, label, build);
    let larger = height_at(TextScale::Larger, label, build);
    assert!(
        larger > default,
        "{what}: Larger ({larger}) must be taller than Default ({default})"
    );
    let ratio = larger / default;
    assert!(
        (1.05..=TextScale::Larger.factor() + 0.05).contains(&ratio),
        "{what}: grew by {ratio:.3}x, expected roughly {}x",
        TextScale::Larger.factor()
    );
}

/// §2 test 1, extended (Review 305 §2 follow-up): every snora-drawn surface
/// orbok renders follows Text size, not only the tab bar. Red first for the
/// notice and the chip (`views.rs:243`, `components.rs:704` took the base,
/// unscaled tokens before this follow-up).
///
/// The sidebar's own tooltip is not checked here: it is an overlay the
/// simulator does not lay out or measure at all (confirmed by reading
/// snora's own source, `sidebar.rs`: *"tooltip text is invisible to
/// `Simulator::find`"*) -- Task 121's own tests hit the same wall and hand
/// wording checks to the catalog instead of the renderer
/// (`the_x_has_the_approved_tooltip_in_both_locales`). This file's own
/// `the_sidebar_and_its_tooltip_take_the_scaled_tokens`, below, checks that
/// the call site passes the scaled tokens, the one part of "the tooltip
/// follows Text size" that can actually be observed short of a real
/// screenshot comparison.
#[test]
fn every_snora_drawn_surface_follows_text_size() {
    let _guard = iced_test_guard();
    assert_grows_with_scale("the tab bar", "Search", app_at);
    assert_grows_with_scale("the notice", "Folder added", app_at_with_notice);
    assert_grows_with_scale("the search row's folder chip", "Docs", app_at_with_chip);
}

/// The sidebar (and, through it, its tooltip -- see the previous test's own
/// comment for why the tooltip's rendered size cannot be measured directly)
/// takes the scaled tokens, not the base ones: the one call site, read from
/// source, the same idiom `task121_folder_chip::the_chip_is_always_outlined`
/// uses for the same kind of overlay gap.
#[test]
fn the_sidebar_and_its_tooltip_take_the_scaled_tokens() {
    let source = include_str!("../shell.rs");
    let from = source.find("let side_bar = app_side_bar(").unwrap();
    let call = &source[from..from + source[from..].find(");").unwrap()];
    assert!(
        call.contains("snora_tokens,"),
        "app_side_bar must be called with the scaled snora_tokens, not the base tokens: {call}"
    );
}

/// §2 test 2: one scale, applied once. `snora_tokens` never reaches
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

    // The scaled copy is a *different* Tokens value: reading `body_s` through
    // it would double the scale, and it must never happen in the app
    // (snora_tokens's own doc comment says so; this proves the arithmetic
    // itself, independent of whether some future call site is added by
    // mistake).
    let scaled = snora_tokens(&tokens, TextScale::Larger);
    let scaled_once = body_s(&tokens, TextScale::Larger).0;
    let would_double = body_s(&scaled, TextScale::Larger).0;
    assert!(
        (would_double / scaled_once - TextScale::Larger.factor()).abs() < 0.001,
        "reading body_s through the scaled copy doubles the scale -- {scaled_once} vs \
         {would_double}; nothing in the app may do this"
    );
}

/// `snora_tokens` scales every role's size and leaves line heights alone.
#[test]
fn snora_tokens_scales_size_only() {
    let tokens = Theme::Light.tokens();
    let scaled = snora_tokens(&tokens, TextScale::Larger);
    let f = TextScale::Larger.factor();
    for (base, s) in [
        (tokens.typography.body, scaled.typography.body),
        (tokens.typography.body_small, scaled.typography.body_small),
        (tokens.typography.label, scaled.typography.label),
        (tokens.typography.title, scaled.typography.title),
        (tokens.typography.heading, scaled.typography.heading),
        (tokens.typography.display, scaled.typography.display),
    ] {
        assert!(
            (s.size / base.size - f).abs() < 0.001,
            "{} -> {} is not scaled by {f}",
            base.size,
            s.size
        );
        assert_eq!(
            s.line_height, base.line_height,
            "line height must not scale"
        );
    }
}

/// `AppState::snora_tokens` is derived, not a value a caller can leave stale:
/// setting `text_scale` through the reducer keeps it in sync (Review 305 §2's
/// own point -- proved directly here, not only through rendering).
#[test]
fn snora_tokens_field_stays_in_sync_with_text_scale() {
    let mut state = AppState::default();
    assert_eq!(
        state.snora_tokens.typography.label.size,
        state.tokens.typography.label.size
    );
    state.update(&crate::state::Message::SetTextScale(TextScale::Larger));
    let expected = snora_tokens(&state.tokens, TextScale::Larger);
    assert_eq!(
        state.snora_tokens.typography.label.size,
        expected.typography.label.size
    );
    assert!(state.snora_tokens.typography.label.size > state.tokens.typography.label.size);
}

/// §1.4: the tab bar grows at Larger; Task 106's narrow-window rule (nothing
/// pushed off-screen) still holds for it at 450 px / Larger, in both locales
/// -- checked through the real shell (`OrbokApp::view`), which Task 106's own
/// tests do not render (they call the page views directly, never the sidebar
/// or tab bar).
#[test]
fn the_grown_tab_bar_stays_inside_a_narrow_window() {
    use crate::i18n::{Locale, MessageKey, tr};
    use crate::state::{Message, NavGroup, ViewId};
    use iced::Size;
    use iced_test::Simulator;

    const WIDTH: f32 = 450.0;
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        // AI: the widest tab bar (three labels), the case most likely to overflow.
        let mut state = AppState {
            locale,
            active_view: ViewId::Sources,
            ..AppState::default()
        };
        state.update(&Message::SetTextScale(TextScale::Larger));
        let app = OrbokApp::with_state(state);
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
        let mut settings_state = AppState {
            locale,
            active_view: ViewId::Settings,
            ..AppState::default()
        };
        settings_state.update(&Message::SetTextScale(TextScale::Larger));
        let settings = OrbokApp::with_state(settings_state);
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
