//! Tests for orbok-fs, validating the design specs of RFC-003 (§14) and
//! RFC-004 (§19). Split into submodules per the project testing
//! guidelines.

mod common;
mod path_guard;
mod rfc037_lifecycle;
mod scanner;
mod task116_time_and_events;
mod task120_what_orbok_skips;
