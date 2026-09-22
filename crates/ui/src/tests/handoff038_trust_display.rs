//! HANDOFF-038 Slices 1 and 2 (RFC-038 criteria 2, 4, 7, 8, 9), extended by
//! Task 082: the trust badge is rendered, and each non-ready result offers
//! every recovery action `trust.recovery_actions` names -- including
//! `OpenAnyway` and `ShowInFolder`, held back until Task 082 lifted
//! HANDOFF-038 §3's hold (both go through the real, catalog-checked launch
//! path; see `result_launch.rs`).

use crate::components::{tone_icon, trust_tone};
use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::state::{
    AppState, Message, ResultTrustDisplay, SearchResultDisplay, SourceCard, ViewId,
};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_search::{
    ResultRecoveryAction as Action, ResultTrustState as Trust, ResultWarningSummary,
};

const NON_READY: [(Trust, MessageKey); 5] = [
    (Trust::NeedsUpdate, MessageKey::TrustNeedsUpdate),
    (Trust::FileNotFound, MessageKey::TrustFileNotFound),
    (
        Trust::StillBeingPrepared,
        MessageKey::TrustStillBeingPrepared,
    ),
    (Trust::PartlyPrepared, MessageKey::TrustPartlyPrepared),
    (Trust::CannotOpen, MessageKey::TrustCannotOpen),
];

fn result(name: &str, trust: ResultTrustDisplay) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: name.into(),
        canonical_path: format!("/docs/{name}"),
        title: Some(name.into()),
        heading_path: None,
        snippet: Some("a snippet".into()),
        keyword_rank: 1,
        badges: vec![],
        trust,
    }
}

fn trust(state: Trust, actions: &[Action]) -> ResultTrustDisplay {
    ResultTrustDisplay {
        state,
        recovery_actions: actions.to_vec(),
        ..ResultTrustDisplay::default()
    }
}

/// Search view state with results showing (a folder exists, a query ran).
fn with_results(locale: Locale, results: Vec<SearchResultDisplay>) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    state.update(&Message::Switch(ViewId::Search));
    state.sources = vec![SourceCard {
        display_name: "Docs".into(),
        display_path: "/docs".into(),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "src-1".into(),
    }];
    state.last_query = Some("q".into());
    state.update(&Message::SearchResultsReady(results));
    state
}

fn finds(state: &AppState, text: &str) -> bool {
    let mut ui = simulator(views::search_view(state));
    ui.find(text).is_ok()
}

fn clicked(state: &AppState, text: &str) -> Vec<Message> {
    let mut ui = simulator(views::search_view(state));
    assert!(ui.find(text).is_ok(), "{text:?} is found");
    let _ = ui.click(text);
    ui.into_messages().collect()
}

// ── Slice 1: the badge ───────────────────────────────────────────────────

/// Criteria 2 and 4: a non-ready result shows its label, and a Ready one
/// shows none of them, in both locales.
#[test]
fn a_non_ready_result_shows_its_trust_label_and_a_ready_one_shows_none() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for (state, key) in NON_READY {
            let app = with_results(locale, vec![result("a.md", trust(state, &[]))]);
            assert!(
                finds(&app, tr(locale, key)),
                "{locale:?}: {state:?} shows {:?}",
                tr(locale, key)
            );
        }
        let ready = with_results(locale, vec![result("a.md", ResultTrustDisplay::default())]);
        for (state, key) in NON_READY {
            assert!(
                !finds(&ready, tr(locale, key)),
                "{locale:?}: a Ready result shows no {state:?} badge"
            );
        }
    }
}

/// Criterion 8: status is not colour alone. Each badge has its own text
/// label, and an icon that is drawn beside it.
#[test]
fn every_trust_badge_has_a_distinct_label_and_an_icon() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let labels: std::collections::HashSet<&str> =
            NON_READY.iter().map(|(_, key)| tr(locale, *key)).collect();
        assert_eq!(
            labels.len(),
            NON_READY.len(),
            "{locale:?}: labels are distinct"
        );
        for (state, _) in NON_READY {
            let app = with_results(locale, vec![result("a.md", trust(state, &[]))]);
            let icon = tone_icon(trust_tone(state)).to_string();
            assert!(
                finds(&app, &icon),
                "{locale:?}: {state:?} draws its icon {icon:?}"
            );
        }
    }
}

