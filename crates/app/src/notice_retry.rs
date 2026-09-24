//! Task 060: every notice button does what it says, or is not shown.
//!
//! Each function builds the message a raise site sends when something fails,
//! with the concrete retry that site knows. Raise sites in `main.rs` call
//! these rather than building the notice inline, so each retry choice is a
//! plain function with its own test -- `main.rs`'s `update` closure cannot be
//! driven from a test.
//!
//! Two rules (Task 060 §1):
//! - **A retry re-issues the failed request**, not whatever the UI now shows.
//! - **A destructive retry re-opens its confirmation**; it never acts.
//!
//! A site that cannot supply a correct retry sends a plain `ShowNotice`, and
//! the notice renders dismiss alone.

use crate::result_launch::{LaunchAction, LaunchFailure};
use orbok_ui::notice::UserNotice;
use orbok_ui::state::{Message, ViewId};

fn with_action(notice: UserNotice, action: Message) -> Message {
    Message::ShowNoticeWithAction {
        notice,
        action: Box::new(action),
    }
}

/// Adding a folder from Sources failed, or its first scan did: "Choose
/// another folder" opens the add-folder picker again.
pub(crate) fn add_folder_failed() -> Message {
    with_action(UserNotice::FolderCouldNotBeAdded, Message::RequestAddSource)
}

/// Adding the folder picked for a search failed: "Choose another folder"
/// opens the search-in-folder picker again.
pub(crate) fn search_folder_failed() -> Message {
    with_action(
        UserNotice::FolderCouldNotBeAdded,
        Message::ChooseFolderRequested,
    )
}

/// Opening a result, or showing it in its folder, did not happen (Task 065):
/// - **not found** -> "Go to Folders", where a missing folder is explained;
/// - **Open failed** -> "Show in folder" for *that same result*, re-validated
///   by `launch_result` when pressed (a new search clears this notice, so the
///   index cannot point at a different file);
/// - **Reveal failed** -> no button: showing it in its folder is what failed;
/// - **not allowed** -> no button: retrying cannot change a permission;
/// - **could not be checked** -> "Try again", the same action on the same
///   result (Tasks 070, 074).
pub(crate) fn result_not_launched(failure: LaunchFailure) -> Message {
    match failure {
        LaunchFailure::NotFound => with_action(
            UserNotice::FileCouldNotBeFound,
            Message::Switch(ViewId::Sources),
        ),
        LaunchFailure::CouldNotOpen {
            index,
            action: LaunchAction::Open,
        } => with_action(
            UserNotice::FileCouldNotBeOpened,
            Message::RevealResult(index),
        ),
        LaunchFailure::CouldNotOpen {
            action: LaunchAction::Reveal,
            ..
        } => Message::ShowNotice(UserNotice::FileCouldNotBeOpened),
        LaunchFailure::NotAllowed => Message::ShowNotice(UserNotice::FileNotAllowed),
        LaunchFailure::CheckFailed { index, action } => with_action(
            UserNotice::FileCheckFailed,
            match action {
                LaunchAction::Open => Message::OpenResult(index),
                LaunchAction::Reveal => Message::RevealResult(index),
            },
        ),
    }
}

/// Task 075: clearing recent searches failed. "Try again" re-opens the
/// confirmation -- a destructive retry never acts on its own.
pub(crate) fn recent_searches_not_cleared() -> Message {
    with_action(
        UserNotice::RecentSearchesNotCleared,
        Message::AskClearRecentSearches,
    )
}

/// Task 075: removing one recent search failed. "Try again" repeats that
/// removal (it has no confirmation to re-open).
pub(crate) fn recent_search_not_removed(id: &orbok_core::SearchHistoryId) -> Message {
    with_action(
        UserNotice::RecentSearchNotRemoved,
        Message::RemoveRecentSearch(id.clone()),
    )
}

/// Task 075: a Safe cleanup itself failed. "Try again" repeats that cleanup.
pub(crate) fn cleanup_did_not_finish(cleanup: &Message) -> Message {
    with_action(UserNotice::CleanupDidNotFinish, cleanup.clone())
}

/// Task 075: checking a folder for changes failed. "Try again" repeats it.
pub(crate) fn folder_not_checked(source_id: &str) -> Message {
    with_action(
        UserNotice::FolderNotChecked,
        Message::SourceRefreshRequested(source_id.to_string()),
    )
}

/// Task 114: widening a folder to cover its subfolders failed, so it was not
/// prepared again. The existing "could not check this folder" copy is true;
/// "Try again" repeats the widening.
pub(crate) fn folder_not_widened(source_id: &str) -> Message {
    with_action(
        UserNotice::FolderNotChecked,
        Message::WidenFolder(source_id.to_string()),
    )
}

/// HANDOFF-038: a recovery action's file is not in the catalog any more
/// (its folder was removed since the search). The existing "could not be
/// found" copy is true, and Go to Folders is where to look.
pub(crate) fn result_not_in_catalog() -> Message {
    result_not_launched(LaunchFailure::NotFound)
}

/// HANDOFF-038: a recovery action could not read or write the catalog.
/// "Try again" re-sends that action.
pub(crate) fn recovery_storage_unavailable(action: &Message) -> Message {
    with_action(UserNotice::StorageUnavailable, action.clone())
}

/// Saving a setting failed: "Try again" re-sends that exact setting change.
pub(crate) fn setting_not_saved(change: &Message) -> Message {
    with_action(UserNotice::SettingCouldNotBeSaved, change.clone())
}

/// A cleanup action could not reach storage: "Try again" re-issues that
/// cleanup action.
pub(crate) fn cleanup_storage_unavailable(cleanup: &Message) -> Message {
    with_action(UserNotice::StorageUnavailable, cleanup.clone())
}

/// Reset could not reach storage, or failed: "Try again" re-opens the reset
/// confirmation -- never `ConfirmResetCatalog`.
pub(crate) fn reset_storage_unavailable() -> Message {
    with_action(UserNotice::StorageUnavailable, Message::AskResetCatalog)
}

pub(crate) fn reset_failed() -> Message {
    with_action(UserNotice::CatalogResetFailed, Message::AskResetCatalog)
}

/// Removing a folder failed: "Try again" re-opens that folder's removal
/// confirmation (Task 062) -- never `SourceRemoved` or
/// `ConfirmRemoveSource` directly.
pub(crate) fn source_not_removed(source_id: &str) -> Message {
    with_action(
        UserNotice::SourceCouldNotBeRemoved,
        Message::AskRemoveSource(source_id.to_string()),
    )
}

#[cfg(test)]
mod tests;
