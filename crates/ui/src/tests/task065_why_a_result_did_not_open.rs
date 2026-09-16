//! Task 065: after a failed open, say whether the file is gone or just
//! would not open.

use crate::i18n::Locale;
use crate::notice::UserNotice;
use crate::shell::OrbokApp;
use crate::state::{AppState, Message, SearchResultDisplay, ViewId};
use crate::tests::iced_test_guard;
use iced_test::simulator;

fn raised(locale: Locale, message: Message) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    state.update(&message);
    state
}

fn find_all(state: &AppState, texts: &[&str]) {
    let app = OrbokApp::with_state(state.clone());
    let mut ui = simulator(app.view());
    for text in texts {
        assert!(
            ui.find(*text).is_ok(),
            "{:?}: {text:?} renders",
            state.locale
        );
    }
}

/// §5 test 2: exact copy and buttons, both locales.
#[test]
fn both_notices_render_the_approved_copy() {
    let _guard = iced_test_guard();
    let not_found = [
        (
            Locale::En,
            "This file could not be found",
            "It may have been moved, renamed or deleted, or its drive may be disconnected.",
            "Go to Folders",
        ),
        (
            Locale::Ja,
            "このファイルが見つかりません",
            "移動・名前の変更・削除されたか、ドライブが接続されていない可能性があります。",
            "フォルダー一覧へ",
        ),
    ];
    for (locale, title, body, button) in not_found {
        let state = raised(
            locale,
            Message::ShowNoticeWithAction {
                notice: UserNotice::FileCouldNotBeFound,
                action: Box::new(Message::Switch(ViewId::Sources)),
            },
        );
        find_all(&state, &[title, body, button]);
    }
    let could_not_open = [
        (
            Locale::En,
            "This file could not be opened",
            "No app on this computer opened it.",
            "Show in folder",
        ),
        (
            Locale::Ja,
            "このファイルを開けませんでした",
            "このコンピューターのアプリでは開けませんでした。",
            "フォルダーで表示",
        ),
    ];
    for (locale, title, body, button) in could_not_open {
        let state = raised(
            locale,
            Message::ShowNoticeWithAction {
                notice: UserNotice::FileCouldNotBeOpened,
                action: Box::new(Message::RevealResult(0)),
            },
        );
        find_all(&state, &[title, body, button]);

        // A failed reveal: the same notice, no button.
        let state = raised(
            locale,
            Message::ShowNotice(UserNotice::FileCouldNotBeOpened),
        );
        let app = OrbokApp::with_state(state.clone());
        let mut ui = simulator(app.view());
        assert!(ui.find(title).is_ok(), "control: the notice rendered");
        assert!(
            ui.find(button).is_err(),
            "{locale:?}: a failed reveal offers no Show in folder"
        );
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

fn open_failed_on_result_0() -> AppState {
    let mut state = AppState::default();
    state.update(&Message::SearchResultsReady(vec![
        result("a.md"),
        result("b.md"),
    ]));
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::FileCouldNotBeOpened,
        action: Box::new(Message::RevealResult(0)),
    });
    state
}

/// §5 test 3: new results clear a launch-failure notice, so its reveal can
/// never point at a different file.
#[test]
fn a_new_search_clears_a_stale_show_in_folder() {
    let mut state = open_failed_on_result_0();
    state.update(&Message::SearchResultsReady(vec![result("other.md")]));
    assert_eq!(state.notice, None);
    assert!(state.take_notice_action().is_none());

    // Positive control: without the new search, the action is result 0's
    // reveal.
    let mut state = open_failed_on_result_0();
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::RevealResult(0))
    ));
}

/// The clear is narrow (Task 064): an unrelated problem notice survives a new
/// search.
#[test]
fn a_new_search_keeps_an_unrelated_problem_notice() {
    let mut state = AppState::default();
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::StorageUnavailable,
        action: Box::new(Message::CleanSnippets),
    });
    state.update(&Message::SearchResultsReady(vec![result("a.md")]));
    assert_eq!(state.notice, Some(UserNotice::StorageUnavailable));
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::CleanSnippets)
    ));
}
