//! Chunk and chunk-location repository (RFC-006 §12).
//!
//! The central operation is [`ChunkRepository::insert_bundle`]: a
//! single transaction that replaces old chunks with new ones and
//! simultaneously updates the FTS index. Old chunks survive if the
//! transaction fails — the previous active index remains usable
//! (RFC-006 §12 "rechunk failure preserves previous active chunks").

use crate::catalog::{Catalog, db_err};
use orbok_core::{ChunkId, ExtractionId, FileId, OrbokResult, SourceId, now_iso8601};
use rusqlite::params;
use sha2::{Digest, Sha256};

/// Data for one chunk being inserted (RFC-006 §5 output).
#[derive(Debug, Clone)]
pub struct ChunkSpec {
    pub chunk_kind: &'static str,
    pub chunk_ordinal: u32,
    pub heading_path: Option<String>,
    pub title: Option<String>,
    /// Normalized text — used for FTS indexing and the content hash.
    /// NOT stored in the catalog (contentless design, RFC-007 §8.1).
    pub normalized_text: String,
    pub line_start: u32,
    pub line_end: u32,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub location_quality: &'static str,
    /// Index of the parent chunk in the same specs slice, if any.
    pub parent_idx: Option<usize>,
}

/// A chunk record returned after insertion.
#[derive(Debug, Clone)]
pub struct ChunkRecord {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub chunk_ordinal: u32,
    pub heading_path: Option<String>,
    pub line_start: u32,
    pub line_end: u32,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub location_quality: String,
}

pub struct ChunkRepository<'a> {
    catalog: &'a Catalog,
}

const CHUNKER_VERSION: &str = "chunker-v1";

