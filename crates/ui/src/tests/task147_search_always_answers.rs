//! Task 147: Search always answers (Review 324 §3). With folders added
//! but none chosen, pressing Search (or Enter) used to store the query
//! and change nothing on screen. Two fixes:
//! - with exactly one usable folder added, it becomes the chosen
//!   location automatically -- nothing to choose between, nothing asked;
//! - with two or more, pressing Search (or Enter) now says, inline,
//!   that a folder needs choosing, instead of silently doing nothing.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, SearchLocation, SourceCard};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_core::SourceId;

fn folder(id: &str, name: &str) -> SourceCard {
    SourceCard {
        display_name: name.into(),
        display_path: format!("/docs/{name}"),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        covers_subfolders: true,
        status: orbok_core::SourceStatus::Active,
        source_id: id.into(),
    }
}

// ── §1.1: one usable folder is chosen by default ────────────────────────

#[test]
fn one_folder_added_none_chosen_is_chosen_automatically() {
    let mut state = AppState::default();
    state.update(&Message::SourceAdded(folder("s_docs", "Docs")));
    assert_eq!(
        state.search_location.selected,
        Some(SearchLocation::remembered(
            SourceId::from_string("s_docs"),
            "Docs"
        )),
        "the one usable folder just added becomes the chosen location"
    );
}

/// Review 326 §3.1 item 2: the *first* `SourcesLoaded` of a session (a
/// folder added in an earlier session, restored at startup) is one of
/// the three moments this rule applies -- not only `SourceAdded`. This
/// is the exact gap the review named: "every 0.30.0 user with one
/// folder, after upgrading."
#[test]
fn one_folder_present_at_the_first_load_is_chosen() {
    let mut state = AppState::default();
    assert!(!state.sources_loaded_once);
    state.update(&Message::SourcesLoaded(vec![folder("s_docs", "Docs")]));
    assert_eq!(
        state.search_location.selected,
        Some(SearchLocation::remembered(
            SourceId::from_string("s_docs"),
            "Docs"
        )),
        "the one usable folder present at the first load is chosen"
    );
    assert!(state.sources_loaded_once);
}

/// A later `SourcesLoaded` (a refresh while files are prepared, the
/// common case) must not re-run the rule -- it would silently undo a
/// `×` a moment after the user pressed it.
#[test]
fn a_later_sources_loaded_does_not_re_choose_after_the_user_cleared_it() {
    let mut state = AppState::default();
    state.update(&Message::SourcesLoaded(vec![folder("s_docs", "Docs")]));
    assert!(state.search_location.selected.is_some());
    state.update(&Message::SearchLocationCleared);
    assert!(state.search_location.selected.is_none());

    // A refresh with the same one folder -- not the first load any more.
    state.update(&Message::SourcesLoaded(vec![folder("s_docs", "Docs")]));
    assert!(
        state.search_location.selected.is_none(),
        "a later SourcesLoaded must not re-choose what the user just cleared"
    );

    // Review 326 §3.1 item 3: pressing Search is still the third
    // moment -- modelled directly here since the router (where
    // `SubmitSearch` actually calls this) is a different crate.
    state.choose_the_only_usable_folder_if_nothing_is_chosen();
    assert_eq!(
        state.search_location.selected,
        Some(SearchLocation::remembered(
            SourceId::from_string("s_docs"),
            "Docs"
        )),
        "Search still chooses the only usable folder afterward"
    );
}

/// With two or more usable folders present from the start (the realistic
/// case this bears on: two folders already added in an earlier session,
/// loaded together at startup), nothing is preselected -- the choice
/// matters there (unchanged from Task 132). Adding them one at a time
/// through `SourceAdded` is a different case, covered below
/// (`an_already_chosen_location_is_never_overridden_by_a_later_add`):
/// the first add already triggers §1.1's own auto-select, so by the
/// time a second folder is added, something is already chosen.
#[test]
fn two_folders_present_from_the_start_none_chosen() {
    let mut state = AppState::default();
    state.update(&Message::SourcesLoaded(vec![
        folder("s_docs", "Docs"),
        folder("s_notes", "Notes"),
    ]));
    assert_eq!(
        state.search_location.selected, None,
        "with two folders, neither is preselected"
    );
}

