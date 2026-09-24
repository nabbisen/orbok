//! Task 099 §5 test 3: the two rebuild confirmations, both locales, exact
//! copy, the counted line present with a count and absent without, Escape
//! cancels and deletes nothing, Enter confirms only while visible.

use crate::i18n::{Locale, MessageKey, fmt_rebuild_prepares, tr};
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::ViewId;
use crate::state::{AppState, Message};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;
use orbok_models::SearchCapability;

fn storage_state(locale: Locale) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Storage,
        show_advanced: true,
        capability: SearchCapability::Hybrid,
        ..AppState::default()
    }
}

fn ctx_for(state: &AppState) -> KeyboardContext {
    KeyboardContext {
        text_input_focused: false,
        active_view: state.active_view,
        confirm_reset: state.confirm_reset,
        confirm_remove_source: state.confirm_remove_source.is_some(),
        confirm_clear_history: state.confirm_clear_history,
        confirm_delete_keyword_index: state.confirm_delete_keyword_index,
        confirm_delete_vector_index: state.confirm_delete_vector_index,
        confirm_add_sensitive_folder: state.pending_folder_add.is_some(),
        confirm_narrow_folder: false,
        wizard_kind: None,
        selected_source_id: None,
        selected_result: None,
    }
}

/// Both buttons render in Advanced view, and each opens its own
/// confirmation with distinct title/button text.
#[test]
fn the_advanced_view_buttons_open_their_own_confirmations() {
    let _guard = iced_test_guard();
    let state = storage_state(Locale::En);
    let mut ui = simulator(views::storage_view(&state));
    let _ = ui.click(tr(Locale::En, MessageKey::StorageRebuildKeywordButton));
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::AskDeleteKeywordIndex)),
        "the keyword button sends AskDeleteKeywordIndex"
    );

    let mut ui = simulator(views::storage_view(&state));
    let _ = ui.click(tr(Locale::En, MessageKey::StorageRebuildVectorButton));
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::AskDeleteVectorIndex)),
        "the meaning button sends AskDeleteVectorIndex"
    );
}

/// The "search by meaning" button does not render at all on a keyword-only
/// install -- there is no vector index to rebuild.
#[test]
fn the_meaning_button_is_absent_without_a_model() {
    let _guard = iced_test_guard();
    let mut state = storage_state(Locale::En);
    state.capability = SearchCapability::KeywordOnly;
    let mut ui = simulator(views::storage_view(&state));
    assert!(
        ui.find(tr(Locale::En, MessageKey::StorageRebuildVectorButton))
            .is_err(),
        "no model configured -- nothing to rebuild"
    );
    assert!(
        ui.find(tr(Locale::En, MessageKey::StorageRebuildKeywordButton))
            .is_ok(),
        "keyword search always works, so its button still renders"
    );
}

/// Both dialogs, both locales: exact copy, Cancel and the danger button,
/// each sending the message it names.
#[test]
fn each_confirmation_renders_its_exact_copy_and_buttons() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for (ask, title_key, cancel_msg, confirm_msg) in [
            (
                Message::AskDeleteKeywordIndex,
                MessageKey::RebuildKeywordConfirmTitle,
                Message::CancelDeleteKeywordIndex,
                Message::ConfirmDeleteKeywordIndex,
            ),
            (
                Message::AskDeleteVectorIndex,
                MessageKey::RebuildVectorConfirmTitle,
                Message::CancelDeleteVectorIndex,
                Message::ConfirmDeleteVectorIndex,
            ),
        ] {
            let mut state = storage_state(locale);
            state.update(&ask);

            let title = tr(locale, title_key);
            let body = tr(locale, MessageKey::RebuildConfirmBody);
            let cancel = tr(locale, MessageKey::Cancel);
            let confirm = tr(locale, MessageKey::RebuildConfirm);

            let mut ui = simulator(views::storage_view(&state));
            for text in [title, body, cancel, confirm] {
                assert!(
                    ui.find(text).is_ok(),
                    "{locale:?} {ask:?}: {text:?} renders"
                );
            }
            let _ = ui.click(confirm);
            let sent: Vec<Message> = ui.into_messages().collect();
            assert!(
                sent.iter()
                    .any(|m| std::mem::discriminant(m) == std::mem::discriminant(&confirm_msg)),
                "{locale:?} {ask:?}: the danger button sends {confirm_msg:?}, got {sent:?}"
            );

            let mut ui = simulator(views::storage_view(&state));
            let _ = ui.click(cancel);
            let sent: Vec<Message> = ui.into_messages().collect();
            assert!(
                sent.iter()
                    .any(|m| std::mem::discriminant(m) == std::mem::discriminant(&cancel_msg)),
                "{locale:?} {ask:?}: Cancel sends {cancel_msg:?}, got {sent:?}"
            );
        }
    }
}

