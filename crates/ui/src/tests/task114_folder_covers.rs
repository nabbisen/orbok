//! Task 114 (RFC-064): the card shows what a folder covers and changes it;
//! narrowing asks first; the search follows the folder.

use crate::OrbokApp;
use crate::i18n::{Locale, MessageKey, search_location_chip, tr};
use crate::shell::key_to_message;
use crate::state::{AppState, Message, SearchFolderScope, SearchLocation, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;
use orbok_core::{SourceId, SourceStatus};

fn card(id: &str, covers_subfolders: bool) -> SourceCard {
    SourceCard {
        display_name: "Docs".into(),
        display_path: "/home/user/Docs".into(),
        indexed: 0,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: SourceStatus::Active,
        source_id: id.into(),
        covers_subfolders,
    }
}

fn folders_state(locale: Locale, covers: bool) -> AppState {
    AppState {
        locale,
        active_view: ViewId::Sources,
        sources: vec![card("s1", covers)],
        ..AppState::default()
    }
}

fn asked(locale: Locale, count: Option<u64>) -> AppState {
    let mut state = folders_state(locale, true);
    state.update(&Message::AskNarrowFolder("s1".into()));
    if let Some(count) = count {
        state.update(&Message::NarrowCountReady("s1".into(), count));
    }
    state
}

fn clicked(state: &AppState, label: &str) -> Vec<Message> {
    let mut ui = simulator(views::sources_view(state));
    assert!(ui.find(label).is_ok(), "{label:?} is found");
    let _ = ui.click(label);
    ui.into_messages().collect()
}

/// §1.3: the card says what the folder covers, and offers the other choice
/// as a button that does the right thing -- asks first to narrow, widens at
/// once. Both locales use the labels the search row already has.
#[test]
fn the_card_shows_the_choice_and_a_button_for_the_other() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let with = folders_state(locale, true);
        let (subfolders, only) = (
            tr(locale, MessageKey::SearchScopeSubfolders),
            tr(locale, MessageKey::SearchScopeOnly),
        );
        let mut ui = simulator(views::sources_view(&with));
        assert!(ui.find(subfolders).is_ok(), "{locale:?}: current choice");
        drop(ui);
        assert!(
            matches!(
                clicked(&with, only).as_slice(),
                [Message::AskNarrowFolder(id)] if id == "s1"
            ),
            "{locale:?}: narrowing asks first, it does not narrow"
        );

        let without = folders_state(locale, false);
        let mut ui = simulator(views::sources_view(&without));
        assert!(ui.find(only).is_ok(), "{locale:?}: current choice");
        drop(ui);
        assert!(
            matches!(
                clicked(&without, subfolders).as_slice(),
                [Message::WidenFolder(id)] if id == "s1"
            ),
            "{locale:?}: widening asks nothing"
        );
    }
}

/// §1.8: the "scanned recursively" hint is gone from the Folders page.
#[test]
fn the_recursive_hint_is_gone() {
    let _guard = iced_test_guard();
    let state = folders_state(Locale::En, true);
    let mut ui = simulator(views::sources_view(&state));
    assert!(ui.find("All sub-folders are scanned recursively.").is_err());
    let state = folders_state(Locale::Ja, true);
    let mut ui = simulator(views::sources_view(&state));
    assert!(
        ui.find("すべてのサブフォルダーが再帰的にスキャンされます。")
            .is_err()
    );
}

