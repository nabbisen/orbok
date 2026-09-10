//! Cache namespaces (Appendix A §7).
//!
//! Namespace strings carry an explicit schema version suffix; payload
//! shape changes bump the version so `purge_stale_versions` can retire
//! old rows safely.

use crate::service::EngineOptions;
use orbok_core::DataClass;
use std::time::Duration;

/// RFC-059 §7/§11 open question 1: the extraction cache's TTL and entry
/// cap. Proposed, not decided -- routed to the architect per the
/// handoff, which explicitly does not expect an invented number.
///
/// Derived from one real measurement pass (RFC-059 §7's own requirement),
/// not guessed: 110 real markdown documents -- this project's own
/// `rfcs/` tree, indexed through the production pipeline -- averaged
/// ~5.4 KB/entry compressed (595,247 payload bytes / 110 entries). At
/// 20,000 entries and that average, the namespace's worst-case size is
/// roughly 100 MB; real corpora with larger documents (PDFs, code) will
/// average higher per entry, which is exactly the uncertainty RFC-059
/// §11 open question 1 leaves for the architect to weigh rather than
/// have this implementation guess at a byte-based figure `localcache`
/// 0.21.1's `EngineOptions` has no field for in the first place (only
/// `max_entries`, a count).
const EXTRACTION_CACHE_TTL: Duration = Duration::from_secs(90 * 24 * 60 * 60); // 90 days
const EXTRACTION_CACHE_MAX_ENTRIES: usize = 20_000;

/// The orbok cache namespaces. Embedding bundles are parameterized by
/// model and vector format so different models never collide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrbokCacheNamespace {
    /// Extracted, normalized segments per source file (RFC-005 output).
    ExtractSegments,
    /// Chunk bundles per source file (RFC-006 output).
    ChunkBundle,
    /// Embedding bundles per source file for one model+format (RFC-008).
    EmbeddingBundle {
        model_id: String,
        vector_format: String,
    },
    /// Rendered preview/snippet payloads (RFC-013 preview pane).
    PreviewCache,
}

impl OrbokCacheNamespace {
    /// The localcache namespace string (Appendix A §7 table).
    pub fn as_namespace(&self) -> String {
        match self {
            Self::ExtractSegments => "extract-segments:v1".to_string(),
            Self::ChunkBundle => "chunk-bundle:v1".to_string(),
            Self::EmbeddingBundle {
                model_id,
                vector_format,
            } => format!("embedding-bundle:{model_id}:{vector_format}:v1"),
            Self::PreviewCache => "preview-cache:v1".to_string(),
        }
    }

    /// localcache payload version for `purge_stale_versions`.
    pub fn payload_version(&self) -> u32 {
        1
    }

    /// Lifecycle class of the payloads (RFC-001 §5, Appendix A §6):
    /// derived pipeline payloads are rebuildable; previews are ephemeral.
    pub fn data_class(&self) -> DataClass {
        match self {
            Self::ExtractSegments | Self::ChunkBundle | Self::EmbeddingBundle { .. } => {
                DataClass::RebuildableIndex
            }
            Self::PreviewCache => DataClass::EphemeralCache,
        }
    }

    /// The engine tuning every open site for this namespace should use
    /// (RFC-059 §7 Slice 3): a single source of truth so every call site
    /// stays in sync, including cleanup call sites that reopen the engine
    /// only to purge it -- passing mismatched options there would
    /// overwrite this namespace's registered `cache_engines` row
    /// (`CacheService::register_engine` upserts on every open) with
    /// stale ttl/max_entries metadata, misleading the storage dashboard
    /// about what is actually configured.
    pub fn default_engine_options(&self) -> EngineOptions {
        match self {
            Self::ExtractSegments => EngineOptions {
                ttl: Some(EXTRACTION_CACHE_TTL),
                max_entries: Some(EXTRACTION_CACHE_MAX_ENTRIES),
            },
            Self::ChunkBundle | Self::EmbeddingBundle { .. } | Self::PreviewCache => {
                EngineOptions::default()
            }
        }
    }
}
