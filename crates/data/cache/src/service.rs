//! Cache service over localcache (Appendix A §8–§12).
//!
//! Rules enforced here:
//! - cache payloads live in `orbok-cache.sqlite3`, never in the catalog
//!   (Appendix A §3);
//! - the catalog stays authoritative — this service stores derived
//!   payloads only, keyed by canonical source path;
//! - reads and writes take a [`ValidatedPath`] so nothing outside the
//!   PathGuard boundary can be cached (RFC-003 §8 carried through);
//! - cleanup runs only from a validated [`CleanupPlan`] (RFC-001 §14);
//! - engines self-register in the catalog `cache_engines` table
//!   (RFC-002 §7.16) so the storage dashboard can enumerate them.

use crate::namespace::{OrbokCacheNamespace, RETIRED_NAMESPACES};
use localcache::{CacheEngine, ChangeDetectionMode, LocalFileCacheError};
use orbok_core::{CleanupAction, CleanupPlan, OrbokError, OrbokResult};
use orbok_db::{CACHE_FILE_NAME, Catalog};
use orbok_fs::ValidatedPath;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Tuning for one engine.
#[derive(Debug, Clone, Default)]
pub struct EngineOptions {
    /// Time-to-live; `None` keeps entries until invalidated.
    pub ttl: Option<Duration>,
    /// LRU bound; `None` is unbounded (storage budget enforced via
    /// cleanup instead).
    pub max_entries: Option<usize>,
}

/// Result of a cache-side cleanup run.
#[derive(Debug, Clone, Default)]
pub struct CacheCleanupOutcome {
    pub removed_entries: u64,
}

/// Per-namespace usage for storage accounting (Appendix A §11).
#[derive(Debug, Clone)]
pub struct NamespaceUsage {
    pub namespace: String,
    pub entries: u64,
    pub payload_bytes: u64,
}

/// The orbok cache service. One per data directory.
pub struct CacheService {
    db_path: PathBuf,
}

impl CacheService {
    /// Create the service for a data directory; the payload database is
    /// `<data_dir>/orbok-cache.sqlite3` (Appendix A §3).
    pub fn new(data_dir: &Path) -> Self {
        Self {
            db_path: data_dir.join(CACHE_FILE_NAME),
        }
    }

    /// Payload database path. Crate-internal only: storage-dashboard sizing
    /// goes through [`CacheService::usage`] instead, and no external caller
    /// should be able to recover a raw path from an already-sealed cache
    /// handle (RFC-049 Correction Request 111 §4 C1). No current internal
    /// caller either; kept `pub(crate)` rather than removed per that
    /// correction's explicit instruction, in case a future in-crate need
    /// (e.g. a diagnostics command) arises.
    #[allow(dead_code)]
    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Open a typed engine for `namespace`, registering it in the
    /// catalog `cache_engines` table. Change detection is
    /// metadata-then-full-hash (Appendix A §8: metadata fast path with
    /// hash confirmation, mirroring the scanner's policy).
    pub fn engine<T: Serialize + DeserializeOwned>(
        &self,
        catalog: &Catalog,
        namespace: &OrbokCacheNamespace,
        options: EngineOptions,
    ) -> OrbokResult<CacheEngine<T>> {
        let mut builder = CacheEngine::<T>::builder()
            .database(&self.db_path)
            .namespace(namespace.as_namespace())
            .payload_version(namespace.payload_version())
            .change_detection(ChangeDetectionMode::MetadataThenFullHash)
            .compress();
        builder = match options.ttl {
            Some(ttl) => builder.ttl(ttl),
            None => builder.no_ttl(),
        };
        if let Some(n) = options.max_entries {
            builder = builder.max_entries(n);
        }
        let engine = builder.build().map_err(cache_err)?;
        self.register_engine::<T>(catalog, namespace, &options)?;
        Ok(engine)
    }

    /// Freshness-checked read: returns the payload only when localcache
    /// confirms the source file is unchanged (Appendix A §8). The
    /// catalog/scanner remains the authority for file state.
    pub fn get_fresh<T: Serialize + DeserializeOwned>(
        engine: &CacheEngine<T>,
        path: &ValidatedPath,
    ) -> OrbokResult<Option<T>> {
        Ok(engine
            .get_if_fresh(&path.canonical)
            .map_err(cache_err)?
            .map(|entry| entry.payload))
    }

    /// Store a derived payload for a boundary-validated source path.
    pub fn put<T: Serialize + DeserializeOwned>(
        engine: &CacheEngine<T>,
        path: &ValidatedPath,
        payload: &T,
    ) -> OrbokResult<()> {
        engine.set(&path.canonical, payload).map_err(cache_err)
    }

