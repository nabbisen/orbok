//! Source repository (RFC-002 §7.2, RFC-003).

use crate::catalog::{Catalog, db_err};
use orbok_core::{
    HiddenFilePolicy, IndexMode, OrbokError, OrbokResult, PersistenceMode, SourceId, SourceStatus,
    SourceType, SymlinkPolicy, now_iso8601,
};
use rusqlite::{Row, params};

/// A registered source (persistent catalog data — never deleted by
/// ordinary cleanup, RFC-001 §7.1).
#[derive(Debug, Clone)]
pub struct SourceRecord {
    pub source_id: SourceId,
    pub source_type: SourceType,
    pub persistence_mode: PersistenceMode,
    pub display_name: Option<String>,
    pub original_path: String,
    pub canonical_path: String,
    pub status: SourceStatus,
    pub index_mode: IndexMode,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub hidden_file_policy: HiddenFilePolicy,
    pub symlink_policy: SymlinkPolicy,
    pub max_file_size_bytes: Option<u64>,
    /// Task 114: `true` covers the folder's subfolders (the default);
    /// `false` is "this folder only".
    pub covers_subfolders: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_scanned_at: Option<String>,
}

/// Parameters for registering a new source (RFC-003 §9.1).
#[derive(Debug, Clone)]
pub struct NewSource {
    pub source_type: SourceType,
    pub persistence_mode: PersistenceMode,
    pub display_name: Option<String>,
    pub original_path: String,
    pub canonical_path: String,
    pub index_mode: IndexMode,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub hidden_file_policy: HiddenFilePolicy,
    pub symlink_policy: SymlinkPolicy,
    pub max_file_size_bytes: Option<u64>,
}

/// What [`SourceRepository::absorb`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AbsorbReport {
    pub files_moved: u64,
    /// File rows that both folders held, of which one copy was erased.
    pub duplicates_erased: u64,
}

/// The body of [`SourceRepository::absorb`], inside the caller's transaction.
fn absorb_in(
    tx: &rusqlite::Transaction<'_>,
    outer: &SourceId,
    inner: &[SourceId],
) -> OrbokResult<AbsorbReport> {
    let outer_path: String = tx
        .query_row(
            "SELECT canonical_path FROM sources WHERE source_id = ?1",
            params![outer.as_str()],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    let mut report = AbsorbReport::default();
    for inner_id in inner {
        // Rows both folders hold. Erase the copy that is less prepared.
        let outer_copy_is_worse = "EXISTS (SELECT 1 FROM files i WHERE i.source_id = ?2 \
             AND i.canonical_path = f.canonical_path AND i.file_status = 'indexed') \
             AND f.file_status != 'indexed'";
        report.duplicates_erased += erase_files(
            tx,
            "f.source_id = ?1",
            &format!("({outer_copy_is_worse})"),
            params![outer.as_str(), inner_id.as_str()],
        )?;
        report.duplicates_erased += erase_files(
            tx,
            "f.source_id = ?2",
            "EXISTS (SELECT 1 FROM files o WHERE o.source_id = ?1 \
             AND o.canonical_path = f.canonical_path)",
            params![outer.as_str(), inner_id.as_str()],
        )?;
        report.files_moved += tx
            .execute(
                "UPDATE files SET source_id = ?1, \
                    display_path = COALESCE(NULLIF(ltrim(substr(canonical_path, \
                        length(?3) + 1), '/\\'), ''), canonical_path), \
                    seen_generation = 0 \
                 WHERE source_id = ?2",
                params![outer.as_str(), inner_id.as_str(), outer_path],
            )
            .map_err(db_err)? as u64;
        tx.execute(
            "UPDATE index_jobs SET source_id = ?1 \
             WHERE source_id = ?2 AND job_type != 'scan'",
            params![outer.as_str(), inner_id.as_str()],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM sources WHERE source_id = ?1",
            params![inner_id.as_str()],
        )
        .map_err(db_err)?;
    }
    Ok(report)
}

/// SQL: a file of the folder whose path has a separator after the folder's
/// own path -- one that is not a direct entry. `?2` is the folder's path and
/// `?3` the platform separator.
const BELOW_TOP_LEVEL: &str = "instr(substr(f.canonical_path, length(?2) + 2), ?3) > 0";

fn source_root(conn: &rusqlite::Connection, id: &SourceId) -> OrbokResult<String> {
    conn.query_row(
        "SELECT canonical_path FROM sources WHERE source_id = ?1",
        params![id.as_str()],
        |r| r.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OrbokError::SourceNotFound,
        other => db_err(other),
    })
}

