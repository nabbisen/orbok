//! RFC-042 PR — search history UI state and copy.
//!
//! Validates the message/update behavior (RFC-042 §13) and the
//! forbidden-vocabulary copy rule (§6.2) at the view-model level.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message};
use orbok_core::{SearchHistoryEntry, SearchHistoryId, StoredSearchFilter};

fn entry(text: &str) -> SearchHistoryEntry {
    SearchHistoryEntry {
        id: SearchHistoryId::new(format!("h-{text}")),
        search_text: text.to_string(),
        filters: vec![StoredSearchFilter::Kind {
            value: orbok_core::StoredKindFilter::Pdfs,
            label: "PDFs".to_string(),
        }],
        created_at: "2026-06-21T10:00:00Z".to_string(),
        last_used_at: "2026-06-21T10:00:00Z".to_string(),
        previous_result_count: Some(3),
        locale: "en".to_string(),
    }
}

#[test]
fn open_and_close_recent_searches_panel() {
    let mut app = AppState::default();
    assert!(!app.search_ui.history_panel_open);
    app.update(&Message::OpenRecentSearches);
    assert!(app.search_ui.history_panel_open);
    app.update(&Message::CloseRecentSearches);
    assert!(!app.search_ui.history_panel_open);
}

#[test]
fn search_again_sets_restoring_and_searching() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("audit log")];
    let id = app.search_ui.history[0].id.clone();
    app.update(&Message::SearchAgain(id.clone()));
    assert_eq!(app.search_ui.restoring_history_id, Some(id.clone()));
    assert!(!app.search_ui.history_panel_open);
    // Finalizing the restore clears the restoring marker.
    app.update(&Message::RecentSearchRestored(id));
    assert!(app.search_ui.restoring_history_id.is_none());
}

#[test]
fn clear_confirmation_flow() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("a"), entry("b")];
    app.update(&Message::AskClearRecentSearches);
    assert!(app.confirm_clear_history);
    // Cancel keeps history.
    app.update(&Message::CancelClearRecentSearches);
    assert!(!app.confirm_clear_history);
    assert_eq!(app.search_ui.history.len(), 2);
    // Confirm + cleared empties the list.
    app.update(&Message::AskClearRecentSearches);
    app.update(&Message::RecentSearchesCleared);
    assert!(app.search_ui.history.is_empty());
    assert!(!app.confirm_clear_history);
}

#[test]
fn toggle_off_clears_visible_history() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("a")];
    app.remember_recent_searches = true;
    app.update(&Message::ToggleRememberRecentSearches(false));
    assert!(!app.remember_recent_searches);
    assert!(app.search_ui.history.is_empty());
}

#[test]
fn remove_single_entry_from_state() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("keep"), entry("drop")];
    let drop_id = app.search_ui.history[1].id.clone();
    app.update(&Message::RemoveRecentSearch(drop_id));
    assert_eq!(app.search_ui.history.len(), 1);
    assert_eq!(app.search_ui.history[0].search_text, "keep");
}

// RFC-042 §6.2: forbidden technical terms must not appear in default copy.
#[test]
fn copy_avoids_forbidden_terms() {
    let keys = [
        MessageKey::RecentSearchesLabel,
        MessageKey::SearchAgainButton,
        MessageKey::ClearRecentSearches,
        MessageKey::RememberRecentSearches,
        MessageKey::RecentSearchesPrivacyNote,
        MessageKey::ClearRecentSearchesConfirmTitle,
        MessageKey::ClearRecentSearchesConfirmBody,
        MessageKey::NoRecentSearches,
    ];
    // §6.2 forbidden default labels (English check).
    let forbidden = [
        "query",
        "snapshot",
        "session",
        "workspace",
        "rehydrate",
        "database",
        "history table",
        "persisted",
    ];
    for key in keys {
        let s = tr(Locale::En, key).to_lowercase();
        for bad in forbidden {
            assert!(
                !s.contains(bad),
                "copy {s:?} contains forbidden term {bad:?}"
            );
        }
    }
}

// RFC-042 §6.3: app name is orbok in the clear-confirmation body.
#[test]
fn clear_body_uses_app_name() {
    let s = tr(Locale::En, MessageKey::ClearRecentSearchesConfirmBody);
    assert!(s.contains("orbok"));
}
