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

/// Opening a result, or showing it in its folder, was refused or failed:
/// "Check folders" goes to Sources, where a missing folder is explained.
pub(crate) fn result_not_launched() -> Message {
    with_action(
        UserNotice::FilesMovedOrMissing,
        Message::Switch(ViewId::Sources),
    )
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

/// Removing a folder failed. There is no removal confirmation to re-open --
/// the Remove button sends `SourceRemoved` directly -- and re-sending that
/// would act destructively, so this notice offers dismiss alone.
pub(crate) fn source_not_removed() -> Message {
    Message::ShowNotice(UserNotice::SourceCouldNotBeRemoved)
}

/// Starting a model download could not reach the model store. No correct
/// retry exists here: the wizard has already moved to `Downloading`, so
/// re-sending `ConfirmModelDownload` would not match its state. Dismiss
/// alone.
pub(crate) fn download_storage_unavailable() -> Message {
    Message::ShowNotice(UserNotice::StorageUnavailable)
}

#[cfg(test)]
mod tests;