/// Erase the `files` rows of alias `f` matching `where_sql AND extra_sql`,
/// keyword-index rows first (they are keyed by chunk, which the file delete
/// cascades to), as `delete_with_all_data` does for a whole source.
fn erase_files(
    tx: &rusqlite::Transaction<'_>,
    where_sql: &str,
    extra_sql: &str,
    binds: impl rusqlite::Params + Copy,
) -> OrbokResult<u64> {
    let chunks = format!(
        "SELECT c.chunk_id FROM chunks c JOIN files f ON f.file_id = c.file_id \
         WHERE {where_sql} AND {extra_sql}"
    );
    for (fts, rowid) in [
        ("chunk_fts", "fts_rowid"),
        ("chunk_fts_trigram", "trigram_fts_rowid"),
    ] {
        tx.execute(
            &format!(
                "DELETE FROM {fts} WHERE rowid IN ( \
                     SELECT k.{rowid} FROM keyword_index_records k \
                     WHERE k.chunk_id IN ({chunks}) AND k.{rowid} IS NOT NULL)"
            ),
            binds,
        )
        .map_err(db_err)?;
    }
    let erased = tx
        .execute(
            &format!("DELETE FROM files WHERE file_id IN (SELECT f.file_id FROM files f WHERE {where_sql} AND {extra_sql})"),
            binds,
        )
        .map_err(db_err)?;
    Ok(erased as u64)
}

/// Repository over the `sources` table.
pub struct SourceRepository<'a> {
    catalog: &'a Catalog,
}

const COLUMNS: &str = "source_id, source_type, persistence_mode, display_name, original_path, \
     canonical_path, status, index_mode, include_patterns_json, exclude_patterns_json, \
     hidden_file_policy, symlink_policy, max_file_size_bytes, created_at, updated_at, \
     last_scanned_at, covers_subfolders";