    /// Invalidate one entry (e.g. file deleted from catalog).
    pub fn remove<T: Serialize + DeserializeOwned>(
        engine: &CacheEngine<T>,
        path: &ValidatedPath,
    ) -> OrbokResult<bool> {
        engine.remove(&path.canonical).map_err(cache_err)
    }

    /// Invalidate one entry by the file's path, for a file that has left its
    /// folder (Task 114) and so has no boundary-validated path any more.
    /// `false` when there was no entry.
    pub fn remove_path<T: Serialize + DeserializeOwned>(
        engine: &CacheEngine<T>,
        path: &std::path::Path,
    ) -> OrbokResult<bool> {
        engine.remove(path).map_err(cache_err)
    }

    /// Safe cleanup driven by a validated plan (RFC-001 §9, Appendix A
    /// §12). Maps each action to its payload namespaces and runs
    /// expiry + missing-file + stale-version maintenance there.
    pub fn run_safe_cleanup(
        &self,
        catalog: &Catalog,
        plan: &CleanupPlan,
    ) -> OrbokResult<CacheCleanupOutcome> {
        plan.assert_safe_for_ordinary_cleanup()?;
        // Task 093: `ClearSnippetCache`'s and `RemoveReplacedStaleIndexes`'s
        // former targets here (`PreviewCache`, `ChunkBundle`) were retired
        // -- neither ever had a producer. This function is unreachable in
        // production today (`ProfileCache::run_safe_cleanup` delegates to
        // `orbok_workers::CleanupService::run_safe` instead, not this one;
        // Review Request 270's research), so these two now map to no
        // namespace work here, same as `ClearExpiredSearchCache` already
        // did -- their real work happens catalog-side either way.
        let namespaces: Vec<OrbokCacheNamespace> = match plan.action {
            CleanupAction::ClearTemporaryExtraction => vec![OrbokCacheNamespace::ExtractSegments],
            CleanupAction::ClearSnippetCache
            | CleanupAction::RemoveReplacedStaleIndexes
            | CleanupAction::ClearExpiredSearchCache => vec![],
            _ => return Err(OrbokError::CleanupWouldTouchPersistentData),
        };
        let mut outcome = CacheCleanupOutcome::default();
        for namespace in namespaces {
            let engine = self.maintenance_engine(catalog, &namespace)?;
            outcome.removed_entries += engine.cleanup_expired().map_err(cache_err)? as u64;
            outcome.removed_entries += engine.cleanup_missing_files().map_err(cache_err)? as u64;
            outcome.removed_entries += engine.purge_stale_versions().map_err(cache_err)? as u64;
            tracing::debug!(
                namespace = namespace.as_namespace(),
                removed = outcome.removed_entries,
                "cache cleanup pass"
            );
        }
        Ok(outcome)
    }

