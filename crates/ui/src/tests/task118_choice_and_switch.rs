//! Task 118: a choice shows what is chosen, and an on/off setting is a switch.
//! Two standards (`components.rs`, "How a setting is shown"). Each place that
//! shows a choice is checked the same way: the chosen option is marked and its
//! press does nothing (`AlreadyChosen`), every other available option sends its
//! own message -- so no option is drawn disabled because it is current.

use crate::components::{ChoiceOption, choice, switch};
use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, SearchFolderScope, SearchLocation, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::theme::{TextScale, Theme};
use crate::views;
use iced_test::simulator;
use orbok_core::{SourceId, SourceStatus};
use orbok_models::SearchCapability;
use orbok_search::SearchMode;

/// What clicking `label` in `view` sends.
fn click(view: iced::Element<'_, Message>, label: &str) -> Vec<Message> {
    let mut ui = simulator(view);
    assert!(ui.find(label).is_ok(), "{label:?} is on the page");
    let _ = ui.click(label);
    ui.into_messages().collect()
}

/// What pressing a switch sends, pressing `at` of the way across it (0.0 is its
/// left edge, 1.0 its right): the switch and its label are one control, so the
/// whole width toggles.
fn toggle(view: iced::Element<'_, Message>, name: &'static str, at: f32) -> Vec<Message> {
    let mut ui = simulator(view);
    let bounds = ui
        .find(crate::components::switch_id(name))
        .unwrap_or_else(|_| panic!("the {name:?} switch is on the page"))
        .bounds();
    ui.point_at(iced::Point::new(
        bounds.x + bounds.width * at,
        bounds.y + bounds.height / 2.0,
    ));
    ui.simulate(iced_test::simulator::click());
    ui.into_messages().collect()
}

fn is_already_chosen(messages: &[Message]) -> bool {
    matches!(messages, [Message::AlreadyChosen])
}

/// The component: the chosen option carries a check icon to its left, is
/// pressable, and does nothing; the others send their own message; an
/// unavailable one sends nothing.
#[test]
fn a_choice_marks_the_chosen_option_and_keeps_the_rest_enabled() {
    let _guard = iced_test_guard();
    let tokens = crate::theme::Theme::Light.tokens();
    let build = || {
        choice(
            &tokens,
            iced::Pixels(14.0),
            vec![
                ChoiceOption {
                    label: "One".into(),
                    chosen: true,
                    available: true,
                    on_press: Message::SetTheme(Theme::Light),
                },
                ChoiceOption {
                    label: "Two".into(),
                    chosen: false,
                    available: true,
                    on_press: Message::SetTheme(Theme::Dark),
                },
                ChoiceOption {
                    label: "Three".into(),
                    chosen: false,
                    available: false,
                    on_press: Message::SetTheme(Theme::System),
                },
            ],
        )
    };
    assert!(is_already_chosen(&click(build(), "One")));
    assert!(matches!(
        click(build(), "Two").as_slice(),
        [Message::SetTheme(Theme::Dark)]
    ));
    assert!(
        click(build(), "Three").is_empty(),
        "unavailable sends nothing"
    );

    // The check icon sits to the left of the chosen label, on the same line.
    let glyph = char::from(snora::lucide::Check).to_string();
    let mut ui = simulator(build());
    let check = ui
        .find(glyph.as_str())
        .expect("the chosen option has a check")
        .bounds();
    let label = ui.find("One").unwrap().bounds();
    assert!(
        check.x + check.width <= label.x + 0.5,
        "{check:?} vs {label:?}"
    );
    assert!(
        (check.y + check.height / 2.0 - (label.y + label.height / 2.0)).abs() < 4.0,
        "same line: {check:?} vs {label:?}"
    );
    // Nothing else is marked: with the chosen option removed there is no check.
    let none = choice(
        &tokens,
        iced::Pixels(14.0),
        vec![ChoiceOption {
            label: "Two".into(),
            chosen: false,
            available: true,
            on_press: Message::AlreadyChosen,
        }],
    );
    assert!(simulator(none).find(glyph.as_str()).is_err());
}