/// A paused (not searchable) folder does not count: with one active
/// folder already chosen-by-default and a second, paused one added,
/// there is still only one *usable* folder, so the selection does not
/// change (and does not become ambiguous).
#[test]
fn a_paused_folder_does_not_count_as_a_second_usable_one() {
    let mut state = AppState::default();
    state.update(&Message::SourceAdded(folder("s_docs", "Docs")));
    let mut paused = folder("s_old", "Old");
    paused.status = orbok_core::SourceStatus::Paused;
    state.update(&Message::SourceAdded(paused));
    assert_eq!(
        state.search_location.selected,
        Some(SearchLocation::remembered(
            SourceId::from_string("s_docs"),
            "Docs"
        )),
        "a paused folder must not count toward \"exactly one usable\""
    );
}

/// If a location was already chosen (even if it's later cleared and a
/// single folder remains), adding another folder must never silently
/// override an existing explicit choice.
#[test]
fn an_already_chosen_location_is_never_overridden_by_a_later_add() {
    let mut state = AppState::default();
    state.update(&Message::SourceAdded(folder("s_docs", "Docs")));
    state.update(&Message::SourceAdded(folder("s_notes", "Notes")));
    state.update(&Message::ExistingSearchFolderChosen(SourceId::from_string(
        "s_notes",
    )));
    // `ExistingSearchFolderChosen` itself is handled in orbok (router);
    // the reducer here only models what SearchLocationSelected would
    // set once the router resolves it -- done directly for this test.
    state.update(&Message::SearchLocationSelected(
        SearchLocation::remembered(SourceId::from_string("s_notes"), "Notes"),
    ));
    state.update(&Message::SourceAdded(folder("s_third", "Third")));
    assert_eq!(
        state.search_location.selected,
        Some(SearchLocation::remembered(
            SourceId::from_string("s_notes"),
            "Notes"
        )),
        "adding a third folder must not disturb an already-chosen location"
    );
}

// ── §1.2: Search without a location says so ─────────────────────────────

fn clicked(state: &AppState, label: &str) -> Vec<Message> {
    let mut ui = simulator(views::search_view(state));
    let _ = ui.click(label);
    ui.into_messages().collect()
}

