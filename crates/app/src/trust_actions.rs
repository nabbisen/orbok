//! HANDOFF-038: the recovery actions on a search result that touch the
//! catalog. Like `backend_actions`, each is a plain function so a test can
//! drive it against a real profile; `main.rs`'s closure cannot be.
//!
//! `RemoveFromResults` and `ViewDetails` change only what is shown, so the
//! reducer does them. `OpenAnyway` and `ShowInFolder` open something outside
//! orbok, and are `result_launch`'s.

use crate::{backend_actions, notice_retry};
use orbok_core::SourceId;
use orbok_db::Catalog;
use orbok_db::repo::{FileRepository, IndexJobRepository};
use orbok_search::ResultRecoveryAction;
use orbok_ui::AppState;
use orbok_ui::state::Message;

/// Do `action` for the result at `result_idx`. `message` is the request
/// itself, which a failure's Try again re-sends.
pub(crate) fn recover(
    catalog: &Catalog,
    state: &mut AppState,
    result_idx: usize,
    action: ResultRecoveryAction,
    message: &Message,
) {
    match action {
        ResultRecoveryAction::PrepareAgain => prepare_again(catalog, state, result_idx, message),
        ResultRecoveryAction::CheckFolder => check_folder(catalog, state, result_idx),
        ResultRecoveryAction::RemoveFromResults
        | ResultRecoveryAction::ViewDetails
        | ResultRecoveryAction::OpenAnyway
        | ResultRecoveryAction::ShowInFolder => {}
    }
}

/// The catalog's row for the result at `result_idx`. `None` when there is no
/// such result, or its file is not in the catalog (its folder was removed
/// since the search): the notice says so, and there is nothing to act on.
/// A catalog that cannot be read raises the storage notice, whose Try again
/// is `message`.
fn file_of_result(
    catalog: &Catalog,
    state: &mut AppState,
    result_idx: usize,
    message: &Message,
) -> Option<orbok_db::repo::FileRecord> {
    let path = state.search_results.get(result_idx)?.canonical_path.clone();
    match FileRepository::new(catalog).find_by_canonical_path(&path) {
        Ok(Some(file)) => Some(file),
        Ok(None) => {
            state.update(&notice_retry::result_not_in_catalog());
            None
        }
        Err(e) => {
            tracing::error!("trust action could not read the file's row: {e}");
            state.update(&notice_retry::recovery_storage_unavailable(message));
            None
        }
    }
}

/// Queue the file for re-preparation, the way a scan queues a changed file,
/// then relabel its row: the job is real, so "still being prepared" is true.
fn prepare_again(catalog: &Catalog, state: &mut AppState, result_idx: usize, message: &Message) {
    let Some(file) = file_of_result(catalog, state, result_idx, message) else {
        return;
    };
    match IndexJobRepository::new(catalog).enqueue_extraction_if_idle(&file.file_id) {
        Ok(_) => state.update(&Message::ResultPreparationQueued { result_idx }),
        Err(e) => {
            tracing::error!("could not queue the file to be prepared again: {e}");
            state.update(&notice_retry::recovery_storage_unavailable(message));
        }
    }
}

/// Run the existing source check for the result's folder -- the same call
/// the Folders page's refresh makes, so its failure is that action's notice
/// with its own retry.
fn check_folder(catalog: &Catalog, state: &mut AppState, result_idx: usize) {
    let Some(path) = state
        .search_results
        .get(result_idx)
        .map(|r| r.canonical_path.clone())
    else {
        return;
    };
    let source_id: Option<SourceId> =
        match FileRepository::new(catalog).find_by_canonical_path(&path) {
            Ok(found) => found.map(|file| file.source_id),
            Err(e) => {
                tracing::error!("Check folder could not read the file's row: {e}");
                state.update(&notice_retry::recovery_storage_unavailable(
                    &Message::TrustRecoveryAction {
                        result_idx,
                        action: ResultRecoveryAction::CheckFolder,
                    },
                ));
                return;
            }
        };
    match source_id {
        Some(source_id) => backend_actions::refresh_source(catalog, state, source_id.as_str()),
        None => state.update(&notice_retry::result_not_in_catalog()),
    }
}

#[cfg(test)]
mod tests;
