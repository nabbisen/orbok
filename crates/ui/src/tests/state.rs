//! AppState transitions, notice handling, theme, and navigation tests.

use crate::state::{
    AppState, Message, ModelDownloadConsent, ModelFlowIdentitySequence, ModelPersistenceState,
    ModelProvenance, ModelTrustPresentation, ViewId, WizardFileCheck, WizardState,
};
use crate::theme::{TextScale, Theme};

#[test]
fn state_transitions() {
    let mut state = AppState::default();
    assert_eq!(state.active_view, ViewId::Search, "default view is Search");

    state.update(&Message::Switch(ViewId::Storage));
    assert_eq!(state.active_view, ViewId::Storage);

    state.update(&Message::ToggleAdvanced);
    assert!(state.show_advanced);
    state.update(&Message::ToggleAdvanced);
    assert!(!state.show_advanced);
}

#[test]
fn navigation_order_is_search_first() {
    assert_eq!(
        ViewId::ALL[0],
        ViewId::Search,
        "Search must be the first view (GUI external design §3.1)"
    );
}

#[test]
fn ui_state_cannot_accept_ready_without_the_app_controller() {
    let mut state = AppState::default();
    let ready_id = state.model_flow_ids.allocate_ready().unwrap();
    state.wizard = Some(WizardState::Ready {
        ready_id,
        model_dir: "/managed/generation".into(),
        provenance: ModelProvenance::AppManaged,
        persistence: ModelPersistenceState::Idle,
    });

    state.update(&Message::WizardAccept);
    assert_eq!(state.active_model_provenance, None);
    assert!(matches!(state.wizard, Some(WizardState::Ready { .. })));
}

#[test]
fn default_model_consent_uses_the_reviewed_manifest_exactly() {
    let consent = ModelDownloadConsent::trusted_default("/managed/models".into());

    assert_eq!(consent.provider, "Hugging Face");
    assert_eq!(consent.source, "intfloat/multilingual-e5-small");
    assert_eq!(
        consent.immutable_revision,
        "614241f622f53c4eeff9890bdc4f31cfecc418b3"
    );
    assert_eq!(consent.exact_size_bytes, 487_351_240);
    assert_eq!(consent.license, "MIT");
    assert_eq!(consent.destination, "/managed/models");
    assert_eq!(consent.trust, ModelTrustPresentation::AppWillVerify);
}

#[test]
fn download_request_opens_consent_without_starting_progress() {
    let mut state = AppState {
        wizard: Some(WizardState::NotConfigured),
        model_download_consent: Some(ModelDownloadConsent::trusted_default(
            "/managed/models".into(),
        )),
        ..Default::default()
    };

    state.update(&Message::DownloadModel);
    assert!(matches!(
        state.wizard,
        Some(WizardState::DownloadConsent { .. })
    ));

    state.update(&Message::CancelModelDownload);
    assert_eq!(state.wizard, Some(WizardState::NotConfigured));
}

#[test]
fn cancel_consent_restores_the_missing_file_context() {
    let original = WizardState::FileMissing {
        previous_dir: "/previous/model".into(),
        checks: vec![WizardFileCheck {
            relative_path: "onnx/model.onnx".into(),
            found: false,
            size_mb: None,
        }],
    };
    let mut state = AppState {
        wizard: Some(original.clone()),
        model_download_consent: Some(ModelDownloadConsent::trusted_default(
            "/managed/models".into(),
        )),
        ..Default::default()
    };

    state.update(&Message::DownloadModel);
    state.update(&Message::CancelModelDownload);

    assert_eq!(state.wizard, Some(original));
}

#[test]
fn model_flow_identity_sequence_never_wraps_or_reuses() {
    let mut sequence = ModelFlowIdentitySequence::with_next(u64::MAX, u64::MAX);
    assert_eq!(sequence.allocate_ready().unwrap().get(), u64::MAX);
    assert!(sequence.allocate_ready().is_none());
    assert_eq!(
        sequence.allocate_persistence_attempt().unwrap().get(),
        u64::MAX
    );
    assert!(sequence.allocate_persistence_attempt().is_none());
}

// Failures surface a notice; success clears it.
#[test]
fn failures_surface_notice_success_clears_it() {
    use crate::state::SearchResultDisplay;
    let mut state = AppState::default();

    // A search error creates a notice.
    state.update(&Message::SearchError("timeout".into()));
    assert!(state.notice.is_some(), "search error must create a notice");
    assert!(!state.search_running);

    // Successful results clear the notice.
    state.update(&Message::SearchResultsReady(vec![SearchResultDisplay {
        display_path: "a.md".into(),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    }]));
    assert!(
        state.notice.is_none(),
        "results ready must clear the notice"
    );
}

// Problem notices have an action; confirmation notices have dismiss instead.
#[test]
fn problem_notices_offer_action_confirmations_do_not() {
    use crate::notice::UserNotice;
    for n in [
        UserNotice::SearchDidNotFinish,
        UserNotice::FolderCouldNotBeAdded,
        UserNotice::DownloadDidNotFinish,
    ] {
        assert!(n.is_problem());
    }
}

