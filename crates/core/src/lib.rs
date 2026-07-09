//! # orbok-core
//!
//! Shared vocabulary for the `orbok` local-first document search
//! application: typed identifiers, the RFC-001 data lifecycle classes,
//! lifecycle status enums shared between the catalog (RFC-002) and the
//! scanner (RFC-004), pipeline version constants, error types, and time
//! helpers.
//!
//! This crate is dependency-light by design. It must not depend on the
//! database, the file system layer, or the UI.

pub mod data_class;
pub mod error;
pub mod history;
pub mod id;
pub mod privacy;
pub mod status;
pub mod timeutil;
pub mod versions;

#[cfg(test)]
mod tests;

pub use data_class::{CleanupAction, CleanupPlan, DataClass, StorageCategory};
pub use error::{ErrorCategory, OrbokError, OrbokResult};
pub use history::{
    SearchHistoryEntry, SearchHistoryId, SearchHistorySettings, StoredChangedFilter,
    StoredKindFilter, StoredLanguageFilter, StoredReadyFilter, StoredSearchFilter,
    StoredSearchStyle,
};
pub use id::{
    ChunkId, EmbeddingId, EventId, ExtractionId, FileId, JobId, ModelId, QueryId, SourceId,
};
pub use privacy::{DiagnosticsPolicy, LocalDataCategory, PrivacyMode, PrivacySettings};
pub use status::{
    FileStatus, HiddenFilePolicy, IndexMode, JobStatus, JobType, PersistenceMode, SourceStatus,
    SourceType, SymlinkPolicy,
};
pub use timeutil::{now_iso8601, system_time_iso8601};
