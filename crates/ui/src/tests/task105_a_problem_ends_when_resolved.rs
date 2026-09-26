//! Review 283 §3: a problem notice is cleared when the action it reports later
//! succeeds. One rule, `UserNotice::is_resolved_by`, checked by the reducer
//! before every message -- not one hand-written exception per notice. A
//! success that is **not** the resolving one leaves a problem in place, and
//! every problem notice is either given its resolving success or named here
//! with the reason it has none.

use crate::notice::UserNotice;
use crate::state::{AppState, Message, SourceCard};
use crate::tests::notice::all;
use orbok_core::SourceStatus;

fn card() -> SourceCard {
    SourceCard {
        display_name: "Docs".into(),
        display_path: "/docs".into(),
        indexed: 0,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    }
}

/// The success that resolves each problem notice, with the retry the notice
/// stores where the resolution depends on it. `None`: see [`NEVER_RESOLVED`].
fn resolving(notice: &UserNotice) -> Option<(Option<Message>, Message)> {
    use UserNotice as N;
    Some(match notice {
        N::FolderCouldNotBeAdded => (None, Message::SourceAdded(card())),
        N::SearchDidNotFinish
        | N::FileCouldNotBeFound
        | N::FileCouldNotBeOpened
        | N::FileNotAllowed
        | N::FileCheckFailed => (None, Message::SearchResultsReady(Vec::new())),
        N::DiagnosticsFileFailed => (None, Message::DiagnosticsBundleCreated("x".into())),
        N::CatalogResetFailed => (None, Message::CatalogResetSucceeded),
        N::SourceCouldNotBeRemoved => (None, Message::SourceRemovalSucceeded("s1".into())),
        N::RecentSearchesNotCleared => (None, Message::RecentSearchesCleared),
        N::RecentSearchNotRemoved => (
            None,
            Message::RecentSearchRemoved(orbok_core::SearchHistoryId::new("h1")),
        ),
        N::CleanupDidNotFinish => (
            Some(Message::CleanSnippets),
            Message::ShowNotice(N::PreviewsCleared),
        ),
        _ => return None,
    })
}

/// Problem notices with no resolving success, each with why (the same reasons
/// are in the comment on `UserNotice::is_resolved_by`).
const NEVER_RESOLVED: &[(&str, &str)] = &[
    (
        "SettingCouldNotBeSaved",
        "a successful save raises no message the window sees",
    ),
    (
        "FolderNotChecked",
        "a successful check arrives as SourcesLoaded, which any list reload also sends",
    ),
    (
        "StorageUnavailable",
        "the notice does not record which action could not open storage",
    ),
    (
        "ModelCouldNotBeLoaded",
        "its own Try again clears it; a later load's success is not a message the window sees",
    ),
    (
        "IndexingCouldNotStart",
        "background preparation does not restart within a session",
    ),
];

fn shown(state: &AppState) -> Option<UserNotice> {
    state.notice.clone()
}

fn with_problem(notice: UserNotice, retry: Option<Message>) -> AppState {
    let mut state = AppState::default();
    match retry {
        Some(action) => state.update(&Message::ShowNoticeWithAction {
            notice,
            action: Box::new(action),
        }),
        None => state.update(&Message::ShowNotice(notice)),
    }
    state
}

/// Every problem notice is decided: resolved by one named success, or named in
/// [`NEVER_RESOLVED`]. A new problem notice fails here until someone decides.
#[test]
fn every_problem_notice_has_its_resolving_success_or_a_reason_it_has_none() {
    for notice in all().into_iter().filter(UserNotice::is_problem) {
        let name = format!("{notice:?}");
        let named_none = NEVER_RESOLVED.iter().any(|(n, _)| *n == name);
        assert_ne!(
            resolving(&notice).is_some(),
            named_none,
            "{name}: give it a resolving success, or list it in NEVER_RESOLVED with a reason (and not both)"
        );
    }
}

/// Feeding the resolving success ends the problem, for every notice that has one.
#[test]
fn the_resolving_success_ends_each_problem() {
    for notice in all().into_iter().filter(UserNotice::is_problem) {
        let Some((retry, success)) = resolving(&notice) else {
            continue;
        };
        let mut state = with_problem(notice.clone(), retry);
        assert_eq!(shown(&state), Some(notice.clone()), "{notice:?} is showing");
        state.update(&success);
        assert!(
            !shown(&state).is_some_and(|n| n == notice),
            "{notice:?} must be gone after {success:?}"
        );
    }
}

/// §3 test 1: "Folder was not added", then a successful add: the notice is gone,
/// and the add's own confirmation is what shows.
#[test]
fn a_successful_add_ends_the_folder_was_not_added_notice() {
    let mut state = with_problem(UserNotice::FolderCouldNotBeAdded, None);
    state.update(&Message::SourceAdded(card()));
    assert_eq!(shown(&state), Some(UserNotice::FolderAdded));
}

/// §3 test 2: a success that is not the resolving one leaves the problem in place.
#[test]
fn a_success_that_is_not_the_resolving_one_leaves_the_problem() {
    let mut state = with_problem(UserNotice::FolderCouldNotBeAdded, None);
    for other in [
        Message::SearchResultsReady(Vec::new()),
        Message::RecentSearchesCleared,
        Message::SourceRemovalSucceeded("s1".into()),
    ] {
        state.update(&other);
        assert_eq!(
            shown(&state),
            Some(UserNotice::FolderCouldNotBeAdded),
            "{other:?} does not resolve a failed add"
        );
    }
    // A cleanup that failed is not resolved by a different cleanup's success.
    let mut state = with_problem(
        UserNotice::CleanupDidNotFinish,
        Some(Message::CleanSnippets),
    );
    state.update(&Message::ShowNotice(UserNotice::SearchCacheCleared));
    assert_eq!(shown(&state), Some(UserNotice::CleanupDidNotFinish));
    state.update(&Message::ShowNotice(UserNotice::PreviewsCleared));
    assert_ne!(shown(&state), Some(UserNotice::CleanupDidNotFinish));
}

/// §3 test 3: the removal case that was written by hand still clears, now by the rule.
#[test]
fn a_successful_removal_still_ends_the_folder_not_removed_notice() {
    let mut state = with_problem(UserNotice::SourceCouldNotBeRemoved, None);
    state.update(&Message::SourceRemovalSucceeded("s1".into()));
    assert_eq!(shown(&state), None);
}

/// A problem that was not caused by the action stays: a confirmation is never a
/// resolution, and an unrelated notice does not disturb a problem.
#[test]
fn a_confirmation_never_clears_a_problem_it_did_not_resolve() {
    let mut state = with_problem(UserNotice::SourceCouldNotBeRemoved, None);
    state.update(&Message::ShowNotice(UserNotice::PreviewsCleared));
    assert_eq!(shown(&state), Some(UserNotice::SourceCouldNotBeRemoved));
}
