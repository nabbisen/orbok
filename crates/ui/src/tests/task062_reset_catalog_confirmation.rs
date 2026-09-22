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
use iced_test::selector::{Candidate, Text as TextTarget};
use iced_test::simulator;

fn storage_state(locale: Locale) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Storage,
        ..AppState::default()
    }
}

/// The dialog's title and the danger button share one label
/// (`MessageKey::StorageResetCatalog`), so a plain `&str` selector finds
/// the title -- the first, non-interactive match in tree order -- rather
/// than the button. This selects the `n`th match specifically (0 = title,
/// 1 = the button's own label), and clicking it lands inside the button's
/// rendered area, exactly as a user clicking the button's visible text
/// does.
fn nth_text_match(
    content: &'static str,
    n: usize,
) -> impl FnMut(Candidate<'_>) -> Option<TextTarget> {
    let mut seen = 0usize;
    move |candidate| match candidate {
        Candidate::Text {
            id,
            bounds,
            visible_bounds,
            content: found,
        } if found == content => {
            let is_match = seen == n;
            seen += 1;
            is_match.then(|| TextTarget::Raw {
                id: id.cloned(),
                bounds,
                visible_bounds,
            })
        }
        _ => None,
    }
}

/// RFC-011 §14 criterion 7's evidence, half 1: the dialog renders with
/// Cancel and the danger button, and each button sends the message it
/// names. Title and the danger button share one label
/// (`MessageKey::StorageResetCatalog`), so clicking it after finding both
/// confirms it is a real button, not just the title text repeated.
#[test]
fn the_reset_confirmation_renders_cancel_and_the_danger_button() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let mut state = storage_state(locale);
        state.update(&Message::AskResetCatalog);
        assert!(state.confirm_reset);

        let title = tr(locale, MessageKey::StorageResetCatalog);
        let warning = tr(locale, MessageKey::StorageResetWarning);
        let cancel = tr(locale, MessageKey::Cancel);

        let mut ui = simulator(views::storage_view(&state));
        for text in [title, warning, cancel] {
            assert!(ui.find(text).is_ok(), "{locale:?}: {text:?} renders");
        }
        // The title is the 0th match; the button's own label is the 1st.
        assert!(
            ui.find(nth_text_match(title, 1)).is_ok(),
            "{locale:?}: the danger button's label is a second, distinct match"
        );
        let _ = ui.click(nth_text_match(title, 1));
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
