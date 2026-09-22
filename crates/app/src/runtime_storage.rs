//! RFC-049 Slice 2 storage boundary (Correction Request 111 §3-4, C1).
//!
//! [`RuntimeStorage`] is the only value permitted to derive a filesystem path
//! from a [`RuntimeContext`] and use it to open a persistent resource. Every
//! resource it produces crosses back out as a sealed handle
//! ([`ProfileCache`], [`ProfileModelStore`], an opened `Catalog`) or an
//! already-loaded value (a settings struct) — never as a `Path`/`PathBuf`.
//! Callers outside this module cannot construct `ProfileCache` or
//! `ProfileModelStore` from an arbitrary path: both have a private field and
//! no public constructor, so the only way to obtain one is through
//! `RuntimeStorage`, which itself only accepts a `RuntimeContext`.
//!
//! This lives in the `orbok` library crate, alongside `runtime_context`,
//! because `RuntimeContext`'s path accessors are `pub(crate)` to this crate.
//! The binary crate (`main.rs`, `bootstrap.rs`, `download.rs`) can only reach
//! them through the sealed API in this module — a caller there cannot even
//! attempt to bypass the boundary, because the raw accessors do not compile
//! outside this crate.

use crate::runtime_context::{
    AllowRuntimePathProbe, RuntimeAccess, RuntimeContext, RuntimePathKind, RuntimePathProbe,
};
use orbok_cache::{EngineOptions, NamespaceUsage, OrbokCacheNamespace};
use orbok_core::{CleanupPlan, OrbokResult};
use orbok_db::Catalog;
use orbok_models::{
    ExclusiveAccess, ManagedModelStore, ModelStoreLockError, ModelStoreMutationGuard, SharedAccess,
};
use orbok_workers::{
    FullCleanupOutcome, ManagedModelStartupOutcome, ModelDeliveryError, ModelDeliveryEvent,
    ModelDeliveryOutcome, ModelLifecycleError, RecoveryReport,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// `<data_dir>/models/<default embedding model id>` — the one default
/// managed-model root derived from a resolved profile data directory.
///
/// `pub(crate)`, not `pub`: Review 113 F1/F2 found this was re-exported into
/// the bin crate and, combined with a `Catalog`-derived data directory,
/// composed into a managed-store construction that bypassed
/// `RuntimeStorage` entirely. Now used only inside this module, by
/// `RuntimeStorage::model_store`.
pub(crate) fn default_model_store_root(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("multilingual-e5-small")
}

/// The only production boundary permitted to perform profile filesystem I/O.
/// Tests replace its probe with a recorder/denier while retaining the exact
/// same operations, so observation surrounds the operation rather than
/// merely returning a selected path to an unrelated caller.
pub struct RuntimeStorage<'a, P: ?Sized> {
    context: &'a RuntimeContext,
    probe: &'a P,
}

impl<'a, P: RuntimePathProbe + ?Sized> RuntimeStorage<'a, P> {
    pub fn new(context: &'a RuntimeContext, probe: &'a P) -> Self {
        Self { context, probe }
    }

