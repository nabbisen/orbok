//! Task 105: the controls behind choosing a folder emit the messages that do
//! something. Enter in the path field submits the typed path (it no longer
//! opens the picker), and "Choose a folder" is a control.

use crate::i18n::{MessageKey, tr};
use crate::state::{AppState, Message};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, key::Named};
use iced_test::simulator;

/// Enter in the Folders page's path field sends `SubmitSourcePath`, and not
/// `RequestAddSource` (which is the Add folder button's message).
#[test]
fn enter_in_the_path_field_submits_the_typed_path() {
    let _guard = iced_test_guard();
    let state = AppState {
        source_path_input: "/tmp/notes".into(),
        ..AppState::default()
    };
    let mut ui = simulator(views::sources_view(&state));
    let _ = ui.click("/tmp/notes");
    ui.tap_key(Key::Named(Named::Enter));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::SubmitSourcePath)),
        "Enter submits the typed path, got {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::RequestAddSource)),
        "Enter must not open the picker"
    );
}

/// "Choose a folder" on the search page's no-folder line is a control that
/// opens the picker (`ChooseSearchFolder`), in both locales.
#[test]
fn choose_a_folder_is_a_control() {
    let _guard = iced_test_guard();
    for locale in crate::i18n::Locale::ALL {
        let state = AppState {
            locale: *locale,
            ..AppState::default()
        };
        let mut ui = simulator(views::search_view(&state));
        let _ = ui.click(tr(*locale, MessageKey::SearchChooseFolder));
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::ChooseSearchFolder)),
            "{locale:?}: pressing 'Choose a folder' asks for the picker, got {messages:?}"
        );
    }
}

/// While a picker is open the control is disabled (Task 047's rule): pressing
/// it sends nothing.
#[test]
fn choose_a_folder_is_disabled_while_a_picker_is_open() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.search_location.picker_in_progress = true;
    let mut ui = simulator(views::search_view(&state));
    let _ = ui.click(tr(state.locale, MessageKey::SearchChooseFolder));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.is_empty(), "got {messages:?}");
}
