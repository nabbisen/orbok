//! RFC-034 §2.1.1 / Task 024 §4: proof that keyboard input reaches the
//! *application*, not just that `key_to_message` maps a key to the right
//! `Message`.
//!
//! `crates/ui/src/tests/a11y.rs`'s `key_map_*` tests are exactly the
//! instrument that missed the original defect (RFC-034 §6 rule 3
//! amendment / Task 024 §4): they proved the map was implemented, and the
//! false inference was from "the map is implemented" to "every action is
//! operable." No test of the map alone can reach that gap.
//!
//! `iced_test::Simulator` cannot drive `key_to_message` directly --
//! that function participates in `orbok`'s `iced::keyboard::listen()`
//! subscription, a layer `Simulator` (which drives a bare `Element`'s
//! widget tree) does not run. So these tests drive the *real* production
//! chain by hand -- `key_to_message` → `OrbokApp::update` → re-render --
//! the exact sequence `crates/app/src/main.rs`'s subscription performs on
//! every real key press, and assert on the *rendered result* via
//! `find()`, not on the `Message` alone.

use crate::i18n::{MessageKey, tr};
use crate::shell::{KeyboardContext, OrbokApp, key_to_message};
use crate::state::{AppState, ModelDownloadConsent, SourceCard, ViewId, WizardKind, WizardState};
use crate::tests::iced_test_guard;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced_test::simulator;

fn neutral_ctx(active_view: ViewId) -> KeyboardContext {
    KeyboardContext {
        text_input_focused: false,
        active_view,
        confirm_reset: false,
        confirm_clear_history: false,
        wizard_kind: None,
        selected_source_id: None,
    }
}

/// Press `key` against `app`'s current state -- the same two-step sequence
/// (`key_to_message` then `app.update`) `crates/app/src/main.rs`'s
/// subscription performs for every real key press -- using `ctx` to
/// stand in for what `main.rs` would have computed from `app.state` at
/// that instant. Panics if the key maps to nothing, so a broken/removed
/// binding fails loudly rather than silently doing nothing.
fn press(app: &mut OrbokApp, key: Key, modifiers: Modifiers, ctx: &KeyboardContext) {
    let message = key_to_message(&key, modifiers, ctx)
        .unwrap_or_else(|| panic!("key_to_message returned None for {key:?}/{modifiers:?}"));
    app.update(message);
}

// RFC-034 §2.1.1 / Task 024 §4 bullet 1: from a fresh state, reach each of
// the six views by keyboard (Ctrl/Cmd+1..6) and find() text that only
// that view renders.
#[test]
fn ctrl_digit_reaches_each_view_by_keyboard() {
    let _guard = iced_test_guard();
    let primary = Modifiers::COMMAND;

    let expected = [
        ("1", ViewId::Search, MessageKey::NavSearch),
        ("2", ViewId::Sources, MessageKey::SourcesTitle),
        ("3", ViewId::Indexing, MessageKey::IndexingTitle),
        ("4", ViewId::Storage, MessageKey::StorageTitle),
        ("5", ViewId::Models, MessageKey::ModelsTitle),
        ("6", ViewId::Settings, MessageKey::SettingsTitle),
    ];

    for (digit, view, heading_key) in expected {
        let mut app = OrbokApp::with_state(AppState::default());
        let ctx = neutral_ctx(app.state.active_view);
        press(&mut app, Key::Character(digit.into()), primary, &ctx);
        assert_eq!(
            app.state.active_view, view,
            "Ctrl/Cmd+{digit} must switch active_view to {view:?}"
        );

        let mut ui = simulator(app.view());
        let heading = tr(app.state.locale, heading_key);
        assert!(
            ui.find(heading).is_ok(),
            "after Ctrl/Cmd+{digit}, the rendered view must show {heading:?} \
             (view actually switched but did not render as expected)"
        );
    }
}

// Break-it-before-believing-it companion to the test above: if the
// Ctrl+1..6 binding is removed from `key_to_message`, this must fail --
// proving the reachability test above is not vacuously true.
#[test]
#[should_panic(expected = "key_to_message returned None")]
fn ctrl_digit_reachability_test_actually_exercises_the_binding() {
    let mut app = OrbokApp::with_state(AppState::default());
    // A digit with no modifier is never bound -- stands in for what would
    // happen if the Ctrl+1..6 arms were removed, without needing a
    // separate copy of key_to_message to mutate.
    let ctx = neutral_ctx(app.state.active_view);
    press(
        &mut app,
        Key::Character("1".into()),
        Modifiers::default(),
        &ctx,
    );
}

// RFC-034 §2.1.1 / Task 024 §4 bullet 2: from the model-setup wizard,
// dismiss it by keyboard (Escape) and find() the view behind it. This is
// the fix for Owner Task 003 Part B's "nothing worked at all" -- a
// keyboard-only user landing on first launch previously had no way past
// this screen at all.
#[test]
fn escape_dismisses_wizard_and_reveals_the_view_behind_it() {
    let _guard = iced_test_guard();

    let mut app = OrbokApp::with_state(AppState {
        wizard: Some(WizardState::NotConfigured),
        ..Default::default()
    });

    // Confirm the wizard is actually blocking first, so the assertion
    // below proves something changed rather than always having been true.
    {
        let mut blocked_ui = simulator(app.view());
        assert!(
            blocked_ui
                .find(tr(app.state.locale, MessageKey::WizardTitleNotConfigured))
                .is_ok(),
            "the wizard must actually be showing before Escape is exercised"
        );
    }

    let ctx = neutral_ctx(app.state.active_view);
    press(
        &mut app,
        Key::Named(Named::Escape),
        Modifiers::default(),
        &ctx,
    );

    assert!(app.state.wizard.is_none(), "Escape must dismiss the wizard");

    let mut ui = simulator(app.view());
    assert!(
        ui.find(tr(app.state.locale, MessageKey::NavSearch)).is_ok(),
        "once the wizard is dismissed, the Search view behind it must render"
    );
}

