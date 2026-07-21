//! Smoke tests for view rendering (iced_test).
//!
//! Deliberately minimal. orbok's logic lives in `AppState::update`, tested
//! directly as a pure function. These tests only confirm the view builders
//! produce a usable interface for representative states — catching accidental
//! panics and vanished key content. Not an exhaustive UI suite; iced_test is
//! young and we keep reliance on it light.

use crate::i18n::{MessageKey, tr};
use crate::state::{AppState, Message, SourceCard, ViewId};
use crate::views;
use iced_test::simulator;
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
