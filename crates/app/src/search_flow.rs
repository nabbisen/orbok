//! Task 068: the search a folder pick resumes.

use orbok_ui::AppState;
use orbok_ui::state::Message;

/// What to dispatch once a folder picked for a pending search is registered
/// and selected: the query that was pending when the picker opened
/// (`SearchLocationState::pending_query`), re-issued through `RetrySearch`,
/// which restores it and dispatches the ordinary `SubmitSearch`. `None` when
/// nothing is pending.
///
/// It used to read `last_query`, which only `SubmitSearch`'s reducer arm sets
/// -- and that arm never runs for a search that opened the picker. So a new
/// user's first search resumed nothing and stayed "Searching…", and a later
/// one resumed the previous query.
pub(crate) fn after_folder_picked(state: &AppState) -> Option<Message> {
    state
        .search_location
        .pending_query
        .clone()
        .filter(|query| !query.trim().is_empty())
        .map(Message::RetrySearch)
}

#[cfg(test)]
mod tests;
