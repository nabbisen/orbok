//! HANDOFF-041 §3/§4: the selected result's file actions, and Enter.

use crate::i18n::{MessageKey, tr};
use crate::shell::{KeyboardContext, key_to_message};
use crate::state::{AppState, Message, SearchResultDisplay, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn state_with_results(selected: Option<usize>) -> AppState {
    let result = |name: &str| SearchResultDisplay {
        display_path: name.into(),
        canonical_path: format!("/docs/{name}"),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    };
    let mut state = AppState {
        sources: vec![SourceCard {
            display_name: "Docs".into(),
            display_path: "/docs".into(),
            indexed: 2,
            stale: 0,
            failed: 0,
            no_text_found: 0,
            unfinished_jobs: 0,
            covers_subfolders: true,
            status: orbok_core::SourceStatus::Active,
            source_id: "src-1".into(),
        }],
        ..AppState::default()
    };
    state.update(&Message::SearchResultsReady(vec![
        result("a.md"),
        result("b.md"),
    ]));
    // Results render only under a completed query.
    state.last_query = Some("docs".into());
    if let Some(index) = selected {
        state.update(&Message::SelectResult(index));
    }
    state
}

fn clicked(state: &AppState, key: MessageKey) -> Vec<Message> {
    let mut ui = simulator(views::search_view(state));
    let _ = ui.click(tr(state.locale, key));
    ui.into_messages().collect()
}

#[test]
fn the_selected_result_offers_open_and_show_in_folder() {
    let _guard = iced_test_guard();
    let state = state_with_results(Some(1));
    assert!(
        clicked(&state, MessageKey::SearchResultOpenFile)
            .iter()
            .any(|m| matches!(m, Message::OpenResult(1))),
        "Open file on the selected row sends OpenResult with its index"
    );
    assert!(
        clicked(&state, MessageKey::SearchResultShowInFolder)
            .iter()
            .any(|m| matches!(m, Message::RevealResult(1))),
        "Show in folder on the selected row sends RevealResult with its index"
    );
}

#[test]
fn no_result_selected_means_no_file_actions() {
    let _guard = iced_test_guard();
    let state = state_with_results(None);
    let mut ui = simulator(views::search_view(&state));
    // Control: the results themselves render, so the absence below is the
    // actions missing, not the whole list.
    assert!(ui.find("a.md").is_ok(), "fixture: the results render");
    assert!(
        ui.find(tr(state.locale, MessageKey::SearchResultOpenFile))
            .is_err(),
        "the actions belong to the selected result only"
    );
}

fn search_ctx(text_input_focused: bool, selected_result: Option<usize>) -> KeyboardContext {
    KeyboardContext {
        text_input_focused,
        active_view: ViewId::Search,
        confirm_reset: false,
        confirm_remove_source: false,
        confirm_clear_history: false,
        confirm_delete_keyword_index: false,
        confirm_delete_vector_index: false,
        confirm_add_sensitive_folder: false,
        confirm_narrow_folder: false,
        wizard_kind: None,
        selected_source_id: None,
        selected_result,
    }
}

#[test]
fn enter_opens_the_selected_result_only_when_not_typing() {
    let enter = Key::Named(Named::Enter);
    let none = Modifiers::default();
    assert!(matches!(
        key_to_message(&enter, none, &search_ctx(false, Some(1))),
        Some(Message::OpenResult(1))
    ));
    assert!(
        matches!(
            key_to_message(&enter, none, &search_ctx(true, Some(1))),
            Some(Message::SubmitSearch)
        ),
        "Enter in the search box still submits the search"
    );
    assert!(
        key_to_message(&enter, none, &search_ctx(false, None)).is_none(),
        "with nothing selected, Enter on the Search view does nothing, as before"
    );
    assert!(
        matches!(
            key_to_message(
                &enter,
                none,
                &KeyboardContext {
                    confirm_reset: true,
                    confirm_remove_source: false,
                    ..search_ctx(false, Some(1))
                }
            ),
            Some(Message::ConfirmResetCatalog)
        ),
        "an open confirm dialog still wins over a selected result"
    );
}
