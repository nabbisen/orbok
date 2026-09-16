//! Embedding storage repository (RFC-008 §12).
//!
//! Embeddings are rebuildable index data — they can be deleted and
//! regenerated from source chunks at any time. Vectors are stored as
//! raw little-endian FP32 BLOBs (`vector_format = 'fp32'`).
//!
//! Log hygiene (RFC-008 §23 test 10 / NFR-014): no vector values are
//! logged by this module.

use crate::catalog::{Catalog, db_err};
use orbok_core::{
    ChunkId, EmbeddingId, FileId, ModelId, OrbokResult, SEARCHABLE_SOURCE_STATUS_SQL, now_iso8601,
};
use rusqlite::params;

/// Data needed to insert one embedding.
pub struct NewEmbedding {
    pub chunk_id: ChunkId,
    pub model_id: ModelId,
    pub dimension: u32,
    /// FP32 vector, already L2-normalized.
    pub vector: Vec<f32>,
}

/// One active embedding record with its vector.
#[derive(Debug, Clone)]
pub struct EmbeddingRecord {
    pub embedding_id: EmbeddingId,
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub vector: Vec<f32>,
}

pub struct EmbeddingRepository<'a> {
    catalog: &'a Catalog,
}

impl<'a> EmbeddingRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Insert or replace the embedding for a chunk+model pair.
    pub fn upsert(&self, new: &NewEmbedding) -> OrbokResult<()> {
        let id = EmbeddingId::generate();
        let now = now_iso8601();
        let blob = orbok_models::vec_to_blob(&new.vector);
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO embeddings \
             (embedding_id, chunk_id, model_id, vector_format, dimension, norm, \
              storage_location, vector_blob, status, created_at, updated_at) \
             VALUES (?1,?2,?3,'fp32',?4,'l2','sqlite_blob',?5,'active',?6,?6) \
             ON CONFLICT(chunk_id, model_id, vector_format) DO UPDATE SET \
             vector_blob=?5, status='active', updated_at=?6",
            params![
                id.as_str(),
                new.chunk_id.as_str(),
                new.model_id.as_str(),
                new.dimension as i64,
                blob,
                now,
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// All active embeddings for exact cosine-similarity scan.
    /// Only returns embeddings for active chunks (RFC-008 §20 stale
    /// exclusion). Vectors are not logged.
    pub fn list_active_for_scan(
        &self,
        model_id: &str,
        dimension: u32,
    ) -> OrbokResult<Vec<EmbeddingRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(&format!(
                // RFC-060 §5: the vector half of the same source-status
                // filter the two keyword queries apply.
                "SELECT e.embedding_id, e.chunk_id, c.file_id, e.vector_blob \
                 FROM embeddings e \
                 JOIN chunks c ON c.chunk_id = e.chunk_id \
                 JOIN files f ON f.file_id = c.file_id \
                 JOIN sources s ON s.source_id = f.source_id \
                 WHERE e.model_id = ?1 AND e.dimension = ?2 \
                   AND e.status = 'active' AND c.chunk_status = 'active' \
                   AND s.status IN {SEARCHABLE_SOURCE_STATUS_SQL}"
            ))
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![model_id, dimension as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (emb_id, chunk_id, file_id, blob) = row.map_err(db_err)?;
            let vector = orbok_models::blob_to_vec(&blob, dimension).unwrap_or_default();
            out.push(EmbeddingRecord {
                embedding_id: EmbeddingId::from_string(emb_id),
                chunk_id: ChunkId::from_string(chunk_id),
                file_id: FileId::from_string(file_id),
                vector,
            });
        }
        Ok(out)
    }

    /// Mark embeddings stale when the model version changes (RFC-008 §16).
    pub fn mark_stale_for_model(&self, model_id: &str) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE embeddings SET status='stale', updated_at=?2 WHERE model_id=?1",
                params![model_id, now_iso8601()],
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// Count semantically active embeddings (embedding active AND chunk active).
    pub fn count_active(&self, model_id: &str) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM embeddings e \
                 JOIN chunks c ON c.chunk_id = e.chunk_id \
                 WHERE e.model_id=?1 AND e.status='active' AND c.chunk_status='active'",
                params![model_id],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }
    /// Store an INT8-quantized embedding (RFC-024 Space Saving mode).
    pub fn upsert_i8(
        &self,
        chunk_id: &orbok_core::ChunkId,
        model_id: &orbok_core::ModelId,
        dimension: u32,
        i8_vector: &[i8],
    ) -> OrbokResult<()> {
        let id = orbok_core::EmbeddingId::generate();
        let now = orbok_core::now_iso8601();
        let blob: Vec<u8> = i8_vector.iter().map(|&b| b as u8).collect();
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO embeddings \
             (embedding_id, chunk_id, model_id, vector_format, dimension, norm, \
              storage_location, vector_blob, status, created_at, updated_at) \
             VALUES (?1,?2,?3,'int8',?4,'l2','sqlite_blob',?5,'active',?6,?6) \
             ON CONFLICT(chunk_id, model_id, vector_format) DO UPDATE SET \
             vector_blob=?5, status='active', updated_at=?6",
            rusqlite::params![
                id.as_str(),
                chunk_id.as_str(),
                model_id.as_str(),
                dimension as i64,
                blob,
                now
            ],
        )
        .map_err(crate::catalog::db_err)?;
        Ok(())
    }
}