/// Settings: language, theme, text size.
#[test]
fn settings_choices_show_what_is_chosen() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let state = AppState {
            locale,
            theme: Theme::Dark,
            text_scale: TextScale::Larger,
            active_view: ViewId::Settings,
            ..AppState::default()
        };
        let other_locale = if locale == Locale::En {
            Locale::Ja
        } else {
            Locale::En
        };
        // Language: the current one is marked, the other switches.
        assert!(is_already_chosen(&click(
            views::settings_view(&state),
            locale.display_name()
        )));
        assert!(matches!(
            click(views::settings_view(&state), other_locale.display_name()).as_slice(),
            [Message::SetLocale(l)] if *l == other_locale
        ));
        // Theme.
        let dark = tr(locale, Theme::Dark.label_key());
        let light = tr(locale, Theme::Light.label_key());
        assert!(
            is_already_chosen(&click(views::settings_view(&state), dark)),
            "{locale:?}"
        );
        assert!(matches!(
            click(views::settings_view(&state), light).as_slice(),
            [Message::SetTheme(Theme::Light)]
        ));
        // Text size.
        let larger = tr(locale, TextScale::Larger.label_key());
        let normal = tr(locale, TextScale::Default.label_key());
        assert!(is_already_chosen(&click(
            views::settings_view(&state),
            larger
        )));
        assert!(matches!(
            click(views::settings_view(&state), normal).as_slice(),
            [Message::SetTextScale(TextScale::Default)]
        ));
    }
}

fn search_state(locale: Locale, mode: SearchMode, capability: SearchCapability) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Search,
        show_advanced: true,
        search_mode: mode,
        capability,
        ..AppState::default()
    }
}

/// Search → Mode: the current mode is finally shown; by meaning is disabled
/// without a model and nothing else is.
#[test]
fn the_search_mode_shows_the_current_mode() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let auto = tr(locale, MessageKey::SearchModeAuto);
        let exact = tr(locale, MessageKey::SearchModeExact);
        let meaning = tr(locale, MessageKey::SearchModeConceptual);
        let with_model = search_state(locale, SearchMode::Exact, SearchCapability::Hybrid);
        assert!(
            is_already_chosen(&click(views::search_view(&with_model), exact)),
            "{locale:?}"
        );
        assert!(matches!(
            click(views::search_view(&with_model), auto).as_slice(),
            [Message::SetSearchMode(SearchMode::Auto)]
        ));
        assert!(matches!(
            click(views::search_view(&with_model), meaning).as_slice(),
            [Message::SetSearchMode(SearchMode::Conceptual)]
        ));
        let without = search_state(locale, SearchMode::Auto, SearchCapability::KeywordOnly);
        assert!(is_already_chosen(&click(
            views::search_view(&without),
            auto
        )));
        assert!(
            click(views::search_view(&without), meaning).is_empty(),
            "the one unavailable option"
        );
        assert!(matches!(
            click(views::search_view(&without), exact).as_slice(),
            [Message::SetSearchMode(SearchMode::Exact)]
        ));
    }
}

fn card(covers_subfolders: bool) -> SourceCard {
    SourceCard {
        display_name: "Docs".into(),
        display_path: "/home/user/Docs".into(),
        indexed: 0,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders,
    }
}

/// The folder card shows **both** coverage labels, the current one chosen; the
/// other one is the button.
#[test]
fn the_folder_card_shows_both_choices_with_the_current_one_chosen() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let subfolders = tr(locale, MessageKey::SearchScopeSubfolders);
        let only = tr(locale, MessageKey::SearchScopeOnly);
        for covers in [true, false] {
            let state = AppState {
                locale,
                active_view: ViewId::Sources,
                sources: vec![card(covers)],
                ..AppState::default()
            };
            let (current, other) = if covers {
                (subfolders, only)
            } else {
                (only, subfolders)
            };
            assert!(
                is_already_chosen(&click(views::sources_view(&state), current)),
                "{locale:?} covers={covers}: the current choice is chosen"
            );
            let sent = click(views::sources_view(&state), other);
            let expected = if covers {
                matches!(sent.as_slice(), [Message::AskNarrowFolder(id)] if id == "s1")
            } else {
                matches!(sent.as_slice(), [Message::WidenFolder(id)] if id == "s1")
            };
            assert!(expected, "{locale:?} covers={covers}: {sent:?}");
        }
    }
}