    fn path(&self, kind: RuntimePathKind) -> io::Result<&'a Path> {
        RuntimeAccess::new(self.context, self.probe).active_path(kind)
    }

    pub fn open_catalog(&self) -> OrbokResult<Catalog> {
        self.open_catalog_staged().map_err(Into::into)
    }

    /// Task 071: [`Self::open_catalog`], keeping which stage failed --
    /// authorising and creating the data folder, or opening the catalog
    /// file in it -- so startup can say which one the user should check.
    pub fn open_catalog_staged(&self) -> Result<Catalog, CatalogOpenError> {
        let path = self
            .path(RuntimePathKind::Catalog)
            .map_err(CatalogOpenError::DataFolder)?;
        std::fs::create_dir_all(self.context.data_dir()).map_err(CatalogOpenError::DataFolder)?;
        Catalog::open(path).map_err(CatalogOpenError::Catalog)
    }

    /// Authorize and construct the sealed cache handle for the active
    /// profile. No lazy open happens here — `ProfileCache::engine` is where
    /// the localcache database actually opens — but the path can never
    /// escape this call as a raw value.
    pub fn cache(&self) -> io::Result<ProfileCache> {
        self.path(RuntimePathKind::Cache)?;
        Ok(ProfileCache::new(self.context.data_dir()))
    }

    /// Authorize, create, and construct the sealed managed-model-store
    /// handle for the active profile.
    pub fn model_store(&self) -> io::Result<ProfileModelStore> {
        self.path(RuntimePathKind::Models)?;
        let root = default_model_store_root(self.context.data_dir());
        std::fs::create_dir_all(&root)?;
        Ok(ProfileModelStore::new(root))
    }

    /// Not test-only: `#[cfg(test)]` in this library crate would not be
    /// active when the `orbok` binary crate compiles its own tests (a
    /// dependency's `cfg(test)` items are never visible downstream), so this
    /// stays a small, always-available helper used only by that test code.
    pub fn ensure_support_dir(&self, kind: RuntimePathKind) -> io::Result<PathBuf> {
        debug_assert!(matches!(
            kind,
            RuntimePathKind::Diagnostics | RuntimePathKind::Temporary
        ));
        let path = self.path(kind)?;
        std::fs::create_dir_all(path)?;
        Ok(path.to_path_buf())
    }

    /// Load a settings value from the active profile's settings file. On a
    /// missing file the default value is persisted at the active resolved
    /// path (and nowhere else) before being returned, matching the
    /// pre-Slice-2 `load_or_default` first-load contract (Correction Request
    /// 111 C4). Any other read/parse error falls back to the default without
    /// writing it.
    pub fn load_settings<T>(&self) -> io::Result<T>
    where
        T: Serialize + DeserializeOwned + Default,
    {
        let path = self.path(RuntimePathKind::Settings)?;
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let default = T::default();
                write_json(path, &default)?;
                Ok(default)
            }
            Err(_) => Ok(T::default()),
        }
    }

    pub fn save_settings<T: Serialize>(&self, value: &T) -> io::Result<()> {
        let path = self.path(RuntimePathKind::Settings)?;
        write_json(path, value)
    }

    pub fn run_startup_recovery(&self, catalog: &Catalog) -> OrbokResult<RecoveryReport> {
        let data_dir = self.path(RuntimePathKind::Recovery)?;
        orbok_workers::run_startup_recovery(catalog, &data_dir.join(orbok_db::CACHE_FILE_NAME))
    }

    /// Run RFC-050 managed-model startup recovery against a store already
    /// sealed to this same active context.
    pub fn run_managed_model_startup(
        &self,
        catalog: &Catalog,
        model_store: &ProfileModelStore,
    ) -> Result<ManagedModelStartupOutcome, ModelLifecycleError> {
        // The model root was authorized immediately before its creation in
        // `model_store()`; nothing here re-resolves or re-authorizes it.
        orbok_workers::run_managed_model_startup(catalog, &model_store.store)
    }

    /// Task 081 (RFC-011 §11): every storage category, measured fresh --
    /// never a stale cached figure (a caller must call this again to see
    /// current numbers; nothing here memoizes across calls, since a stale
    /// number is the same class of defect as the false zero it replaces,
    /// Review Request 257 §3). Each measurement below says how exact it is
    /// in its own comment; the caller decides how to render a failure to
    /// read one path or open the cache (`Unknown`, never a silent zero).
    ///
    /// **`persistent_catalog` already includes `keyword_index` and
    /// `vector_index`'s own bytes** -- all three live in the one catalog
    /// file. The two index categories are exact `dbstat` sub-measurements
    /// of that same file, shown so a user can see what is inside it, not
    /// additional bytes; a caller that sums every row into one grand total
    /// must exclude `keyword_index`/`vector_index` from that sum or it
    /// double-counts. `storage_view` does this; see its own comment.
    pub fn measure_storage(
        &self,
        catalog: &Catalog,
    ) -> (
        Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
        Option<u64>,
    ) {
        use orbok_core::StorageCategory as Cat;

        let mut out = Vec::with_capacity(Cat::ALL.len());

        // persistent_catalog: the catalog file's own size on disk, plus
        // its row count (registered files) as the "items" figure --
        // exact, a plain `fs::metadata` read.
        out.push((
            Cat::PersistentCatalog,
            match std::fs::metadata(self.context.catalog_file()) {
                Ok(meta) => storage_measurement::Measurement {
                    bytes: meta.len(),
                    items: storage_measurement::count(catalog, "SELECT COUNT(*) FROM files"),
                }
                .into(),
                Err(_) => orbok_core::StorageMeasurement::Unknown,
            },
        ));

        // keyword_index / vector_index: exact, via SQLite's own `dbstat`
        // virtual table -- confirmed present in this build by a direct
        // query (bundled libsqlite3-sys compiles it in; no separate
        // rusqlite Cargo feature gates the C-level table itself, only a
        // typed Rust wrapper this file has no need of). Real page bytes,
        // not a row-count estimate.
        out.push((
            Cat::KeywordIndex,
            storage_measurement::dbstat(
                catalog,
                "name LIKE 'chunk_fts%' OR name LIKE '%keyword_index%'",
                "SELECT COUNT(*) FROM keyword_index_records",
            ),
        ));
        out.push((
            Cat::VectorIndex,
            storage_measurement::dbstat(
                catalog,
                "name LIKE '%embeddings%'",
                "SELECT COUNT(*) FROM embeddings",
            ),
        ));

        // snippet_cache: two real backing stores are cleared by one
        // action (`ClearSnippetCache`, `cleanup.rs`'s `clear_snippet_cache`
        // plus `cleanup_service.rs`'s cache-side purge of `PreviewCache`)
        // -- the catalog's own `snippet_cache` table (dbstat) and the
        // separate localcache `PreviewCache` namespace
        // (`CacheService::usage`). Summed: both are real space that one
        // button reclaims.
        let snippet_table = storage_measurement::dbstat(
            catalog,
            "name LIKE 'snippet_cache%'",
            "SELECT COUNT(*) FROM snippet_cache",
        );
        let snippet_namespace = self
            .cache()
            .ok()
            .and_then(|cache| {
                cache
                    .usage(catalog, &[OrbokCacheNamespace::PreviewCache])
                    .ok()
            })
            .map(storage_measurement::from_namespace_usage)
            .unwrap_or(orbok_core::StorageMeasurement::Unknown);
        out.push((
            Cat::SnippetCache,
            storage_measurement::add(snippet_table, snippet_namespace),
        ));

        // search_cache: the catalog's own tables -- `ClearExpiredSearchCache`
        // deletes from `search_result_cache` (`cleanup.rs`); `search_queries`
        // is its companion table. No separate localcache namespace backs
        // this category (`OrbokCacheNamespace` has none named for it).
        out.push((
            Cat::SearchCache,
            storage_measurement::dbstat(
                catalog,
                "name LIKE 'search_result_cache%' OR name LIKE 'search_queries%'",
                "SELECT (SELECT COUNT(*) FROM search_result_cache) + \
                 (SELECT COUNT(*) FROM search_queries)",
            ),
        ));

        // temporary_extraction: the localcache ExtractSegments namespace,
        // with Task 079's retired `extract-segments:v1` rows folded in --
        // the namespace it retired, so a user sees one "temporary
        // extraction" figure, not a separate line for a namespace string
        // nobody should need to know about.
        let extract_current = self
            .cache()
            .ok()
            .and_then(|cache| {
                cache
                    .usage(catalog, &[OrbokCacheNamespace::ExtractSegments])
                    .ok()
            })
            .map(storage_measurement::from_namespace_usage)
            .unwrap_or(orbok_core::StorageMeasurement::Unknown);
        let extract_retired = self
            .cache()
            .ok()
            .and_then(|cache| cache.service().retired_namespace_usage().ok())
            .map(storage_measurement::from_namespace_usage)
            .unwrap_or(orbok_core::StorageMeasurement::Unknown);
        out.push((
            Cat::TemporaryExtraction,
            storage_measurement::add(extract_current, extract_retired),
        ));

        // model_files: the model store directory's size, walked
        // recursively -- exact, real bytes and file count on disk.
        out.push((
            Cat::ModelFiles,
            match storage_measurement::dir_size(self.context.models_dir()) {
                Ok((bytes, items)) => orbok_core::StorageMeasurement::Measured { bytes, items },
                Err(_) => orbok_core::StorageMeasurement::Unknown,
            },
        ));

        // logs: orbok writes no log files (Task 067 §0 -- tracing output
        // goes to stderr only, never a file). Not "unmeasured": this is a
        // known, certain zero, not a stand-in for a measurement that
        // failed.
        out.push((
            Cat::Logs,
            orbok_core::StorageMeasurement::Measured { bytes: 0, items: 0 },
        ));

        // Task 081 §2: the cache file's own size, not one of the eight
        // categories above (see this method's own doc comment).
        let cache_file_bytes = std::fs::metadata(self.context.cache_file())
            .map(|m| m.len())
            .ok();

        (out, cache_file_bytes)
    }
}