// ── Slice 2: the recovery buttons ────────────────────────────────────────

/// Criterion 7, and RFC-038 §9's wireframes: every recovery action a
/// result's trust names is a button that sends its own message -- the
/// rendered row matches `trust.recovery_actions` exactly, in order,
/// including `OpenAnyway` and `ShowInFolder` (Task 082; previously these
/// two were asserted to never render, per HANDOFF-038 §3's now-lifted
/// hold).
#[test]
fn recovery_buttons_match_the_state_and_send_their_action() {
    let _guard = iced_test_guard();
    // (state, the trust's actions, the buttons rendered -- one per action,
    // in the same order, since every action now has a label)
    let cases = [
        (
            Trust::NeedsUpdate,
            vec![Action::PrepareAgain, Action::OpenAnyway],
            vec![
                (MessageKey::TrustActionPrepareAgain, Action::PrepareAgain),
                (MessageKey::TrustActionOpenAnyway, Action::OpenAnyway),
            ],
        ),
        (
            Trust::FileNotFound,
            vec![Action::CheckFolder, Action::RemoveFromResults],
            vec![
                (MessageKey::TrustActionCheckFolder, Action::CheckFolder),
                (
                    MessageKey::TrustActionRemoveFromResults,
                    Action::RemoveFromResults,
                ),
            ],
        ),
        (
            Trust::PartlyPrepared,
            vec![Action::OpenAnyway, Action::ViewDetails],
            vec![
                (MessageKey::TrustActionOpenAnyway, Action::OpenAnyway),
                (MessageKey::TrustActionViewDetails, Action::ViewDetails),
            ],
        ),
        (
            Trust::PartlyPrepared,
            vec![Action::PrepareAgain, Action::ViewDetails],
            vec![
                (MessageKey::TrustActionPrepareAgain, Action::PrepareAgain),
                (MessageKey::TrustActionViewDetails, Action::ViewDetails),
            ],
        ),
        (
            Trust::CannotOpen,
            vec![Action::ShowInFolder],
            vec![(MessageKey::TrustActionShowInFolder, Action::ShowInFolder)],
        ),
        (Trust::StillBeingPrepared, vec![], vec![]),
    ];
    for locale in [Locale::En, Locale::Ja] {
        for (state, actions, buttons) in &cases {
            let app = with_results(locale, vec![result("a.md", trust(*state, actions))]);
            for (key, action) in buttons {
                let messages = clicked(&app, tr(locale, *key));
                assert!(
                    matches!(
                        messages.as_slice(),
                        [Message::TrustRecoveryAction { result_idx: 0, action: a }] if a == action
                    ),
                    "{locale:?} {state:?}: {:?} sends {action:?}, got {messages:?}",
                    tr(locale, *key)
                );
            }
            let expected = buttons.len();
            let rendered = [
                MessageKey::TrustActionPrepareAgain,
                MessageKey::TrustActionCheckFolder,
                MessageKey::TrustActionRemoveFromResults,
                MessageKey::TrustActionViewDetails,
                MessageKey::TrustActionOpenAnyway,
                MessageKey::TrustActionShowInFolder,
            ]
            .iter()
            .filter(|key| finds(&app, tr(locale, **key)))
            .count();
            assert_eq!(rendered, expected, "{locale:?} {state:?}: no extra buttons");
        }
    }
}

