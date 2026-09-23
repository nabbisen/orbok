//! Task 062: removing a folder asks first, and Enter never removes anything
//! by itself.

use crate::i18n::{Locale, MessageKey, fmt_remove_source_title, tr};
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::ViewId;
use crate::state::{AppState, Message, SourceCard};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn folders_ctx(selected: Option<&str>) -> KeyboardContext {
    KeyboardContext {
        text_input_focused: false,
        active_view: ViewId::Sources,
        confirm_reset: false,
        confirm_remove_source: false,
        confirm_clear_history: false,
        wizard_kind: None,
        selected_source_id: selected.map(str::to_string),
        selected_result: None,
    }
}

/// §4 test 1: Enter on Folders with a folder selected and no dialog open does
/// nothing.
#[test]
fn enter_on_a_selected_folder_removes_nothing() {
    let got = key_to_message(
        &Key::Named(Named::Enter),
        Modifiers::default(),
        &folders_ctx(Some("src-1")),
    );
    assert!(
        got.is_none(),
        "Enter must not act on a selected folder, got {got:?}"
    );
}

fn card(id: &str, name: &str) -> SourceCard {
    SourceCard {
        display_name: name.into(),
        display_path: format!("/home/user/{name}"),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: id.into(),
    }
}

fn folders_state(locale: Locale) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Sources,
        sources: vec![card("src-1", "Docs"), card("src-2", "Reports")],
        ..AppState::default()
    }
}

/// The context `main.rs` computes from state for the next key.
fn ctx_for(state: &AppState, text_input_focused: bool) -> KeyboardContext {
    KeyboardContext {
        text_input_focused,
        active_view: state.active_view,
        confirm_reset: state.confirm_reset,
        confirm_remove_source: state.confirm_remove_source.is_some(),
        confirm_clear_history: state.confirm_clear_history,
        wizard_kind: None,
        selected_source_id: state
            .selected_source
            .and_then(|i| state.sources.get(i))
            .map(|c| c.source_id.clone()),
        selected_result: None,
    }
}

fn press(state: &mut AppState, key: Named) -> Option<Message> {
    let message = key_to_message(
        &Key::Named(key),
        Modifiers::default(),
        &ctx_for(state, false),
    );
    if let Some(message) = &message {
        state.update(message);
    }
    message
}

/// §4 test 2: Delete opens the dialog for the selected folder, and Enter
/// inside it removes exactly that folder.
#[test]
fn delete_opens_the_confirmation_and_enter_confirms_it() {
    let mut state = folders_state(Locale::En);
    state.update(&Message::SelectNextSource);
    assert!(matches!(
        press(&mut state, Named::Delete),
        Some(Message::AskRemoveSource(id)) if id == "src-1"
    ));
    assert_eq!(state.confirm_remove_source.as_deref(), Some("src-1"));
    assert_eq!(state.sources.len(), 2, "opening the dialog removes nothing");

    assert!(matches!(
        press(&mut state, Named::Enter),
        Some(Message::ConfirmRemoveSource)
    ));
    assert!(matches!(
        state.take_confirmed_removal(),
        Some(Message::SourceRemoved(id)) if id == "src-1"
    ));
    assert_eq!(state.confirm_remove_source, None, "the dialog closes");
}

/// §4 test 2: Escape closes the dialog; nothing is removed.
#[test]
fn escape_cancels_the_confirmation_and_removes_nothing() {
    let mut state = folders_state(Locale::En);
    state.update(&Message::SelectNextSource);
    press(&mut state, Named::Delete);
    assert!(state.confirm_remove_source.is_some());
    assert!(matches!(
        press(&mut state, Named::Escape),
        Some(Message::DismissOverlay)
    ));
    assert_eq!(state.confirm_remove_source, None);
    assert_eq!(state.sources.len(), 2);
    assert!(
        state.take_confirmed_removal().is_none(),
        "no removal was produced"
    );
}

