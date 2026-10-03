//! Task 132 §1.1/§2 test 1: "Choose a folder" offers the folders already
//! added, in the Folders page's own order, instead of only the system
//! picker -- and choosing one sends `ExistingSearchFolderChosen`, not a
//! picker-opening message.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, SourceCard};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_core::SourceId;

fn folder(id: &str, name: &str) -> SourceCard {
    SourceCard {
        display_name: name.into(),
        display_path: format!("/docs/{name}"),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        covers_subfolders: true,
        status: orbok_core::SourceStatus::Active,
        source_id: id.into(),
    }
}

fn clicked(state: &AppState, label: &str) -> Vec<Message> {
    let mut ui = simulator(views::search_view(state));
    let _ = ui.click(label);
    ui.into_messages().collect()
}

/// With two folders added and none selected, both are offered by name, and
/// so is the existing "Choose another folder" control -- no new copy.
#[test]
fn two_added_folders_are_both_offered_by_name() {
    let _guard = iced_test_guard();
    let state = AppState {
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    let mut ui = simulator(views::search_view(&state));
    assert!(ui.find("Docs").is_ok(), "the first added folder is offered");
    assert!(
        ui.find("Notes").is_ok(),
        "the second added folder is offered"
    );
    assert!(
        ui.find(tr(Locale::En, MessageKey::NoticeActionChooseFolder))
            .is_ok(),
        "the system picker is still one choice away, under its existing label"
    );
}

/// Clicking an added folder's name sends `ExistingSearchFolderChosen` for
/// that folder's id -- never a message that opens the system picker.
#[test]
fn choosing_an_added_folder_sends_its_id_not_a_picker_request() {
    let _guard = iced_test_guard();
    let state = AppState {
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    let messages = clicked(&state, "Notes");
    let expected = SourceId::from_string("s_notes");
    assert!(
        matches!(messages.as_slice(), [Message::ExistingSearchFolderChosen(id)] if *id == expected),
        "choosing an already-added folder never opens a picker Task, got {messages:?}"
    );
}

/// "Choose another folder" still sends the one message that opens the
/// system picker, unchanged from before this task.
#[test]
fn choose_another_folder_still_opens_the_system_picker() {
    let _guard = iced_test_guard();
    let state = AppState {
        sources: vec![folder("s_docs", "Docs")],
        ..AppState::default()
    };
    let messages = clicked(&state, tr(Locale::En, MessageKey::NoticeActionChooseFolder));
    assert!(matches!(messages.as_slice(), [Message::ChooseSearchFolder]));
}

/// §2 test 2: with no folders added, the prompt is the plain "Choose a
/// folder" control -- unchanged, straight to the picker.
#[test]
fn no_added_folders_falls_back_to_the_plain_prompt() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let messages = clicked(&state, tr(Locale::En, MessageKey::SearchChooseFolder));
    assert!(matches!(messages.as_slice(), [Message::ChooseSearchFolder]));
}

/// Both locales offer the same two folders and the same fallback label --
/// RFC-031: no literal ever only renders in one locale.
#[test]
fn the_offer_renders_in_both_locales() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let state = AppState {
            locale: *locale,
            sources: vec![folder("s_docs", "Docs")],
            ..AppState::default()
        };
        let mut ui = simulator(views::search_view(&state));
        assert!(ui.find("Docs").is_ok(), "{locale:?}: the folder is offered");
        assert!(
            ui.find(tr(*locale, MessageKey::NoticeActionChooseFolder))
                .is_ok(),
            "{locale:?}: the fallback label renders"
        );
    }
}