/// §2.3: no count, no line -- before it arrives, on a failed read, and
/// when the count is genuinely zero ("never a zero").
#[test]
fn no_count_no_line_before_it_arrives_on_failure_or_when_zero() {
    let _guard = iced_test_guard();
    let mut state = storage_state(Locale::En);
    state.update(&Message::AskDeleteKeywordIndex);
    assert_eq!(state.rebuild_file_count, None);

    let some_line = fmt_rebuild_prepares(Locale::En, 3);
    {
        let mut ui = simulator(views::storage_view(&state));
        assert!(ui.find(some_line.as_str()).is_err());
    }

    state.update(&Message::RebuildCountsFailed);
    assert_eq!(
        state.rebuild_file_count, None,
        "a failed read leaves it None"
    );

    state.update(&Message::RebuildCountsReady(0));
    assert_eq!(
        state.rebuild_file_count, None,
        "a genuinely zero count still shows no line"
    );
    let zero_line = fmt_rebuild_prepares(Locale::En, 0);
    let mut ui = simulator(views::storage_view(&state));
    assert!(ui.find(zero_line.as_str()).is_err());
}

/// Once a non-zero count arrives, the line renders with the exact
/// owner-approved text, and clears again on close.
#[test]
fn the_line_renders_the_exact_count_once_it_arrives() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let mut state = storage_state(locale);
        state.update(&Message::AskDeleteVectorIndex);
        state.update(&Message::RebuildCountsReady(7));
        assert_eq!(state.rebuild_file_count, Some(7));

        let expected = fmt_rebuild_prepares(locale, 7);
        {
            let mut ui = simulator(views::storage_view(&state));
            assert!(
                ui.find(expected.as_str()).is_ok(),
                "{locale:?}: {expected:?} renders"
            );
        }

        state.update(&Message::CancelDeleteVectorIndex);
        assert_eq!(
            state.rebuild_file_count, None,
            "closing clears it, not stale"
        );
    }
}

/// Escape cancels either dialog and deletes nothing -- the only route to
/// an actual delete is the `ConfirmDelete*` messages reaching the backend,
/// which Escape never sends.
#[test]
fn escape_cancels_either_confirmation_and_deletes_nothing() {
    for ask in [
        Message::AskDeleteKeywordIndex,
        Message::AskDeleteVectorIndex,
    ] {
        let mut state = storage_state(Locale::En);
        state.update(&ask);

        let escape = key_to_message(
            &Key::Named(Named::Escape),
            Modifiers::default(),
            &ctx_for(&state),
        );
        assert!(
            matches!(escape, Some(Message::DismissOverlay)),
            "{ask:?}: Escape must dismiss, not confirm, got {escape:?}"
        );
        state.update(&escape.unwrap());
        assert!(
            !state.confirm_delete_keyword_index && !state.confirm_delete_vector_index,
            "{ask:?}: the dialog closes"
        );
    }
}

/// Enter confirms only while the matching dialog is visible -- never when
/// neither is open.
#[test]
fn enter_confirms_only_the_visible_rebuild_dialog() {
    let mut state = storage_state(Locale::En);
    let neutral_ctx = ctx_for(&state);
    assert!(
        key_to_message(
            &Key::Named(Named::Enter),
            Modifiers::default(),
            &neutral_ctx
        )
        .is_none(),
        "Enter with neither dialog open confirms nothing on Storage"
    );

    state.update(&Message::AskDeleteKeywordIndex);
    assert!(
        matches!(
            key_to_message(
                &Key::Named(Named::Enter),
                Modifiers::default(),
                &ctx_for(&state)
            ),
            Some(Message::ConfirmDeleteKeywordIndex)
        ),
        "Enter confirms the visible keyword dialog"
    );

    state.update(&Message::CancelDeleteKeywordIndex);
    state.update(&Message::AskDeleteVectorIndex);
    assert!(
        matches!(
            key_to_message(
                &Key::Named(Named::Enter),
                Modifiers::default(),
                &ctx_for(&state)
            ),
            Some(Message::ConfirmDeleteVectorIndex)
        ),
        "Enter confirms the visible meaning dialog"
    );
}
