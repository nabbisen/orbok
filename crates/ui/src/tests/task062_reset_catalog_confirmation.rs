//! Task 062: resetting the catalog asks first, with Cancel and a danger
//! button -- not the typed-word confirmation RFC-011 §9 originally asked
//! for. Written for Task 086 (RFC-011 §9's amendment), which needs this as
//! evidence: Task 062 built and shipped this dialog, but no test rendered
//! it directly, or proved Escape resets nothing, until now.

use crate::i18n::{Locale, MessageKey, tr};
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::ViewId;
use crate::state::{AppState, Message};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn storage_state(locale: Locale) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Storage,
        ..AppState::default()
    }
}

/// RFC-011 §14 criterion 7's evidence, half 1: the dialog renders with
/// Cancel and the danger button, each with its own text, and each button
/// sends the message it names. Task 091: the title asks a question and the
/// button names the action -- they must differ, the way the removal
/// dialog's title/button already do (Review Request 264 §3 found them
/// identical, which forced a same-text disambiguation workaround here;
/// that workaround is gone along with the defect).
#[test]
fn the_reset_confirmation_renders_cancel_and_the_danger_button() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let mut state = storage_state(locale);
        state.update(&Message::AskResetCatalog);
        assert!(state.confirm_reset);

        let title = tr(locale, MessageKey::StorageResetConfirmTitle);
        let warning = tr(locale, MessageKey::StorageResetWarning);
        let cancel = tr(locale, MessageKey::Cancel);
        let confirm = tr(locale, MessageKey::StorageResetConfirm);
        assert_ne!(
            title, confirm,
            "{locale:?}: the title asks, the button names the action -- they must differ"
        );

        let mut ui = simulator(views::storage_view(&state));
        for text in [title, warning, cancel, confirm] {
            assert!(ui.find(text).is_ok(), "{locale:?}: {text:?} renders");
        }
        let _ = ui.click(confirm);
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::ConfirmResetCatalog)),
            "{locale:?}: the danger button sends ConfirmResetCatalog"
        );

        let mut ui = simulator(views::storage_view(&state));
        let _ = ui.click(cancel);
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::CancelResetCatalog)),
            "{locale:?}: Cancel sends CancelResetCatalog"
        );
    }
}

/// RFC-011 §14 criterion 7's evidence, half 2: Escape cancels and nothing
/// is reset -- stronger than "the flag flips false", since the only route
/// to an actual reset is `Message::ConfirmResetCatalog` reaching the
/// backend (Task 075's tests cover what happens once it does; this proves
/// Escape never sends it in the first place).
#[test]
fn escape_cancels_the_reset_confirmation_and_resets_nothing() {
    let mut state = storage_state(Locale::En);
    state.update(&Message::AskResetCatalog);
    assert!(state.confirm_reset);

    let ctx = KeyboardContext {
        text_input_focused: false,
        active_view: ViewId::Storage,
        confirm_reset: state.confirm_reset,
        confirm_remove_source: false,
        confirm_clear_history: false,
        wizard_kind: None,
        selected_source_id: None,
        selected_result: None,
    };
    let escape = key_to_message(&Key::Named(Named::Escape), Modifiers::default(), &ctx);
    assert!(
        matches!(escape, Some(Message::DismissOverlay)),
        "Escape must dismiss, not confirm, got {escape:?}"
    );

    state.update(&escape.unwrap());
    assert!(!state.confirm_reset, "the dialog closes");
}