    /// Task 079: delete every entry under every namespace this project has
    /// retired ([`RETIRED_NAMESPACES`]). Returns the number of entries
    /// removed.
    ///
    /// Opens a raw engine over each retired namespace *string* rather than
    /// going through [`Self::engine`] (which takes an [`OrbokCacheNamespace`]
    /// -- a retired namespace has no such variant any more) or
    /// [`Self::maintenance_engine`] (same requirement). No `payload_version`
    /// is set, which `localcache` treats as "match any version" (the
    /// version check inside `get`/`get_if_fresh` is skipped when it is 0),
    /// so every row is addressed regardless of the shape it was written in
    /// -- the same reasoning that makes reading it back unsafe is exactly
    /// why deleting it without decoding it is safe. Never touches a live
    /// namespace: `retired_namespaces_are_never_a_live_namespace` in this
    /// crate's tests guards `RETIRED_NAMESPACES` itself.
    ///
    /// Does not call `shrink_database`: a caller purging alongside another
    /// namespace erasure (`CleanupService::run_cache_side`) should VACUUM
    /// once, after every deletion in the same pass, not once per namespace.
    pub fn purge_retired_namespaces(&self) -> OrbokResult<u64> {
        let mut removed = 0u64;
        for namespace in RETIRED_NAMESPACES {
            let engine = CacheEngine::<serde_json::Value>::builder()
                .database(&self.db_path)
                .namespace((*namespace).to_string())
                .change_detection(ChangeDetectionMode::MetadataThenFullHash)
                .build()
                .map_err(cache_err)?;
            for key in engine.keys(None).map_err(cache_err)? {
                if engine.remove(&key).map_err(cache_err)? {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    /// Task 079 §1.4: usage for every namespace this project has retired,
    /// the same shape [`Self::usage`] returns for a live one -- so a
    /// retired namespace's space is visible rather than silently missing
    /// from a total, while it still exists. No `cache_engines` row is
    /// registered for these (that table is for engines this project still
    /// opens for real work); the same raw-namespace-string engine
    /// [`Self::purge_retired_namespaces`] uses, since `cache_stats` is a
    /// metadata aggregate that never decodes a payload.
    pub fn retired_namespace_usage(&self) -> OrbokResult<Vec<NamespaceUsage>> {
        let mut out = Vec::new();
        for namespace in RETIRED_NAMESPACES {
            let engine = CacheEngine::<serde_json::Value>::builder()
                .database(&self.db_path)
                .namespace((*namespace).to_string())
                .change_detection(ChangeDetectionMode::MetadataThenFullHash)
                .build()
                .map_err(cache_err)?;
            let stats = engine.cache_stats().map_err(cache_err)?;
            out.push(NamespaceUsage {
                namespace: stats.namespace,
                entries: stats.total_entries as u64,
                payload_bytes: stats.total_payload_bytes,
            });
        }
        Ok(out)
    }

    /// Reclaim file space after large deletions (storage dashboard's
    /// explicit "shrink" action; Appendix A §12). `shrink_database` VACUUMs
    /// the whole cache file, not just one namespace -- any live namespace's
    /// engine handle reaches it; `ExtractSegments` is the only one left
    /// (Task 093).
    pub fn shrink(&self, catalog: &Catalog) -> OrbokResult<()> {
        let engine = self.maintenance_engine(catalog, &OrbokCacheNamespace::ExtractSegments)?;
        engine.shrink_database().map_err(cache_err)
    }

    /// Usage per namespace for storage accounting (Appendix A §11).
    pub fn usage(
        &self,
        catalog: &Catalog,
        namespaces: &[OrbokCacheNamespace],
    ) -> OrbokResult<Vec<NamespaceUsage>> {
        let mut out = Vec::new();
        for namespace in namespaces {
            let engine = self.maintenance_engine(catalog, namespace)?;
            let stats = engine.cache_stats().map_err(cache_err)?;
            out.push(NamespaceUsage {
                namespace: stats.namespace,
                entries: stats.total_entries as u64,
                payload_bytes: stats.total_payload_bytes,
            });
        }
        Ok(out)
    }

    /// Untyped (JSON-payload) engine for maintenance operations that do
    /// not deserialize payloads.
    fn maintenance_engine(
        &self,
        catalog: &Catalog,
        namespace: &OrbokCacheNamespace,
    ) -> OrbokResult<CacheEngine<serde_json::Value>> {
        // RFC-059 §7 Slice 3: per-namespace tuning (ExtractSegments' TTL
        // and entry cap), not a blanket default -- see
        // `OrbokCacheNamespace::default_engine_options`'s own doc comment
        // for why passing the wrong options here would corrupt that
        // namespace's registered `cache_engines` metadata.
        self.engine::<serde_json::Value>(catalog, namespace, namespace.default_engine_options())
    }

    /// Upsert the engine registration row (RFC-002 §7.16).
    fn register_engine<T>(
        &self,
        catalog: &Catalog,
        namespace: &OrbokCacheNamespace,
        options: &EngineOptions,
    ) -> OrbokResult<()> {
        let data_class = match namespace.data_class() {
            orbok_core::DataClass::RebuildableIndex => "rebuildable_index",
            _ => "ephemeral_cache",
        };
        let id = format!("ce_{}", namespace.as_namespace().replace([':', '/'], "_"));
        let now = orbok_core::now_iso8601();
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO cache_engines (cache_engine_id, engine_kind, database_path, namespace, \
             data_class, payload_type, payload_version, ttl_seconds, max_entries, status, \
             created_at, updated_at) VALUES (?1,'localcache',?2,?3,?4,?5,?6,?7,?8,'active',?9,?9) \
             ON CONFLICT(engine_kind, database_path, namespace) DO UPDATE SET \
             payload_type = ?5, payload_version = ?6, ttl_seconds = ?7, max_entries = ?8, \
             status = 'active', updated_at = ?9",
            rusqlite::params![
                id,
                self.db_path.to_string_lossy(),
                namespace.as_namespace(),
                data_class,
                std::any::type_name::<T>(),
                namespace.payload_version(),
                options.ttl.map(|d| d.as_secs() as i64),
                options.max_entries.map(|n| n as i64),
                now,
            ],
        )
        .map_err(|e| OrbokError::Database(e.to_string()))?;
        Ok(())
    }
}

fn cache_err(e: LocalFileCacheError) -> OrbokError {
    OrbokError::Cache(e.to_string())
}
