//! RFC-041 search UI state tests (§24.1, §24.4 copy compliance).

use crate::AppState;
use crate::state::Message;
use crate::state::search::{ResultsStatus, SearchUiState};
use orbok_search::filter::{ActiveFilter, ChangedFilter, KindFilter};

// ── SearchUiState operations ──────────────────────────────────────────

#[test]
fn apply_suggested_adds_filter() {
    use orbok_search::filter::SuggestedFilter;
    let mut state = SearchUiState {
        suggested_filters: vec![SuggestedFilter {
            filter: ActiveFilter::Kind {
                value: KindFilter::Pdfs,
                label: "PDFs".into(),
            },
            estimated_result_count: 5,
        }],
        ..Default::default()
    };
    state.apply_suggested(0);
    assert_eq!(state.active_filters.len(), 1);
}

#[test]
fn apply_suggested_does_not_duplicate() {
    use orbok_search::filter::SuggestedFilter;
    let mut state = SearchUiState::default();
    state.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    state.suggested_filters = vec![SuggestedFilter {
        filter: ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        estimated_result_count: 5,
    }];
    state.apply_suggested(0);
    assert_eq!(state.active_filters.len(), 1, "no duplicate filter added");
}

#[test]
fn remove_filter_removes_only_that_index() {
    let mut state = SearchUiState {
        active_filters: vec![
            ActiveFilter::Kind {
                value: KindFilter::Pdfs,
                label: "PDFs".into(),
            },
            ActiveFilter::Changed {
                value: ChangedFilter::ThisWeek,
                label: "This week".into(),
            },
        ],
        ..Default::default()
    };
    state.remove_filter(0);
    assert_eq!(state.active_filters.len(), 1);
    assert!(matches!(
        &state.active_filters[0],
        ActiveFilter::Changed { .. }
    ));
}

#[test]
fn clear_filters_leaves_text_untouched() {
    // RFC-041 §15.3: Clear does not clear search text.
    let mut state = SearchUiState {
        text: "authentication token".into(),
        ..Default::default()
    };
    state.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    state.clear_filters();
    assert!(state.active_filters.is_empty());
    assert_eq!(
        state.text, "authentication token",
        "search text must be preserved"
    );
}

#[test]
fn has_active_filters_reflects_state() {
    let mut state = SearchUiState::default();
    assert!(!state.has_active_filters());
    state.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    assert!(state.has_active_filters());
}

// ── App state message handling ────────────────────────────────────────

#[test]
fn query_changed_message_updates_both_query_and_search_ui_text() {
    let mut state = AppState::default();
    state.update(&Message::QueryChanged("auth".into()));
    assert_eq!(state.query, "auth");
    assert_eq!(state.search_ui.text, "auth");
}

#[test]
fn clear_filters_message_clears_without_touching_text() {
    let mut state = AppState {
        query: "token".into(),
        search_ui: SearchUiState {
            text: "token".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    state.search_ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    state.update(&Message::ClearFilters);
    assert!(state.search_ui.active_filters.is_empty());
    assert_eq!(state.search_ui.text, "token");
}

#[test]
fn open_more_ways_message_opens_panel() {
    let mut state = AppState::default();
    assert!(!state.search_ui.more_panel_open);
    state.update(&Message::OpenMoreWays);
    assert!(state.search_ui.more_panel_open);
    state.update(&Message::CloseMoreWays);
    assert!(!state.search_ui.more_panel_open);
}

// ── Results status transitions ────────────────────────────────────────

#[test]
fn submit_search_sets_searching_status() {
    let mut state = AppState {
        query: "auth".into(),
        ..Default::default()
    };
    state.update(&Message::SubmitSearch);
    assert!(matches!(
        state.search_ui.results_status,
        ResultsStatus::Searching
    ));
}

#[test]
fn search_results_ready_sets_correct_status() {
    use crate::state::{ResultTrustDisplay, SearchResultDisplay};
    let mut state = AppState::default();
    state.update(&Message::SearchResultsReady(vec![SearchResultDisplay {
        display_path: "docs/auth.md".into(),
        title: Some("auth".into()),
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: ResultTrustDisplay::default(),
    }]));
    assert!(matches!(
        state.search_ui.results_status,
        ResultsStatus::Ready { total_count: 1 }
    ));
}

#[test]
fn empty_results_with_filters_sets_empty_after_filtering() {
    let mut state = AppState::default();
    state.search_ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    state.update(&Message::SearchResultsReady(vec![]));
    assert!(matches!(
        state.search_ui.results_status,
        ResultsStatus::EmptyAfterFiltering
    ));
}

// ── Copy compliance: default UI avoids forbidden terms ────────────────

#[test]
fn default_ui_copy_avoids_forbidden_technical_terms() {
    // RFC-041 §8.2: forbidden terms in default UI.
    use crate::i18n::{Locale, MessageKey, tr};
    let forbidden = [
        "query",
        "source",
        "index",
        "cache",
        "vector",
        "embedding",
        "bm25",
        "rrf",
        "chunk",
        "schema",
        "engine",
        "backend",
    ];
    let filter_keys = [
        MessageKey::SearchNarrowResults,
        MessageKey::SearchNarrowedBy,
        MessageKey::SearchMoreWays,
        MessageKey::SearchClearFilters,
        MessageKey::SearchNoResultsFiltered,
        MessageKey::SearchInThisFolder,
        MessageKey::FilterKind,
        MessageKey::FilterChanged,
        MessageKey::FilterSearchIn,
        MessageKey::FilterKindPdfs,
        MessageKey::FilterKindNotes,
        MessageKey::FilterKindCode,
        MessageKey::FilterAllFolders,
    ];
    for key in filter_keys {
        let copy = tr(Locale::En, key).to_lowercase();
        for term in &forbidden {
            assert!(
                !copy.contains(term),
                "i18n key {key:?} copy '{copy}' contains forbidden term '{term}'"
            );
        }
    }
}

#[test]
fn project_name_is_orbok_not_orbit() {
    // RFC-041 §8.3 / §2: use orbok, not orbit.
    use crate::i18n::{Locale, MessageKey, tr};
    let keys = [
        MessageKey::SearchNoResultsFiltered,
        MessageKey::SourceFilesNotDeletedNotice,
        MessageKey::SourceFolderNotFoundDetail,
    ];
    for key in keys {
        let copy = tr(Locale::En, key).to_lowercase();
        assert!(
            !copy.contains("orbit"),
            "i18n key {key:?} still uses former name 'orbit': '{copy}'"
        );
    }
}