impl<'a> ChunkRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Replace-on-success bundle insert (RFC-006 §12):
    ///
    /// 1. Insert new chunks + locations as active.
    /// 2. Insert FTS rows and keyword_index_records.
    /// 3. Mark old chunks (same file, different extraction) stale.
    /// 4. Mark the file as indexed.
    ///
    /// All steps are inside one transaction. A failure leaves the
    /// previous active chunks untouched.
    pub fn insert_bundle(
        &self,
        file_id: &FileId,
        extraction_id: &ExtractionId,
        specs: &[ChunkSpec],
    ) -> OrbokResult<Vec<ChunkRecord>> {
        let now = now_iso8601();
        // Assign IDs up front so parent references resolve.
        let ids: Vec<ChunkId> = (0..specs.len()).map(|_| ChunkId::generate()).collect();

        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;

        let mut records = Vec::with_capacity(specs.len());
        for (i, spec) in specs.iter().enumerate() {
            let chunk_id = &ids[i];
            let parent_id = spec.parent_idx.map(|pi| ids[pi].as_str().to_string());
            let content_hash = sha256_text(&spec.normalized_text);
            let char_count = spec.normalized_text.chars().count() as i64;

            tx.execute(
                "INSERT INTO chunks \
                 (chunk_id, file_id, extraction_id, parent_chunk_id, chunk_kind, \
                  chunk_ordinal, heading_path, title, char_count, content_hash, \
                  chunk_status, created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?11)",
                params![
                    chunk_id.as_str(),
                    file_id.as_str(),
                    extraction_id.as_str(),
                    parent_id,
                    spec.chunk_kind,
                    spec.chunk_ordinal as i64,
                    spec.heading_path,
                    spec.title,
                    char_count,
                    content_hash,
                    now,
                ],
            )
            .map_err(db_err)?;

            // Insert into unicode61 FTS and record rowid.
            tx.execute(
                "INSERT INTO chunk_fts (title, heading_path, normalized_text) \
                 VALUES (?1, ?2, ?3)",
                params![spec.title, spec.heading_path, spec.normalized_text],
            )
            .map_err(db_err)?;
            let fts_rowid = tx.last_insert_rowid();

            // Insert into trigram FTS (RFC-014 §12) for Japanese/CJK recall.
            tx.execute(
                "INSERT INTO chunk_fts_trigram (title, heading_path, normalized_text) \
                 VALUES (?1, ?2, ?3)",
                params![spec.title, spec.heading_path, spec.normalized_text],
            )
            .map_err(db_err)?;
            let trigram_fts_rowid = tx.last_insert_rowid();

            tx.execute(
                "INSERT INTO keyword_index_records \
                 (chunk_id, fts_rowid, trigram_fts_rowid, index_engine, tokenizer_name, \
                  tokenizer_version, indexed_at, status) \
                 VALUES (?1, ?2, ?3, 'sqlite-fts5', 'unicode61', ?4, ?5, 'active') \
                 ON CONFLICT(chunk_id) DO UPDATE SET fts_rowid = ?2, trigram_fts_rowid = ?3, \
                  index_engine = 'sqlite-fts5', tokenizer_name = 'unicode61', \
                  tokenizer_version = ?4, indexed_at = ?5, status = 'active'",
                params![
                    chunk_id.as_str(),
                    fts_rowid,
                    trigram_fts_rowid,
                    CHUNKER_VERSION,
                    now,
                ],
            )
            .map_err(db_err)?;

            // Chunk location.
            tx.execute(
                "INSERT INTO chunk_locations \
                 (chunk_id, byte_start, byte_end, line_start, line_end, \
                  location_quality, created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
                params![
                    chunk_id.as_str(),
                    spec.byte_start.map(|v| v as i64),
                    spec.byte_end.map(|v| v as i64),
                    spec.line_start as i64,
                    spec.line_end as i64,
                    spec.location_quality,
                    now,
                ],
            )
            .map_err(db_err)?;

            records.push(ChunkRecord {
                chunk_id: chunk_id.clone(),
                file_id: file_id.clone(),
                chunk_ordinal: spec.chunk_ordinal,
                heading_path: spec.heading_path.clone(),
                line_start: spec.line_start,
                line_end: spec.line_end,
                byte_start: spec.byte_start,
                byte_end: spec.byte_end,
                location_quality: spec.location_quality.to_string(),
            });
        }

        // Mark old chunks for this file that belong to a different extraction stale.
        tx.execute(
            "UPDATE chunks SET chunk_status = 'stale', updated_at = ?3 \
             WHERE file_id = ?1 AND extraction_id != ?2 AND chunk_status = 'active'",
            params![file_id.as_str(), extraction_id.as_str(), now],
        )
        .map_err(db_err)?;

        // Mark file indexed.
        tx.execute(
            "UPDATE files SET file_status = 'indexed', last_indexed_at = ?2, updated_at = ?2 \
             WHERE file_id = ?1",
            params![file_id.as_str(), now],
        )
        .map_err(db_err)?;

        tx.commit().map_err(db_err)?;
        Ok(records)
    }

    /// RFC-037 §8/§12, Task 035 §5.3: cascade a source's just-marked-missing
    /// files to their chunks. `Scanner::scan` calls this right after
    /// `FileRepository::mark_missing_unseen` in the same scan.
    ///
    /// Both `fts5.rs`'s keyword search and `vector.rs`'s exact scan
    /// (via `EmbeddingRepository::list_active_for_scan`) gate solely on
    /// `chunk_status = 'active'` — neither joins back to `files` at all —
    /// so a file's own `file_status` flipping to `missing` had no effect on
    /// whether its content kept surfacing in search. Marked `stale` rather
    /// than `deleted`: unlike a genuinely superseded extraction, a missing
    /// file can return with byte-identical content (RFC-004 §11), and
    /// [`ChunkRepository::reactivate_last_stale_generation`] needs the
    /// chunks intact to restore. `remove_replaced_stale_indexes`
    /// (`cleanup.rs`) only purges `stale`/`deleted` chunks for a file that
    /// still has an *active* replacement, so a fully-missing file's chunks
    /// (now entirely non-active) are left alone by that cleanup until
    /// reactivated or genuinely superseded by a fresh extraction.
    ///
    /// Idempotent and safe to call every scan regardless of what's newly
    /// missing this pass: already-`stale`/`deleted` chunks don't match
    /// `chunk_status = 'active'` again, so re-running it for a file that
    /// was already missing touches zero rows.
    pub fn deactivate_for_missing_files(&self, source_id: &SourceId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE chunks SET chunk_status = 'stale', updated_at = ?2 \
                 WHERE chunk_status = 'active' AND file_id IN \
                 (SELECT file_id FROM files WHERE source_id = ?1 AND file_status = 'missing')",
                params![source_id.as_str(), now_iso8601()],
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// RFC-004 §11, Task 035 §5.3's counterpart: a file that went missing
    /// and reappeared with byte-identical content (`Scanner::process_file`'s
    /// `restored_status` path) gets no new extraction, so there is no new
    /// chunk generation to make active — the chunks
    /// [`ChunkRepository::deactivate_for_missing_files`] set `stale` are
    /// still exactly right and just need reactivating.
    ///
    /// Reactivates only the most recently touched `extraction_id` among
    /// this file's `stale` chunks (not every stale chunk the file has ever
    /// had) — `chunks.rs`'s insert path guarantees at most one
    /// `extraction_id` is `active` for a file at a time, and
    /// `deactivate_for_missing_files` stamps `updated_at = now()` on
    /// exactly that generation when the file goes missing, so it is always
    /// the newest `updated_at` among that file's stale rows at the moment
    /// of reactivation — never an older, genuinely superseded generation.
    pub fn reactivate_last_stale_generation(&self, file_id: &FileId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE chunks SET chunk_status = 'active', updated_at = ?2 \
                 WHERE chunk_status = 'stale' AND file_id = ?1 AND extraction_id = ( \
                     SELECT extraction_id FROM chunks \
                     WHERE file_id = ?1 AND chunk_status = 'stale' \
                     ORDER BY updated_at DESC LIMIT 1 \
                 )",
                params![file_id.as_str(), now_iso8601()],
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// Retrieve chunk records for a file (used by snippet loader and
    /// tests).
    pub fn list_for_file(&self, file_id: &FileId) -> OrbokResult<Vec<ChunkRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT c.chunk_id, c.file_id, c.chunk_ordinal, c.heading_path, \
                  l.line_start, l.line_end, l.byte_start, l.byte_end, l.location_quality \
                 FROM chunks c \
                 LEFT JOIN chunk_locations l ON l.chunk_id = c.chunk_id \
                 WHERE c.file_id = ?1 AND c.chunk_status = 'active' \
                 ORDER BY c.chunk_ordinal",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![file_id.as_str()], |row| {
                Ok(ChunkRecord {
                    chunk_id: ChunkId::from_string(row.get::<_, String>(0)?),
                    file_id: FileId::from_string(row.get::<_, String>(1)?),
                    chunk_ordinal: row.get::<_, i64>(2)? as u32,
                    heading_path: row.get(3)?,
                    line_start: row.get::<_, i64>(4)? as u32,
                    line_end: row.get::<_, i64>(5)? as u32,
                    byte_start: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                    byte_end: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                    location_quality: row.get(8).unwrap_or_else(|_| "unknown".to_string()),
                })
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err)?);
        }
        Ok(out)
    }
}

fn sha256_text(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let d = h.finalize();
    let mut s = String::with_capacity(d.len() * 2);
    for b in d.iter() {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}
