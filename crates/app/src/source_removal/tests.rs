//! Task 073 §3 tests 1, 2 and 4: removing a folder, against a real catalog.

use super::remove;
use crate::bootstrap;
use iced::keyboard::{Key, Modifiers, key::Named};
use orbok_db::Catalog;
use orbok_ui::notice::UserNotice;
use orbok_ui::state::{Confirmation, Message, ViewId};
use orbok_ui::{AppState, OrbokApp, key_to_message};
use std::path::{Path, PathBuf};

/// An on-disk catalog with one registered folder, a short busy timeout so a
/// locked catalog fails fast, and the Folders view showing that folder.
fn folders_with_one(temp: &Path) -> (Catalog, PathBuf, AppState, String) {
    let folder = temp.join("Docs");
    std::fs::create_dir_all(&folder).unwrap();
    let db = temp.join("orbok-catalog.sqlite3");
    let catalog = Catalog::open(&db).unwrap();
    bootstrap::add_source_expect_added(&catalog, &folder.to_string_lossy()).unwrap();
    catalog
        .lock()
        .busy_timeout(std::time::Duration::from_millis(50))
        .unwrap();
    let mut state = AppState::default();
    state.update(&Message::Switch(ViewId::Sources));
    state.update(&Message::SourcesLoaded(bootstrap::get_sources(&catalog)));
    let id = state.sources[0].source_id.clone();
    state.update(&Message::SelectNextSource);
    (catalog, db, state, id)
}

/// `AskRemoveSource` then `ConfirmRemoveSource`, as the dialog does, then
/// the removal `main.rs` performs for the message the confirmation returns.
fn confirm_removal(catalog: &Catalog, state: &mut AppState, id: &str) {
    state.update(&Message::AskRemoveSource(id.to_string()));
    let request = state.take_confirmed_removal();
    let Some(Message::SourceRemoved(requested)) = request else {
        panic!("the confirmation dispatches SourceRemoved, got {request:?}");
    };
    remove(catalog, state, &requested);
}

fn enter(state: &AppState) -> Option<Message> {
    let context = OrbokApp::with_state(state.clone()).keyboard_context();
    key_to_message(&Key::Named(Named::Enter), Modifiers::empty(), &context)
}

/// §3 test 1: a removal the catalog refuses (a write lock held by another
/// connection -- a real write failure) keeps the folder listed, with the
/// notice. §3 test 2: its Try again opens a dialog that is visible and that
/// Enter confirms.
#[test]
fn a_failed_removal_keeps_the_folder_listed_and_try_again_is_visible() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, db, mut state, id) = folders_with_one(temp.path());
    let locker = rusqlite::Connection::open(&db).unwrap();
    locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

    confirm_removal(&catalog, &mut state, &id);
    locker.execute_batch("ROLLBACK;").unwrap();

    assert_eq!(state.notice, Some(UserNotice::SourceCouldNotBeRemoved));
    assert!(
        state.sources.iter().any(|card| card.source_id == id),
        "the folder is still registered, so it stays listed; sources: {:?}",
        state
            .sources
            .iter()
            .map(|c| &c.source_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        bootstrap::get_sources(&catalog).len(),
        1,
        "control: the catalog still has the folder"
    );

    // Try again's retry, read without pressing it: pressing clears the
    // notice (Task 060), and the case below needs it still showing.
    let retry = state.notice_action.as_deref().cloned();
    assert!(
        matches!(&retry, Some(Message::AskRemoveSource(retry_id)) if *retry_id == id),
        "{retry:?}"
    );
    // Delete on the still-selected folder sends the same message.
    assert_eq!(state.selected_source, Some(0), "the folder stays selected");
    state.update(&retry.unwrap());
    assert_eq!(
        state.visible_confirmation(),
        Some(Confirmation::RemoveSource),
        "Try again's dialog is visible"
    );
    assert!(matches!(enter(&state), Some(Message::ConfirmRemoveSource)));

    // With the lock released, confirming removes it, and the notice that
    // said it was not removed goes with it.
    let request = state.take_confirmed_removal();
    let Some(Message::SourceRemoved(requested)) = request else {
        panic!("the confirmation dispatches SourceRemoved, got {request:?}");
    };
    remove(&catalog, &mut state, &requested);
    assert!(
        state.sources.is_empty(),
        "the retried removal removes the card"
    );
    assert_eq!(
        state.notice, None,
        "\"Folder not removed\" is no longer true"
    );
}

/// §3 test 4: a removal that succeeds removes the card and clears the
/// selection, with no notice.
#[test]
fn a_successful_removal_removes_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, _db, mut state, id) = folders_with_one(temp.path());
    assert_eq!(
        state.selected_source,
        Some(0),
        "control: the folder is selected"
    );

    confirm_removal(&catalog, &mut state, &id);

    assert!(state.sources.is_empty(), "the card is gone");
    assert_eq!(state.selected_source, None);
    assert_eq!(state.notice, None);
    assert!(bootstrap::get_sources(&catalog).is_empty());
}
