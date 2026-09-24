//! Task 057: a saved model that cannot be loaded says so, and "Try again"
//! loads it again.

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::{
    AppState, Message, ModelPersistenceState, ModelProvenance, ViewId, WizardKind, WizardState,
};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn load_failed_state(locale: Locale) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    let ready_id = state.model_flow_ids.allocate_ready().unwrap();
    let attempt = state.model_flow_ids.allocate_persistence_attempt().unwrap();
    state.wizard = Some(WizardState::Ready {
        ready_id,
        model_dir: "/user/model".into(),
        provenance: ModelProvenance::UserSupplied,
        persistence: ModelPersistenceState::LoadFailed(attempt),
    });
    state
}

/// §3 test 1 (view half): the load-failed step shows the load copy and never
/// the save copy, and its "Try again" asks to load the model again.
#[test]
fn a_load_failure_says_the_model_was_saved_and_retries_loading() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let state = load_failed_state(locale);
        let mut ui = simulator(views::wizard_view(&state));
        assert!(
            ui.find(tr(locale, MessageKey::ModelLoadFailed)).is_ok(),
            "{locale:?}: the load failure is named"
        );
        assert!(
            ui.find(tr(locale, MessageKey::ModelPersistenceFailed))
                .is_err(),
            "{locale:?}: a saved model must never be described as not saved"
        );
        let _ = ui.click(tr(locale, MessageKey::ModelLoadRetry));
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::WizardRetryModelLoad)),
            "{locale:?}: Try again retries loading"
        );
        assert!(
            !messages.iter().any(|m| matches!(m, Message::WizardAccept)),
            "{locale:?}: Try again must not re-run saving"
        );
    }
}

#[test]
fn enter_on_the_load_failed_step_retries_loading() {
    assert_eq!(
        load_failed_state(Locale::En)
            .wizard
            .as_ref()
            .map(WizardState::kind),
        Some(WizardKind::ReadyLoadFailed)
    );
    let got = key_to_message(
        &Key::Named(Named::Enter),
        Modifiers::default(),
        &KeyboardContext {
            text_input_focused: false,
            active_view: ViewId::Search,
            confirm_reset: false,
            confirm_remove_source: false,
            confirm_clear_history: false,
            confirm_delete_keyword_index: false,
            confirm_delete_vector_index: false,
            confirm_add_sensitive_folder: false,
            wizard_kind: Some(WizardKind::ReadyLoadFailed),
            selected_source_id: None,
            selected_result: None,
        },
    );
    assert!(matches!(got, Some(Message::WizardRetryModelLoad)));
}

/// §2.3: the indexing side's notice offers "Try again", and that button asks
/// background preparation to load the model again -- not just dismiss.
#[test]
fn the_model_load_notice_try_again_asks_to_load_again() {
    let _guard = iced_test_guard();
    // Task 060: the host raises it with its retry, as `scheduler_host` does.
    let mut state = AppState::default();
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::ModelCouldNotBeLoaded,
        action: Box::new(Message::RetryModelLoad),
    });
    // Task 064: notices render in the shell, above every view.
    let app = crate::shell::OrbokApp::with_state(state.clone());
    let mut ui = simulator(app.view());
    assert!(
        ui.find(tr(state.locale, MessageKey::ModelLoadFailed))
            .is_ok()
    );
    let _ = ui.click(tr(state.locale, MessageKey::ModelLoadRetry));
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::NoticeActionPressed)),
        "the notice's Try again is pressed"
    );
    assert!(
        matches!(state.take_notice_action(), Some(Message::RetryModelLoad)),
        "and dispatches RetryModelLoad"
    );
    assert_eq!(state.notice, None, "retrying dismisses the notice");
}
