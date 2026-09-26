//! Task 123: the model setup pages say what is true, and let you go back.
//! The ready page states a model *ready to use*, not search already on; it has
//! Back; its body shows only while **Use this model** is offered; the first
//! page names the download once; the approved copy, in both locales.

use crate::i18n::{Locale, MessageKey, fmt_wizard_ready_body, tr};
use crate::state::{
    AppState, Message, ModelPersistenceState, ModelProvenance, WizardFileCheck, WizardState,
};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::selector::Candidate;
use iced_test::simulator;
use std::cell::Cell;

fn ready_state(
    locale: Locale,
    provenance: ModelProvenance,
    persistence: impl FnOnce(&mut AppState) -> ModelPersistenceState,
) -> AppState {
    let mut state = AppState {
        locale,
        ..AppState::default()
    };
    let ready_id = state.model_flow_ids.allocate_ready().unwrap();
    let persistence = persistence(&mut state);
    state.wizard = Some(WizardState::Ready {
        ready_id,
        model_dir: "/models/e5".into(),
        provenance,
        persistence,
    });
    state.wizard_path_input = "/models/e5".into();
    state
}

/// How a test makes the page's persistence state (some states carry an attempt id).
type MakePersistence = fn(&mut AppState) -> ModelPersistenceState;

fn idle(locale: Locale, provenance: ModelProvenance) -> AppState {
    ready_state(locale, provenance, |_| ModelPersistenceState::Idle)
}

fn found(state: &AppState, text: &str) -> bool {
    simulator(views::wizard_view(state)).find(text).is_ok()
}

/// §3.2: the ready page has Back, and pressing it sends `WizardBack`.
#[test]
fn the_ready_page_offers_back() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let state = idle(*locale, ModelProvenance::UserSupplied);
        let mut ui = simulator(views::wizard_view(&state));
        let back = tr(*locale, MessageKey::WizardBack);
        assert!(
            ui.find(back).is_ok(),
            "{locale:?}: Back is on the ready page"
        );
        let _ = ui.click(back);
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            matches!(messages.as_slice(), [Message::WizardBack]),
            "{locale:?}: {messages:?}"
        );
    }
}

/// §3.2: for a chosen folder, Back returns to the folder choice with the path
/// still in the field; after a download, to the first page.
#[test]
fn back_returns_to_the_folder_choice_with_the_path_kept() {
    let mut chosen = idle(Locale::En, ModelProvenance::UserSupplied);
    chosen.update(&Message::WizardBack);
    assert!(matches!(chosen.wizard, Some(WizardState::NotConfigured)));
    assert_eq!(chosen.wizard_path_input, "/models/e5", "the path stays");

    let mut downloaded = idle(Locale::En, ModelProvenance::AppManaged);
    downloaded.update(&Message::WizardBack);
    assert!(
        matches!(downloaded.wizard, Some(WizardState::NotConfigured)),
        "after a download, the first page"
    );
}

/// The checklist page's Back still clears the path, as before.
#[test]
fn back_from_the_checklist_still_clears_the_path() {
    let mut state = AppState {
        wizard: Some(WizardState::Checked {
            model_dir: "/x".into(),
            checks: vec![WizardFileCheck {
                relative_path: "tokenizer.json".into(),
                found: false,
                size_mb: None,
            }],
            all_ok: false,
        }),
        wizard_path_input: "/x".into(),
        ..AppState::default()
    };
    state.update(&Message::WizardBack);
    assert!(matches!(state.wizard, Some(WizardState::NotConfigured)));
    assert_eq!(state.wizard_path_input, "");
}

/// §3.2: the body shows only while **Use this model** is offered; while saving,
/// or after a failure, the existing messages speak for themselves. Back is offered
/// only where nothing is running either.
#[test]
fn the_ready_body_shows_only_while_use_this_model_is_offered() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let body = fmt_wizard_ready_body(*locale);
        assert!(
            found(&idle(*locale, ModelProvenance::UserSupplied), &body),
            "{locale:?}: the body shows while Use this model is offered"
        );
        let cases: [(&str, MakePersistence); 3] = [
            ("saving", |s| {
                ModelPersistenceState::InFlight(
                    s.model_flow_ids.allocate_persistence_attempt().unwrap(),
                )
            }),
            ("save failed", |_| ModelPersistenceState::Failed),
            ("load failed", |s| {
                ModelPersistenceState::LoadFailed(
                    s.model_flow_ids.allocate_persistence_attempt().unwrap(),
                )
            }),
        ];
        for (name, persistence) in cases {
            let state = ready_state(*locale, ModelProvenance::UserSupplied, persistence);
            assert!(
                !found(&state, &body),
                "{locale:?}, {name}: the ready body must not show"
            );
        }
        let saving = ready_state(*locale, ModelProvenance::UserSupplied, |s| {
            ModelPersistenceState::InFlight(
                s.model_flow_ids.allocate_persistence_attempt().unwrap(),
            )
        });
        assert!(
            !found(&saving, tr(*locale, MessageKey::WizardBack)),
            "{locale:?}: no Back while saving"
        );
    }
}

/// §3.3: the first page names the download once -- the button; not a label above it.
#[test]
fn the_first_page_names_the_download_once() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let state = AppState {
            locale: *locale,
            wizard: Some(WizardState::NotConfigured),
            ..AppState::default()
        };
        let label = tr(*locale, MessageKey::WizardDownloadAction);
        assert!(found(&state, label), "{locale:?}: the button is there");
        // A selector that finds the *second* text with this content: it must fail.
        let seen = Cell::new(0usize);
        let second = simulator(views::wizard_view(&state)).find(move |c: Candidate<'_>| match c {
            Candidate::Text { content, .. } if content == label => {
                seen.set(seen.get() + 1);
                (seen.get() == 2).then_some(())
            }
            _ => None,
        });
        assert!(
            second.is_err(),
            "{locale:?}: {label:?} appears more than once"
        );
    }
}

/// §3.4: the exact approved copy, in both locales; the body names the button.
#[test]
fn the_approved_copy_is_exact_in_both_locales() {
    let table = [
        (
            MessageKey::WizardTitleReady,
            "The model is ready to use",
            "モデルを使う準備ができました",
        ),
        (
            MessageKey::ModelTrustUserSupplied,
            "You provided this model. orbok cannot confirm where it came from.",
            "ご自身で用意したモデルです。入手元は orbok では確認できません。",
        ),
        (
            MessageKey::WizardBodyNotConfigured,
            "Keyword search is ready. To also search by meaning, orbok needs a local AI model on this computer. No files are uploaded; the model runs on this computer.",
            "キーワード検索は利用可能です。意味による検索を使用するには、このコンピューターにローカルAIモデルが必要です。ファイルはアップロードされず、モデルはこのコンピューターで動作します。",
        ),
    ];
    for (key, en, ja) in table {
        assert_eq!(tr(Locale::En, key), en, "{key:?} (en)");
        assert_eq!(tr(Locale::Ja, key), ja, "{key:?} (ja)");
    }
    assert_eq!(
        fmt_wizard_ready_body(Locale::En),
        "Choose Use this model to turn on search by meaning."
    );
    assert_eq!(
        fmt_wizard_ready_body(Locale::Ja),
        "「このモデルを使用」を選ぶと、意味による検索が使えるようになります。"
    );
    for locale in Locale::ALL {
        assert!(
            fmt_wizard_ready_body(*locale).contains(tr(*locale, MessageKey::WizardActionUseModel)),
            "{locale:?}: the body names the button by its label"
        );
    }
}
