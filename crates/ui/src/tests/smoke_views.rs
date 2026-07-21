//! Smoke tests for view rendering (iced_test).
//!
//! Deliberately minimal. orbok's logic lives in `AppState::update`, tested
//! directly as a pure function. These tests only confirm the view builders
//! produce a usable interface for representative states — catching accidental
//! panics and vanished key content. Not an exhaustive UI suite; iced_test is
//! young and we keep reliance on it light.

use crate::i18n::{Locale, MessageKey, model_exact_size, tr};
use crate::state::{
    AppState, Message, ModelConsentReturn, ModelDeliveryFailure, ModelDownloadConsent,
    ModelPersistenceState, ModelProvenance, SourceCard, ViewId, WizardState,
};
use crate::views;
use iced_test::{Simulator, simulator};
use orbok_models::SearchCapability;
use std::sync::{Mutex, MutexGuard};

static ICED_TEST_LOCK: Mutex<()> = Mutex::new(());

fn iced_test_guard() -> MutexGuard<'static, ()> {
    ICED_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// A fresh app with no sources shows the "add a source" call to action.
#[test]
fn search_empty_state_offers_add_source() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let mut ui = simulator(views::search_view(&state));
    assert!(
        ui.find(tr(state.locale, MessageKey::SearchAddSource))
            .is_ok(),
        "empty search view must offer an 'add source' action"
    );
}

// Clicking the empty-state CTA emits a Switch to the Sources view.
#[test]
fn search_empty_cta_switches_to_sources() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let mut ui = simulator(views::search_view(&state));
    let _ = ui.click(tr(state.locale, MessageKey::SearchAddSource));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Switch(ViewId::Sources))),
        "clicking the CTA should switch to the Sources view"
    );
}

// The settings view renders and exposes the advanced-view toggle.
#[test]
fn settings_view_has_advanced_toggle() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let mut ui = simulator(views::settings_view(&state));
    assert!(
        ui.find(tr(state.locale, MessageKey::SettingsAdvancedOff))
            .is_ok(),
        "settings must show the advanced-view toggle"
    );
}

// Sources view renders for both empty and populated states without panicking.
#[test]
fn sources_view_renders_both_states() {
    let _guard = iced_test_guard();
    let empty = AppState::default();
    let _ = simulator(views::sources_view(&empty));

    let mut populated = AppState::default();
    populated.sources.push(SourceCard {
        display_name: "Docs".into(),
        display_path: "/home/user/Docs".into(),
        indexed: 12,
        stale: 0,
        failed: 0,
        active: true,
        source_id: "src-1".into(),
    });
    let mut ui = simulator(views::sources_view(&populated));
    assert!(
        ui.find("Docs").is_ok(),
        "populated sources view must list the source name"
    );
}

#[test]
fn model_consent_renders_every_required_fact_and_action_in_both_locales() {
    let _guard = iced_test_guard();

    for locale in Locale::ALL {
        let consent = ModelDownloadConsent::trusted_default(
            "/a/representative/platform/path/models/multilingual-e5-small".into(),
        );
        let mut state = AppState {
            locale: *locale,
            wizard: Some(WizardState::NotConfigured),
            model_download_consent: Some(consent.clone()),
            ..Default::default()
        };
        state.update(&Message::DownloadModel);
        let mut ui = Simulator::with_size(
            Default::default(),
            [800.0, 600.0],
            views::wizard_view(&state),
        );

        let expected = [
            consent.model_name.to_string(),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentProvider),
                consent.provider
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentSource),
                consent.source
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentRevision),
                consent.immutable_revision
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentExactSize),
                model_exact_size(*locale, consent.exact_size_bytes)
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentLicense),
                consent.license
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentLocation),
                consent.destination
            ),
            format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelConsentVerification),
                tr(*locale, MessageKey::ModelTrustAppWillVerify)
            ),
            tr(*locale, MessageKey::ModelConsentPrivacy).to_string(),
            tr(*locale, MessageKey::ModelConsentConfirm).to_string(),
            tr(*locale, MessageKey::ModelConsentCancel).to_string(),
        ];

        for visible in expected {
            assert!(
                ui.find(visible.as_str()).is_ok(),
                "consent must render {visible:?} for {locale:?}"
            );
        }

        let _ = ui.click(tr(*locale, MessageKey::ModelConsentConfirm));
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::ConfirmModelDownload)),
            "consent confirmation must emit the guarded start message"
        );

        let mut cancel_ui = Simulator::with_size(
            Default::default(),
            [800.0, 600.0],
            views::wizard_view(&state),
        );
        let _ = cancel_ui.click(tr(*locale, MessageKey::ModelConsentCancel));
        let cancel_messages: Vec<Message> = cancel_ui.into_messages().collect();
        assert!(
            cancel_messages
                .iter()
                .any(|message| matches!(message, Message::CancelModelDownload)),
            "consent back action must emit the typed cancel message"
        );
    }
}

#[test]
fn models_view_distinguishes_persistent_managed_and_manual_provenance() {
    let _guard = iced_test_guard();

    for locale in Locale::ALL {
        for (provenance, status_key) in [
            (
                ModelProvenance::AppManaged,
                MessageKey::ModelTrustAppVerified,
            ),
            (
                ModelProvenance::UserSupplied,
                MessageKey::ModelTrustUserSupplied,
            ),
        ] {
            let state = AppState {
                locale: *locale,
                active_view: ViewId::Models,
                capability: SearchCapability::Hybrid,
                active_model_provenance: Some(provenance),
                ..Default::default()
            };
            let mut ui = simulator(views::models_view(&state));
            let expected = format!(
                "{}: {}",
                tr(*locale, MessageKey::ModelsVerification),
                tr(*locale, status_key)
            );
            assert!(
                ui.find(expected.as_str()).is_ok(),
                "Models view must render {expected:?} for {locale:?}"
            );
        }
    }
}

#[test]
fn model_failure_and_persistence_retry_are_visible_in_both_locales() {
    let _guard = iced_test_guard();

    for locale in Locale::ALL {
        let consent = ModelDownloadConsent::trusted_default("/managed/models".into());
        let failure_state = AppState {
            locale: *locale,
            wizard: Some(WizardState::DownloadFailed {
                presentation: consent,
                return_to: ModelConsentReturn::NotConfigured,
                failure: ModelDeliveryFailure::Connection,
            }),
            ..Default::default()
        };
        let mut failure_ui = simulator(views::wizard_view(&failure_state));
        assert!(
            failure_ui
                .find(tr(*locale, MessageKey::ModelDeliveryConnection))
                .is_ok()
        );
        let _ = failure_ui.click(tr(*locale, MessageKey::ModelDownloadRetry));
        assert!(
            failure_ui
                .into_messages()
                .any(|message| matches!(message, Message::RetryModelDownload))
        );

        let mut ready_state = AppState {
            locale: *locale,
            ..Default::default()
        };
        let ready_id = ready_state.model_flow_ids.allocate_ready().unwrap();
        ready_state.wizard = Some(WizardState::Ready {
            ready_id,
            model_dir: "/managed/generation".into(),
            provenance: ModelProvenance::AppManaged,
            persistence: ModelPersistenceState::Failed,
        });
        let mut ready_ui = simulator(views::wizard_view(&ready_state));
        assert!(
            ready_ui
                .find(tr(*locale, MessageKey::ModelPersistenceFailed))
                .is_ok()
        );
        let _ = ready_ui.click(tr(*locale, MessageKey::ModelPersistenceRetry));
        assert!(
            ready_ui
                .into_messages()
                .any(|message| matches!(message, Message::WizardAccept))
        );
    }
}
