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
//!
//! `crate::i18n::ALL_KEYS` (every `MessageKey` variant, used by the
//! exhaustiveness tests in `tests::i18n`) is generated from the same list
//! that defines the `MessageKey` enum -- see `i18n::message_keys` -- rather
//! than hand-maintained here, so it cannot drift from the enum (Review 134
//! §4, Review 138 §3(a)).

pub mod a11y;
pub mod components;
pub mod i18n;
mod rfc041_search;
pub mod rfc041_search_state;
pub mod rfc042_history;
pub mod rfc045_location;
pub mod smoke_views;
pub mod state;