/// Confirmation removes the folder the dialog was opened for, even if the
/// selection moved while it was open.
#[test]
fn confirming_removes_the_folder_the_dialog_named_not_the_current_selection() {
    let mut state = folders_state(Locale::En);
    state.update(&Message::SelectNextSource); // Docs
    press(&mut state, Named::Delete);
    state.update(&Message::SelectNextSource); // selection moves to Reports
    assert_eq!(
        ctx_for(&state, false).selected_source_id.as_deref(),
        Some("src-2")
    );
    assert!(matches!(
        state.take_confirmed_removal(),
        Some(Message::SourceRemoved(id)) if id == "src-1"
    ));
}

/// §4 test 5: Delete (and Backspace) while typing never opens a removal.
#[test]
fn delete_while_typing_does_nothing() {
    for key in [Named::Delete, Named::Backspace] {
        let got = key_to_message(
            &Key::Named(key),
            Modifiers::default(),
            &KeyboardContext {
                text_input_focused: true,
                ..folders_ctx(Some("src-1"))
            },
        );
        assert!(
            got.is_none(),
            "{key:?} while typing must not act, got {got:?}"
        );
    }
}

/// Backspace is the key labelled "delete" on macOS (winit maps it to
/// `Named::Backspace`), so it opens the confirmation too.
#[test]
fn backspace_opens_the_confirmation_like_delete() {
    assert!(matches!(
        key_to_message(
            &Key::Named(Named::Backspace),
            Modifiers::default(),
            &folders_ctx(Some("src-1"))
        ),
        Some(Message::AskRemoveSource(id)) if id == "src-1"
    ));
}

/// §4 test 3: the card's Remove button opens the confirmation, not removal.
#[test]
fn clicking_remove_opens_the_confirmation() {
    let _guard = iced_test_guard();
    let state = folders_state(Locale::En);
    let mut ui = simulator(views::sources_view(&state));
    let _ = ui.click(tr(Locale::En, MessageKey::SourceActionRemoveFromOrbok));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::AskRemoveSource(id)] if id == "src-1"),
        "Remove opens the confirmation for that folder, got {messages:?}"
    );
}

/// §4 test 4: the exact copy, with the folder's display name, in both
/// locales -- and its buttons send Cancel and Confirm.
#[test]
fn the_confirmation_renders_the_approved_copy() {
    let _guard = iced_test_guard();
    let expected = [
        (
            Locale::En,
            "Remove \"Docs\" from orbok?",
            "Your files are never changed or deleted. orbok removes what it prepared to search this folder, and prepares it again if you add it back.",
            "Cancel",
            "Remove",
        ),
        (
            Locale::Ja,
            "「Docs」を orbok から削除しますか?",
            "ファイルは変更も削除もされません。このフォルダーの検索の準備内容は削除され、もう一度追加すると準備し直します。",
            "キャンセル",
            "削除",
        ),
    ];
    for (locale, title, body, cancel, confirm) in expected {
        let mut state = folders_state(locale);
        state.update(&Message::AskRemoveSource("src-1".into()));
        assert_eq!(fmt_remove_source_title(locale, "Docs"), title);
        assert_eq!(tr(locale, MessageKey::SourceRemoveConfirmBody), body);
        let mut ui = simulator(views::sources_view(&state));
        for text in [title, body, cancel] {
            assert!(ui.find(text).is_ok(), "{locale:?}: {text:?} renders");
        }
        let _ = ui.click(confirm);
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::ConfirmRemoveSource)),
            "{locale:?}: the confirm button sends ConfirmRemoveSource"
        );
        let mut ui = simulator(views::sources_view(&state));
        let _ = ui.click(cancel);
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::CancelRemoveSource))
        );
    }
}

/// At most one confirmation is open, and switching views closes the removal
/// dialog so it cannot be confirmed unseen.
#[test]
fn one_confirmation_at_a_time_and_none_left_open_unseen() {
    let mut state = folders_state(Locale::En);
    state.update(&Message::AskRemoveSource("src-1".into()));
    state.update(&Message::AskResetCatalog);
    assert!(state.confirm_reset);
    assert_eq!(
        state.confirm_remove_source, None,
        "opening reset closes removal"
    );
    state.update(&Message::AskRemoveSource("src-1".into()));
    assert!(!state.confirm_reset, "opening removal closes reset");
    state.update(&Message::Switch(ViewId::Search));
    assert_eq!(state.confirm_remove_source, None);
}
