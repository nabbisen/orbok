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

/// Repository over the `sources` table.
pub struct SourceRepository<'a> {
    catalog: &'a Catalog,
}

const COLUMNS: &str = "source_id, source_type, persistence_mode, display_name, original_path, \
     canonical_path, status, index_mode, include_patterns_json, exclude_patterns_json, \
     hidden_file_policy, symlink_policy, max_file_size_bytes, created_at, updated_at, \
     last_scanned_at";

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
        let tx = conn.transaction().map_err(db_err)?;
        let source_chunks_subquery = "SELECT c.chunk_id FROM chunks c \
             JOIN files f ON f.file_id = c.file_id WHERE f.source_id = ?1";
        tx.execute(
            &format!(
                "DELETE FROM chunk_fts WHERE rowid IN ( \
                     SELECT k.fts_rowid FROM keyword_index_records k \
                     WHERE k.chunk_id IN ({source_chunks_subquery}) \
                       AND k.fts_rowid IS NOT NULL \
                 )"
            ),
            params![id.as_str()],
        )
        .map_err(db_err)?;
        tx.execute(
            &format!(
                "DELETE FROM chunk_fts_trigram WHERE rowid IN ( \
                     SELECT k.trigram_fts_rowid FROM keyword_index_records k \
                     WHERE k.chunk_id IN ({source_chunks_subquery}) \
                       AND k.trigram_fts_rowid IS NOT NULL \
                 )"
            ),
            params![id.as_str()],
        )
        .map_err(db_err)?;
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
        })
    })())
}
