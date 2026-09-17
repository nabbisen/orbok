//! Task 060 §4 tests 1-2: each raise site's notice, and what its action
//! button dispatches -- driven through `AppState` exactly as `main.rs` does:
//! `update` with the raised message, then `take_notice_action` for the
//! pressed button.

use super::*;
use crate::result_launch::{LaunchAction, LaunchFailure};
use orbok_ui::AppState;
use orbok_ui::Theme;

/// Raise `message` and return (the notice shown, what its button dispatches).
fn raise(message: Message) -> (Option<UserNotice>, Option<Message>) {
    let mut state = AppState::default();
    state.update(&message);
    let notice = state.notice.clone();
    (notice, state.take_notice_action())
}

#[test]
fn adding_a_folder_retries_with_the_add_folder_picker() {
    let (notice, action) = raise(add_folder_failed());
    assert_eq!(notice, Some(UserNotice::FolderCouldNotBeAdded));
    assert!(matches!(action, Some(Message::RequestAddSource)));
}

#[test]
fn adding_a_search_folder_retries_with_the_search_folder_picker() {
    let (notice, action) = raise(search_folder_failed());
    assert_eq!(notice, Some(UserNotice::FolderCouldNotBeAdded));
    assert!(matches!(action, Some(Message::ChooseFolderRequested)));
}

/// Task 065: each launch failure's notice and button.
#[test]
fn a_file_not_found_goes_to_folders() {
    let (notice, action) = raise(result_not_launched(LaunchFailure::NotFound));
    assert_eq!(notice, Some(UserNotice::FileCouldNotBeFound));
    assert!(matches!(action, Some(Message::Switch(ViewId::Sources))));
}

#[test]
fn a_file_that_would_not_open_offers_to_show_that_same_file_in_its_folder() {
    let (notice, action) = raise(result_not_launched(LaunchFailure::CouldNotOpen {
        index: 3,
        action: LaunchAction::Open,
    }));
    assert_eq!(notice, Some(UserNotice::FileCouldNotBeOpened));
    assert!(matches!(action, Some(Message::RevealResult(3))));
}

#[test]
fn a_failed_reveal_offers_no_button() {
    let (notice, action) = raise(result_not_launched(LaunchFailure::CouldNotOpen {
        index: 3,
        action: LaunchAction::Reveal,
    }));
    assert_eq!(notice, Some(UserNotice::FileCouldNotBeOpened));
    assert!(
        action.is_none(),
        "showing it in its folder is what just failed"
    );
}

/// Task 070 §B.4 test 2.
#[test]
fn a_file_not_allowed_offers_no_button() {
    let (notice, action) = raise(result_not_launched(LaunchFailure::NotAllowed));
    assert_eq!(notice, Some(UserNotice::FileNotAllowed));
    assert!(action.is_none(), "retrying cannot change a permission");
}

#[test]
fn check_failed_retries_the_same_action_on_the_same_result() {
    let (notice, action) = raise(result_not_launched(LaunchFailure::CheckFailed {
        index: 2,
        action: LaunchAction::Open,
    }));
    assert_eq!(notice, Some(UserNotice::FileCheckFailed));
    assert!(matches!(action, Some(Message::OpenResult(2))), "{action:?}");
    let (notice, action) = raise(result_not_launched(LaunchFailure::CheckFailed {
        index: 2,
        action: LaunchAction::Reveal,
    }));
    assert_eq!(notice, Some(UserNotice::FileCheckFailed));
    assert!(
        matches!(action, Some(Message::RevealResult(2))),
        "a failed reveal retries the reveal, got {action:?}"
    );
}

#[test]
fn a_setting_not_saved_re_sends_that_exact_change() {
    let (notice, action) = raise(setting_not_saved(&Message::SetTheme(Theme::Dark)));
    assert_eq!(notice, Some(UserNotice::SettingCouldNotBeSaved));
    assert!(matches!(action, Some(Message::SetTheme(Theme::Dark))));
    let (_, action) = raise(setting_not_saved(&Message::SetReducedMotion(true)));
    assert!(matches!(action, Some(Message::SetReducedMotion(true))));
}

#[test]
fn a_cleanup_without_storage_re_issues_that_cleanup() {
    for cleanup in [
        Message::CleanSnippets,
        Message::CleanSearchCache,
        Message::CleanTemporaryExtraction,
        Message::RemoveReplacedStaleIndexes,
    ] {
        let expected = format!("{cleanup:?}");
        let (notice, action) = raise(cleanup_storage_unavailable(&cleanup));
        assert_eq!(notice, Some(UserNotice::StorageUnavailable));
        assert_eq!(
            action.map(|m| format!("{m:?}")),
            Some(expected.clone()),
            "Try again re-issues {expected}"
        );
    }
}

/// §4 test 2: a destructive retry re-opens its confirmation, never acts.
#[test]
fn a_failed_reset_re_opens_the_confirmation_and_never_resets() {
    for raised in [reset_failed(), reset_storage_unavailable()] {
        let (notice, action) = raise(raised);
        assert!(notice.is_some());
        assert!(
            matches!(action, Some(Message::AskResetCatalog)),
            "Try again re-opens the reset confirmation, got {action:?}"
        );
        assert!(!matches!(action, Some(Message::ConfirmResetCatalog)));
    }
}

/// Task 062 §2 (inverting Task 060's no-retry test): a failed removal's Try
/// again re-opens *that* folder's confirmation and never removes directly.
#[test]
fn a_failed_folder_removal_re_opens_its_confirmation() {
    let (notice, action) = raise(source_not_removed("src-7"));
    assert_eq!(notice, Some(UserNotice::SourceCouldNotBeRemoved));
    assert!(
        matches!(&action, Some(Message::AskRemoveSource(id)) if id == "src-7"),
        "Try again re-opens the confirmation for the same folder, got {action:?}"
    );
    assert!(!matches!(
        action,
        Some(Message::SourceRemoved(_) | Message::ConfirmRemoveSource)
    ));
}