/// Task 082 §3 test 2: every non-ready state with any recovery action
/// renders at least one button. `StillBeingPrepared` is the one state with
/// no actions at all -- named explicitly, not skipped silently.
#[test]
fn no_state_with_actions_is_button_less() {
    let _guard = iced_test_guard();
    let cases: [(Trust, Vec<Action>); 6] = [
        (
            Trust::NeedsUpdate,
            vec![Action::PrepareAgain, Action::OpenAnyway],
        ),
        (
            Trust::FileNotFound,
            vec![Action::CheckFolder, Action::RemoveFromResults],
        ),
        (Trust::PartlyPrepared, vec![Action::OpenAnyway]),
        (
            Trust::PartlyPrepared,
            vec![Action::PrepareAgain, Action::ViewDetails],
        ),
        (Trust::CannotOpen, vec![Action::ShowInFolder]),
        (Trust::StillBeingPrepared, vec![]),
    ];
    let all_action_keys = [
        MessageKey::TrustActionPrepareAgain,
        MessageKey::TrustActionCheckFolder,
        MessageKey::TrustActionRemoveFromResults,
        MessageKey::TrustActionViewDetails,
        MessageKey::TrustActionOpenAnyway,
        MessageKey::TrustActionShowInFolder,
    ];
    for locale in [Locale::En, Locale::Ja] {
        for (state, actions) in &cases {
            let app = with_results(locale, vec![result("a.md", trust(*state, actions))]);
            let any_button = all_action_keys
                .iter()
                .any(|key| finds(&app, tr(locale, *key)));
            if actions.is_empty() {
                assert_eq!(
                    *state,
                    Trust::StillBeingPrepared,
                    "only StillBeingPrepared has no actions"
                );
                assert!(
                    !any_button,
                    "{locale:?} {state:?}: no actions means no button"
                );
            } else {
                assert!(
                    any_button,
                    "{locale:?} {state:?}: {actions:?} names an action but no button rendered"
                );
            }
        }
    }
}

/// Criterion 9: the trust detail. It shows for a result whose View details
/// was pressed, and for every non-ready result while Advanced view is on;
/// never for a Ready result. Pressing View details leaves nothing to press.
#[test]
fn view_details_shows_the_detail_and_advanced_view_shows_it_unasked() {
    let _guard = iced_test_guard();
    let partly = ResultTrustDisplay {
        state: Trust::PartlyPrepared,
        recovery_actions: vec![Action::OpenAnyway, Action::ViewDetails],
        warnings: vec![ResultWarningSummary::PossiblyScannedPdf],
    };
    for locale in [Locale::En, Locale::Ja] {
        let state_detail = tr(locale, MessageKey::TrustPartlyPreparedDetail);
        let warning_detail = tr(locale, MessageKey::TrustScannedPdfDetail);

        let mut app = with_results(locale, vec![result("a.pdf", partly.clone())]);
        assert!(!finds(&app, state_detail), "{locale:?}: closed by default");

        app.update(&Message::TrustRecoveryAction {
            result_idx: 0,
            action: Action::ViewDetails,
        });
        assert!(finds(&app, state_detail), "{locale:?}: the state's detail");
        assert!(
            finds(&app, warning_detail),
            "{locale:?}: the warning's detail"
        );
        assert!(
            !finds(&app, tr(locale, MessageKey::TrustActionViewDetails)),
            "{locale:?}: once shown, there is nothing left to press"
        );

        let mut advanced = with_results(locale, vec![result("a.pdf", partly.clone())]);
        advanced.show_advanced = true;
        assert!(
            finds(&advanced, state_detail),
            "{locale:?}: Advanced shows it unasked"
        );

        let mut ready = with_results(locale, vec![result("a.pdf", ResultTrustDisplay::default())]);
        ready.show_advanced = true;
        assert!(
            !finds(&ready, state_detail),
            "{locale:?}: a Ready result has no detail"
        );
    }
}

/// New results replace the list, so their details start closed.
#[test]
fn new_results_close_every_detail() {
    let partly = trust(
        Trust::PartlyPrepared,
        &[Action::OpenAnyway, Action::ViewDetails],
    );
    let mut app = with_results(Locale::En, vec![result("a.pdf", partly.clone())]);
    app.update(&Message::TrustRecoveryAction {
        result_idx: 0,
        action: Action::ViewDetails,
    });
    assert!(!app.search_ui.trust_details_open.is_empty());
    app.update(&Message::SearchResultsReady(vec![result("a.pdf", partly)]));
    assert!(app.search_ui.trust_details_open.is_empty());
}

// ── Slice 2: what the reducer does with each action ─────────────────────

