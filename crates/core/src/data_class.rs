//! RFC-001 data lifecycle classification.
//!
//! Every piece of application-managed data belongs to exactly one
//! [`DataClass`]. Cleanup operations must be expressed as a
//! [`CleanupPlan`] before execution (RFC-001 §14: "No cleanup operation
//! should run without first producing a `CleanupPlan`"). Ordinary (safe)
//! cleanup must never touch [`DataClass::PersistentCatalog`].

use serde::{Deserialize, Serialize};

/// The five lifecycle classes of RFC-001 §14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    /// User configuration and known catalog state. Never deleted by
    /// ordinary cleanup (sources, policies, file catalog, settings,
    /// model registry, migrations).
    PersistentCatalog,
    /// Derived from source files and local models; deletable with
    /// confirmation, rebuildable (keyword index, embeddings, chunks).
    RebuildableIndex,
    /// Speed/convenience only; deletable automatically by TTL/LRU
    /// (search cache, snippets, rerank scores, extraction buffers).
    EphemeralCache,
    /// Local model files: removable only with strong confirmation.
    LocalDependency,
    /// Logs and events, deletable under log policy.
    OperationalLog,
}

/// Storage accounting categories (RFC-001 §10, RFC-002 §7.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageCategory {
    PersistentCatalog,
    KeywordIndex,
    VectorIndex,
    SnippetCache,
    SearchCache,
    TemporaryExtraction,
    ModelFiles,
    Logs,
}

impl StorageCategory {
    /// All categories, for iteration in accounting and the Storage view.
    pub const ALL: [StorageCategory; 8] = [
        StorageCategory::PersistentCatalog,
        StorageCategory::KeywordIndex,
        StorageCategory::VectorIndex,
        StorageCategory::SnippetCache,
        StorageCategory::SearchCache,
        StorageCategory::TemporaryExtraction,
        StorageCategory::ModelFiles,
        StorageCategory::Logs,
    ];

    /// Catalog/key string (matches RFC-001 §10 names).
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageCategory::PersistentCatalog => "persistent_catalog",
            StorageCategory::KeywordIndex => "keyword_index",
            StorageCategory::VectorIndex => "vector_index",
            StorageCategory::SnippetCache => "snippet_cache",
            StorageCategory::SearchCache => "search_cache",
            StorageCategory::TemporaryExtraction => "temporary_extraction",
            StorageCategory::ModelFiles => "model_files",
            StorageCategory::Logs => "logs",
        }
    }

    /// The lifecycle class this storage category belongs to.
    pub fn data_class(&self) -> DataClass {
        match self {
            StorageCategory::PersistentCatalog => DataClass::PersistentCatalog,
            StorageCategory::KeywordIndex | StorageCategory::VectorIndex => {
                DataClass::RebuildableIndex
            }
            StorageCategory::SnippetCache
            | StorageCategory::SearchCache
            | StorageCategory::TemporaryExtraction => DataClass::EphemeralCache,
            StorageCategory::ModelFiles => DataClass::LocalDependency,
            StorageCategory::Logs => DataClass::OperationalLog,
        }
    }
}

/// One category's measured storage (RFC-011 §11, Task 081). Never a zero
/// standing in for "not measured": a category orbok cannot measure exactly
/// is [`Self::Unknown`], rendered as such, and left out of any total --
/// the false-zero the Storage page showed before this measured every
/// namespace's `usage()` had no caller (Review Request 257 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMeasurement {
    Measured { bytes: u64, items: u64 },
    Unknown,
}

/// Cleanup actions exposed by the Storage view (RFC-001 §9, RFC-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupAction {
    /// Safe: expired search cache entries.
    ClearExpiredSearchCache,
    /// Safe: expired/temporary snippets.
    ClearSnippetCache,
    /// Safe: temporary extraction buffers and cache-engine payloads.
    ClearTemporaryExtraction,
    /// Safe: stale indexes that have already been replaced.
    RemoveReplacedStaleIndexes,
    /// Space recovery: delete keyword index (rebuild required).
    DeleteKeywordIndex,
    /// Space recovery: delete vector index / embeddings (rebuild required).
    DeleteVectorIndex,
    /// Space recovery: delete temporary-source indexes.
    RemoveTemporarySourceIndexes,
    /// Destructive: reset the whole catalog (strong confirmation).
    ResetCatalog,
}

impl CleanupAction {
    /// Lifecycle classes this action is allowed to touch.
    pub fn affected_classes(&self) -> &'static [DataClass] {
        match self {
            CleanupAction::ClearExpiredSearchCache
            | CleanupAction::ClearSnippetCache
            | CleanupAction::ClearTemporaryExtraction => &[DataClass::EphemeralCache],
            CleanupAction::RemoveReplacedStaleIndexes
            | CleanupAction::DeleteKeywordIndex
            | CleanupAction::DeleteVectorIndex
            | CleanupAction::RemoveTemporarySourceIndexes => &[DataClass::RebuildableIndex],
            CleanupAction::ResetCatalog => &[
                DataClass::PersistentCatalog,
                DataClass::RebuildableIndex,
                DataClass::EphemeralCache,
                DataClass::OperationalLog,
            ],
        }
    }

    /// Whether running this action makes reindexing necessary.
    pub fn requires_rebuild(&self) -> bool {
        !matches!(
            self,
            CleanupAction::ClearExpiredSearchCache
                | CleanupAction::ClearSnippetCache
                | CleanupAction::ClearTemporaryExtraction
        )
    }

    /// Whether the UI must show an explicit confirmation dialog.
    pub fn requires_confirmation(&self) -> bool {
        self.requires_rebuild()
    }

    /// Whether this action may touch persistent catalog data.
    pub fn touches_persistent_catalog(&self) -> bool {
        self.affected_classes()
            .contains(&DataClass::PersistentCatalog)
    }
}

/// A cleanup plan: produced first, executed second (RFC-001 §14).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub action: CleanupAction,
    pub affected_classes: Vec<DataClass>,
    pub estimated_recovered_bytes: u64,
    pub requires_rebuild: bool,
    pub requires_confirmation: bool,
}

impl CleanupPlan {
    /// Build the plan for an action with an estimated byte impact.
    pub fn for_action(action: CleanupAction, estimated_recovered_bytes: u64) -> Self {
        Self {
            action,
            affected_classes: action.affected_classes().to_vec(),
            estimated_recovered_bytes,
            requires_rebuild: action.requires_rebuild(),
            requires_confirmation: action.requires_confirmation(),
        }
    }

    /// Safe cleanup must never include the persistent catalog. Executors
    /// call this before running anything that has not been explicitly
    /// confirmed as a destructive reset.
    pub fn assert_safe_for_ordinary_cleanup(&self) -> Result<(), crate::error::OrbokError> {
        if self
            .affected_classes
            .contains(&DataClass::PersistentCatalog)
        {
            return Err(crate::error::OrbokError::CleanupWouldTouchPersistentData);
        }
        Ok(())
    }
}