/// Two folders, none chosen, Search pressed: the line appears beside the
/// choices, and the query is kept (not cleared).
#[test]
fn two_folders_none_chosen_search_shows_the_line_and_keeps_the_query() {
    let _guard = iced_test_guard();
    let mut state = AppState {
        query: "invoice".into(),
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    // The real app's own gate (`router.rs`'s `SubmitSearch` handler)
    // dispatches this when `!has_selected() && !sources.is_empty()`;
    // modelled directly here since the reducer under test is this
    // message's own effect on state, not the router's gating logic.
    state.update(&Message::SearchFolderChoicesPending);

    let mut ui = simulator(views::search_view(&state));
    assert!(
        ui.find(tr(Locale::En, MessageKey::SearchFolderNotChosen))
            .is_ok(),
        "the inline line must appear once Search found nothing chosen"
    );
    assert_eq!(
        state.query, "invoice",
        "the query must be kept, not cleared"
    );
    assert!(ui.find("Docs").is_ok(), "the folder choices stay visible");
    assert!(ui.find("Notes").is_ok(), "the folder choices stay visible");
}

/// Then choosing a folder: the line goes (the search itself resuming is
/// `search_flow::after_folder_picked`'s own existing job, in `orbok-app`,
/// not this crate's).
#[test]
fn choosing_a_folder_clears_the_line() {
    let _guard = iced_test_guard();
    let mut state = AppState {
        query: "invoice".into(),
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    state.update(&Message::SearchFolderChoicesPending);
    assert!(state.search_location.folder_not_chosen);

    state.update(&Message::SearchLocationSelected(
        SearchLocation::remembered(SourceId::from_string("s_notes"), "Notes"),
    ));

    assert!(
        !state.search_location.folder_not_chosen,
        "choosing a folder must clear the inline line"
    );
    let mut ui = simulator(views::search_view(&state));
    assert!(
        ui.find(tr(Locale::En, MessageKey::SearchFolderNotChosen))
            .is_err(),
        "the line must not still render once a folder is chosen"
    );
}

/// Opening the system picker ("Choose another folder") instead is a
/// different flow -- the inline line is not about it, and must not still
/// show underneath while, or after, that picker is used.
#[test]
fn opening_the_system_picker_also_clears_the_line() {
    let _guard = iced_test_guard();
    let mut state = AppState {
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    state.update(&Message::SearchFolderChoicesPending);
    assert!(state.search_location.folder_not_chosen);

    state.update(&Message::ChooseFolderRequested);

    assert!(
        !state.search_location.folder_not_chosen,
        "opening the system picker is a different flow from the inline line"
    );
}

/// §1.3: the line is plain text, not a focusable control -- it must not
/// appear in the page's clickable messages (a click anywhere on its own
/// text sends nothing).
#[test]
fn the_line_does_not_take_focus_or_intercept_clicks() {
    let _guard = iced_test_guard();
    let mut state = AppState {
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    state.update(&Message::SearchFolderChoicesPending);

    let messages = clicked(&state, tr(Locale::En, MessageKey::SearchFolderNotChosen));
    assert!(
        messages.is_empty(),
        "the inline line must not be a pressable control, got {messages:?}"
    );
}

// ── Unchanged (§2's own requirement) ────────────────────────────────────

/// Zero folders still goes straight to the picker -- the empty-state
/// prompt is untouched by this task.
#[test]
fn zero_folders_still_goes_to_the_picker() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let messages = clicked(&state, tr(Locale::En, MessageKey::SearchChooseFolder));
    assert!(matches!(messages.as_slice(), [Message::ChooseSearchFolder]));
}

/// Choosing one of the offered folders still sends the one message it
/// always has -- Task 147 added a sibling line, not a new path here.
#[test]
fn choosing_an_offered_folder_still_works_as_before() {
    let _guard = iced_test_guard();
    let state = AppState {
        sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
        ..AppState::default()
    };
    let messages = clicked(&state, "Notes");
    let expected = SourceId::from_string("s_notes");
    assert!(
        matches!(messages.as_slice(), [Message::ExistingSearchFolderChosen(id)] if *id == expected)
    );
}

/// Both locales show the new line.
#[test]
fn the_line_renders_in_both_locales() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let mut state = AppState {
            locale,
            sources: vec![folder("s_docs", "Docs"), folder("s_notes", "Notes")],
            ..AppState::default()
        };
        state.update(&Message::SearchFolderChoicesPending);
        let mut ui = simulator(views::search_view(&state));
        assert!(
            ui.find(tr(locale, MessageKey::SearchFolderNotChosen))
                .is_ok(),
            "{locale:?}: the inline line must render"
        );
    }
}

// ── §4: headless evidence (Review 325 §3/§4.2 -- no real window) ───────

/// Not run by `cargo test` on its own (CI must never write evidence
/// files as a side effect of an ordinary run): `cargo test -p orbok-ui
/// --lib task147_evidence -- --ignored`. Two captures, covering the two
/// new states together, each at a different end of the matrix:
/// - **worst case:** one folder chosen by default, Japanese, Larger, the
///   narrow window;
/// - **baseline:** two folders with none chosen (the line showing),
///   English, Default, 1280x720.
#[test]
#[ignore]
fn task147_evidence() {
    use crate::tests::iced_test_guard;
    use crate::tests::task148_layout_matrix::{
        search_one_folder_app, search_two_folders_no_location_app, snapshot,
    };
    use crate::theme::TextScale;
    use iced::Size;

    let _guard = iced_test_guard();
    snapshot(
        search_one_folder_app,
        Locale::Ja,
        TextScale::Larger,
        Size::new(450.0, 1400.0),
        "task147",
        "one-folder-chosen-ja-larger-narrow",
    );
    snapshot(
        search_two_folders_no_location_app,
        Locale::En,
        TextScale::Default,
        Size::new(1280.0, 720.0),
        "task147",
        "two-folders-none-chosen-en-default-1280x720",
    );
}