/// Task 081's storage-measurement helpers: `dbstat` byte sums, a
/// directory walk, and the small conversions between this module's
/// `NamespaceUsage`/row shapes and [`orbok_core::StorageMeasurement`].
/// Free functions, not methods, since none needs `RuntimeStorage`'s own
/// path-boundary state -- each already takes exactly the borrowed handle
/// it touches.
mod storage_measurement {
    use orbok_core::StorageMeasurement;
    use orbok_db::Catalog;

    pub(super) struct Measurement {
        pub bytes: u64,
        pub items: u64,
    }

    impl From<Measurement> for StorageMeasurement {
        fn from(m: Measurement) -> Self {
            StorageMeasurement::Measured {
                bytes: m.bytes,
                items: m.items,
            }
        }
    }

    /// Sum of `pgsize` (real on-disk page bytes) over every `dbstat` row
    /// whose table name matches `name_where`, plus a separate row count
    /// from `count_sql` -- `dbstat` reports pages, not rows, so the two
    /// numbers come from two different queries. `Unknown` only if the
    /// `dbstat` query itself fails (the virtual table is missing, e.g. an
    /// exotic SQLite build); a table that simply has no rows yet reports
    /// zero, not unknown, since `dbstat` still answers the question.
    pub(super) fn dbstat(
        catalog: &Catalog,
        name_where: &str,
        count_sql: &str,
    ) -> StorageMeasurement {
        // The guard must not outlive this block: `count` below takes its
        // own lock on the same `Catalog`, and `MutexGuard` is not
        // reentrant -- holding this one across that call deadlocks the
        // single thread that took it (confirmed: the first version of
        // this function did exactly that, hanging every call site).
        let bytes: Result<i64, _> = {
            let conn = catalog.lock();
            conn.query_row(
                &format!("SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE {name_where}"),
                [],
                |r| r.get(0),
            )
        };
        match bytes {
            Ok(bytes) => Measurement {
                bytes: bytes.max(0) as u64,
                items: count(catalog, count_sql),
            }
            .into(),
            Err(_) => StorageMeasurement::Unknown,
        }
    }

