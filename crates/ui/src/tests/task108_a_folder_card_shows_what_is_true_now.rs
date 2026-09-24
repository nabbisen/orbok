//! Task 108: a folder card says what is true now. The state order, the
//! in-place refresh that keeps the selection, and the Preparing page naming
//! the one folder that is preparing.

use crate::i18n::{Locale, MessageKey, preparing_folder_for_search, tr};
use crate::state::{AppState, IndexHealth, Message, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_core::SourceStatus;

fn card(id: &str, name: &str) -> SourceCard {
    SourceCard {
        display_name: name.into(),
        display_path: format!("/home/user/{name}"),
        indexed: 0,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: SourceStatus::Active,
        source_id: id.into(),
    }
}

/// Missing / Cannot open first, then Preparing, then Needs update, then
/// Ready -- with queued work under every combination.
#[test]
fn the_card_state_follows_the_fixed_order() {
    let busy = |status, stale| SourceCard {
        status,
        stale,
        unfinished_jobs: 5,
        ..card("a", "A")
    };
    assert_eq!(
        busy(SourceStatus::Missing, 1).state_label_key(),
        MessageKey::SourceStateFolderNotFound
    );
    assert_eq!(
        busy(SourceStatus::PermissionDenied, 1).state_label_key(),
        MessageKey::SourceStateCannotOpen
    );
    assert_eq!(
        busy(SourceStatus::Active, 1).state_label_key(),
        MessageKey::SourceStatePreparing,
        "preparing outranks needs-update"
    );
    let idle = |stale| SourceCard {
        stale,
        ..card("a", "A")
    };
    assert_eq!(
        idle(1).state_label_key(),
        MessageKey::SourceStateNeedsUpdate
    );
    assert_eq!(idle(0).state_label_key(), MessageKey::SourceStateReady);
}

/// The Folders view shows "Preparing" for a preparing folder, in both
/// locales, and still offers Prepare again.
#[test]
fn a_preparing_card_says_preparing_and_keeps_prepare_again() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let state = AppState {
            locale,
            active_view: ViewId::Sources,
            sources: vec![SourceCard {
                unfinished_jobs: 3,
                ..card("a", "Docs")
            }],
            ..AppState::default()
        };
        let mut ui = simulator(views::sources_view(&state));
        assert!(
            ui.find(tr(locale, MessageKey::SourceStatePreparing))
                .is_ok(),
            "{locale:?}: the card says Preparing"
        );
        assert!(
            ui.find(tr(locale, MessageKey::SourceActionPrepareAgain))
                .is_ok(),
            "{locale:?}: Prepare again does not come and go"
        );
    }
}

/// Test 2: a refresh keeps a selection that still exists and updates the
/// counts; a folder removed meanwhile is neither brought back by a stale read
/// nor left selected.
#[test]
fn a_refresh_keeps_the_selection_and_never_resurrects_a_removed_folder() {
    let mut state = AppState {
        sources: vec![card("a", "A"), card("b", "B")],
        selected_source: Some(1),
        ..AppState::default()
    };

    let fresh = |id: &str, name: &str, indexed| SourceCard {
        indexed,
        ..card(id, name)
    };
    state.update(&Message::SourceCardsRefreshed(vec![
        fresh("a", "A", 4),
        fresh("b", "B", 7),
    ]));
    assert_eq!(
        state.selected_source,
        Some(1),
        "the selection survives a tick"
    );
    assert_eq!(state.sources[0].indexed, 4);
    assert_eq!(state.sources[1].indexed, 7);

    // B is removed; a read that began before the removal arrives after it.
    state.update(&Message::SourceRemovalSucceeded("b".into()));
    assert_eq!(
        state.selected_source, None,
        "the removed folder is not selected"
    );
    state.update(&Message::SourceCardsRefreshed(vec![
        fresh("a", "A", 9),
        fresh("b", "B", 7),
    ]));
    assert_eq!(state.sources.len(), 1, "a stale read does not bring B back");
    assert_eq!(state.sources[0].indexed, 9, "A is still refreshed");
}

/// A refresh does not add a folder either: adding is `SourceAdded`'s job.
#[test]
fn a_refresh_adds_no_card() {
    let mut state = AppState {
        sources: vec![card("a", "A")],
        ..AppState::default()
    };
    state.update(&Message::SourceCardsRefreshed(vec![
        card("a", "A"),
        card("z", "Z"),
    ]));
    assert_eq!(state.sources.len(), 1);
}

/// Test 5: two folders exist and only one is preparing -- the Preparing page
/// names that one, in both locales. With two preparing it names neither.
#[test]
fn the_preparing_page_names_the_one_folder_that_is_preparing() {
    let _guard = iced_test_guard();
    let health = IndexHealth {
        indexed: 1,
        stale: 0,
        failed: 0,
        queued: 4,
    };
    for locale in [Locale::En, Locale::Ja] {
        let mut state = AppState {
            locale,
            active_view: ViewId::Indexing,
            health,
            sources: vec![
                card("a", "Quiet"),
                SourceCard {
                    unfinished_jobs: 4,
                    ..card("b", "Busy")
                },
            ],
            ..AppState::default()
        };
        let named = preparing_folder_for_search(locale, "Busy");
        {
            let mut ui = simulator(views::indexing_view(&state));
            assert!(
                ui.find(named.as_str()).is_ok(),
                "{locale:?}: names the folder that is preparing: {named:?}"
            );
        }

        state.sources[0].unfinished_jobs = 2;
        let mut ui = simulator(views::indexing_view(&state));
        assert!(
            ui.find(named.as_str()).is_err(),
            "{locale:?}: with two preparing, it names neither"
        );
        assert!(ui.find(tr(locale, MessageKey::IndexingRunning)).is_ok());
    }
}
