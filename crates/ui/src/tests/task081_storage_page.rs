//! Task 081: the Storage page shows what orbok really stores.

use crate::i18n::{Locale, MessageKey, tr};
use crate::notice::UserNotice;
use crate::shell::OrbokApp;
use crate::state::{AppState, Message};
use crate::tests::iced_test_guard;
use iced_test::simulator;
use orbok_core::{StorageCategory, StorageMeasurement};

fn measured_rows() -> Vec<(StorageCategory, StorageMeasurement)> {
    StorageCategory::ALL
        .iter()
        .map(|c| {
            (
                *c,
                StorageMeasurement::Measured {
                    bytes: 1024,
                    items: 1,
                },
            )
        })
        .collect()
}

/// Task 081 test 5: a failed measurement raises `StorageUnavailable` and
/// leaves whatever was already on screen untouched -- never replaced by a
/// fresh zero or an empty page.
#[test]
fn storage_measurement_failed_keeps_previous_rows_and_raises_the_notice() {
    let previous = measured_rows();
    let mut state = AppState {
        storage_rows: previous.clone(),
        storage_cache_file_bytes: Some(2048),
        storage_measuring: true,
        ..AppState::default()
    };
    state.update(&Message::StorageMeasurementFailed);

    assert_eq!(
        state.storage_rows, previous,
        "a failed measurement must not touch the rows already on screen"
    );
    assert_eq!(state.storage_cache_file_bytes, Some(2048));
    assert!(!state.storage_measuring);
    assert_eq!(state.notice, Some(UserNotice::StorageUnavailable));
    assert!(
        matches!(
            state.notice_action.as_deref(),
            Some(Message::StorageMeasurementRequested)
        ),
        "Try again must re-send the same request, got {:?}",
        state.notice_action
    );
}

/// A successful measurement clears `storage_measuring` and replaces the
/// rows -- the ordinary path `StorageMeasurementFailed` above is the
/// exception to.
#[test]
fn storage_data_ready_replaces_rows_and_clears_the_in_flight_flag() {
    let mut state = AppState {
        storage_measuring: true,
        ..AppState::default()
    };
    let rows = measured_rows();
    state.update(&Message::StorageDataReady {
        rows: rows.clone(),
        cache_file_bytes: Some(4096),
    });
    assert_eq!(state.storage_rows, rows);
    assert_eq!(state.storage_cache_file_bytes, Some(4096));
    assert!(!state.storage_measuring);
}

/// RFC-011 §13.1's empty state, owner-approved copy: before any
/// measurement, the page shows the "not calculated yet" message and a
/// "Calculate now" button, not a zero total.
#[test]
fn never_measured_shows_the_empty_state_not_a_zero() {
    let _guard = iced_test_guard();
    let state = AppState {
        active_view: crate::state::ViewId::Storage,
        ..AppState::default()
    };
    assert!(state.storage_rows.is_empty());
    let app = OrbokApp::with_state(state);
    let mut ui = simulator(app.view());
    assert!(
        ui.find(tr(Locale::En, MessageKey::StorageNotCalculatedYet))
            .is_ok(),
        "the RFC-011 §13.1 empty-state message must render"
    );
    assert!(
        ui.find(tr(Locale::En, MessageKey::StorageCalculateNow))
            .is_ok(),
        "the Calculate now button must render"
    );
}

/// An `Unknown` category renders the owner-approved "Unknown" copy in
/// Advanced view, never a zero.
#[test]
fn an_unknown_category_renders_as_unknown_in_advanced_view() {
    let _guard = iced_test_guard();
    let mut rows = measured_rows();
    for (cat, m) in rows.iter_mut() {
        if *cat == StorageCategory::ModelFiles {
            *m = StorageMeasurement::Unknown;
        }
    }
    let state = AppState {
        storage_rows: rows,
        show_advanced: true,
        active_view: crate::state::ViewId::Storage,
        ..AppState::default()
    };
    let app = OrbokApp::with_state(state);
    let mut ui = simulator(app.view());
    // `find` matches a widget's whole text content, not a substring -- the
    // Unknown copy is composed into one line with its category label
    // (`fmt_label_value`), so the expected string is built the same way,
    // not just the bare "Unknown" word.
    let expected = crate::i18n::fmt_label_value(
        Locale::En,
        tr(Locale::En, MessageKey::StorageCategoryModelFiles),
        tr(Locale::En, MessageKey::StorageValueUnknown),
    );
    assert!(
        ui.find(format!("  {expected}")).is_ok(),
        "an Unknown category must render the Unknown copy, not a zero: expected {expected:?}"
    );
}
