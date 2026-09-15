//! Task 047 §2: one add-folder dialog at a time, mirroring RFC-045's
//! `search_location.picker_in_progress`.

use crate::state::{AppState, Message};

#[test]
fn add_source_picker_flag_is_set_on_open_and_cleared_on_cancel_or_finished_pick() {
    let mut state = AppState::default();
    assert!(!state.add_source_picker_in_progress);

    state.update(&Message::RequestAddSource);
    assert!(
        state.add_source_picker_in_progress,
        "opening the add-folder dialog must set the flag"
    );

    state.update(&Message::AddSourceFolderPickerCancelled);
    assert!(
        !state.add_source_picker_in_progress,
        "cancelling the dialog must clear the flag"
    );

    state.update(&Message::RequestAddSource);
    state.update(&Message::AddSourceFolderPicked(std::path::PathBuf::from(
        "notes",
    )));
    assert!(
        !state.add_source_picker_in_progress,
        "a finished pick must clear the flag"
    );
}
