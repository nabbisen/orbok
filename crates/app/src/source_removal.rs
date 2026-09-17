//! Task 073: removing a folder, from the confirmed request to what the
//! Folders list shows.
//!
//! `main.rs`'s `update` closure cannot be driven from a test, so the whole
//! sequence lives here as a plain function over the catalog and the state.

use crate::{bootstrap, notice_retry};
use orbok_db::Catalog;
use orbok_ui::AppState;
use orbok_ui::state::Message;

/// Remove `source_id` from the catalog, then update the state to match what
/// the catalog now holds: on success the card goes; on failure it stays, and
/// the notice offers Try again, which re-opens the confirmation for that
/// still-listed folder.
///
/// `main.rs` returns after calling this, so its generic `app.update(message)`
/// never applies `SourceRemoved` -- a request, which changes no state.
pub(crate) fn remove(catalog: &Catalog, state: &mut AppState, source_id: &str) {
    match bootstrap::remove_source(catalog, source_id) {
        Ok(()) => state.update(&Message::SourceRemovalSucceeded(source_id.to_string())),
        Err(e) => {
            tracing::error!("remove source failed: {e}");
            state.update(&notice_retry::source_not_removed(source_id));
        }
    }
}

#[cfg(test)]
mod tests;