    /// One scalar `COUNT(*)`-shaped query, 0 on any failure (a failed
    /// item count does not sink an otherwise-successful byte measurement
    /// -- the category is still `Measured`, just with `items: 0` rather
    /// than a number this call could not get).
    pub(super) fn count(catalog: &Catalog, sql: &str) -> u64 {
        let conn = catalog.lock();
        conn.query_row(sql, [], |r| r.get::<_, i64>(0))
            .map(|n| n.max(0) as u64)
            .unwrap_or(0)
    }

    /// Sum of `payload_bytes`/`entries` over a `usage()`/
    /// `retired_namespace_usage()` result -- both already exact per-entry
    /// byte counts `localcache` itself reports (the compressed stored
    /// payload, not a derived estimate); summing several namespaces' rows
    /// is exact addition, not a new estimate.
    pub(super) fn from_namespace_usage(
        rows: Vec<orbok_cache::NamespaceUsage>,
    ) -> StorageMeasurement {
        let bytes = rows.iter().map(|r| r.payload_bytes).sum();
        let items = rows.iter().map(|r| r.entries).sum();
        Measurement { bytes, items }.into()
    }

    /// Combine two measurements for one category (Task 081: `snippet_cache`
    /// and `temporary_extraction` each have two real backing stores).
    /// `Unknown` is infectious: a category with one known and one unknown
    /// backing store is itself unknown, never half-reported as if the
    /// unknown side were zero.
    pub(super) fn add(a: StorageMeasurement, b: StorageMeasurement) -> StorageMeasurement {
        match (a, b) {
            (
                StorageMeasurement::Measured {
                    bytes: b1,
                    items: i1,
                },
                StorageMeasurement::Measured {
                    bytes: b2,
                    items: i2,
                },
            ) => StorageMeasurement::Measured {
                bytes: b1 + b2,
                items: i1 + i2,
            },
            _ => StorageMeasurement::Unknown,
        }
    }

