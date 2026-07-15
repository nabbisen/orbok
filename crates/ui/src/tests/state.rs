//! AppState transitions, notice handling, theme, and navigation tests.

use crate::state::{AppState, Message, ViewId, WizardModelProvenance, WizardState};
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
fn downloaded_model_ready_state_retains_managed_provenance() {
    let mut state = AppState::default();

    state.update(&Message::DownloadAllComplete {
        dest_dir: "/managed/generation".into(),
    });

    assert_eq!(
        state.wizard,
        Some(WizardState::Ready {
            model_dir: "/managed/generation".into(),
            provenance: WizardModelProvenance::Managed,
        })
    );
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
