//! SQLite FTS5 keyword engine (RFC-007 §8.1, Option A).
//!
//! The `chunk_fts` table is contentless (`content=''`,
//! `contentless_delete=1`): tokens are indexed, values are discarded.
//! The `keyword_index_records.fts_rowid` column carries the only
//! chunk ↔ fts mapping; deletion goes through it.

use crate::query::{build_match_expression, build_match_pair_expression};
use crate::{KeywordCandidate, KeywordDocument, KeywordSearchEngine};
use orbok_core::{ChunkId, FileId, OrbokError, OrbokResult, now_iso8601};
use orbok_db::Catalog;
use rusqlite::params;

/// Engine identity recorded per indexed chunk (RFC-007 §9 versioning).
const ENGINE_NAME: &str = "sqlite-fts5";
const TOKENIZER_NAME: &str = "unicode61";
const TOKENIZER_VERSION: &str = "v1";

/// FTS5-backed keyword engine bound to the catalog.
pub struct Fts5KeywordEngine<'a> {
    catalog: &'a Catalog,
}

impl<'a> Fts5KeywordEngine<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn search_pairs(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_with_expr(build_match_pair_expression(query), limit)
    }

    fn search_with_expr(
        &self,
        match_expr: Option<String>,
        limit: u32,
    ) -> OrbokResult<Vec<KeywordCandidate>> {
        let Some(match_expr) = match_expr else {
            return Ok(Vec::new());
        };
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT r.chunk_id, c.file_id, bm25(chunk_fts) AS score \
                 FROM chunk_fts \
                 JOIN keyword_index_records r ON r.fts_rowid = chunk_fts.rowid \
                 JOIN chunks c ON c.chunk_id = r.chunk_id \
                 WHERE chunk_fts MATCH ?1 AND r.status = 'active' \
                   AND c.chunk_status = 'active' \
                 ORDER BY score LIMIT ?2",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map(params![match_expr, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(db)?;
        let mut out = Vec::new();
        for (i, row) in rows.enumerate() {
            let (chunk_id, file_id, score) = row.map_err(db)?;
            out.push(KeywordCandidate {
                chunk_id: ChunkId::from_string(chunk_id),
                file_id: FileId::from_string(file_id),
                rank: (i + 1) as u32,
                score,
            });
        }
        Ok(out)
    }
}

impl KeywordSearchEngine for Fts5KeywordEngine<'_> {
    fn index(&self, documents: &[KeywordDocument]) -> OrbokResult<()> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db)?;
        for doc in documents {
            // Replace-on-reindex: drop any previous fts row first.
            tx.execute(
                "DELETE FROM chunk_fts WHERE rowid = \
                 (SELECT fts_rowid FROM keyword_index_records WHERE chunk_id = ?1)",
                params![doc.chunk_id.as_str()],
            )
            .map_err(db)?;
            tx.execute(
                "INSERT INTO chunk_fts (title, heading_path, normalized_text) \
                 VALUES (?1, ?2, ?3)",
                params![doc.title, doc.heading_path, doc.normalized_text],
            )
            .map_err(db)?;
            let rowid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO keyword_index_records \
                 (chunk_id, fts_rowid, index_engine, tokenizer_name, tokenizer_version, \
                  indexed_at, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active') \
                 ON CONFLICT(chunk_id) DO UPDATE SET fts_rowid = ?2, index_engine = ?3, \
                  tokenizer_name = ?4, tokenizer_version = ?5, indexed_at = ?6, \
                  status = 'active'",
                params![
                    doc.chunk_id.as_str(),
                    rowid,
                    ENGINE_NAME,
                    TOKENIZER_NAME,
                    TOKENIZER_VERSION,
                    now_iso8601(),
                ],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)
    }

    fn delete(&self, chunk_ids: &[ChunkId]) -> OrbokResult<()> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db)?;
        for chunk_id in chunk_ids {
            tx.execute(
                "DELETE FROM chunk_fts WHERE rowid = \
                 (SELECT fts_rowid FROM keyword_index_records WHERE chunk_id = ?1)",
                params![chunk_id.as_str()],
            )
            .map_err(db)?;
            tx.execute(
                "DELETE FROM keyword_index_records WHERE chunk_id = ?1",
                params![chunk_id.as_str()],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)
    }

    fn search(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_with_expr(build_match_expression(query), limit)
    }
}

fn db(e: rusqlite::Error) -> OrbokError {
    OrbokError::Database(e.to_string())
}
