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

/// RFC-059 Amendment 1 §2a.2 (Review 213 §3, Critical→High): **not** a
/// write-time `EngineOptions.max_entries`. localcache enforces that bound
/// on every `set()`/`batch_set()`, evicting LRU by `last_accessed_at` --
/// but Extract and Chunk jobs share one priority (FIFO), so a scan's *N*
/// extractions all run before any chunk job, and Embedding runs after all
/// of those. For any corpus above the cap, a write-time bound evicts the
/// first files' entries before their own chunk jobs read them: the chunk
/// job hard-fails ("extraction cache miss"), and the embedding job
/// silently succeeds with no vectors written. §7's "the cache is by
/// definition rebuildable" is true of the data and false of the pipeline,
/// which treats this namespace as its only source of text.
///
/// The bound still matters (an unbounded cache is what made "purge
/// expired" a no-op in the first place) -- it is enforced instead when the
/// indexing pipeline is idle (RFC-059 Amendment 2 §2b, criterion 10): the
/// scheduler host's idle branch trims `ExtractSegments` to this cap,
/// least recently accessed first, once per transition to idle and only
/// with no index job pending anywhere. The value is unchanged from the
/// original measurement; only the enforcement point moved.
pub const EXTRACTION_CACHE_CLEANUP_ENTRY_CAP: usize = 20_000;

/// Namespace strings this project no longer writes.
///
/// `localcache`'s `keys`/`list_entries`/`remove` filter by namespace
/// string alone, with no requirement that the namespace be one
/// [`OrbokCacheNamespace::as_namespace`] currently produces -- so a
/// retired string here can still be addressed and deleted.
///
/// **A typo that names a namespace still in [`OrbokCacheNamespace`] would
/// delete live data.** `retired_namespaces_are_never_a_live_namespace` in
/// `crates/data/cache/src/tests.rs` checks every entry here against every
/// live namespace this crate can produce today.
pub const RETIRED_NAMESPACES: &[&str] = &[
    // Task 077 (2026-09-22): `ExtractWarning` was internally tagged, which
    // bincode could write but never read back. A `v1` entry may be
    // unreadable, or -- worse, if it happened to decode -- read as the
    // wrong shape under `:v2`'s current layout. Replaced by
    // `extract-segments:v2`.
    "extract-segments:v1",
    // Task 093 (2026-09-23, Review Request 270 §3 / Review 270 §3):
    // `ChunkBundle` (RFC-006) was specified in Appendix A §5/§10 and never
    // built -- no production code ever wrote it, confirmed both on a real,
    // used profile (one indexed folder, a search run, a result expanded:
    // zero rows under this namespace) and across this project's entire git
    // history (no commit ever added a write call). Listed here anyway, not
    // just removed from the enum below: the same defensive reasoning as
    // `extract-segments:v1` applies if that verification is ever wrong for
    // some profile this project never saw.
    "chunk-bundle:v1",
    // Task 093: `PreviewCache` (RFC-013's preview pane) -- same finding,
    // same verification. Search snippets have always been rendered from
    // `ExtractSegments` (`crates/search/engine/src/snippet.rs`), never
    // from a separate preview cache.
    "preview-cache:v1",
    // `EmbeddingBundle` (RFC-008) is also retired in the same sense --
    // never written, anywhere, ever -- but it cannot be listed here: it is
    // parameterized by model id and vector format
    // (`embedding-bundle:<model_id>:<vector_format>:v1`), and no concrete
    // instance of that pattern was ever produced to retire. There is
    // nothing for `purge_retired_namespaces` to address by name.
];

/// The orbok cache namespaces.
///
/// Task 093 (2026-09-23): this project's original design (Appendix A §5,
/// RFC-006, RFC-008) specified four namespaces. Only this one -- extracted
/// text, read by both the chunking and embedding stages -- ever got a
/// producer (`crates/pipeline/workers/src/extract.rs`). `ChunkBundle`,
/// `EmbeddingBundle` and `PreviewCache` were declared, exercised only by
/// cleanup code, storage measurement and tests, and never written by any
/// released version; see `RETIRED_NAMESPACES` above and Appendix A's dated
/// amendment for how that was verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrbokCacheNamespace {
    /// Extracted, normalized segments per source file (RFC-005 output).
    ExtractSegments,
}

impl OrbokCacheNamespace {
    /// The localcache namespace string (Appendix A §7 table).
    pub fn as_namespace(&self) -> String {
        match self {
            // Task 077: `v2`. `ExtractWarning` was internally tagged, which
            // bincode can write but not read, so every `v1` entry that
            // carried a warning was unreadable, and a `v1` entry of the old
            // shape must never be decoded as the new one. A new namespace
            // makes a `v1` entry a miss (the chunk job then re-extracts, Task
            // 056) instead of relying on it decoding wrongly.
            Self::ExtractSegments => "extract-segments:v2".to_string(),
        }
    }

    /// localcache payload version for `purge_stale_versions`.
    pub fn payload_version(&self) -> u32 {
        1
    }

    /// Lifecycle class of the payloads (RFC-001 §5, Appendix A §6):
    /// derived pipeline payloads are rebuildable.
    pub fn data_class(&self) -> DataClass {
        match self {
            Self::ExtractSegments => DataClass::RebuildableIndex,
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
            // RFC-059 Amendment 1 §2a.2: `max_entries` is deliberately
            // `None` here -- see `EXTRACTION_CACHE_CLEANUP_ENTRY_CAP`'s own
            // doc comment for why a write-time bound broke the indexing
            // pipeline it was meant to protect. The TTL alone is safe at
            // write time: it only ever makes an entry *older* than 90 days
            // expire, which cannot happen mid-run.
            Self::ExtractSegments => EngineOptions {
                ttl: Some(EXTRACTION_CACHE_TTL),
                max_entries: None,
            },
        }
    }

    /// The entry cap for this namespace, if any (RFC-059 Amendment 1
    /// §2a.2, Amendment 2 §2b) -- applied only when the indexing pipeline
    /// is idle, by the scheduler host's idle branch, never at write time
    /// and not inside a cleanup action.
    pub fn cleanup_time_entry_cap(&self) -> Option<usize> {
        match self {
            Self::ExtractSegments => Some(EXTRACTION_CACHE_CLEANUP_ENTRY_CAP),
        }
    }
}
