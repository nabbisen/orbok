//! orbok-ui test suite.
//!
//! This file is the module router. Tests live in submodules under `tests/`:
//!
//! | Module | Coverage |
//! |---|---|
//! | `i18n` | i18n catalog completeness, locale detection, parameterized messages |
//! | `glossary` | Task 100: one glossary of drifted terms/promises, read by one guard |
//! | `state` | AppState transitions, theme/scale/motion, navigation, notices |
//! | `components` | RFC-033 adapter smoke tests and tone-mapping |
//! | `notice` | Task 037 §2: `UserNotice` text never relies on tone alone |
//! | `a11y` | RFC-034 contrast guard, keyboard map, RFC-035 CVD + scale |
//! | `smoke_views` | headless view-render smoke tests |
//! | `keyboard_reachability` | RFC-034 §2.1.1 keyboard-only reachability through the real app |
//!
//! `crate::i18n::ALL_KEYS` (every `MessageKey` variant, used by the
//! exhaustiveness tests in `tests::i18n`) is generated from the same list
//! that defines the `MessageKey` enum -- see `i18n::message_keys` -- rather
//! than hand-maintained here, so it cannot drift from the enum (Review 134
//! §4, Review 138 §3(a)).

pub mod a11y;
pub mod components;
mod glossary;
mod handoff038_trust_display;
mod handoff041_open_result;
pub mod i18n;
pub mod keyboard_reachability;
pub mod notice;
mod rfc041_search;
pub mod rfc041_search_state;
pub mod rfc042_history;
pub mod rfc045_location;
pub mod smoke_views;
pub mod state;
mod task047_add_source_picker;
mod task053_search_mode;
mod task057_model_load_failed;
mod task059_failed_pages_way_out;
mod task060_notice_actions;
mod task062_folder_removal_confirmation;
mod task062_reset_catalog_confirmation;
mod task063_download_start_failure;
mod task064_notices_on_every_view;
mod task065_why_a_result_did_not_open;
mod task069_confirm_only_what_is_on_screen;
mod task070_truthful_failure_copy;
mod task071_startup_failure;
mod task072_align_icons_and_labels;
mod task073_removal_confirmation_matches_the_list;
mod task081_storage_page;
mod task088_toggle_labels;
mod task099_rebuild_confirmation;
mod task104_folders_and_preparing_agree;
mod task108_a_folder_card_shows_what_is_true_now;
mod task109_every_key_is_shown;

/// Serializes every `iced_test::Simulator`-using test across this whole
/// test binary, not just within one file. `smoke_views.rs` originally
/// defined its own private copy of this lock, which only serialized its
/// *own* tests against each other -- `keyboard_reachability.rs`'s later
/// Simulator tests, with a second independent lock, could still run
/// concurrently against `smoke_views.rs`'s, and did: a `SIGSEGV` in the
/// renderer reproduced with `cargo test -p orbok-ui --lib` (default
/// parallel) and never with `--test-threads=1`, which is exactly the
/// signature of two Simulator instances racing across files. One shared
/// lock, used by every module that touches `iced_test::Simulator`, closes
/// that gap.
use std::sync::{Mutex, MutexGuard};

static ICED_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn iced_test_guard() -> MutexGuard<'static, ()> {
    ICED_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
