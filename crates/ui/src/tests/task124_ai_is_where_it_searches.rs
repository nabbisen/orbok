//! Task 124: AI is where it searches. Sidebar: **Search** (Search), **AI**
//! (Folders · Preparing · Models, opening on Folders), **Settings** (Settings ·
//! Storage). The group is derived from the page, the tab bars are built from one
//! list, and a group with one page shows no tab bar.

use crate::i18n::{Locale, MessageKey, tr};
use crate::shell::OrbokApp;
use crate::state::{AppState, NavGroup, ViewId};
use crate::tests::iced_test_guard;
use iced_test::simulator;

/// §2.1: group membership, for every `ViewId`, and the order in each group.
#[test]
fn every_page_is_in_the_decided_group_in_the_decided_order() {
    let expected = [
        (ViewId::Search, NavGroup::Search),
        (ViewId::Sources, NavGroup::Ai),
        (ViewId::Indexing, NavGroup::Ai),
        (ViewId::Models, NavGroup::Ai),
        (ViewId::Settings, NavGroup::Settings),
        (ViewId::Storage, NavGroup::Settings),
    ];
    assert_eq!(expected.len(), ViewId::ALL.len(), "every ViewId is decided");
    for (view, group) in expected {
        assert_eq!(view.group(), group, "{view:?}");
    }
    assert_eq!(ViewId::pages(NavGroup::Search), [ViewId::Search]);
    assert_eq!(
        ViewId::pages(NavGroup::Ai),
        [ViewId::Sources, ViewId::Indexing, ViewId::Models]
    );
    assert_eq!(
        ViewId::pages(NavGroup::Settings),
        [ViewId::Settings, ViewId::Storage]
    );
    // The two views of the same fact agree.
    for group in [NavGroup::Search, NavGroup::Ai, NavGroup::Settings] {
        for page in ViewId::pages(group) {
            assert_eq!(page.group(), group, "{page:?}");
        }
    }
}

/// §2.2: each group opens on its first page; AI on Folders.
#[test]
fn each_group_opens_on_its_first_page() {
    assert_eq!(ViewId::group_default(NavGroup::Search), ViewId::Search);
    assert_eq!(ViewId::group_default(NavGroup::Ai), ViewId::Sources);
    assert_eq!(ViewId::group_default(NavGroup::Settings), ViewId::Settings);
    for group in [NavGroup::Search, NavGroup::Ai, NavGroup::Settings] {
        assert_eq!(ViewId::group_default(group), ViewId::pages(group)[0]);
    }
    let mut state = AppState::default();
    state.update(&crate::state::Message::SwitchGroup(NavGroup::Ai));
    assert_eq!(
        state.active_view,
        ViewId::Sources,
        "pressing AI opens Folders"
    );
}

/// x of the first text with this exact content, or `None`.
fn x_of(app: &OrbokApp, text: &str) -> Option<f32> {
    let mut ui = simulator(app.view());
    ui.find(text).ok().map(|found| found.bounds().x)
}

fn app_on(locale: Locale, view: ViewId) -> OrbokApp {
    OrbokApp::with_state(AppState {
        locale,
        active_view: view,
        ..AppState::default()
    })
}

/// §2.3: the tab bars, in both locales: AI's three labels in order, Settings' two,
/// and none on Search.
#[test]
fn the_tab_bars_show_each_groups_pages_in_order_and_search_has_none() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let label = |view: ViewId| tr(*locale, view.label_key());
        let ordered = |app: &OrbokApp, views: &[ViewId]| {
            let xs: Vec<f32> = views
                .iter()
                .map(|v| {
                    x_of(app, label(*v))
                        .unwrap_or_else(|| panic!("{locale:?}: tab {:?} is missing", v))
                })
                .collect();
            assert!(
                xs.windows(2).all(|w| w[0] < w[1]),
                "{locale:?}: tabs {views:?} are in order, got {xs:?}"
            );
        };
        // AI: Folders, Preparing, Models -- and not Storage.
        for view in [ViewId::Sources, ViewId::Indexing, ViewId::Models] {
            let app = app_on(*locale, view);
            ordered(&app, &[ViewId::Sources, ViewId::Indexing, ViewId::Models]);
            assert!(
                x_of(&app, label(ViewId::Storage)).is_none(),
                "{locale:?}: Storage is not a tab under AI"
            );
        }
        // Settings: Settings, Storage.
        for view in [ViewId::Settings, ViewId::Storage] {
            let app = app_on(*locale, view);
            ordered(&app, &[ViewId::Settings, ViewId::Storage]);
            assert!(
                x_of(&app, label(ViewId::Models)).is_none(),
                "{locale:?}: Models is not a tab under Settings"
            );
        }
        // Search: one page, no tab bar (none of the other groups' labels appear).
        let app = app_on(*locale, ViewId::Search);
        for other in [
            ViewId::Sources,
            ViewId::Indexing,
            ViewId::Models,
            ViewId::Storage,
        ] {
            assert!(
                x_of(&app, label(other)).is_none(),
                "{locale:?}: no tab bar on Search, but {other:?} is on the page"
            );
        }
    }
    // The catalog keys the labels come from are the existing ones.
    assert_eq!(ViewId::Sources.label_key(), MessageKey::NavSources);
}
