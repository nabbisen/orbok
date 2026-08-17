//! orbok-ui test suite.
//!
//! This file is the module router. Tests live in submodules under `tests/`:
//!
//! | Module | Coverage |
//! |---|---|
//! | `i18n` | i18n catalog completeness, locale detection, parameterized messages |
//! | `state` | AppState transitions, theme/scale/motion, navigation, notices |
//! | `components` | RFC-033 adapter smoke tests and tone-mapping |
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
pub mod i18n;
pub mod keyboard_reachability;
mod rfc041_search;
pub mod rfc041_search_state;
pub mod rfc042_history;
pub mod rfc045_location;
pub mod smoke_views;
pub mod state;

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
