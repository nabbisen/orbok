//! Task 106: no control is pushed off-screen. At 450 px wide with text size
//! Larger, in both locales, every control named below lies inside the window
//! and is not clipped. The simulator lays the real view out at that size and
//! reports each element's bounds, so "reachable" is asserted, not eyeballed;
//! the same check would cover any control whose label is listed.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, FileCountState, ViewId};
use crate::tests::iced_test_guard;
use crate::theme::{TextScale, Theme};
use crate::views;
use iced::{Element, Size};
use iced_test::Simulator;

const WIDTH: f32 = 450.0;

fn state(locale: Locale, view: ViewId) -> AppState {
    AppState {
        locale,
        active_view: view,
        show_advanced: true,
        text_scale: TextScale::Larger,
        ..AppState::default()
    }
}

/// Every label is found, lies inside `[0, WIDTH]`, and is not clipped
/// (`visible_bounds` equals `bounds`).
fn assert_reachable(view: Element<'_, crate::state::Message>, labels: &[String], what: &str) {
    let mut ui = Simulator::with_size(iced::Settings::default(), Size::new(WIDTH, 1400.0), view);
    for label in labels {
        let found = ui
            .find(label.as_str())
            .unwrap_or_else(|_| panic!("{what}: {label:?} is not on the page"));
        let bounds = found.bounds();
        assert!(
            bounds.x >= -0.5 && bounds.x + bounds.width <= WIDTH + 0.5,
            "{what}: {label:?} is pushed outside the {WIDTH} px window: x {} .. {}",
            bounds.x,
            bounds.x + bounds.width
        );
        let visible = found.visible_bounds();
        assert!(
            visible.is_some_and(|v| (v.width - bounds.width).abs() < 0.5),
            "{what}: {label:?} is clipped: bounds {bounds:?}, visible {visible:?}"
        );
    }
}

fn labels(locale: Locale, keys: &[MessageKey]) -> Vec<String> {
    keys.iter().map(|k| tr(locale, *k).to_string()).collect()
}

/// The search page: the Search button, the mode row, "Choose a folder".
#[test]
fn the_search_page_keeps_its_controls_in_a_narrow_window() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let s = state(locale, ViewId::Search);
        assert_reachable(
            views::search_view(&s),
            &labels(
                locale,
                &[
                    MessageKey::SearchButton,
                    MessageKey::SearchModeAuto,
                    MessageKey::SearchModeExact,
                    MessageKey::SearchModeConceptual,
                    MessageKey::SearchChooseFolder,
                    MessageKey::SourcesAddFolder,
                ],
            ),
            &format!("search, {locale:?}"),
        );
    }
}

/// Settings: every language, theme and text-size choice, and the toggles.
#[test]
fn the_settings_page_keeps_its_controls_in_a_narrow_window() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let s = state(locale, ViewId::Settings);
        let mut wanted: Vec<String> = Theme::ALL
            .iter()
            .map(|t| tr(locale, t.label_key()).to_string())
            .collect();
        wanted.extend(
            TextScale::ALL
                .iter()
                .map(|t| tr(locale, t.label_key()).to_string()),
        );
        wanted.extend(Locale::ALL.iter().map(|l| l.display_name().to_string()));
        wanted.extend(labels(locale, &[MessageKey::ClearRecentSearches]));
        assert_reachable(
            views::settings_view(&s),
            &wanted,
            &format!("settings, {locale:?}"),
        );
        // Task 118: the three switches are single widgets (their label is inside
        // them, so it is not a text a selector finds); each lies inside the window.
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            Size::new(WIDTH, 1400.0),
            views::settings_view(&s),
        );
        for name in ["reduce-motion", "remember-recent-searches", "advanced-view"] {
            let bounds = ui
                .find(crate::components::switch_id(name))
                .unwrap_or_else(|_| panic!("settings, {locale:?}: the {name} switch"))
                .bounds();
            assert!(
                bounds.x >= -0.5 && bounds.x + bounds.width <= WIDTH + 0.5,
                "settings, {locale:?}: the {name} switch lies outside the window: {bounds:?}"
            );
        }
    }
}

/// The Preparing page's count cells, all four in Advanced view.
#[test]
fn the_preparing_page_keeps_its_counts_in_a_narrow_window() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let s = state(locale, ViewId::Indexing);
        let mut wanted = labels(locale, &[MessageKey::IndexingHealthQueued]);
        wanted.extend(
            [
                FileCountState::Ready,
                FileCountState::NeedsUpdate,
                FileCountState::Failed,
            ]
            .iter()
            .map(|c| tr(locale, c.label_key()).to_string()),
        );
        assert_reachable(
            views::indexing_view(&s),
            &wanted,
            &format!("preparing, {locale:?}"),
        );
    }
}

/// The Folders page's Add folder button, and (Task 118) a card's two coverage
/// options, both shown whichever is chosen.
#[test]
fn the_folders_page_keeps_its_controls_in_a_narrow_window() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let mut s = state(locale, ViewId::Sources);
        s.sources.push(crate::state::SourceCard {
            display_name: "Documents".into(),
            display_path: "/home/user/Documents".into(),
            indexed: 3,
            stale: 0,
            failed: 0,
            no_text_found: 0,
            unfinished_jobs: 0,
            status: orbok_core::SourceStatus::Active,
            source_id: "s1".into(),
            covers_subfolders: true,
        });
        assert_reachable(
            views::sources_view(&s),
            &labels(
                locale,
                &[
                    MessageKey::SourcesAddFolder,
                    MessageKey::SearchScopeSubfolders,
                    MessageKey::SearchScopeOnly,
                ],
            ),
            &format!("folders, {locale:?}"),
        );
    }
}