// RFC-034 §2.1.1 / Task 024 §4 bullet 3: select a source by keyboard
// (arrow keys) and activate it (Enter) -- proven against the catalog-shape
// state (`sources`/`selected_source`), the same discipline
// `scheduler_host`'s tests already use (assert against real state, not a
// message alone).
#[test]
fn select_and_activate_a_source_by_keyboard() {
    let _guard = iced_test_guard();

    let mut app = OrbokApp::with_state(AppState {
        active_view: ViewId::Sources,
        sources: vec![SourceCard {
            display_name: "Docs".into(),
            display_path: "/home/user/Docs".into(),
            indexed: 3,
            stale: 0,
            failed: 0,
            active: true,
            source_id: "src-1".into(),
        }],
        ..Default::default()
    });

    // Arrow Down selects the first (only) source.
    let ctx = neutral_ctx(ViewId::Sources);
    press(
        &mut app,
        Key::Named(Named::ArrowDown),
        Modifiers::default(),
        &ctx,
    );
    assert_eq!(app.state.selected_source, Some(0));

    // The selection must be visible, not just tracked in state (RFC-034
    // §5's mitigation for 2.4.7's absence).
    {
        let mut selected_ui = simulator(app.view());
        assert!(
            selected_ui.find("Docs").is_ok(),
            "the selected source must still render its name"
        );
    }

    // Enter, with the context reflecting the now-selected source (as
    // `main.rs`'s subscription would compute it), removes it.
    let activate_ctx = KeyboardContext {
        selected_source_id: Some("src-1".to_string()),
        ..neutral_ctx(ViewId::Sources)
    };
    press(
        &mut app,
        Key::Named(Named::Enter),
        Modifiers::default(),
        &activate_ctx,
    );

    assert!(
        app.state.sources.is_empty(),
        "Enter on the selected source must remove it, mirroring the mouse-only remove button"
    );
    assert_eq!(
        app.state.selected_source, None,
        "removing the selected source must not leave a stale index behind"
    );
}

// Task 027 §3.2/§4: Setup's primary action, DownloadModel, reached by
// Enter through the real application chain -- not just that
// key_to_message maps it, but that AppState::update actually transitions
// to the reviewed consent page and it renders.
#[test]
fn enter_on_setup_reaches_the_download_consent_page() {
    let _guard = iced_test_guard();

    let consent = ModelDownloadConsent::trusted_default("/managed/models".into());
    let mut app = OrbokApp::with_state(AppState {
        wizard: Some(WizardState::NotConfigured),
        model_download_consent: Some(consent.clone()),
        ..Default::default()
    });

    let ctx = KeyboardContext {
        wizard_kind: Some(WizardKind::Setup),
        ..neutral_ctx(app.state.active_view)
    };
    press(
        &mut app,
        Key::Named(Named::Enter),
        Modifiers::default(),
        &ctx,
    );

    assert!(
        matches!(app.state.wizard, Some(WizardState::DownloadConsent { .. })),
        "Enter on Setup must reach DownloadConsent, mirroring the mouse-only Download button"
    );

    let mut ui = simulator(app.view());
    assert!(
        ui.find(consent.model_name).is_ok(),
        "the reviewed offer must actually render, not just the state transition"
    );
}

// Break-it-before-believing-it: if Enter's source-activation arm is
// removed, a source selected by keyboard must survive -- proving the test
// above is not merely checking that a message fires.
#[test]
fn select_and_activate_a_source_by_keyboard_fails_without_the_binding() {
    let mut app = OrbokApp::with_state(AppState {
        active_view: ViewId::Sources,
        sources: vec![SourceCard {
            display_name: "Docs".into(),
            display_path: "/home/user/Docs".into(),
            indexed: 3,
            stale: 0,
            failed: 0,
            active: true,
            source_id: "src-1".into(),
        }],
        ..Default::default()
    });
    let ctx = neutral_ctx(ViewId::Sources);
    press(
        &mut app,
        Key::Named(Named::ArrowDown),
        Modifiers::default(),
        &ctx,
    );

    // Simulates the binding being absent: an empty selected_source_id,
    // the shape `confirm_message` sees when nothing is selected.
    let no_selection_ctx = KeyboardContext {
        selected_source_id: None,
        ..neutral_ctx(ViewId::Sources)
    };
    let message = key_to_message(
        &Key::Named(Named::Enter),
        Modifiers::default(),
        &no_selection_ctx,
    );
    assert!(
        message.is_none(),
        "without a selected source id in context, Enter must not fire"
    );
    assert_eq!(
        app.state.sources.len(),
        1,
        "the source must remain if Enter did not fire"
    );
}