    /// Recursive directory size and file count -- exact, real bytes on
    /// disk for every regular file under `path` (symlinks are not
    /// followed, matching the model store's own on-disk shape: it never
    /// creates one). No new dependency: a plain `read_dir` walk.
    pub(super) fn dir_size(path: &std::path::Path) -> std::io::Result<(u64, u64)> {
        let mut bytes = 0u64;
        let mut items = 0u64;
        if !path.exists() {
            return Ok((0, 0));
        }
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    stack.push(entry.path());
                } else if file_type.is_file() {
                    bytes += entry.metadata()?.len();
                    items += 1;
                }
            }
        }
        Ok((bytes, items))
    }
}

/// Task 071: which stage of opening the catalog failed.
#[derive(Debug)]
pub enum CatalogOpenError {
    /// The data folder could not be authorised or created.
    DataFolder(io::Error),
    /// The folder was usable, but the catalog file in it could not be
    /// opened, migrated, or was written by a newer orbok.
    Catalog(orbok_core::OrbokError),
}

impl From<CatalogOpenError> for orbok_core::OrbokError {
    fn from(error: CatalogOpenError) -> Self {
        match error {
            CatalogOpenError::DataFolder(error) => Self::Io(error),
            CatalogOpenError::Catalog(error) => error,
        }
    }
}

pub fn open_catalog_staged(context: &RuntimeContext) -> Result<Catalog, CatalogOpenError> {
    RuntimeStorage::new(context, &AllowRuntimePathProbe).open_catalog_staged()
}

pub fn open_catalog(context: &RuntimeContext) -> OrbokResult<Catalog> {
    open_catalog_with(context, &AllowRuntimePathProbe)
}

pub fn open_catalog_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> OrbokResult<Catalog> {
    RuntimeStorage::new(context, probe).open_catalog()
}

pub fn cache(context: &RuntimeContext) -> io::Result<ProfileCache> {
    cache_with(context, &AllowRuntimePathProbe)
}

pub fn cache_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> io::Result<ProfileCache> {
    RuntimeStorage::new(context, probe).cache()
}

pub fn model_store(context: &RuntimeContext) -> io::Result<ProfileModelStore> {
    model_store_with(context, &AllowRuntimePathProbe)
}

pub fn model_store_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> io::Result<ProfileModelStore> {
    RuntimeStorage::new(context, probe).model_store()
}

