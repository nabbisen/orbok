//! Task 070: failure copy that says only what is true.
//!
//! - Part A: the failed-download heading gives no connection advice.
//! - Part B: a refused open is Not allowed or Busy, never "Files may have
//!   moved".

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::shell::OrbokApp;
use crate::state::{
    AppState, Message, ModelConsentReturn, ModelDeliveryFailure, ModelDownloadConsent,
    SearchResultDisplay, WizardState,
};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

#[test]
fn the_failed_download_heading_gives_no_connection_advice() {
    let _guard = iced_test_guard();
    let expected = [
        (Locale::En, "Download did not finish", "connection"),
        (Locale::Ja, "ダウンロードが完了しませんでした", "接続"),
    ];
    // Cancelled is intercepted by the reducer and never rendered here.
    let failures = [
        ModelDeliveryFailure::StoreUnavailable,
        ModelDeliveryFailure::Connection,
        ModelDeliveryFailure::Verification,
        ModelDeliveryFailure::LocalStorage,
        ModelDeliveryFailure::InternalState,
    ];
    for (locale, heading, advice) in expected {
        let actual = tr(locale, MessageKey::ModelDownloadFailed);
        assert!(
            !actual.contains(advice),
            "{locale:?}: the heading must not give connection advice, got {actual:?}"
        );
        assert_eq!(actual, heading, "{locale:?}");
        for failure in failures {
            let state = AppState {
                locale,
                wizard: Some(WizardState::DownloadFailed {
                    presentation: ModelDownloadConsent::trusted_default("/managed/models".into()),
                    return_to: ModelConsentReturn::NotConfigured,
                    failure,
                }),
                ..AppState::default()
            };
            let mut ui = simulator(views::wizard_view(&state));
            assert!(
                ui.find(heading).is_ok(),
                "{locale:?} {failure:?}: the heading renders exactly"
            );
        }
    }
}

// ── Part B: Not allowed and Busy ─────────────────────────────────────────

fn raised(locale: Locale, message: Message) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    state.update(&message);
    state
}

/// §B.4 test 3: both notices render their exact copy in both locales,
/// through the shell. Not allowed renders no button; Busy renders Try again.
#[test]
fn not_allowed_and_busy_render_the_approved_copy() {
    let _guard = iced_test_guard();
    let copy = [
        (
            Locale::En,
            "This file could not be opened",
            "orbok isn't allowed to open it.",
            "orbok was busy. Try again in a moment.",
            "Try again",
        ),
        (
            Locale::Ja,
            "このファイルを開けませんでした",
            "orbok にはこのファイルを開く権限がありません。",
            "orbok が処理中でした。少し待ってからもう一度お試しください。",
            "もう一度試す",
        ),
    ];
    for (locale, title, not_allowed, busy, try_again) in copy {
        let state = raised(locale, Message::ShowNotice(UserNotice::FileNotAllowed));
        let app = OrbokApp::with_state(state);
        let mut ui = simulator(app.view());
        assert!(ui.find(title).is_ok(), "{locale:?}: Not allowed title");
        assert!(ui.find(not_allowed).is_ok(), "{locale:?}: Not allowed body");
        assert!(
            ui.find(try_again).is_err(),
            "{locale:?}: Not allowed offers no button"
        );

        let state = raised(
            locale,
            Message::ShowNoticeWithAction {
                notice: UserNotice::FileBusy,
                action: Box::new(Message::RevealResult(2)),
            },
        );
        let app = OrbokApp::with_state(state);
        let mut ui = simulator(app.view());
        for text in [title, busy, try_again] {
            assert!(ui.find(text).is_ok(), "{locale:?}: Busy renders {text:?}");
        }
    }
}

fn result(name: &str) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: name.into(),
        canonical_path: format!("/docs/{name}"),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    }
}

fn busy_on_result_1() -> AppState {
    let mut state = AppState::default();
    state.update(&Message::SearchResultsReady(vec![
        result("a.md"),
        result("b.md"),
    ]));
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::FileBusy,
        action: Box::new(Message::OpenResult(1)),
    });
    state
}

/// §B.4 test 4: new results clear a Busy notice and its indexed retry.
#[test]
fn a_new_search_clears_a_busy_notice_and_its_retry() {
    let mut state = busy_on_result_1();
    state.update(&Message::SearchResultsReady(vec![result("other.md")]));
    assert_eq!(state.notice, None);
    assert!(state.take_notice_action().is_none());

    // Positive control: without new results, Try again opens result 1.
    let mut state = busy_on_result_1();
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::OpenResult(1))
    ));
}