// RFC-032: theme selection drives the active token preset.
#[test]
fn set_theme_swaps_token_preset() {
    let mut state = AppState::default();
    assert_eq!(state.theme, Theme::System, "default theme is System");

    state.update(&Message::SetTheme(Theme::Dark));
    assert_eq!(state.theme, Theme::Dark);
    assert_eq!(state.tokens, snora::design::Tokens::dark());

    state.update(&Message::SetTheme(Theme::HighContrastLight));
    assert_eq!(state.theme, Theme::HighContrastLight);
    assert_eq!(state.tokens, snora::design::Tokens::high_contrast_light());

    state.update(&Message::SetTheme(Theme::Light));
    assert_eq!(state.tokens, snora::design::Tokens::light());
}

// RFC-032: every concrete theme maps to its snora preset; string round-trip.
#[test]
fn theme_tokens_and_string_roundtrip() {
    let cases = [
        (Theme::Light, snora::design::Tokens::light()),
        (Theme::Dark, snora::design::Tokens::dark()),
        (
            Theme::HighContrastLight,
            snora::design::Tokens::high_contrast_light(),
        ),
        (
            Theme::HighContrastDark,
            snora::design::Tokens::high_contrast_dark(),
        ),
    ];
    for (theme, expected) in cases {
        assert_eq!(theme.tokens(), expected, "{theme:?} preset");
    }
    for theme in Theme::ALL {
        assert_eq!(
            Theme::parse(theme.as_str()),
            Some(*theme),
            "round-trip {theme:?}"
        );
    }
}

// RFC-032: ORBOK_THEME env override resolves concrete themes; system/unset → None.
#[test]
fn theme_from_env_resolves_override() {
    let prev = std::env::var("ORBOK_THEME").ok();
    unsafe {
        std::env::set_var("ORBOK_THEME", "dark");
    }
    assert_eq!(Theme::from_env(), Some(Theme::Dark));
    unsafe {
        std::env::set_var("ORBOK_THEME", "system");
    }
    assert_eq!(Theme::from_env(), None, "system override is not concrete");
    unsafe {
        std::env::remove_var("ORBOK_THEME");
    }
    assert_eq!(Theme::from_env(), None, "unset yields None");
    unsafe {
        match prev {
            Some(v) => std::env::set_var("ORBOK_THEME", v),
            None => std::env::remove_var("ORBOK_THEME"),
        }
    }
}

// RFC-035: text scale factor and string round-trip.
#[test]
fn text_scale_roundtrip() {
    assert!((TextScale::Default.factor() - 1.0).abs() < f32::EPSILON);
    assert!((TextScale::Large.factor() - 1.15).abs() < f32::EPSILON);
    assert!((TextScale::Larger.factor() - 1.3).abs() < f32::EPSILON);
    for scale in TextScale::ALL {
        assert_eq!(
            TextScale::parse(scale.as_str()),
            Some(*scale),
            "round-trip {scale:?}"
        );
    }
}

// RFC-035: SetTextScale persists to state.
#[test]
fn set_text_scale_updates_state() {
    let mut state = AppState::default();
    assert_eq!(state.text_scale, TextScale::Default);
    state.update(&Message::SetTextScale(TextScale::Larger));
    assert_eq!(state.text_scale, TextScale::Larger);
    state.update(&Message::SetTextScale(TextScale::Large));
    assert_eq!(state.text_scale, TextScale::Large);
}

// RFC-035: SetReducedMotion persists to state.
#[test]
fn set_reduced_motion_updates_state() {
    let mut state = AppState::default();
    assert!(!state.reduced_motion, "default is false");
    state.update(&Message::SetReducedMotion(true));
    assert!(state.reduced_motion);
    state.update(&Message::SetReducedMotion(false));
    assert!(!state.reduced_motion);
}

// RFC-031 notice tone mapping.
#[test]
fn notice_tone_mapping_is_consistent() {
    use crate::notice::UserNotice;
    use snora::design::Tone;
    // Danger: hard failures.
    for n in [
        UserNotice::SearchDidNotFinish,
        UserNotice::FolderCouldNotBeAdded,
        UserNotice::DownloadDidNotFinish,
    ] {
        assert_eq!(n.tone(), Tone::Danger, "{n:?} must be Danger");
    }
    // Warning: cautions.
    for n in [
        UserNotice::FilesMovedOrMissing,
        UserNotice::SensitiveSourceAdded,
    ] {
        assert_eq!(n.tone(), Tone::Warning, "{n:?} must be Warning");
    }
    // Success: positive confirmations.
    for n in [UserNotice::FolderAdded, UserNotice::SearchReady] {
        assert_eq!(n.tone(), Tone::Success, "{n:?} must be Success");
    }
}
