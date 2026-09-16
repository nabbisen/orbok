//! Task 060 §4 tests 1-2: each raise site's notice, and what its action
//! button dispatches -- driven through `AppState` exactly as `main.rs` does:
//! `update` with the raised message, then `take_notice_action` for the
//! pressed button.

use super::*;
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

#[test]
fn a_result_not_launched_checks_folders() {
    let (notice, action) = raise(result_not_launched());
    assert_eq!(notice, Some(UserNotice::FilesMovedOrMissing));
    assert!(matches!(action, Some(Message::Switch(ViewId::Sources))));
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

#[test]
fn a_download_without_a_model_store_offers_no_retry() {
    let (notice, action) = raise(download_storage_unavailable());
    assert_eq!(notice, Some(UserNotice::StorageUnavailable));
    assert!(action.is_none());
}