/// Task 081: every storage category, measured fresh (see
/// `RuntimeStorage::measure_storage`'s own doc comment for what "fresh"
/// means here and why).
pub fn measure_storage(
    context: &RuntimeContext,
    catalog: &Catalog,
) -> (
    Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
    Option<u64>,
) {
    RuntimeStorage::new(context, &AllowRuntimePathProbe).measure_storage(catalog)
}

pub fn load_settings<T>(context: &RuntimeContext) -> io::Result<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    load_settings_with(context, &AllowRuntimePathProbe)
}

pub fn load_settings_with<T, P>(context: &RuntimeContext, probe: &P) -> io::Result<T>
where
    T: Serialize + DeserializeOwned + Default,
    P: RuntimePathProbe + ?Sized,
{
    RuntimeStorage::new(context, probe).load_settings()
}

pub fn save_settings<T: Serialize>(context: &RuntimeContext, value: &T) -> io::Result<()> {
    save_settings_with(context, &AllowRuntimePathProbe, value)
}

pub fn save_settings_with<T, P>(context: &RuntimeContext, probe: &P, value: &T) -> io::Result<()>
where
    T: Serialize,
    P: RuntimePathProbe + ?Sized,
{
    RuntimeStorage::new(context, probe).save_settings(value)
}

/// Write `value` to `path` as an atomic replace: serialize, write to a
/// sibling temp file, harden its permissions, `fsync`, then rename over the
/// target. The temp file lives in the same directory as `path` so the final
/// rename stays on one filesystem (a cross-filesystem rename is not atomic
/// and can fail or silently degrade to copy-then-delete) — this is Task
/// 016 (Review 166 §3): `settings.json` previously went through a plain
/// truncate-then-write, which a crash or power loss mid-write could leave
/// truncated, and which was created at the process umask rather than
/// hardened.
///
/// Deliberately not `model_durability::durable_rename`: that helper
/// requires the destination to be absent (`DestinationExists`) and both
/// paths under the managed model root, neither of which holds for
/// replacing an existing settings file outside that root.
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings file has no parent directory",
        )
    })?;
    std::fs::create_dir_all(directory)?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let temp_path = sibling_temp_path(path);
    let result = write_and_rename(&temp_path, path, &bytes);
    if result.is_err() {
        // Best-effort: a failure here does not change which error the
        // caller sees, it only avoids leaving the temp file behind.
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn sibling_temp_path(path: &Path) -> PathBuf {
    // Unique per process, not merely predictable (Task 018 §2, following
    // Review 167 §3): a fixed name would let two `orbok` processes on the
    // same profile interleave their writes through the open call's
    // truncate -- nothing in `crates/app` enforces single-instance. Keeps
    // `create`'s truncate-on-open shape rather than `create_new`, whose
    // failure mode is worse: a temp file orphaned by a hard crash (where
    // the cleanup below never ran) would block every future write
    // permanently instead of just risking a rare, already-narrow
    // collision window. Simplest of the shapes considered, and sufficient
    // given how unlikely two live instances on one profile are.
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp.");
    name.push(std::process::id().to_string());
    PathBuf::from(name)
}

fn write_and_rename(temp_path: &Path, target: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;

    let mut file = open_hardened_temp_file(temp_path)?;

    file.write_all(bytes)?;
    // Crash durability, not just atomicity: without this, the temp file's
    // content can still be sitting in a page cache the OS never flushes
    // before a power loss, and the rename that follows would durably point
    // at nothing recoverable.
    file.sync_all()?;
    drop(file);

    std::fs::rename(temp_path, target)
}

/// Open (create-or-truncate) `temp_path` with its final permissions
/// already applied at the `open(2)` call itself (Task 018 §1, following a
/// refinement offered back by the `app-json-settings` maintainers) rather
/// than via a later `set_permissions`: on Unix the file is then never
/// observable at the process umask, not even empty, not even for one
/// syscall -- the previous window was already vanishingly small, but this
/// removes it rather than shrinking it further.
fn open_hardened_temp_file(temp_path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    // Windows: no `0600` equivalent attempted here — ACL-based hardening
    // is out of scope for this task (Task 016 §3). `std::fs::rename` in
    // `write_and_rename` already replaces an existing destination on
    // Windows (`MOVEFILE_REPLACE_EXISTING`), so replace semantics come
    // free regardless.
    options.open(temp_path)
}

/// A managed-model store sealed to the profile it was constructed for. Owns
/// no reference back into `RuntimeContext`, so it is `Send + 'static` and can
/// cross a `tokio::spawn`/`iced::Task` boundary — but it can only ever be
/// constructed by [`RuntimeStorage::model_store`], never from an arbitrary
/// path handed to it directly.
pub struct ProfileModelStore {
    store: ManagedModelStore,
}

impl ProfileModelStore {
    fn new(root: PathBuf) -> Self {
        Self {
            store: ManagedModelStore::default_embedding(root),
        }
    }

    /// A display-only rendering of the store's root, for the model-download
    /// consent UI label. Not a `Path`/`PathBuf`: it cannot be threaded into a
    /// filesystem-opening API by construction, the same rationale as
    /// `RuntimeContext::descriptor` (Review 113 F2).
    pub fn models_dir_display(&self) -> String {
        self.store.models_dir().display().to_string()
    }

    /// Whether `candidate` resolves inside this store's root, using the same
    /// canonicalization-with-fallback as construction-time physical-alias
    /// validation. Used to distinguish a manually-configured model directory
    /// from one the managed store itself owns (RFC-050 provenance) — moved
    /// here from a free function in the bin crate so the comparison no
    /// longer needs the store's raw path handed across the boundary
    /// (Review 113 F2).
    pub fn contains(&self, candidate: &Path) -> bool {
        fn comparable(path: &Path) -> PathBuf {
            crate::physical_identity::physical_location(path)
                .map(|location| location.resolved_path)
                .or_else(|_| std::path::absolute(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
        crate::runtime_context::path_is_within(
            &comparable(candidate),
            &comparable(self.store.models_dir()),
        )
    }

    /// The currently active managed generation's directory, if the catalog
    /// records one, holding the shared lock guard for as long as the result
    /// is retained. `Ok(None)` means no managed generation is currently
    /// active. Moved here from a bin-crate free function that re-derived the
    /// store root from `Catalog::path()` instead of the active context
    /// (Review 113 F1).
    pub fn current_generation_dir(
        &self,
        catalog: &Catalog,
        timeout: std::time::Duration,
    ) -> Result<
        Option<(ModelStoreMutationGuard<SharedAccess>, PathBuf)>,
        ManagedGenerationLookupError,
    > {
        let guard = self
            .store
            .acquire_shared(timeout)
            .map_err(ManagedGenerationLookupError::StoreLock)?;
        let snapshot = orbok_db::repo::ManagedGenerationRepository::new(catalog)
            .load_shared(&guard)
            .map_err(|_| ManagedGenerationLookupError::Catalog)?;
        let Some(generation_id) = snapshot.profile.current_generation_id else {
            return Ok(None);
        };
        let generation_dir = self
            .store
            .models_dir()
            .join("generations")
            .join(generation_id.as_str());
        Ok(Some((guard, generation_dir)))
    }

    /// Acquire an exclusive lock on this store. A thin delegate over the
    /// wrapped `ManagedModelStore`, exposed so tests can simulate a
    /// competing writer (lock contention) without needing the store's raw
    /// path to construct a second, unsealed handle on it.
    pub fn acquire_exclusive(
        &self,
        timeout: std::time::Duration,
    ) -> Result<ModelStoreMutationGuard<ExclusiveAccess>, ModelStoreLockError> {
        self.store.acquire_exclusive(timeout)
    }

    /// Run the reviewed RFC-050 installer end to end against this sealed
    /// store. This is the only production entry point that performs the
    /// managed-model download/install filesystem work.
    pub async fn install_default_model(
        &self,
        catalog: &Catalog,
        events: futures::channel::mpsc::Sender<ModelDeliveryEvent>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<ModelDeliveryOutcome, ModelDeliveryError> {
        orbok_workers::install_default_model(catalog, &self.store, events, cancel).await
    }
}

/// A cache service sealed to the profile it was constructed for. Re-exposes
/// only the operations the app needs; it never exposes its backing path.
pub struct ProfileCache {
    service: orbok_cache::CacheService,
    db_path: PathBuf,
}

impl ProfileCache {
    fn new(data_dir: &Path) -> Self {
        Self {
            service: orbok_cache::CacheService::new(data_dir),
            db_path: data_dir.join(orbok_db::CACHE_FILE_NAME),
        }
    }

    /// The wrapped cache service, for composing with lower-layer worker APIs
    /// (`ExtractionWorker`, `ChunkAndIndexWorker`) that take `&CacheService`
    /// directly and cannot depend on this crate. This does not leak a path:
    /// the returned value is already bound to the active profile's data
    /// directory and cannot be rebound to a different one.
    pub fn service(&self) -> &orbok_cache::CacheService {
        &self.service
    }

    /// Open a typed engine for `namespace`. This is where the localcache
    /// database actually opens.
    pub fn engine<T: Serialize + DeserializeOwned>(
        &self,
        catalog: &Catalog,
        namespace: &OrbokCacheNamespace,
        options: EngineOptions,
    ) -> OrbokResult<localcache::CacheEngine<T>> {
        self.service.engine(catalog, namespace, options)
    }

    pub fn run_safe_cleanup(&self, catalog: &Catalog, plan: &CleanupPlan) -> OrbokResult<()> {
        orbok_workers::CleanupService::new(catalog, &self.service, &self.db_path)
            .run_safe(plan)
            .map(|_: FullCleanupOutcome| ())
    }

    pub fn run_reset(
        &self,
        catalog: &Catalog,
        plan: &CleanupPlan,
        keep_settings: bool,
    ) -> OrbokResult<()> {
        orbok_workers::CleanupService::new(catalog, &self.service, &self.db_path)
            .run_reset(plan, keep_settings)
            .map(|_: FullCleanupOutcome| ())
    }

    /// Trim the extraction cache to `cap` entries, least recently accessed
    /// first (RFC-059 Amendment 2 §2b). Scheduler-idle maintenance only --
    /// see `orbok_workers::CleanupService::trim_extraction_cache_to` for why
    /// it must never run with an index job queued.
    pub fn trim_extraction_cache_to(&self, catalog: &Catalog, cap: usize) -> OrbokResult<u64> {
        orbok_workers::CleanupService::new(catalog, &self.service, &self.db_path)
            .trim_extraction_cache_to(cap)
    }

    pub fn shrink(&self, catalog: &Catalog) -> OrbokResult<()> {
        self.service.shrink(catalog)
    }

    pub fn usage(
        &self,
        catalog: &Catalog,
        namespaces: &[OrbokCacheNamespace],
    ) -> OrbokResult<Vec<NamespaceUsage>> {
        self.service.usage(catalog, namespaces)
    }
}

/// Failure resolving the currently active managed generation directory via
/// [`ProfileModelStore::current_generation_dir`].
#[derive(Debug)]
pub enum ManagedGenerationLookupError {
    StoreLock(ModelStoreLockError),
    Catalog,
}

impl std::fmt::Display for ManagedGenerationLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoreLock(error) => {
                write!(formatter, "managed model store is unavailable: {error}")
            }
            Self::Catalog => formatter.write_str("managed model catalog state is unavailable"),
        }
    }
}

impl std::error::Error for ManagedGenerationLookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StoreLock(error) => Some(error),
            Self::Catalog => None,
        }
    }
}

#[cfg(test)]
mod tests;
