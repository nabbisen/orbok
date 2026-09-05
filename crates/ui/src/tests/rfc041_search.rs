//! RFC-041 acceptance tests: Search, Narrow Results, Browse Around.
//!
//! Covers §24.1 unit tests, §25 acceptance criteria, and §8 copy rules.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, ResultsStatus, SearchResultDisplay, SearchUiState};
use orbok_search::{ActiveFilter, ChangedFilter, KindFilter};

fn make_result(path: &str) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: path.into(),
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

// ── §25.11 / §8.3, RFC-045 §22.12: copy does not contain forbidden terms ──
// Task 041: the two tests this replaced (here and
// `rfc041_search_state.rs::default_ui_copy_avoids_forbidden_technical_terms`,
// now deleted) each checked a hand-curated array of keys -- and RFC-041's
// own §25 criterion 8 acceptance evidence turned out to omit exactly the
// three keys that violate the rule (Review 201 §5(b), Review 202 §6).
// Exhaustive over `crate::i18n::ALL_KEYS` instead: a new `MessageKey`
// added to the catalog is checked the day it exists, not the day someone
// remembers to add it to an array. `EXEMPTIONS` names every case where a
// forbidden term legitimately survives, so an omission has to be a
// deliberate, justified line rather than a silent gap in the array again.
//
// Per-locale term lists, not one list run against both: RFC-041 §8.2's
// list is written in English, and the English substring "source" does not
// appear in a Japanese string that uses the equivalent concept — the
// katakana loanword `ソース` or the kanji compound `情報源` does. Each
// Japanese term below is one already used elsewhere in this catalog for
// that exact concept (verified by grep before use, not invented), so a
// real drift is what gets caught, not a translation choice this test
// happens to disagree with.
const FORBIDDEN_EN: &[&str] = &[
    "source",
    "index",
    "catalog",
    "cache",
    "embedding",
    "vector",
    "bm25",
    "rrf",
    "chunk",
    "query",
    "schema",
    "engine",
    "backend",
];
const FORBIDDEN_JA: &[&str] = &[
    "ソース",
    "情報源",
    "インデックス",
    "索引",
    "カタログ",
    "キャッシュ",
    "埋め込み",
    "エンベディング",
    "ベクトル",
    "チャンク",
    "クエリ",
    "スキーマ",
    "エンジン",
    "バックエンド",
    "BM25",
    "RRF",
];

/// `(key, term)` pairs explicitly permitted to contain that forbidden
/// term, each with the reason on its own line (Task 041 §2). Checked for
/// completeness by `every_exemption_is_load_bearing` below -- an entry
/// that stops matching (because the copy changed) must be removed in the
/// same change, not left as a stale permission nobody re-examines.
const EXEMPTIONS: &[(MessageKey, &str)] = &[
    // Model *provenance* metadata ("Source: <model id>", alongside
    // "Provider:"/"Revision:" rows in the download-consent screen,
    // `ModelDownloadConsent.source` = `DEFAULT_TRUSTED_MODEL.model.id`) --
    // a different sense of "source" than the document/folder concept
    // RFC-041 §8.2 targets. Both locales: en.rs's "Source", ja.rs's
    // "ソース" name the same field.
    (MessageKey::ModelConsentSource, "source"),
    (MessageKey::ModelConsentSource, "ソース"),
];

fn forbidden_terms_for(locale: Locale) -> &'static [&'static str] {
    match locale {
        Locale::En => FORBIDDEN_EN,
        Locale::Ja => FORBIDDEN_JA,
    }
}

fn contains_term(locale: Locale, copy: &str, term: &str) -> bool {
    if locale == Locale::Ja {
        copy.contains(term)
    } else {
        copy.to_lowercase().contains(&term.to_lowercase())
    }
}

#[test]
fn default_ui_copy_avoids_forbidden_terms() {
    let mut violations = Vec::new();
    for &locale in Locale::ALL {
        for &key in crate::i18n::ALL_KEYS {
            let copy = tr(locale, key);
            for &term in forbidden_terms_for(locale) {
                if !contains_term(locale, copy, term) {
                    continue;
                }
                if EXEMPTIONS.contains(&(key, term)) {
                    continue;
                }
                violations.push(format!(
                    "{locale:?} {key:?} contains forbidden term '{term}': {copy:?}"
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "\n{}\n{} violation(s) -- RFC-041 §8.2 / §25 criterion 8, RFC-045 §22 \
         criterion 12. Either fix the copy or add a justified entry to \
         EXEMPTIONS.",
        violations.join("\n"),
        violations.len()
    );
}

/// Every `EXEMPTIONS` entry must actually match something, in at least one
/// locale -- otherwise it is a permission nobody needs any more, which is
/// exactly the kind of stale exception that made the two tests this
/// replaced too permissive in the first place.
#[test]
fn every_exemption_is_load_bearing() {
    for &(key, term) in EXEMPTIONS {
        let matches_somewhere = Locale::ALL
            .iter()
            .any(|&locale| contains_term(locale, tr(locale, key), term));
        assert!(
            matches_somewhere,
            "EXEMPTIONS entry ({key:?}, {term:?}) does not match any locale's \
             copy any more -- remove it"
        );
    }
}

// ── §25.11: Product name is orbok, not orbit ─────────────────────────

#[test]
fn copy_uses_orbok_not_orbit() {
    let all_keys = [
        MessageKey::SearchNarrowResults,
        MessageKey::SourceActionRemoveFromOrbok,
        MessageKey::SourceFilesNotDeletedNotice,
        MessageKey::SourceManyFilesChanged,
    ];
    for key in all_keys {
        let copy = tr(Locale::En, key);
        assert!(
            !copy.contains("orbit"),
            "key {key:?} must say 'orbok', not 'orbit': \"{copy}\""
        );
    }
}
