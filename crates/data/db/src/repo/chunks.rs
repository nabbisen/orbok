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
    /// What `line_start`/`line_end` mean for this chunk's format (RFC-060
    /// §6): `"lines"`, `"pages"`, `"paragraphs"`, `"blocks"` or
    /// `"unknown"`. Only `"lines"` may be read from the file as lines.
    pub location_kind: &'static str,
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
    /// `chunk_locations.location_kind`; `"unknown"` for a row written
    /// before migration 0008, which yields no snippet rather than a wrong
    /// one (RFC-060 §6).
    pub location_kind: String,
}

pub struct ChunkRepository<'a> {
    catalog: &'a Catalog,
}

const CHUNKER_VERSION: &str = "chunker-v1";

/// RFC-059 §6's two counts that must agree, read together (Task 102: the one
/// implementation of the check, used by the erasure tests and the keyword
/// rebuild tests alike).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeywordIndexCounts {
    pub fts: i64,
    pub records: i64,
    pub trigram: i64,
    pub records_with_trigram: i64,
}

impl KeywordIndexCounts {
    /// `None` when both invariants hold: `count(chunk_fts) ==
    /// count(keyword_index_records)` and `count(chunk_fts_trigram) ==
    /// count(keyword_index_records WHERE trigram_fts_rowid IS NOT NULL)`.
    pub fn violation(&self) -> Option<String> {
        if self.fts != self.records {
            return Some(format!(
                "count(chunk_fts)={} must equal count(keyword_index_records)={}",
                self.fts, self.records
            ));
        }
        if self.trigram != self.records_with_trigram {
            return Some(format!(
                "count(chunk_fts_trigram)={} must equal \
                 count(keyword_index_records WHERE trigram_fts_rowid IS NOT NULL)={}",
                self.trigram, self.records_with_trigram
            ));
        }
        None
    }
}

/// The three keyword-index rows of one chunk: the unicode61 FTS row, the
/// trigram FTS row (RFC-014 §12, Japanese/CJK recall) and the
/// `keyword_index_records` mapping between them and the chunk. Kept together
/// so RFC-059 §6's `count(chunk_fts) == count(keyword_index_records)` holds
/// wherever they are written (`insert_bundle`, `reuse_existing_chunks`).
fn write_keyword_rows(
    tx: &rusqlite::Transaction<'_>,
    chunk_id: &ChunkId,
    spec: &ChunkSpec,
    now: &str,
) -> OrbokResult<()> {
    tx.execute(
        "INSERT INTO chunk_fts (title, heading_path, normalized_text) \
         VALUES (?1, ?2, ?3)",
        params![spec.title, spec.heading_path, spec.normalized_text],
    )
    .map_err(db_err)?;
    let fts_rowid = tx.last_insert_rowid();

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
    Ok(())
}

/// What [`ChunkRepository::reuse_existing_chunks`] found (Task 102).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingChunks {
    /// This extraction has no active chunks: insert a bundle.
    None,
    /// It has, and they are the chunks the caller derived. Their missing
    /// keyword rows were written (`keyword_rows_written`, 0 when every chunk
    /// already had them). The chunk ids did not change, so nothing that
    /// hangs off them (embeddings) needs redoing.
    Reused { keyword_rows_written: usize },
    /// It has, and they are not the chunks the caller derived. Nothing was
    /// written.
    Different,
}

/// RFC-059 §6/§0(i): the previous generation's FTS rows and their mapping
/// rows, addressed by file, for every active chunk of a *different*
/// extraction (see `insert_bundle`'s own comment for why by file and not by
/// chunk id).
fn delete_superseded_keyword_rows(
    tx: &rusqlite::Transaction<'_>,
    file_id: &FileId,
    extraction_id: &ExtractionId,
) -> OrbokResult<()> {
    let superseded_chunks = "SELECT chunk_id FROM chunks WHERE file_id = ?1 AND extraction_id != ?2 \
             AND chunk_status = 'active'";
    tx.execute(
        &format!(
            "DELETE FROM chunk_fts WHERE rowid IN ( \
                     SELECT fts_rowid FROM keyword_index_records \
                     WHERE chunk_id IN ({superseded_chunks}) AND fts_rowid IS NOT NULL \
                 )"
        ),
        params![file_id.as_str(), extraction_id.as_str()],
    )
    .map_err(db_err)?;
    tx.execute(
        &format!(
            "DELETE FROM chunk_fts_trigram WHERE rowid IN ( \
                     SELECT trigram_fts_rowid FROM keyword_index_records \
                     WHERE chunk_id IN ({superseded_chunks}) AND trigram_fts_rowid IS NOT NULL \
                 )"
        ),
        params![file_id.as_str(), extraction_id.as_str()],
    )
    .map_err(db_err)?;
    tx.execute(
        &format!("DELETE FROM keyword_index_records WHERE chunk_id IN ({superseded_chunks})"),
        params![file_id.as_str(), extraction_id.as_str()],
    )
    .map_err(db_err)?;
    Ok(())
}

