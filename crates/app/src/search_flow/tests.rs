//! Task 068 §3: the first search through the folder picker finishes, and
//! searches for what was typed.

use super::after_folder_picked;
use orbok_core::SourceId;
use orbok_ui::AppState;
use orbok_ui::SearchLocation;
use orbok_ui::state::Message;

fn docs() -> SearchLocation {
    SearchLocation::remembered(SourceId::from_string("src-1".to_string()), "Docs")
}

/// Drive the picker flow the way `router.rs` does: `SubmitSearch` with no
/// location opens the picker (the reducer sees `ChooseFolderRequested`, not
/// `SubmitSearch`); the pick selects the folder; then the resume decides.
fn pick_folder_for_pending_search(state: &mut AppState) -> Option<Message> {
    state.update(&Message::ChooseFolderRequested);
    state.update(&Message::SearchLocationSelected(docs()));
    after_folder_picked(state)
}

/// §3 test 1: on a fresh profile, never Searching with no search issued.
#[test]
fn a_first_search_through_the_picker_issues_a_search_or_is_not_searching() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("alpha".into()));
    let issued = pick_folder_for_pending_search(&mut state);
    assert!(
        issued.is_some() || !state.search_running,
        "Searching with no search issued: search_running={}, issued={issued:?}",
        state.search_running
    );
    assert!(
        matches!(&issued, Some(Message::RetrySearch(q)) if q == "alpha"),
        "the typed query is resumed, got {issued:?}"
    );
}

/// §3 test 2: the typed query runs, not the earlier one.
#[test]
fn the_picker_resumes_the_typed_query_not_the_earlier_one() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("alpha".into()));
    state.update(&Message::SearchLocationSelected(docs()));
    state.update(&Message::SubmitSearch);
    assert_eq!(state.last_query.as_deref(), Some("alpha"));

    state.update(&Message::SearchLocationCleared);
    state.update(&Message::QueryChanged("beta".into()));
    let issued = pick_folder_for_pending_search(&mut state);
    assert!(
        matches!(&issued, Some(Message::RetrySearch(q)) if q == "beta"),
        "the query typed before picking runs, got {issued:?}"
    );
}

/// §3 test 3: cancelling the picker stays neutral.
#[test]
fn cancelling_the_picker_stays_neutral() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("alpha".into()));
    state.update(&Message::ChooseFolderRequested);
    state.update(&Message::FolderPickerCancelled);
    assert!(!state.search_running);
    assert_eq!(state.query, "alpha", "the query is preserved");
    assert!(state.notice.is_none());
    assert!(after_folder_picked(&state).is_none(), "nothing is pending");
}

/// The resumed message goes through the ordinary search path: applying it,
/// then the `SubmitSearch` `main.rs` dispatches after it, sets `last_query`
/// to the typed query and enters Searching -- with a search issued.
#[test]
fn the_resume_enters_the_ordinary_submit_path() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("alpha".into()));
    let resume = pick_folder_for_pending_search(&mut state).expect("a search is pending");
    assert!(
        !state.search_running,
        "not Searching before a search is issued"
    );
    state.update(&resume);
    state.update(&Message::SubmitSearch);
    assert!(state.search_running);
    assert_eq!(state.last_query.as_deref(), Some("alpha"));
    assert!(
        state.search_location.pending_query.is_none(),
        "the pending search is resolved"
    );
}

/// §3 test 4 (history): a search is recorded only from `SubmitSearchCompleted`,
/// which only the `SubmitSearch` handler produces. The picker's resume used to
/// run its own `run_search` and never recorded history; it must now contain no
/// search of its own and hand over to `after_folder_picked`.
#[test]
fn the_folder_picked_handler_has_no_search_path_of_its_own() {
    // Task 084: the handler this test inspects moved from `main.rs`'s
    // `update` closure into `router.rs`'s `route` function -- a plain
    // text move, so this test's own text-scanning approach just needed
    // its source file name updated, not its assertions.
    let router = include_str!("../router.rs");
    let start = router
        .find("Message::FolderPicked(path) =>")
        .expect("the FolderPicked handler");
    let end = start
        + router[start..]
            .find("Message::SearchAgain(id) =>")
            .expect("the next handler");
    let handler = &router[start..end];
    assert!(
        !handler.contains("run_search("),
        "FolderPicked must not run its own search"
    );
    assert!(handler.contains("search_flow::after_folder_picked"));
    assert_eq!(
        router.matches("history::record_search(").count(),
        1,
        "history is recorded in one place"
    );
    assert!(router.contains("Message::SubmitSearchCompleted { query, outcome } =>"));
}
