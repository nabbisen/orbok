//! Review 299 §3.1: the search row's folder chip is snora's two-part chip
//! (RFC-045 §7.3): the label **changes** the folder, the `×` **clears** it and
//! keeps the search text; always outlined; the `×` target is at least 24 × 24;
//! the `×` has the approved tooltip, in both locales.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::location::{SearchFolderScope, SearchLocation};
use crate::state::{AppState, Message, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced::{Point, Rectangle, Size};
use iced_test::Simulator;
use iced_test::selector::Candidate;

fn state(locale: Locale) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    state.update(&Message::Switch(ViewId::Search));
    state.update(&Message::QueryChanged("meeting notes".into()));
    state.update(&Message::SearchLocationSelected(
        SearchLocation::remembered(orbok_core::id::SourceId::generate(), "Docs")
            .with_scope(SearchFolderScope::FolderAndSubfolders),
    ));
    state
}

fn x() -> String {
    char::from(snora::lucide::X).to_string()
}

/// §3.1: the `×` clears the folder and **keeps the query**.
#[test]
fn the_x_clears_the_folder_and_keeps_the_search_text() {
    let mut state = state(Locale::En);
    assert!(state.search_location.selected.is_some());
    state.update(&Message::SearchLocationCleared);
    assert!(
        state.search_location.selected.is_none(),
        "the folder is cleared"
    );
    assert_eq!(state.query, "meeting notes", "the search text stays");
}

/// §3.1: the label opens the picker (the same one "Choose a folder" opens), in
/// both locales; while a picker is already open it does nothing.
#[test]
fn the_label_opens_the_picker_and_does_nothing_while_one_is_open() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let state = state(*locale);
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            Size::new(900.0, 400.0),
            views::search_view(&state),
        );
        let _ = ui.click("Docs");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            matches!(messages.as_slice(), [Message::ChooseSearchFolder]),
            "{locale:?}: {messages:?}"
        );
    }
    let mut open = state(Locale::En);
    open.search_location.picker_in_progress = true;
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        Size::new(900.0, 400.0),
        views::search_view(&open),
    );
    let _ = ui.click("Docs");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.is_empty(), "no second picker: {messages:?}");
}

/// §3.1: the `×` target is at least 24 × 24 (WCAG 2.5.8), measured on the real
/// button: the outermost container that holds the glyph's centre and is no wider
/// than a control, in every text scale.
#[test]
fn the_x_target_is_at_least_24_by_24() {
    let _guard = iced_test_guard();
    for scale in crate::theme::TextScale::ALL {
        let mut state = state(Locale::En);
        state.text_scale = *scale;
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            Size::new(900.0, 400.0),
            views::search_view(&state),
        );
        let glyph = x();
        let centre: Point = ui.find(glyph.as_str()).unwrap().bounds().center();
        let target: Rectangle = ui
            .find(move |candidate: Candidate<'_>| match candidate {
                Candidate::Container { bounds, .. }
                    if bounds.contains(centre) && bounds.width <= 60.0 =>
                {
                    Some(bounds)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            target.width >= 24.0 - 0.01 && target.height >= 24.0 - 0.01,
            "{scale:?}: the × target is {} × {}",
            target.width,
            target.height
        );
    }
}

/// §3.1: both locales show the approved tooltip words. The tooltip is drawn as an
/// overlay the simulator does not see, so the wording is asserted on the catalog.
#[test]
fn the_x_has_the_approved_tooltip_in_both_locales() {
    assert_eq!(
        tr(Locale::En, MessageKey::SearchLocationClear),
        "Clear this folder"
    );
    assert_eq!(
        tr(Locale::Ja, MessageKey::SearchLocationClear),
        "このフォルダーの選択を解除"
    );
}

/// §3.1: always outlined -- a chip is not a choice (Task 118). The one place the
/// chip is built passes `selected = false`.
#[test]
fn the_chip_is_always_outlined() {
    let source = include_str!("../components.rs");
    let from = source.find("pub fn removable_chip").unwrap();
    let body = &source[from..from + source[from..].find("\n}\n").unwrap()];
    assert!(
        body.contains("removable_with_tooltip(") && body.contains("        false,\n"),
        "removable_chip must pass selected = false"
    );
}
