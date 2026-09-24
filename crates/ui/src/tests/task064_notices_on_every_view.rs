//! Task 064: a notice is visible wherever the user is.

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::shell::OrbokApp;
use crate::state::{AppState, Message, ViewId, WizardState};
use crate::tests::iced_test_guard;
use iced_test::simulator;

/// §3 test 1: for every view and for an open wizard, a raised problem notice
/// renders. Search is the positive control.
#[test]
fn a_notice_is_visible_on_every_view_and_over_the_wizard() {
    let _guard = iced_test_guard();
    let title = tr(Locale::En, MessageKey::NoticeSettingSaveFailTitle);
    let mut missing = Vec::new();
    for view in ViewId::ALL {
        let mut state = AppState {
            active_view: *view,
            ..AppState::default()
        };
        state.update(&Message::ShowNotice(UserNotice::SettingCouldNotBeSaved));
        let app = OrbokApp::with_state(state);
        let mut ui = simulator(app.view());
        if ui.find(title).is_err() {
            missing.push(format!("{view:?}"));
        }
    }
    let mut state = AppState {
        wizard: Some(WizardState::NotConfigured),
        ..AppState::default()
    };
    state.update(&Message::ShowNotice(UserNotice::SettingCouldNotBeSaved));
    let app = OrbokApp::with_state(state);
    let mut ui = simulator(app.view());
    if ui.find(title).is_err() {
        missing.push("wizard".into());
    }
    assert!(missing.is_empty(), "notice not visible on: {missing:?}");
}

/// §3 test 2: a problem survives a view switch, with its action; an info
/// notice does not.
#[test]
fn a_problem_survives_a_view_switch_and_info_does_not() {
    let mut state = AppState::default();
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::CatalogResetFailed,
        action: Box::new(Message::AskResetCatalog),
    });
    // Search is the default view: switch to a different one, so the view
    // really changes (a switch to the current view proved nothing -- the
    // mutation that clears problems too survived it).
    assert_eq!(state.active_view, ViewId::Search);
    state.update(&Message::Switch(ViewId::Storage));
    assert_eq!(state.notice, Some(UserNotice::CatalogResetFailed));
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::AskResetCatalog)
    ));

    let mut state = AppState::default();
    state.update(&Message::ShowNotice(UserNotice::FolderAdded));
    state.update(&Message::Switch(ViewId::Storage));
    assert_eq!(
        state.notice, None,
        "an info notice is cleared by a view change"
    );
}

/// An info notice stays while the view does not change.
#[test]
fn an_info_notice_stays_on_its_own_view() {
    let mut state = AppState::default();
    state.update(&Message::ShowNotice(UserNotice::FolderAdded));
    state.update(&Message::QueryChanged("q".into()));
    assert_eq!(state.notice, Some(UserNotice::FolderAdded));
}

/// §3 test 3: info never replaces a problem; a problem replaces info.
#[test]
fn info_never_replaces_a_problem() {
    let mut state = AppState::default();
    state.update(&Message::SearchError {
        query: "alpha".into(),
        error: "timeout".into(),
    });
    state.update(&Message::ShowNotice(UserNotice::FolderAdded));
    assert_eq!(state.notice, Some(UserNotice::SearchDidNotFinish));
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::RetrySearch(query)) if query == "alpha"
    ));

    let mut state = AppState::default();
    state.update(&Message::ShowNotice(UserNotice::FolderAdded));
    state.update(&Message::ShowNotice(UserNotice::StorageUnavailable));
    assert_eq!(state.notice, Some(UserNotice::StorageUnavailable));
}

/// §3 test 4: the latest problem wins, with its own action.
#[test]
fn the_latest_problem_wins_with_its_own_action() {
    let mut state = AppState::default();
    state.update(&Message::SearchError {
        query: "alpha".into(),
        error: "timeout".into(),
    });
    state.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::SettingCouldNotBeSaved,
        action: Box::new(Message::SetReducedMotion(true)),
    });
    assert_eq!(state.notice, Some(UserNotice::SettingCouldNotBeSaved));
    assert!(matches!(
        state.take_notice_action(),
        Some(Message::SetReducedMotion(true))
    ));
}

/// The class rule is the tone: every Danger/Warning notice is a problem and
/// every Success/Info notice is not.
#[test]
fn a_notice_class_follows_its_tone() {
    use snora::design::Tone;
    for notice in &crate::tests::notice::all() {
        let expected = matches!(notice.tone(), Tone::Danger | Tone::Warning);
        assert_eq!(notice.is_problem(), expected, "{notice:?}");
    }
    assert!(
        UserNotice::FileCheckFailed.is_problem(),
        "a Warning is a problem"
    );
}

/// §3 test 5: exactly one render site -- `friendly_notice` is called once in
/// production code, from `shell.rs`.
#[test]
fn a_notice_renders_from_exactly_one_place() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut calls = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (i, line) in text.lines().enumerate() {
                    if line.contains("friendly_notice(") && !line.contains("fn friendly_notice(") {
                        calls.push(format!(
                            "{}:{}",
                            path.strip_prefix(&src).unwrap().display(),
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    assert_eq!(calls.len(), 1, "friendly_notice call sites: {calls:?}");
    assert!(calls[0].starts_with("shell.rs:"), "{calls:?}");
}
