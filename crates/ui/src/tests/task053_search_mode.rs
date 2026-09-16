//! Task 053: the search-mode selector on a keyword-only install.
//!
//! Conceptual has no keyword half (`hybrid.rs`'s `Limits::for_mode`), so
//! without a model it returns nothing. The view disables it and the reducer
//! drops a stale selection; `run_search` itself stays honest and has no
//! fallback.

use crate::i18n::{MessageKey, tr};
use crate::state::{AppState, Message, WizardState};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_models::SearchCapability;
use orbok_search::SearchMode;

fn advanced_search_state(capability: SearchCapability) -> AppState {
    AppState {
        show_advanced: true,
        capability,
        ..AppState::default()
    }
}

fn conceptual_clicks(state: &AppState) -> usize {
    let mut ui = simulator(views::search_view(state));
    let _ = ui.click(tr(state.locale, MessageKey::SearchModeConceptual));
    ui.into_messages()
        .filter(|m| matches!(m, Message::SetSearchMode(SearchMode::Conceptual)))
        .count()
}

#[test]
fn conceptual_button_is_disabled_on_a_keyword_only_install() {
    let _guard = iced_test_guard();
    // Control: with a model the same click must reach the reducer, so an
    // empty result below means the button is disabled, not that the click
    // missed it.
    assert_eq!(
        conceptual_clicks(&advanced_search_state(SearchCapability::Hybrid)),
        1,
        "with a model, clicking Conceptual must select it"
    );
    let keyword_only = advanced_search_state(SearchCapability::KeywordOnly);
    let mut ui = simulator(views::search_view(&keyword_only));
    assert!(
        ui.find(tr(keyword_only.locale, MessageKey::SearchModeConceptual))
            .is_ok(),
        "Conceptual stays visible on a keyword-only install, only disabled"
    );
    assert_eq!(
        conceptual_clicks(&keyword_only),
        0,
        "without a model, Conceptual must have no on_press"
    );
}

#[test]
fn falling_back_to_keyword_only_resets_a_conceptual_selection() {
    let mut state = AppState {
        capability: SearchCapability::Hybrid,
        search_mode: SearchMode::Conceptual,
        wizard: Some(WizardState::NotConfigured),
        ..AppState::default()
    };
    state.update(&Message::WizardSkip);
    assert_eq!(state.capability, SearchCapability::KeywordOnly);
    assert_eq!(
        state.search_mode,
        SearchMode::Auto,
        "a Conceptual selection cannot survive the loss of the model"
    );
}

#[test]
fn falling_back_to_keyword_only_keeps_exact() {
    let mut state = AppState {
        capability: SearchCapability::Hybrid,
        search_mode: SearchMode::Exact,
        wizard: Some(WizardState::NotConfigured),
        ..AppState::default()
    };
    state.update(&Message::WizardSkip);
    assert_eq!(
        state.search_mode,
        SearchMode::Exact,
        "Exact works without a model and is not reset"
    );
}
