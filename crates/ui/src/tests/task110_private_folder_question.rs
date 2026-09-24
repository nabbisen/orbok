//! Task 110 §3.6: the private-folder question, both locales, exact copy,
//! rendered by the page that asked; Escape cancels; Enter confirms only while
//! the dialog is visible.

use crate::OrbokApp;
use crate::i18n::{Locale, MessageKey, tr};
use crate::shell::key_to_message;
use crate::state::{AppState, FolderAddOrigin, Message, PendingFolderAdd, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

const PATH: &str = "/home/user/.ssh";

fn asked(locale: Locale, origin: FolderAddOrigin) -> AppState {
    let mut state = AppState {
        locale,
        active_view: match origin {
            FolderAddOrigin::FoldersPage => ViewId::Sources,
            FolderAddOrigin::SearchPage => ViewId::Search,
        },
        ..AppState::default()
    };
    state.update(&Message::AskAddSensitiveFolder(PendingFolderAdd {
        path: PATH.into(),
        origin,
    }));
    state
}

fn page(state: &AppState) -> iced::Element<'_, Message> {
    match state.active_view {
        ViewId::Sources => views::sources_view(state),
        _ => views::search_view(state),
    }
}

const EN_BODY: &str = "It may include SSH keys, browser profiles, or other sensitive data. \
    If you add it, what it contains can be found by searching in orbok. \
    Documents are processed on this computer only. \
    You can remove it at any time with Remove from orbok in Folders.";
const JA_BODY: &str = "SSH鍵、ブラウザのプロフィール、またはその他の機密データが含まれている可能性があります。\
    追加すると、その内容を orbok で検索できるようになります。\
    文書はこのコンピューター上でのみ処理されます。\
    「フォルダー」の「orbokから削除」で、いつでも削除できます。";

/// Exact owner-approved copy, on the page that asked, in both locales.
#[test]
fn the_question_is_worded_as_approved_on_each_page() {
    let _guard = iced_test_guard();
    for origin in [FolderAddOrigin::FoldersPage, FolderAddOrigin::SearchPage] {
        for (locale, title, body, confirm) in [
            (
                Locale::En,
                "Add a folder that may contain private files?",
                EN_BODY,
                "Add anyway",
            ),
            (
                Locale::Ja,
                "機密ファイルを含む可能性のあるフォルダーを追加しますか?",
                JA_BODY,
                "追加する",
            ),
        ] {
            let state = asked(locale, origin);
            let mut ui = simulator(page(&state));
            for text in [title, body, confirm, PATH, tr(locale, MessageKey::Cancel)] {
                assert!(
                    ui.find(text).is_ok(),
                    "{origin:?} {locale:?}: the dialog shows {text:?}"
                );
            }
        }
    }
}

/// The dialog's labels are read from the catalog, so renaming `NavSources` or
/// `SourceActionRemoveFromOrbok` changes the body with it.
#[test]
fn the_body_names_the_labels_the_app_really_has() {
    for locale in Locale::ALL {
        let body = crate::i18n::fmt_add_sensitive_body(*locale);
        for key in [
            MessageKey::NavSources,
            MessageKey::SourceActionRemoveFromOrbok,
        ] {
            assert!(
                body.contains(tr(*locale, key)),
                "{locale:?}: the body names {:?}",
                tr(*locale, key)
            );
        }
    }
}

/// Escape cancels and adds nothing (the state closes), on both pages.
#[test]
fn escape_cancels_the_question() {
    for origin in [FolderAddOrigin::FoldersPage, FolderAddOrigin::SearchPage] {
        let mut app = OrbokApp::with_state(asked(Locale::En, origin));
        let escape = key_to_message(
            &Key::Named(Named::Escape),
            Modifiers::default(),
            &app.keyboard_context(),
        );
        assert!(
            matches!(escape, Some(Message::DismissOverlay)),
            "{origin:?}: Escape dismisses, got {escape:?}"
        );
        app.update(escape.unwrap());
        assert!(app.state.pending_folder_add.is_none(), "{origin:?}: closed");
    }
}

/// Enter confirms only while the dialog is visible: on the page that asked,
/// yes; and switching page closes it, so a stale Enter confirms nothing.
#[test]
fn enter_confirms_only_the_visible_question() {
    for origin in [FolderAddOrigin::FoldersPage, FolderAddOrigin::SearchPage] {
        let mut app = OrbokApp::with_state(asked(Locale::En, origin));
        let enter = |app: &OrbokApp| {
            key_to_message(
                &Key::Named(Named::Enter),
                Modifiers::default(),
                &app.keyboard_context(),
            )
        };
        assert!(
            matches!(enter(&app), Some(Message::ConfirmAddSensitiveFolder)),
            "{origin:?}: Enter confirms while it is visible"
        );
        // Another page: the question closes, Enter confirms nothing.
        app.update(Message::Switch(ViewId::Storage));
        assert!(
            app.state.pending_folder_add.is_none(),
            "closed by the switch"
        );
        assert!(
            !matches!(enter(&app), Some(Message::ConfirmAddSensitiveFolder)),
            "{origin:?}: a stale Enter confirms nothing"
        );
    }
}

/// A question that is open but not on screen (its page is not the active
/// one) is not confirmed by Enter -- however the state came to be.
#[test]
fn enter_does_not_confirm_a_question_that_is_not_on_screen() {
    for origin in [FolderAddOrigin::FoldersPage, FolderAddOrigin::SearchPage] {
        let mut state = asked(Locale::En, origin);
        state.active_view = ViewId::Storage;
        let app = OrbokApp::with_state(state);
        let got = key_to_message(
            &Key::Named(Named::Enter),
            Modifiers::default(),
            &app.keyboard_context(),
        );
        assert!(
            !matches!(got, Some(Message::ConfirmAddSensitiveFolder)),
            "{origin:?}: hidden, so Enter confirms nothing, got {got:?}"
        );
    }
}