/// §2.3: the owner-approved copy, both locales; the counted line only with a
/// count, singular and plural, never a zero.
#[test]
fn the_question_is_worded_as_approved_with_a_count_only_when_there_is_one() {
    let _guard = iced_test_guard();
    for (locale, title, body, confirm, many, one) in [
        (
            Locale::En,
            "Stop including subfolders?",
            "orbok removes what it prepared for files in this folder's subfolders. \
             Your files are never changed or deleted.",
            "Stop including",
            "This removes what orbok prepared for 12 files.",
            "This removes what orbok prepared for 1 file.",
        ),
        (
            Locale::Ja,
            "サブフォルダーを含めないようにしますか?",
            "orbok はこのフォルダーのサブフォルダーにあるファイルについて、\
             準備したデータを削除します。ファイルは変更も削除もされません。",
            "含めない",
            "ファイル 12 件について、準備したデータを削除します。",
            "ファイル 1 件について、準備したデータを削除します。",
        ),
    ] {
        let state = asked(locale, Some(12));
        let mut ui = simulator(views::sources_view(&state));
        for text in [title, body, confirm, many, tr(locale, MessageKey::Cancel)] {
            assert!(ui.find(text).is_ok(), "{locale:?}: shows {text:?}");
        }
        assert!(
            ui.find("/home/user/Docs").is_ok(),
            "{locale:?}: names the folder"
        );
        drop(ui);

        let state = asked(locale, Some(1));
        assert!(simulator(views::sources_view(&state)).find(one).is_ok());

        // No count yet, a failed read, and a genuine zero: the same, no line.
        let mut states = vec![asked(locale, None)];
        let mut failed = asked(locale, Some(5));
        failed.update(&Message::NarrowCountFailed);
        states.push(failed);
        states.push(asked(locale, Some(0)));
        for state in &states {
            assert_eq!(state.narrow_file_count, None);
            let mut ui = simulator(views::sources_view(state));
            assert!(ui.find(title).is_ok());
            for line in [
                many,
                one,
                "This removes what orbok prepared for 0 files.",
                "ファイル 0 件について、準備したデータを削除します。",
            ] {
                assert!(
                    ui.find(line).is_err(),
                    "{locale:?}: no counted line ({line:?})"
                );
            }
        }
    }
}

/// §2.3: Cancel and Escape change nothing.
#[test]
fn cancel_and_escape_change_nothing() {
    let untouched = folders_state(Locale::En, true).sources;
    let mut state = asked(Locale::En, Some(3));
    state.update(&Message::CancelNarrowFolder);
    assert_eq!(state.confirm_narrow_source, None);
    assert_eq!(state.narrow_file_count, None);
    assert_eq!(
        state.sources, untouched,
        "the folder still covers its subfolders"
    );
    assert_eq!(state.notice, None);

    let mut app = OrbokApp::with_state(asked(Locale::En, Some(3)));
    let escape = key_to_message(
        &Key::Named(Named::Escape),
        Modifiers::default(),
        &app.keyboard_context(),
    );
    assert!(matches!(escape, Some(Message::DismissOverlay)));
    app.update(escape.unwrap());
    assert_eq!(app.state.confirm_narrow_source, None);
    assert_eq!(app.state.sources, untouched);
}

/// §2.3: Enter confirms only while the question is on screen.
#[test]
fn enter_confirms_only_while_the_question_is_visible() {
    let enter = |app: &OrbokApp| {
        key_to_message(
            &Key::Named(Named::Enter),
            Modifiers::default(),
            &app.keyboard_context(),
        )
    };
    let mut app = OrbokApp::with_state(asked(Locale::En, None));
    assert!(matches!(enter(&app), Some(Message::ConfirmNarrowFolder)));
    // Another page: the question closes, so a stale Enter confirms nothing.
    app.update(Message::Switch(ViewId::Storage));
    assert_eq!(app.state.confirm_narrow_source, None);
    assert!(!matches!(enter(&app), Some(Message::ConfirmNarrowFolder)));

    // Open but not on screen, however the state came to be.
    let mut state = asked(Locale::En, None);
    state.active_view = ViewId::Search;
    let app = OrbokApp::with_state(state);
    assert!(!matches!(enter(&app), Some(Message::ConfirmNarrowFolder)));
}

/// A question about a folder that is not listed, or that no longer covers
/// its subfolders, does not open.
#[test]
fn only_a_folder_that_covers_its_subfolders_can_be_asked_about() {
    let mut state = folders_state(Locale::En, false);
    state.update(&Message::AskNarrowFolder("s1".into()));
    assert_eq!(state.confirm_narrow_source, None);
    let mut state = folders_state(Locale::En, true);
    state.update(&Message::AskNarrowFolder("nope".into()));
    assert_eq!(state.confirm_narrow_source, None);
}

/// Confirming hands the folder to the router and closes the question; the
/// request itself changes no state (the erasure has not happened yet).
#[test]
fn confirming_requests_the_narrowing_and_changes_nothing_yet() {
    let mut state = asked(Locale::En, Some(3));
    let request = state.take_confirmed_narrowing();
    assert!(matches!(
        request,
        Some(Message::NarrowFolderRequested(ref id)) if id == "s1"
    ));
    assert_eq!(state.confirm_narrow_source, None);
    let before = state.sources.clone();
    state.update(&request.unwrap());
    assert_eq!(
        state.sources, before,
        "the folder is shown as it is until the catalog says"
    );
}

