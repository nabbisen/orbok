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

/// Task 094 tests 2/3: a catalog reset empties the on-screen list too
/// (the catalog-side half is `CleanupExecutor::run_reset_catalog`,
/// covered separately by `orbok-workers`'s
/// `reset_clears_search_history`), and closes the panel if it was open --
/// but leaves the "Remember recent searches" setting itself untouched.
/// That setting lives in `settings.json`, a file reset never opens
/// (`reset_never_touches_settings_or_model_artifacts`,
/// `crates/pipeline/workers/src/tests/rfc059_reset_erasure.rs`); this is
/// the UI-state half of the same guarantee.
#[test]
fn catalog_reset_clears_the_on_screen_history_but_not_the_setting() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("a"), entry("b")];
    app.search_ui.history_panel_open = true;
    app.remember_recent_searches = true;

    app.update(&Message::CatalogResetSucceeded);

    assert!(
        app.search_ui.history.is_empty(),
        "a reset must clear the on-screen list too"
    );
    assert!(
        !app.search_ui.history_panel_open,
        "a reset closes the panel, the same as RecentSearchesCleared"
    );
    assert!(
        app.remember_recent_searches,
        "the setting is not history -- a reset must not touch it"
    );
}

/// Task 075: turning the setting off shows at once, but the visible list
/// empties only when orbok reports the catalog cleared it.
#[test]
fn toggle_off_clears_visible_history() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("a")];
    app.remember_recent_searches = true;
    app.update(&Message::ToggleRememberRecentSearches(false));
    assert!(!app.remember_recent_searches);
    assert_eq!(
        app.search_ui.history.len(),
        1,
        "the toggle alone clears nothing"
    );
    app.update(&Message::RecentSearchesCleared);
    assert!(app.search_ui.history.is_empty());
}

/// Task 075: `RemoveRecentSearch` is the request; the entry goes on
/// `RecentSearchRemoved`, which orbok sends once the catalog removed it.
#[test]
fn remove_single_entry_from_state() {
    let mut app = AppState::default();
    app.search_ui.history = vec![entry("keep"), entry("drop")];
    let drop_id = app.search_ui.history[1].id.clone();
    app.update(&Message::RemoveRecentSearch(drop_id.clone()));
    assert_eq!(
        app.search_ui.history.len(),
        2,
        "the request alone removes nothing"
    );
    app.update(&Message::RecentSearchRemoved(drop_id));
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
