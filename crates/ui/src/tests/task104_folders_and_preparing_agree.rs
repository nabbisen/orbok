//! Task 104 §3.4: the Folders card and the Preparing page count the same
//! states with the same words. Asserted on `MessageKey`s -- through
//! `FileCountState::label_key` -- not on copied strings, so a label change is
//! made in one place and a screen that stops reading that place fails here.

use crate::i18n::{Locale, source_summary, tr};
use crate::state::{AppState, FileCountState, IndexHealth, SourceCard, ViewId};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

const READY: u64 = 12;
const STALE: u64 = 1;
const FAILED: u64 = 2;
const NO_TEXT: u64 = 3;

fn card() -> SourceCard {
    SourceCard {
        display_name: "Docs".into(),
        display_path: "/home/user/Docs".into(),
        indexed: READY,
        stale: STALE,
        failed: FAILED,
        no_text_found: NO_TEXT,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "src-1".into(),
    }
}

/// For one `IndexHealth`, each state the two screens share is labelled by the
/// same key on both: the Preparing page renders that key's text as a cell,
/// and the card's summary line contains it.
#[test]
fn the_two_screens_use_the_same_label_for_each_state() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let state = AppState {
            locale,
            active_view: ViewId::Indexing,
            health: IndexHealth {
                indexed: READY,
                stale: STALE,
                failed: FAILED,
                queued: 0,
            },
            ..AppState::default()
        };
        let summary = source_summary(locale, READY, STALE, FAILED, NO_TEXT);

        let mut preparing = simulator(views::indexing_view(&state));
        for shared in [
            FileCountState::Ready,
            FileCountState::NeedsUpdate,
            FileCountState::Failed,
        ] {
            let label = tr(locale, shared.label_key());
            assert!(
                preparing.find(label).is_ok(),
                "{locale:?}: the Preparing page shows {shared:?} as {label:?}"
            );
            assert!(
                summary.contains(label),
                "{locale:?}: the Folders line names {shared:?} as {label:?}, got {summary:?}"
            );
        }
        // Counted on the card only -- the Preparing page has no such cell.
        assert!(summary.contains(tr(locale, FileCountState::NoText.label_key())));

        // And the card really renders that summary.
        let sources = AppState {
            locale,
            active_view: ViewId::Sources,
            sources: vec![card()],
            ..AppState::default()
        };
        let mut ui = simulator(views::sources_view(&sources));
        assert!(
            ui.find(summary.as_str()).is_ok(),
            "{locale:?}: the Folders card renders {summary:?}"
        );
    }
}
