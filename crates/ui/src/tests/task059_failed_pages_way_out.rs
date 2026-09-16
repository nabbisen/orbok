//! Task 059: the load-failure notice has its own title, and neither failed
//! Ready page is a dead end.

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::state::{AppState, Message, ModelPersistenceState, ModelProvenance, WizardState};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;
use orbok_models::SearchCapability;

/// §3 test 1: the notice no longer shows one sentence twice.
#[test]
fn the_model_load_notice_title_differs_from_its_body() {
    for locale in [Locale::En, Locale::Ja] {
        let notice = UserNotice::ModelCouldNotBeLoaded;
        assert_ne!(
            notice.title(locale),
            notice.body(locale),
            "{locale:?}: title and body must be different sentences"
        );
        assert_eq!(
            notice.title(locale),
            tr(locale, MessageKey::ModelLoadFailedTitle)
        );
        assert_eq!(notice.body(locale), tr(locale, MessageKey::ModelLoadFailed));
    }
}

/// A Ready page whose save or load failed, reached from a session that had a
/// model (so `KeywordOnly` afterwards is an observed change).
fn failed_ready_state(locale: Locale, load_failed: bool) -> AppState {
    let mut state = AppState {
        locale,
        capability: SearchCapability::Hybrid,
        ..AppState::default()
    };
    let ready_id = state.model_flow_ids.allocate_ready().unwrap();
    let attempt = state.model_flow_ids.allocate_persistence_attempt().unwrap();
    state.wizard = Some(WizardState::Ready {
        ready_id,
        model_dir: "/user/model".into(),
        provenance: ModelProvenance::UserSupplied,
        persistence: if load_failed {
            ModelPersistenceState::LoadFailed(attempt)
        } else {
            ModelPersistenceState::Failed
        },
    });
    state
}

/// §3 test 2: Escape leaves both failed pages for keyword-only search.
#[test]
fn escape_leaves_both_failed_pages_for_keyword_search() {
    for load_failed in [false, true] {
        let mut state = failed_ready_state(Locale::En, load_failed);
        state.update(&Message::DismissOverlay);
        assert_eq!(
            state.wizard, None,
            "load_failed={load_failed}: Escape closes the failed page"
        );
        assert_eq!(state.capability, SearchCapability::KeywordOnly);
    }
}

/// §3 test 3: both failed pages render Skip, which sends `WizardSkip`. The
/// retry button is asserted first, so a missing Skip cannot pass because the
/// page did not render.
#[test]
fn both_failed_pages_offer_skip() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for (load_failed, retry_key) in [
            (false, MessageKey::ModelPersistenceRetry),
            (true, MessageKey::ModelLoadRetry),
        ] {
            let state = failed_ready_state(locale, load_failed);
            let mut ui = simulator(views::wizard_view(&state));
            assert!(
                ui.find(tr(locale, retry_key)).is_ok(),
                "{locale:?} load_failed={load_failed}: control -- the page rendered its retry"
            );
            let _ = ui.click(tr(locale, MessageKey::WizardActionSkip));
            assert!(
                ui.into_messages().any(|m| matches!(m, Message::WizardSkip)),
                "{locale:?} load_failed={load_failed}: Skip is offered and sends WizardSkip"
            );
        }
    }
}