impl<'a> SourceRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Register a source as Active. The transaction requirement of
    /// RFC-002 §9 item 1 is satisfied by the single-statement insert.
    pub fn insert(&self, new: NewSource) -> OrbokResult<SourceRecord> {
        let id = SourceId::generate();
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO sources (source_id, source_type, persistence_mode, display_name, \
             original_path, canonical_path, status, index_mode, include_patterns_json, \
             exclude_patterns_json, hidden_file_policy, symlink_policy, max_file_size_bytes, \
             created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)",
            params![
                id.as_str(),
                new.source_type.as_str(),
                new.persistence_mode.as_str(),
                new.display_name,
                new.original_path,
                new.canonical_path,
                SourceStatus::Active.as_str(),
                new.index_mode.as_str(),
                serde_json::to_string(&new.include_patterns).unwrap_or_default(),
                serde_json::to_string(&new.exclude_patterns).unwrap_or_default(),
                new.hidden_file_policy.as_str(),
                new.symlink_policy.as_str(),
                new.max_file_size_bytes.map(|v| v as i64),
                now,
            ],
        )
        .map_err(db_err)?;
        drop(conn);
        self.get(&id)?.ok_or(OrbokError::SourceNotFound)
    }

    /// Task 113 (RFC-064 §3.3): make `inner` sources part of `outer`, in one
    /// transaction. `inner` is in the order they are to be taken (shallowest
    /// first), so when two of them hold the same file the higher folder's row
    /// is the one seen first.
    ///
    /// Everything that refers to a folder by `source_id` is moved or dropped
    /// here, and nothing that refers to a file by `file_id` changes, so the
    /// chunks, embeddings, keyword index rows and recent searches that hang
    /// off a file stay valid:
    ///
    /// * `files`: the rows move to `outer` and `display_path` is recomputed
    ///   relative to it. When `outer` already holds a row for the same
    ///   canonical path (an overlap an older version allowed), one row is
    ///   kept: the prepared one, or `outer`'s when both or neither are. The
    ///   other is erased with its keyword-index rows, as removing a folder
    ///   erases them, so the erasure invariant holds.
    /// * `index_jobs`: the jobs for files move with them. An `inner` scan
    ///   goes with the folder: `outer` is scanned by its own job.
    /// * the `sources` rows of `inner` are deleted.
    ///
    /// Returns how many file rows moved and how many duplicates were erased.
    pub fn absorb(&self, outer: &SourceId, inner: &[SourceId]) -> OrbokResult<AbsorbReport> {
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        let report = absorb_in(&tx, outer, inner)?;
        tx.commit().map_err(db_err)?;
        Ok(report)
    }

    /// Task 114: "This folder and subfolders", and -- when the folder now
    /// covers added folders -- their [`absorb`](Self::absorb), in one
    /// transaction, so a folder is never left covering subfolders while
    /// another folder holds the same files.
    pub fn widen_and_absorb(&self, id: &SourceId, inner: &[SourceId]) -> OrbokResult<AbsorbReport> {
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        tx.execute(
            "UPDATE sources SET covers_subfolders = 1, updated_at = ?2 WHERE source_id = ?1",
            params![id.as_str(), now_iso8601()],
        )
        .map_err(db_err)?;
        let report = absorb_in(&tx, id, inner)?;
        tx.commit().map_err(db_err)?;
        Ok(report)
    }

    /// Task 114: how many of a folder's files lie below its top level -- the
    /// files "This folder only" would drop.
    pub fn count_below_top_level(&self, id: &SourceId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let root = source_root(&conn, id)?;
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM files f WHERE f.source_id = ?1 AND {BELOW_TOP_LEVEL}"
                ),
                params![id.as_str(), root, std::path::MAIN_SEPARATOR.to_string()],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(count as u64)
    }

    /// Task 114: the canonical paths of a folder's files below its top level
    /// -- what `narrow_to_top_level` erases, read first so the caller can evict
    /// their extraction-cache entries before the catalog changes.
    pub fn paths_below_top_level(&self, id: &SourceId) -> OrbokResult<Vec<String>> {
        let conn = self.catalog.lock();
        let root = source_root(&conn, id)?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT f.canonical_path FROM files f WHERE f.source_id = ?1 AND {BELOW_TOP_LEVEL}"
            ))
            .map_err(db_err)?;
        stmt.query_map(
            params![id.as_str(), root, std::path::MAIN_SEPARATOR.to_string()],
            |r| r.get(0),
        )
        .map_err(db_err)?
        .collect::<Result<_, _>>()
        .map_err(db_err)
    }

    /// Task 114 (RFC-064 §3.2): "This folder only". In one transaction, the
    /// folder stops covering its subfolders and everything orbok holds for the
    /// files below its top level is erased -- their `files` rows, and through
    /// them chunks, embeddings, `keyword_index_records`, the `chunk_fts` and
    /// `chunk_fts_trigram` rows, and the folder's queued jobs for them (the FK
    /// cascade; the scheduler skips a popped job whose row is gone). The files
    /// are out of the folder, not missing: no row is left to be marked so.
    ///
    /// Returns the erased files' canonical paths, for the caller to evict from
    /// the extraction cache, which is outside this database.
    pub fn narrow_to_top_level(&self, id: &SourceId) -> OrbokResult<Vec<String>> {
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        let root = source_root(&tx, id)?;
        let separator = std::path::MAIN_SEPARATOR.to_string();
        let paths: Vec<String> = {
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT f.canonical_path FROM files f WHERE f.source_id = ?1 AND {BELOW_TOP_LEVEL}"
                ))
                .map_err(db_err)?;
            stmt.query_map(params![id.as_str(), root, separator], |r| r.get(0))
                .map_err(db_err)?
                .collect::<Result<_, _>>()
                .map_err(db_err)?
        };
        erase_files(
            &tx,
            "f.source_id = ?1",
            BELOW_TOP_LEVEL,
            params![id.as_str(), root, separator],
        )?;
        tx.execute(
            "UPDATE sources SET covers_subfolders = 0, updated_at = ?2 WHERE source_id = ?1",
            params![id.as_str(), now_iso8601()],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(paths)
    }

    /// Task 120: everything orbok holds for what lies at `path` in folder `id` is
    /// erased, in one transaction -- the same erasure as
    /// [`Self::narrow_to_top_level`], for one path: the file at exactly `path`,
    /// or every file under it when `path` is a folder. A scan that skips a hidden
    /// file or folder (or leaves a file out by the folder's policy) calls this so
    /// what an earlier scan prepared there is out of the folder, not missing.
    ///
    /// Returns the erased files' canonical paths, for the caller to evict from
    /// the extraction cache, which is outside this database. A path with nothing
    /// in the catalog costs one read and no write lock, since a scan asks this of
    /// every entry it skips.
    pub fn erase_files_at(&self, id: &SourceId, path: &str) -> OrbokResult<Vec<String>> {
        // Every path below a folder starts with the folder's path and a
        // separator; the range ends at the next character, so the unique
        // (source, path) index serves it.
        let prefix = format!("{path}{}", std::path::MAIN_SEPARATOR);
        let upper = {
            let mut end = prefix.clone();
            let last = end.pop().unwrap_or('\0');
            end.push(char::from_u32(last as u32 + 1).unwrap_or(last));
            end
        };
        let at = "(f.canonical_path = ?2 OR (f.canonical_path >= ?3 AND f.canonical_path < ?4))";
        let select =
            format!("SELECT f.canonical_path FROM files f WHERE f.source_id = ?1 AND {at}");
        let held = |conn: &rusqlite::Connection| -> OrbokResult<Vec<String>> {
            let mut stmt = conn.prepare(&select).map_err(db_err)?;
            stmt.query_map(params![id.as_str(), path, prefix, upper], |r| r.get(0))
                .map_err(db_err)?
                .collect::<Result<_, _>>()
                .map_err(db_err)
        };
        if held(&self.catalog.lock())?.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        let paths = held(&tx)?;
        if !paths.is_empty() {
            erase_files(
                &tx,
                "f.source_id = ?1",
                at,
                params![id.as_str(), path, prefix, upper],
            )?;
        }
        tx.commit().map_err(db_err)?;
        Ok(paths)
    }

    /// Fetch one source by id.
    pub fn get(&self, id: &SourceId) -> OrbokResult<Option<SourceRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM sources WHERE source_id = ?1"
            ))
            .map_err(db_err)?;
        let mut rows = stmt
            .query_map(params![id.as_str()], row_to_record)
            .map_err(db_err)?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(db_err)??)),
            None => Ok(None),
        }
    }

    /// The non-removed source registered at `canonical_path`, newest first if
    /// a catalog already holds more than one (Task 047: nothing prevented
    /// duplicates before, and no unique constraint exists).
    pub fn find_by_canonical_path(
        &self,
        canonical_path: &str,
    ) -> OrbokResult<Option<SourceRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM sources WHERE canonical_path = ?1 \
                 AND status != 'removed' ORDER BY created_at DESC LIMIT 1"
            ))
            .map_err(db_err)?;
        let mut rows = stmt
            .query_map(params![canonical_path], row_to_record)
            .map_err(db_err)?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(db_err)??)),
            None => Ok(None),
        }
    }

    /// All sources except Removed, newest first.
    pub fn list(&self) -> OrbokResult<Vec<SourceRecord>> {
        self.query_records(&format!(
            "SELECT {COLUMNS} FROM sources WHERE status != 'removed' ORDER BY created_at DESC"
        ))
    }

    /// Sources eligible for scanning (Active only, RFC-004 §10).
    pub fn list_active(&self) -> OrbokResult<Vec<SourceRecord>> {
        self.query_records(&format!(
            "SELECT {COLUMNS} FROM sources WHERE status = 'active' ORDER BY created_at"
        ))
    }

    /// Count of sources a user would see registered -- same `status !=
    /// 'removed'` filter as [`Self::list`], so this is the number [`Self::list`]
    /// would return the length of, without materializing every row.
    pub fn count(&self) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sources WHERE status != 'removed'",
                [],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    fn query_records(&self, sql: &str) -> OrbokResult<Vec<SourceRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn.prepare(sql).map_err(db_err)?;
        let rows = stmt.query_map([], row_to_record).map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err)??);
        }
        Ok(out)
    }

    /// Update status (pause/resume/missing/permission_denied/removed).
    pub fn set_status(&self, id: &SourceId, status: SourceStatus) -> OrbokResult<()> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE sources SET status = ?2, updated_at = ?3 WHERE source_id = ?1",
                params![id.as_str(), status.as_str(), now_iso8601()],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err(OrbokError::SourceNotFound);
        }
        Ok(())
    }

    /// Start a scan of this folder: take the next scan number (Task 116).
    /// Every file the scan sees records it, and the scan's end marks the files
    /// with an older one as missing.
    pub fn begin_scan(&self, id: &SourceId) -> OrbokResult<i64> {
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        tx.execute(
            "UPDATE sources SET scan_generation = scan_generation + 1 WHERE source_id = ?1",
            params![id.as_str()],
        )
        .map_err(db_err)?;
        let generation = tx
            .query_row(
                "SELECT scan_generation FROM sources WHERE source_id = ?1",
                params![id.as_str()],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => OrbokError::SourceNotFound,
                other => db_err(other),
            })?;
        tx.commit().map_err(db_err)?;
        Ok(generation)
    }

    /// Record a completed scan.
    pub fn touch_scanned(&self, id: &SourceId) -> OrbokResult<()> {
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "UPDATE sources SET last_scanned_at = ?2, updated_at = ?2 WHERE source_id = ?1",
            params![id.as_str(), now],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Remove-source option 3 (RFC-003 §10.3): delete the source row and
    /// let foreign keys cascade through files → extraction → chunks →
    /// indexes. Source files on disk are never touched.
    ///
    /// RFC-059 Amendment 1 §2a.1 (Review 213): the cascade cannot actually
    /// reach the indexes on its own. Both `chunk_fts` and `chunk_fts_trigram`
    /// are contentless, so `keyword_index_records` is the only chunk_id <->
    /// FTS-rowid link that exists; `keyword_index_records.chunk_id` is `ON
    /// DELETE CASCADE`, so the `sources` delete below destroys that mapping
    /// before anything could use it if it ran first, stranding every FTS row
    /// the removed folder ever wrote. Delete the FTS rows first, addressed
    /// via `files -> chunks -> keyword_index_records` for this source, in
    /// the same transaction as the `sources` delete -- the same shape as
    /// `remove_replaced_stale_indexes`'s own fix (RFC-059 §6).
    pub fn delete_with_all_data(&self, id: &SourceId) -> OrbokResult<()> {
        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_err)?;
        // The same erasure a file leaving a folder gets (`narrow_to_top_level`),
        // for every file of the folder: one implementation, at file
        // granularity (Task 114).
        erase_files(&tx, "f.source_id = ?1", "1", params![id.as_str()])?;
        tx.execute(
            "DELETE FROM sources WHERE source_id = ?1",
            params![id.as_str()],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }
}

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<OrbokResult<SourceRecord>> {
    let parse_patterns = |s: Option<String>| -> Vec<String> {
        s.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    };
    let source_type: String = row.get(1)?;
    let persistence: String = row.get(2)?;
    let status: String = row.get(6)?;
    let index_mode: String = row.get(7)?;
    let hidden: String = row.get(10)?;
    let symlink: String = row.get(11)?;
    let max_size: Option<i64> = row.get(12)?;

    Ok((|| {
        Ok(SourceRecord {
            source_id: SourceId::from_string(row.get::<_, String>(0).map_err(db_err)?),
            source_type: SourceType::parse(&source_type)?,
            persistence_mode: PersistenceMode::parse(&persistence)?,
            display_name: row.get(3).map_err(db_err)?,
            original_path: row.get(4).map_err(db_err)?,
            canonical_path: row.get(5).map_err(db_err)?,
            status: SourceStatus::parse(&status)?,
            index_mode: IndexMode::parse(&index_mode)?,
            include_patterns: parse_patterns(row.get(8).map_err(db_err)?),
            exclude_patterns: parse_patterns(row.get(9).map_err(db_err)?),
            hidden_file_policy: HiddenFilePolicy::parse(&hidden)?,
            symlink_policy: SymlinkPolicy::parse(&symlink)?,
            max_file_size_bytes: max_size.map(|v| v as u64),
            created_at: row.get(13).map_err(db_err)?,
            updated_at: row.get(14).map_err(db_err)?,
            last_scanned_at: row.get(15).map_err(db_err)?,
            covers_subfolders: row.get::<_, i64>(16).map_err(db_err)? != 0,
        })
    })())
}