/// The search row: both scopes for a folder with subfolders; a "this folder
/// only" folder has only its own scope, chosen, and nothing to switch to.
#[test]
fn the_search_row_shows_both_scopes_with_the_current_one_chosen() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let subfolders = tr(locale, MessageKey::SearchScopeSubfolders);
        let only = tr(locale, MessageKey::SearchScopeOnly);
        let mut state = AppState {
            locale,
            active_view: ViewId::Search,
            sources: vec![card(true)],
            ..AppState::default()
        };
        state.search_location.selected = Some(
            SearchLocation::remembered(SourceId::from_string("s1".to_string()), "Docs")
                .with_scope(SearchFolderScope::FolderOnly),
        );
        assert!(is_already_chosen(&click(views::search_view(&state), only)));
        assert!(matches!(
            click(views::search_view(&state), subfolders).as_slice(),
            [Message::SearchScopeChanged(
                SearchFolderScope::FolderAndSubfolders
            )]
        ));

        // A folder set to "this folder only": one option, chosen, no other.
        state.sources = vec![card(false)];
        state.update(&Message::SourceCardsRefreshed(vec![card(false)]));
        assert!(is_already_chosen(&click(views::search_view(&state), only)));
        assert!(
            simulator(views::search_view(&state))
                .find(subfolders)
                .is_err(),
            "{locale:?}: nothing to switch to"
        );
    }
}

/// The three switches on the Settings page: each sends its message, whichever
/// end of it is pressed (the switch or its label), and no "On"/"Off" button
/// remains.
#[test]
fn every_setting_switch_toggles_from_either_end() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for on in [true, false] {
            let state = AppState {
                locale,
                active_view: ViewId::Settings,
                reduced_motion: on,
                remember_recent_searches: on,
                show_advanced: on,
                ..AppState::default()
            };
            for at in [0.05, 0.95] {
                let view = || views::settings_view(&state);
                assert!(
                    matches!(
                        toggle(view(), "reduce-motion", at).as_slice(),
                        [Message::SetReducedMotion(v)] if *v == !on
                    ),
                    "{locale:?} at {at}"
                );
                assert!(matches!(
                    toggle(view(), "remember-recent-searches", at).as_slice(),
                    [Message::ToggleRememberRecentSearches(v)] if *v == !on
                ));
                assert!(matches!(
                    toggle(view(), "advanced-view", at).as_slice(),
                    [Message::ToggleAdvanced]
                ));
            }
            let mut ui = simulator(views::settings_view(&state));
            assert!(ui.find("On").is_err() && ui.find("Off").is_err());
        }
    }
}

/// The component itself, at 450 px with text size Larger: the switch and its
/// label stay one widget, so the label wrapping onto a second line never
/// separates it from the switch.
#[test]
fn a_switch_and_its_wrapped_label_stay_one_control() {
    let _guard = iced_test_guard();
    let tokens = Theme::Light.tokens();
    let label = "A long label that will wrap onto more than one line in a narrow window";
    let view = switch(&tokens, iced::Pixels(20.0), "long", label, false, |v| {
        Message::SetReducedMotion(v)
    });
    let mut ui = iced_test::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(300.0, 400.0),
        view,
    );
    let bounds = ui
        .find(crate::components::switch_id("long"))
        .unwrap()
        .bounds();
    assert!(bounds.height > 40.0, "the label wrapped: {bounds:?}");
    for (x, y) in [(0.05, 0.1), (0.95, 0.9)] {
        ui.point_at(iced::Point::new(
            bounds.x + bounds.width * x,
            bounds.y + bounds.height * y,
        ));
        ui.simulate(iced_test::simulator::click());
    }
    let sent: Vec<Message> = ui.into_messages().collect();
    assert_eq!(
        sent.len(),
        2,
        "both ends of the wrapped control toggle: {sent:?}"
    );
}

/// Both switch colour pairs keep a 3:1 contrast (WCAG 1.4.11) in every preset:
/// the track against the surface, and the knob against the track.
#[test]
fn the_switch_colours_meet_contrast_in_every_preset() {
    use snora::design::contrast::contrast_ratio;
    for tokens in [
        Theme::Light.tokens(),
        Theme::Dark.tokens(),
        Theme::HighContrastLight.tokens(),
        Theme::HighContrastDark.tokens(),
    ] {
        let p = &tokens.palette;
        for (track, knob) in [(p.accent, p.accent_text), (p.text_secondary, p.surface)] {
            assert!(contrast_ratio(track, p.surface) >= 3.0, "track vs surface");
            assert!(contrast_ratio(knob, track) >= 3.0, "knob vs track");
        }
    }
}