fn with_location(covers: bool, scope: SearchFolderScope) -> AppState {
    let mut state = AppState {
        active_view: ViewId::Search,
        sources: vec![card("s1", covers)],
        ..AppState::default()
    };
    state.search_location.selected = Some(
        SearchLocation::remembered(SourceId::from_string("s1".to_string()), "Docs")
            .with_scope(scope),
    );
    state
}

/// §2.7: a search in a **this folder only** folder shows that scope and no
/// toggle; a folder with subfolders keeps both.
#[test]
fn a_this_folder_only_folder_offers_no_scope_toggle() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let toggle_to_subfolders = tr(locale, MessageKey::SearchScopeSubfolders);
        let toggle_to_only = tr(locale, MessageKey::SearchScopeOnly);

        let mut only = with_location(false, SearchFolderScope::FolderOnly);
        only.locale = locale;
        let mut ui = simulator(views::search_view(&only));
        assert!(
            ui.find(search_location_chip(
                locale,
                "Docs",
                SearchFolderScope::FolderOnly
            ))
            .is_ok(),
            "{locale:?}: the chip shows the scope"
        );
        assert!(
            ui.find(toggle_to_subfolders).is_err(),
            "{locale:?}: no toggle"
        );
        assert!(ui.find(toggle_to_only).is_err(), "{locale:?}: no toggle");

        let mut with = with_location(true, SearchFolderScope::FolderAndSubfolders);
        with.locale = locale;
        let mut ui = simulator(views::search_view(&with));
        assert!(
            ui.find(toggle_to_only).is_ok(),
            "{locale:?}: the toggle is offered"
        );
    }
}

/// §2.7: a remembered "and subfolders" for a folder that is now "this folder
/// only" is shown -- and stored -- as "only", however the card arrives.
#[test]
fn a_stale_remembered_subfolders_scope_is_shown_and_stored_as_only() {
    let _guard = iced_test_guard();
    for arrive in [
        Message::SourcesLoaded(vec![card("s1", false)]),
        Message::SourceCardsRefreshed(vec![card("s1", false)]),
    ] {
        let mut state = with_location(true, SearchFolderScope::FolderAndSubfolders);
        state.update(&arrive);
        assert_eq!(
            state.search_location.selected.as_ref().unwrap().scope(),
            SearchFolderScope::FolderOnly,
            "{arrive:?}: no contradiction is stored"
        );
        let mut ui = simulator(views::search_view(&state));
        assert!(
            ui.find(search_location_chip(
                Locale::En,
                "Docs",
                SearchFolderScope::FolderOnly
            ))
            .is_ok()
        );
    }
    // The same when it is selected after the fact.
    let mut state = AppState {
        sources: vec![card("s1", false)],
        ..AppState::default()
    };
    state.update(&Message::SearchLocationSelected(
        SearchLocation::remembered(SourceId::from_string("s1".to_string()), "Docs"),
    ));
    assert_eq!(
        state.search_location.selected.as_ref().unwrap().scope(),
        SearchFolderScope::FolderOnly
    );
}

/// A search limited to a subfolder of a folder that was narrowed has nothing
/// left to look at: the location goes, the query stays. A location on the
/// folder itself stays and becomes "only".
#[test]
fn narrowing_clears_a_location_inside_the_folder_and_keeps_the_query() {
    let mut state = AppState {
        query: "notes".into(),
        sources: vec![card("s1", true)],
        ..AppState::default()
    };
    state.search_location.selected = Some(SearchLocation::within(
        SourceId::from_string("s1".to_string()),
        "sub",
        "/home/user/Docs/sub",
    ));
    state.update(&Message::FolderNarrowed("s1".into()));
    assert!(state.search_location.selected.is_none());
    assert_eq!(state.query, "notes");

    let mut state = with_location(true, SearchFolderScope::FolderAndSubfolders);
    state.update(&Message::FolderNarrowed("s1".into()));
    state.update(&Message::SourceCardsRefreshed(vec![card("s1", false)]));
    let location = state.search_location.selected.as_ref().unwrap();
    assert_eq!(location.scope(), SearchFolderScope::FolderOnly);
}
