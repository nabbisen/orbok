//! Task 060: every notice button does what it says, or is not shown. The
//! raise sites in `orbok` are covered by `notice_retry::tests`; these cover
//! the UI half: the button, what it dispatches, and a failed search's retry.

use crate::i18n::{MessageKey, tr};
use crate::notice::UserNotice;
use crate::state::{AppState, Message};
use crate::tests::iced_test_guard;
use iced_test::simulator;

fn failed_search(query: &str) -> Message {
    Message::SearchError {
        query: query.into(),
        error: "timeout".into(),
    }
}

/// The defect (red on the old API, where the button sent only
/// `ClearNotice`): Try again on a failed search does more than close the
/// notice -- it is the action button, and what it dispatches re-runs the
/// search.
#[test]
fn try_again_on_a_failed_search_does_more_than_close_the_notice() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.update(&failed_search("alpha"));
    // Task 064: notices render in the shell, above every view.
    let app = crate::shell::OrbokApp::with_state(state.clone());
    let mut ui = simulator(app.view());
    let _ = ui.click(tr(state.locale, MessageKey::NoticeActionTryAgain));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(!messages.is_empty(), "the button rendered and was pressed");
    assert!(
        messages
            .iter()
            .all(|m| matches!(m, Message::NoticeActionPressed)),
        "Try again must be the notice action, got {messages:?}"
    );
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::RetrySearch(query)) if query == "alpha"
    ));
    assert_eq!(state.notice, None, "pressing the action clears the notice");
}

/// §4 test 3: a failed search, a different query typed, then Try again --
/// the *original* query is re-run.
#[test]
fn try_again_re_runs_the_query_that_failed_not_the_one_typed_since() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("alpha".into()));
    state.update(&Message::SubmitSearch);
    state.update(&failed_search("alpha"));
    state.update(&Message::QueryChanged("beta".into()));

    let Some(retry) = state.take_notice_action() else {
        panic!("a failed search offers a retry")
    };
    assert!(matches!(&retry, Message::RetrySearch(query) if query == "alpha"));
    // What orbok does with it: the reducer restores the query, then orbok
    // dispatches SubmitSearch.
    state.update(&retry);
    state.update(&Message::SubmitSearch);
    assert_eq!(state.query, "alpha");
    assert_eq!(state.last_query.as_deref(), Some("alpha"));
}

/// §3: the diagnostics notice's action creates the diagnostics file.
#[test]
fn a_failed_diagnostics_file_offers_to_create_it_again() {
    let mut state = AppState::default();
    state.update(&Message::DiagnosticsBundleFailed);
    assert_eq!(state.notice, Some(UserNotice::DiagnosticsFileFailed));
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::DiagnosticsCreateBundle)
    ));
}

/// §4 test 4: a notice raised without retry context renders no action
/// button. Control: the notice itself rendered.
#[test]
fn a_notice_without_a_retry_renders_no_action_button() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.update(&Message::ShowNotice(UserNotice::SearchDidNotFinish));
    // Task 064: notices render in the shell, above every view.
    let app = crate::shell::OrbokApp::with_state(state.clone());
    let mut ui = simulator(app.view());
    assert!(
        ui.find(tr(state.locale, MessageKey::NoticeSearchFailTitle))
            .is_ok(),
        "control: the notice rendered"
    );
    assert!(
        ui.find(tr(state.locale, MessageKey::NoticeActionTryAgain))
            .is_err(),
        "a label with no retry behind it must not render"
    );
    drop(ui);
    assert!(state.take_notice_action().is_none());
}

/// A new notice without a retry does not inherit the previous notice's.
#[test]
fn a_plain_notice_replaces_an_earlier_retry() {
    let mut state = AppState::default();
    state.update(&failed_search("alpha"));
    state.update(&Message::ShowNotice(UserNotice::SearchDidNotFinish));
    assert!(state.take_notice_action().is_none());
}