/// `RemoveFromResults`: one row leaves the list, and nothing else changes
/// that a stale index could break -- the selection, the count, and a launch
/// notice whose retry is a result index (Task 065's reason).
#[test]
fn removing_a_result_drops_only_that_row_and_keeps_the_rest_consistent() {
    let gone = trust(
        Trust::FileNotFound,
        &[Action::CheckFolder, Action::RemoveFromResults],
    );
    let mut app = with_results(
        Locale::En,
        vec![
            result("a.md", gone.clone()),
            result("b.md", ResultTrustDisplay::default()),
            result("c.md", ResultTrustDisplay::default()),
        ],
    );
    app.update(&Message::SelectResult(2));
    app.update(&Message::TrustRecoveryAction {
        result_idx: 0,
        action: Action::RemoveFromResults,
    });
    let names: Vec<_> = app
        .search_results
        .iter()
        .map(|r| r.display_path.as_str())
        .collect();
    assert_eq!(names, ["b.md", "c.md"], "only the first row is gone");
    assert_eq!(
        app.selected_result,
        Some(1),
        "the selection follows its row"
    );
    assert!(matches!(
        app.search_ui.results_status,
        crate::state::ResultsStatus::Ready { total_count: 2 }
    ));

    // Removing the selected row leaves nothing selected.
    app.update(&Message::TrustRecoveryAction {
        result_idx: 1,
        action: Action::RemoveFromResults,
    });
    assert_eq!(app.selected_result, None);

    // Removing the last row leaves the empty-results state, not a stale count.
    app.update(&Message::TrustRecoveryAction {
        result_idx: 0,
        action: Action::RemoveFromResults,
    });
    assert!(app.search_results.is_empty());
    assert!(matches!(
        app.search_ui.results_status,
        crate::state::ResultsStatus::EmptyAfterSearch
    ));

    // An index with no row changes nothing.
    app.update(&Message::TrustRecoveryAction {
        result_idx: 9,
        action: Action::RemoveFromResults,
    });
    assert!(app.search_results.is_empty());
}

#[test]
fn removing_a_result_clears_a_launch_notice_whose_retry_is_an_index() {
    let gone = trust(
        Trust::FileNotFound,
        &[Action::CheckFolder, Action::RemoveFromResults],
    );
    let mut app = with_results(
        Locale::En,
        vec![
            result("a.md", gone),
            result("b.md", ResultTrustDisplay::default()),
        ],
    );
    app.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::FileCheckFailed,
        action: Box::new(Message::RevealResult(1)),
    });
    app.update(&Message::TrustRecoveryAction {
        result_idx: 0,
        action: Action::RemoveFromResults,
    });
    assert_eq!(
        app.notice, None,
        "RevealResult(1) would now point at another file"
    );

    // An unrelated problem stays (Task 064).
    app.update(&Message::ShowNoticeWithAction {
        notice: UserNotice::StorageUnavailable,
        action: Box::new(Message::CleanSnippets),
    });
    app.update(&Message::TrustRecoveryAction {
        result_idx: 0,
        action: Action::RemoveFromResults,
    });
    assert_eq!(app.notice, Some(UserNotice::StorageUnavailable));
}

/// `PrepareAgain` and `CheckFolder` touch the catalog, so the reducer
/// leaves them to orbok; the row relabels only once the job is queued.
#[test]
fn the_catalog_actions_change_nothing_until_orbok_reports_them() {
    let stale = trust(
        Trust::NeedsUpdate,
        &[Action::PrepareAgain, Action::OpenAnyway],
    );
    let mut app = with_results(Locale::En, vec![result("a.md", stale.clone())]);
    for action in [Action::PrepareAgain, Action::CheckFolder] {
        app.update(&Message::TrustRecoveryAction {
            result_idx: 0,
            action,
        });
        assert_eq!(
            app.search_results[0].trust, stale,
            "{action:?} alone changes nothing"
        );
    }

    app.update(&Message::ResultPreparationQueued { result_idx: 0 });
    let row = &app.search_results[0].trust;
    assert_eq!(row.state, Trust::StillBeingPrepared);
    assert!(row.recovery_actions.is_empty(), "nothing left to press");
    assert!(finds(
        &app,
        tr(Locale::En, MessageKey::TrustStillBeingPrepared)
    ));
    assert!(!finds(
        &app,
        tr(Locale::En, MessageKey::TrustActionPrepareAgain)
    ));
}
