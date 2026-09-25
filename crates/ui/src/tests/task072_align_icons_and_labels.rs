//! Task 072 §4 tests 2-3: every notice offers dismiss, and the location
//! chips are found by their labels.

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::shell::OrbokApp;
use crate::state::location::{SearchFolderScope, SearchLocation};
use crate::state::{AppState, Message, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

/// The dismiss control snora's `Notice` renders: since snora 0.51, with the
/// `lucide-icons` feature, a lucide `X` (before that, the text "×").
fn dismiss() -> String {
    char::from(snora::lucide::X).to_string()
}

/// §4 test 2: a notice with a stored action shows the action **and** the
/// dismiss control; dismiss sends `ClearNotice`.
#[test]
fn a_notice_with_an_action_also_offers_dismiss() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::SourceCouldNotBeRemoved,
        action: Box::new(Message::AskRemoveSource("src".into())),
    });
    let app = OrbokApp::with_state(state.clone());
    let mut ui = simulator(app.view());
    let try_again = tr(Locale::En, MessageKey::NoticeActionTryAgain);
    assert!(ui.find(try_again).is_ok(), "the action renders");
    let dismiss = dismiss();
    assert!(
        ui.find(dismiss.as_str()).is_ok(),
        "dismiss renders beside the action"
    );
    let _ = ui.click(dismiss.as_str());
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::ClearNotice]),
        "dismiss sends ClearNotice, got {messages:?}"
    );
}

fn with_location(scope: SearchFolderScope) -> AppState {
    let mut state = AppState::default();
    state.update(&Message::Switch(ViewId::Search));
    state.update(&Message::SearchLocationSelected(
        SearchLocation::remembered(orbok_core::id::SourceId::generate(), "Docs").with_scope(scope),
    ));
    state
}

fn clicked(state: &AppState, label: &str) -> Vec<Message> {
    let mut ui = simulator(views::search_view(state));
    assert!(ui.find(label).is_ok(), "{label:?} is found");
    let _ = ui.click(label);
    ui.into_messages().collect()
}

/// §4 test 3: the location chip is found by its label and removes the
/// location; the scope toggle is found by its label and changes scope.
#[test]
fn the_location_chip_and_the_scope_toggle_are_found_by_their_labels() {
    let _guard = iced_test_guard();
    let state = with_location(SearchFolderScope::FolderAndSubfolders);
    // Task 118: the chip is the folder's own name; the scope is a choice beside it.
    let messages = clicked(&state, "Docs");
    assert!(
        matches!(messages.as_slice(), [Message::SearchLocationCleared]),
        "the chip removes the location, got {messages:?}"
    );

    let toggle = tr(Locale::En, MessageKey::SearchScopeOnly);
    let messages = clicked(&state, toggle);
    assert!(
        matches!(
            messages.as_slice(),
            [Message::SearchScopeChanged(SearchFolderScope::FolderOnly)]
        ),
        "the toggle switches scope, got {messages:?}"
    );
}
