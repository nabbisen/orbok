//! RFC-041 acceptance tests: Search, Narrow Results, Browse Around.
//!
//! Covers §24.1 unit tests, §25 acceptance criteria, and §8 copy rules.

use crate::state::{AppState, Message, ResultsStatus, SearchResultDisplay, SearchUiState};
use orbok_search::{ActiveFilter, ChangedFilter, KindFilter};

fn make_result(path: &str) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: path.into(),
        canonical_path: path.into(),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    }
}

fn state_with_results(n: usize) -> AppState {
    let mut s = AppState::default();
    let results: Vec<_> = (0..n)
        .map(|i| make_result(&format!("file{i}.md")))
        .collect();
    s.update(&Message::SearchResultsReady(results));
    s
}

// ── §25.1: No filter form before first search ─────────────────────────

#[test]
fn no_active_filters_before_first_search() {
    let s = AppState::default();
    assert!(s.search_ui.active_filters.is_empty());
    assert!(s.search_ui.suggested_filters.is_empty());
    assert!(!s.search_ui.more_panel_open);
    assert_eq!(s.search_ui.results_status, ResultsStatus::NotSearchedYet);
}

// ── §25.3 / §24.1: Active filter add / remove / clear ────────────────

#[test]
fn apply_suggested_filter_adds_to_active() {
    let mut ui = SearchUiState::default();
    use orbok_search::SuggestedFilter;
    ui.suggested_filters.push(SuggestedFilter {
        filter: ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        estimated_result_count: 4,
    });
    ui.apply_suggested(0);
    assert_eq!(ui.active_filters.len(), 1);
    assert_eq!(ui.active_filters[0].label(), "PDFs");
}

#[test]
fn apply_suggested_does_not_duplicate() {
    let mut ui = SearchUiState::default();
    use orbok_search::SuggestedFilter;
    let sf = SuggestedFilter {
        filter: ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        estimated_result_count: 4,
    };
    ui.suggested_filters.push(sf.clone());
    ui.suggested_filters.push(sf);
    ui.apply_suggested(0);
    ui.apply_suggested(1);
    assert_eq!(
        ui.active_filters.len(),
        1,
        "duplicate kind filter must not be added"
    );
}

#[test]
fn remove_one_filter_removes_only_that() {
    let mut ui = SearchUiState::default();
    ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Notes,
        label: "Notes".into(),
    });
    ui.remove_filter(0);
    assert_eq!(ui.active_filters.len(), 1);
    assert_eq!(ui.active_filters[0].label(), "Notes");
}

#[test]
fn clear_filters_removes_all_preserves_nothing() {
    let mut ui = SearchUiState::default();
    ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    ui.active_filters.push(ActiveFilter::Changed {
        value: ChangedFilter::ThisWeek,
        label: "This week".into(),
    });
    ui.clear_filters();
    assert!(ui.active_filters.is_empty());
}

// ── §25.4: Clear does not clear search text ───────────────────────────

#[test]
fn clear_filters_preserves_search_text() {
    let mut s = AppState::default();
    s.update(&Message::QueryChanged("token rotation".into()));
    s.update(&Message::ApplySuggestedFilter(0)); // no-op, no suggestions
    s.update(&Message::ClearFilters);
    assert_eq!(s.query, "token rotation", "search text must be preserved");
}

// ── §25.2: Results show status ────────────────────────────────────────

#[test]
fn ready_status_set_after_results() {
    let s = state_with_results(5);
    assert_eq!(
        s.search_ui.results_status,
        ResultsStatus::Ready { total_count: 5 }
    );
}

#[test]
fn empty_after_search_when_no_results_no_filters() {
    let mut s = AppState::default();
    s.update(&Message::SearchResultsReady(vec![]));
    assert_eq!(s.search_ui.results_status, ResultsStatus::EmptyAfterSearch);
}

#[test]
fn empty_after_filtering_when_filters_active() {
    let mut s = AppState::default();
    // Manually set an active filter to simulate the filtered case.
    s.search_ui.active_filters.push(ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    });
    s.update(&Message::SearchResultsReady(vec![]));
    assert_eq!(
        s.search_ui.results_status,
        ResultsStatus::EmptyAfterFiltering
    );
}

// ── §25.6: More ways panel open/close ────────────────────────────────

#[test]
fn more_ways_panel_opens_and_closes() {
    let mut s = AppState::default();
    s.update(&Message::OpenMoreWays);
    assert!(s.search_ui.more_panel_open);
    s.update(&Message::CloseMoreWays);
    assert!(!s.search_ui.more_panel_open);
}

// §25.8 / §25.11 / §8.2 / §8.3, RFC-045 §22.12 (copy avoids forbidden
// terms, including the former project name), and §24.1's "one Japanese
// word for folder" property are now checked by `crate::tests::glossary`'s
// `default_ui_copy_follows_the_glossary` and
// `every_glossary_exemption_is_load_bearing`, which replaced the guards
// this file and `i18n.rs` used to carry (Task 100).
