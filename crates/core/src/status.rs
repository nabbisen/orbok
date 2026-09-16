//! Lifecycle status vocabulary shared by the catalog (RFC-002), the
//! source boundary (RFC-003), and the scanner (RFC-004).
//!
//! Each enum maps 1-to-1 onto a CHECK-constrained catalog column. The
//! `as_str`/`parse` pairs are the single conversion point; repositories
//! must not hand-write these strings.

use crate::error::OrbokError;
use serde::{Deserialize, Serialize};

macro_rules! catalog_enum {
    ($(#[$doc:meta])* $name:ident, $column:literal, { $($variant:ident => $s:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Stable catalog string.
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $s),+
                }
            }

            /// Parse the catalog string; invalid values are catalog
            /// corruption and surface as a typed error.
            pub fn parse(s: &str) -> Result<Self, OrbokError> {
                match s {
                    $($s => Ok(Self::$variant),)+
                    other => Err(OrbokError::InvalidCatalogValue {
                        column: $column,
                        value: other.to_string(),
                    }),
                }
            }
        }
    };
}

catalog_enum!(
    /// `sources.source_type` (RFC-003 §5).
    SourceType,
    "sources.source_type",
    { Directory => "directory", File => "file" }
);

catalog_enum!(
    /// `sources.persistence_mode` (RFC-003 §5.1–5.2).
    PersistenceMode,
    "sources.persistence_mode",
    { Persistent => "persistent", Temporary => "temporary" }
);

catalog_enum!(
    /// `sources.status` (external design §11.2).
    SourceStatus,
    "sources.status",
    {
        Active => "active",
        Paused => "paused",
        Missing => "missing",
        PermissionDenied => "permission_denied",
        Removed => "removed",
    }
);

impl SourceStatus {
    /// Whether a source in this state contributes search candidates
    /// (RFC-060 §5, §11 criterion 6).
    ///
    /// Mirrors `orbok_fs::source_lifecycle::SourceState::is_searchable`
    /// for the states the catalog can actually hold: that vocabulary is
    /// RFC-037's wider one (it adds `Preparing` and `NeedsUpdate`), while
    /// `sources.status`'s CHECK admits only the five values above -- so a
    /// row can never carry the two extra searchable states, and `Active`
    /// is the whole searchable set here.
    pub fn is_searchable(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Every searchable `sources.status` value as a SQL `IN` list, for the
/// retrieval joins (RFC-060 §5). One definition rather than the literal
/// repeated at four query sites; `searchable_status_sql_matches_the_enum`
/// in this crate's tests keeps it equal to [`SourceStatus::is_searchable`].
/// Built from `as_str` values, never from caller input.
pub const SEARCHABLE_SOURCE_STATUS_SQL: &str = "('active')";

catalog_enum!(
    /// `sources.index_mode` (FR-120 quality modes).
    IndexMode,
    "sources.index_mode",
    {
        Balanced => "balanced",
        HighAccuracy => "high_accuracy",
        SpaceSaving => "space_saving",
    }
);

catalog_enum!(
    /// `sources.hidden_file_policy` (RFC-003 §6.1; default Exclude).
    HiddenFilePolicy,
    "sources.hidden_file_policy",
    { Exclude => "exclude", Include => "include", Warn => "warn" }
);

catalog_enum!(
    /// `sources.symlink_policy` (RFC-003 §6.2; default Ignore).
    SymlinkPolicy,
    "sources.symlink_policy",
    {
        Ignore => "ignore",
        FollowWithinSource => "follow_within_source",
        FollowAllWithWarning => "follow_all_with_warning",
    }
);

catalog_enum!(
    /// `files.file_status` (RFC-004 §7).
    FileStatus,
    "files.file_status",
    {
        Discovered => "discovered",
        Indexed => "indexed",
        Stale => "stale",
        Missing => "missing",
        Deleted => "deleted",
        PermissionDenied => "permission_denied",
        Unsupported => "unsupported",
        Failed => "failed",
    }
);

catalog_enum!(
    /// `index_jobs.job_type` (RFC-002 §7.9).
    JobType,
    "index_jobs.job_type",
    {
        Scan => "scan",
        Extract => "extract",
        Chunk => "chunk",
        KeywordIndex => "keyword_index",
        Embedding => "embedding",
        DeleteStale => "delete_stale",
        Rebuild => "rebuild",
    }
);

catalog_enum!(
    /// `index_jobs.status` (RFC-002 §7.9; RFC-036 adds Paused/WaitingForDependency).
    JobStatus,
    "index_jobs.status",
    {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        Canceled => "canceled",
        Blocked => "blocked",
        Paused => "paused",
        WaitingForDependency => "waiting_for_dependency",
    }
);

#[allow(clippy::derivable_impls)]
impl Default for HiddenFilePolicy {
    fn default() -> Self {
        HiddenFilePolicy::Exclude
    }
}

#[allow(clippy::derivable_impls)]
impl Default for SymlinkPolicy {
    fn default() -> Self {
        SymlinkPolicy::Ignore
    }
}

#[allow(clippy::derivable_impls)]
impl Default for IndexMode {
    fn default() -> Self {
        IndexMode::Balanced
    }
}
