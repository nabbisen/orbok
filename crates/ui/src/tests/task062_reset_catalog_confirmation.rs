//! Task 062: resetting the catalog asks first, with Cancel and a danger
//! button -- not the typed-word confirmation RFC-011 §9 originally asked
//! for. Written for Task 086 (RFC-011 §9's amendment), which needs this as
//! evidence: Task 062 built and shipped this dialog, but no test rendered
//! it directly, or proved Escape resets nothing, until now.

use crate::i18n::{Locale, MessageKey, fmt_reset_removes, tr};
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::ViewId;
use crate::state::{AppState, Message, ResetCounts};
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

/// Task 092 test 4: no counts, no line -- before they arrive
/// (`AskResetCatalog` alone never populates `reset_counts`; the router's
/// `Task::perform` does that separately) and when the read failed. Never
/// a placeholder zero either way.
#[test]
fn no_counts_no_line_before_they_arrive_or_on_failure() {
    let _guard = iced_test_guard();
    let mut state = storage_state(Locale::En);
    state.update(&Message::AskResetCatalog);
    assert_eq!(
        state.reset_counts, None,
        "AskResetCatalog alone does not populate counts"
    );

    let some_counts = fmt_reset_removes(Locale::En, 1, 1, false);
    let zero_counts = fmt_reset_removes(Locale::En, 0, 0, false);
    {
        let mut ui = simulator(views::storage_view(&state));
        assert!(
            ui.find(zero_counts.as_str()).is_err(),
            "never a placeholder zero while counts are missing"
        );
        // Nothing that looks like the reset-removes line at all.
        assert!(ui.find(some_counts.as_str()).is_err());
    }

    state.update(&Message::ResetCountsFailed);
    assert_eq!(state.reset_counts, None, "a failed read leaves it None");
    let mut ui = simulator(views::storage_view(&state));
    assert!(ui.find(some_counts.as_str()).is_err());
}

/// Task 092 test: once the counts arrive, the line renders with the exact
/// owner-approved text, and disappears again on close (Cancel or a fresh
/// `AskResetCatalog`), never lingering stale from a previous opening.
#[test]
fn the_line_renders_the_exact_counts_once_they_arrive() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let mut state = storage_state(locale);
        state.update(&Message::AskResetCatalog);
        state.update(&Message::ResetCountsReady(ResetCounts {
            folders: 3,
            files: 12,
            history: 0,
        }));
        assert_eq!(
            state.reset_counts,
            Some(ResetCounts {
                folders: 3,
                files: 12,
                history: 0,
            })
        );

        let expected = fmt_reset_removes(locale, 3, 12, false);
        {
            let mut ui = simulator(views::storage_view(&state));
            assert!(
                ui.find(expected.as_str()).is_ok(),
                "{locale:?}: {expected:?} renders"
            );
        }

        state.update(&Message::CancelResetCatalog);
        assert_eq!(state.reset_counts, None, "closing clears it, not stale");
    }
}

/// Task 094 test 4: the history clause appears only when both halves say
/// it should -- a non-zero count is not enough on its own if the setting
/// is off (a stale list from before it was turned off), and the setting
/// being on is not enough on its own if there is genuinely nothing to
/// clear. Both locales.
#[test]
fn the_history_clause_appears_only_when_on_and_non_empty() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for (remember, history, expect_clause) in [
            (true, 3, true),
            (true, 0, false),
            (false, 3, false),
            (false, 0, false),
        ] {
            let mut state = storage_state(locale);
            state.remember_recent_searches = remember;
            state.update(&Message::AskResetCatalog);
            state.update(&Message::ResetCountsReady(ResetCounts {
                folders: 1,
                files: 1,
                history,
            }));

            let expected = fmt_reset_removes(locale, 1, 1, expect_clause);
            let mut ui = simulator(views::storage_view(&state));
            assert!(
                ui.find(expected.as_str()).is_ok(),
                "{locale:?} remember={remember} history={history}: \
                 expected {expected:?} (clause={expect_clause}) to render"
            );
        }
    }
}

/// Task 092 test 5: Reset confirms immediately, whether or not the counts
/// have arrived -- the button and Enter are never gated on them.
#[test]
fn reset_confirms_immediately_with_or_without_the_line() {
    let mut state = storage_state(Locale::En);
    state.update(&Message::AskResetCatalog);
    assert_eq!(state.reset_counts, None, "counts have not arrived yet");

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
    assert!(
        matches!(
            key_to_message(&Key::Named(Named::Enter), Modifiers::default(), &ctx),
            Some(Message::ConfirmResetCatalog)
        ),
        "Enter confirms even though the line has not appeared yet"
    );
}
