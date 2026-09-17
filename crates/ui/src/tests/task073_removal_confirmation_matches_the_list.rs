//! Task 073 §3 test 3: a removal confirmation for a folder that is not in
//! the list is not visible, cannot be confirmed, and does not render.

use crate::i18n::{Locale, MessageKey, tr};
use crate::shell::{OrbokApp, key_to_message};
use crate::state::{AppState, Message, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn on_folders() -> AppState {
    let mut state = AppState::default();
    state.update(&Message::Switch(ViewId::Sources));
    state
}

fn enter(state: &AppState) -> Option<Message> {
    let context = OrbokApp::with_state(state.clone()).keyboard_context();
    key_to_message(&Key::Named(Named::Enter), Modifiers::empty(), &context)
}

#[test]
fn a_confirmation_for_a_folder_not_in_the_list_is_hidden_and_unconfirmable() {
    let _guard = iced_test_guard();
    let mut state = on_folders();
    state.confirm_remove_source = Some("src-gone".into());

    assert_eq!(
        state.visible_confirmation(),
        None,
        "no card, no visible dialog"
    );
    assert!(
        !matches!(enter(&state), Some(Message::ConfirmRemoveSource)),
        "Enter must not confirm a dialog that is not on screen"
    );
    let mut ui = simulator(views::sources_view(&state));
    assert!(
        ui.find(tr(Locale::En, MessageKey::SourceRemoveConfirm))
            .is_err(),
        "no dialog renders"
    );
}

#[test]
fn asking_to_remove_a_folder_not_in_the_list_opens_nothing() {
    let mut state = on_folders();
    state.update(&Message::AskRemoveSource("src-gone".into()));
    assert_eq!(state.confirm_remove_source, None);
    assert_eq!(
        state
            .removal_target()
            .map(|c: &SourceCard| c.source_id.clone()),
        None
    );
}
