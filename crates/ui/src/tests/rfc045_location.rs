//! RFC-045 PR 1 — search-location state and data types.
//!
//! These validate the design spec (RFC-045 §5, §6.3, §7, §17), not just
//! the code: default state has no location, default scope reaches into
//! subfolders, chip labels read in friendly "folder" language in both
//! locales, and changing scope never changes the remembered-folder
//! identity (RFC-045 §6.3).

use crate::i18n::Locale;
use crate::state::{
    AppState, SearchFolderScope, SearchLocation, SearchLocationState, SearchLocationSummary,
};
use orbok_core::id::SourceId;

fn sample_location() -> SearchLocation {
    SearchLocation::remembered(SourceId::from_string("src_documents"), "Documents")
}

#[test]
fn default_location_state_has_nothing_selected() {
    let state = SearchLocationState::default();
    assert!(state.selected.is_none());
    assert!(!state.has_selected());
    assert!(state.recent_locations.is_empty());
    assert!(!state.picker_in_progress);
}

#[test]
fn app_state_default_has_no_search_location() {
    // RFC-045 §7.1: first run starts with no chosen folder.
    let app = AppState::default();
    assert!(app.search_location.selected.is_none());
}

#[test]
fn default_scope_is_folder_and_subfolders() {
    // RFC-045 §6.3: recursive search is the safe default.
    assert_eq!(
        SearchFolderScope::default(),
        SearchFolderScope::FolderAndSubfolders
    );
    assert!(SearchFolderScope::default().includes_subfolders());
    assert!(!SearchFolderScope::FolderOnly.includes_subfolders());
}

#[test]
fn remembered_constructor_uses_default_scope() {
    let location = sample_location();
    assert_eq!(location.display_name(), "Documents");
    assert_eq!(location.scope(), SearchFolderScope::FolderAndSubfolders);
    assert_eq!(
        location.source_id().map(|id| id.as_str()),
        Some("src_documents")
    );
}

/// RFC-045 §19.4 forbidden default copy, for the two scope labels the search
/// row and the folder card show (Task 118 replaced the combined chip label with
/// the folder's own name beside a choice of these two).
#[test]
fn scope_labels_never_say_source_or_recursive() {
    use crate::i18n::{MessageKey, tr};
    for locale in Locale::ALL {
        for key in [
            MessageKey::SearchScopeSubfolders,
            MessageKey::SearchScopeOnly,
        ] {
            let label = tr(*locale, key);
            let lowered = label.to_lowercase();
            assert!(!lowered.contains("source"), "leaked 'source': {label}");
            assert!(
                !lowered.contains("recursive"),
                "leaked 'recursive': {label}"
            );
        }
    }
}

#[test]
fn changing_scope_preserves_folder_identity() {
    // RFC-045 §6.3: scope is a search-time restriction; it must not change
    // which remembered folder this is.
    let mut state = SearchLocationState {
        selected: Some(sample_location()),
        ..Default::default()
    };
    let before_id = state.selected.as_ref().and_then(|l| l.source_id()).cloned();

    state.set_scope(SearchFolderScope::FolderOnly);

    let after = state.selected.as_ref().expect("location still selected");
    assert_eq!(after.scope(), SearchFolderScope::FolderOnly);
    assert_eq!(after.source_id().cloned(), before_id);
    assert_eq!(after.display_name(), "Documents");
}

#[test]
fn set_scope_is_noop_without_selection() {
    let mut state = SearchLocationState::default();
    state.set_scope(SearchFolderScope::FolderOnly);
    assert!(state.selected.is_none());
}

#[test]
fn clearing_location_preserves_query_text() {
    // RFC-045 §11.3: clearing the chip keeps the typed query.
    let mut app = AppState {
        query: "renewal policy".to_string(),
        ..Default::default()
    };
    app.search_location.selected = Some(sample_location());

    app.search_location.clear();

    assert!(app.search_location.selected.is_none());
    assert_eq!(app.query, "renewal policy");
}

#[test]
fn recent_location_summary_carries_name_and_id() {
    let summary = SearchLocationSummary {
        source_id: SourceId::from_string("src_downloads"),
        display_name: "Downloads".to_string(),
    };
    assert_eq!(summary.display_name, "Downloads");
    assert_eq!(summary.source_id.as_str(), "src_downloads");
}