/// Old chunks of this file that belong to a different extraction become
/// `stale`.
fn mark_other_generations_stale(
    tx: &rusqlite::Transaction<'_>,
    file_id: &FileId,
    extraction_id: &ExtractionId,
    now: &str,
) -> OrbokResult<()> {
    tx.execute(
        "UPDATE chunks SET chunk_status = 'stale', updated_at = ?3 \
         WHERE file_id = ?1 AND extraction_id != ?2 AND chunk_status = 'active'",
        params![file_id.as_str(), extraction_id.as_str(), now],
    )
    .map_err(db_err)?;
    Ok(())
}

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

        // RFC-059 §6/§0(i): delete the previous generation's FTS rows --
        // and their now-meaningless `keyword_index_records` mapping rows --
        // before inserting the new ones, addressed by file_id, not
        // chunk_id. A fresh chunk_id is minted for every chunk below on
        // every call, so a chunk_id never repeats and a delete keyed on it
        // (which is what the RFC found `Fts5KeywordEngine::index`'s own
        // "replace-on-reindex" delete does, on a path with no production
        // caller) can never match the previous generation. Without this,
        // the old chunks' FTS rows keep matching searches -- only the
        // catalog-row status flips to 'stale' just below -- until
        // `remove_replaced_stale_indexes` cascades them away later, and
        // even that path used to orphan them permanently before this same
        // slice fixed it.
        //
        // The mapping row is deleted here too, not left for that later
        // cascade: `remove_replaced_stale_indexes` only deletes the
        // `chunks` row itself (cascading `keyword_index_records`), but
        // `insert_bundle` deliberately does not delete `chunks` here (the
        // stale row is kept, e.g. for `reactivate_last_stale_generation`'s
        // missing-file case elsewhere). Left behind, that mapping row
        // would still count toward `keyword_index_records` while its FTS
        // row no longer exists -- the exact `count(chunk_fts) ==
        // count(keyword_index_records)` invariant (§6) failing immediately
        // after every re-index, confirmed by first shipping this fix
        // without the mapping delete and watching the invariant test fail
        // with `keyword_index_records` one row ahead.
        delete_superseded_keyword_rows(&tx, file_id, extraction_id)?;

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

            write_keyword_rows(&tx, chunk_id, spec, &now)?;

            // Chunk location.
            tx.execute(
                "INSERT INTO chunk_locations \
                 (chunk_id, byte_start, byte_end, line_start, line_end, \
                  location_quality, location_kind, created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                params![
                    chunk_id.as_str(),
                    spec.byte_start.map(|v| v as i64),
                    spec.byte_end.map(|v| v as i64),
                    spec.line_start as i64,
                    spec.line_end as i64,
                    spec.location_quality,
                    spec.location_kind,
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
                location_kind: spec.location_kind.to_string(),
            });
        }

        mark_other_generations_stale(&tx, file_id, extraction_id, &now)?;

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

    /// The counts [`KeywordIndexCounts::violation`] checks.
    pub fn keyword_index_counts(&self) -> OrbokResult<KeywordIndexCounts> {
        let conn = self.catalog.lock();
        let count = |sql: &str| -> OrbokResult<i64> {
            conn.query_row(sql, [], |r| r.get(0)).map_err(db_err)
        };
        Ok(KeywordIndexCounts {
            fts: count("SELECT COUNT(*) FROM chunk_fts")?,
            records: count("SELECT COUNT(*) FROM keyword_index_records")?,
            trigram: count("SELECT COUNT(*) FROM chunk_fts_trigram")?,
            records_with_trigram: count(
                "SELECT COUNT(*) FROM keyword_index_records WHERE trigram_fts_rowid IS NOT NULL",
            )?,
        })
    }

    /// Task 102: a `Chunk` job for an extraction that already has active
    /// chunks is an ordinary event (a per-file Prepare again on a healthy
    /// file, a duplicated job, a keyword rebuild over a fresh extraction
    /// cache), not an error. When the chunks the caller derived are the ones
    /// stored -- same count, and for each ordinal the same content hash,
    /// heading path, title, kind and line range -- the missing keyword rows are written
    /// under the **existing chunk ids**, in one transaction, and nothing else
    /// changes: embeddings hang off those ids and stay valid. When they
    /// differ, nothing is written and the caller makes a new generation.
    ///
    /// Any keyword rows still belonging to a superseded generation of this
    /// file are removed here too, exactly as `insert_bundle` does, so this
    /// path cannot leave a stale row behind.
    pub fn reuse_existing_chunks(
        &self,
        file_id: &FileId,
        extraction_id: &ExtractionId,
        specs: &[ChunkSpec],
    ) -> OrbokResult<ExistingChunks> {
        let now = now_iso8601();
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;

        type Stored = (
            String,
            i64,
            Option<String>,
            Option<String>,
            String,
            String,
            Option<i64>,
            Option<i64>,
        );
        let stored: Vec<Stored> = {
            let mut stmt = tx
                .prepare(
                    "SELECT c.chunk_id, c.chunk_ordinal, c.heading_path, c.title, c.chunk_kind, \
                     c.content_hash, l.line_start, l.line_end FROM chunks c \
                     LEFT JOIN chunk_locations l ON l.chunk_id = c.chunk_id \
                     WHERE c.file_id = ?1 AND c.extraction_id = ?2 AND c.chunk_status = 'active' \
                     ORDER BY c.chunk_ordinal",
                )
                .map_err(db_err)?;
            stmt.query_map(params![file_id.as_str(), extraction_id.as_str()], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<_, _>>()
            .map_err(db_err)?
        };
        if stored.is_empty() {
            return Ok(ExistingChunks::None);
        }
        let same = stored.len() == specs.len()
            && stored.iter().zip(specs).all(|(row, spec)| {
                row.1 == spec.chunk_ordinal as i64
                    && row.2 == spec.heading_path
                    && row.3 == spec.title
                    && row.4 == spec.chunk_kind
                    && row.5 == sha256_text(&spec.normalized_text)
                    && row.6 == Some(spec.line_start as i64)
                    && row.7 == Some(spec.line_end as i64)
            });
        if !same {
            return Ok(ExistingChunks::Different);
        }

        let mut written = 0;
        for (row, spec) in stored.iter().zip(specs) {
            let has_rows: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM keyword_index_records \
                     WHERE chunk_id = ?1 AND fts_rowid IS NOT NULL \
                     AND trigram_fts_rowid IS NOT NULL)",
                    params![row.0],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            if !has_rows {
                write_keyword_rows(&tx, &ChunkId::from_string(row.0.clone()), spec, &now)?;
                written += 1;
            }
        }
        delete_superseded_keyword_rows(&tx, file_id, extraction_id)?;
        mark_other_generations_stale(&tx, file_id, extraction_id, &now)?;

        // A healthy file (every row present, already indexed) changes nothing.
        tx.execute(
            "UPDATE files SET file_status = 'indexed', last_indexed_at = ?2, updated_at = ?2 \
             WHERE file_id = ?1 AND (?3 > 0 OR file_status != 'indexed')",
            params![file_id.as_str(), now, written as i64],
        )
        .map_err(db_err)?;

        tx.commit().map_err(db_err)?;
        Ok(ExistingChunks::Reused {
            keyword_rows_written: written,
        })
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
    /// Reactivates only the newest `extraction_id` among this file's `stale`
    /// chunks (not every stale chunk the file has ever had) — `chunks.rs`'s
    /// insert path guarantees at most one `extraction_id` is `active` for a
    /// file at a time, and that one is the newest generation inserted, so it
    /// is the one `deactivate_for_missing_files` made stale when the file went
    /// missing — never an older, genuinely superseded generation.
    ///
    /// "Newest" is **insertion order** (`rowid`), not `updated_at` (Task 116):
    /// which generation came last is an event, and two `updated_at` readings
    /// can compare the wrong way.
    pub fn reactivate_last_stale_generation(&self, file_id: &FileId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE chunks SET chunk_status = 'active', updated_at = ?2 \
                 WHERE chunk_status = 'stale' AND file_id = ?1 AND extraction_id = ( \
                     SELECT extraction_id FROM chunks \
                     WHERE file_id = ?1 AND chunk_status = 'stale' \
                     ORDER BY rowid DESC LIMIT 1 \
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
                  l.line_start, l.line_end, l.byte_start, l.byte_end, l.location_quality, \
                  l.location_kind \
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
                    location_kind: row.get(9).unwrap_or_else(|_| "unknown".to_string()),
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
