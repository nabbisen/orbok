//! Model registry repository (RFC-012 §6).
//!
//! The registry is persistent catalog data — it remembers which local AI
//! models the user has registered, their on-disk paths, and their current
//! availability. No model file is downloaded silently; every registration
//! or installation action requires explicit user confirmation (RFC-012
//! §13 "no silent download").

use crate::catalog::{Catalog, db_err};
use orbok_core::{ModelId, OrbokResult, now_iso8601};
use rusqlite::params;

/// The role a model serves in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Embedding,
    Reranker,
}

impl ModelRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelRole::Embedding => "embedding",
            ModelRole::Reranker => "reranker",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "embedding" => Some(Self::Embedding),
            "reranker" => Some(Self::Reranker),
            _ => None,
        }
    }
}

/// Model availability in the local registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    Available,
    Missing,
    Invalid,
    Installing,
    Disabled,
}

impl ModelStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelStatus::Available => "available",
            ModelStatus::Missing => "missing",
            ModelStatus::Invalid => "invalid",
            ModelStatus::Installing => "installing",
            ModelStatus::Disabled => "disabled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "available" => Some(Self::Available),
            "missing" => Some(Self::Missing),
            "invalid" => Some(Self::Invalid),
            "installing" => Some(Self::Installing),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// A registered model record.
#[derive(Debug, Clone)]
pub struct ModelRecord {
    pub model_id: ModelId,
    pub role: ModelRole,
    pub model_name: String,
    pub model_version: String,
    pub local_path: Option<String>,
    pub license_summary: Option<String>,
    pub size_bytes: Option<u64>,
    pub backend: Option<String>,
    pub dimension: Option<u32>,
    pub status: ModelStatus,
    pub last_validated_at: Option<String>,
}

/// Parameters for registering a new model.
#[derive(Debug, Clone)]
pub struct NewModel {
    pub role: ModelRole,
    pub model_name: String,
    pub model_version: String,
    pub local_path: Option<String>,
    pub license_summary: Option<String>,
    pub size_bytes: Option<u64>,
    pub backend: Option<String>,
    pub dimension: Option<u32>,
    pub status: ModelStatus,
}

pub struct ModelRepository<'a> {
    catalog: &'a Catalog,
}

impl<'a> ModelRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Register a new model. Returns the generated ModelId.
    pub fn insert(&self, new: NewModel) -> OrbokResult<ModelRecord> {
        let id = ModelId::generate();
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO models \
             (model_id, role, model_name, model_version, local_path, license_summary, \
              size_bytes, backend, dimension, status, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
            params![
                id.as_str(),
                new.role.as_str(),
                new.model_name,
                new.model_version,
                new.local_path,
                new.license_summary,
                new.size_bytes.map(|v| v as i64),
                new.backend,
                new.dimension.map(|v| v as i64),
                new.status.as_str(),
                now,
            ],
        )
        .map_err(db_err)?;
        drop(conn);
        self.get(&id)?.ok_or(orbok_core::OrbokError::SourceNotFound)
    }

    /// Fetch one model by ID.
    pub fn get(&self, id: &ModelId) -> OrbokResult<Option<ModelRecord>> {
        let conn = self.catalog.lock();
        let result = conn.query_row(
            "SELECT model_id, role, model_name, model_version, local_path, license_summary, \
             size_bytes, backend, dimension, status, last_validated_at \
             FROM models WHERE model_id = ?1",
            params![id.as_str()],
            row_to_record,
        );
        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(db_err(e)),
        }
    }

    /// All models of a given role.
    pub fn list_by_role(&self, role: ModelRole) -> OrbokResult<Vec<ModelRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT model_id, role, model_name, model_version, local_path, license_summary, \
                 size_bytes, backend, dimension, status, last_validated_at \
                 FROM models WHERE role = ?1 ORDER BY model_name, model_version",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![role.as_str()], row_to_record)
            .map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err)?);
        }
        Ok(out)
    }

    /// All models (all roles).
    pub fn list_all(&self) -> OrbokResult<Vec<ModelRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT model_id, role, model_name, model_version, local_path, license_summary, \
                 size_bytes, backend, dimension, status, last_validated_at \
                 FROM models ORDER BY role, model_name",
            )
            .map_err(db_err)?;
        let rows = stmt.query_map([], row_to_record).map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err)?);
        }
        Ok(out)
    }

    /// Update model status (available / missing / invalid / disabled).
    pub fn set_status(&self, id: &ModelId, status: ModelStatus) -> OrbokResult<()> {
        let conn = self.catalog.lock();
        conn.execute(
            "UPDATE models SET status = ?2, updated_at = ?3 WHERE model_id = ?1",
            params![id.as_str(), status.as_str(), now_iso8601()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Validate a model: check the file exists and matches expected dimension.
    /// Updates `status` and `last_validated_at` in the catalog.
    pub fn validate(&self, id: &ModelId, expected_dim: Option<u32>) -> OrbokResult<ModelStatus> {
        let record = match self.get(id)? {
            Some(r) => r,
            None => return Ok(ModelStatus::Missing),
        };
        let status = if let Some(path) = &record.local_path {
            if std::path::Path::new(path).exists() {
                // Dimension check if expected.
                if let (Some(expected), Some(actual)) = (expected_dim, record.dimension) {
                    if expected != actual {
                        ModelStatus::Invalid
                    } else {
                        ModelStatus::Available
                    }
                } else {
                    ModelStatus::Available
                }
            } else {
                ModelStatus::Missing
            }
        } else {
            ModelStatus::Missing
        };
        let now = now_iso8601();
        {
            let conn = self.catalog.lock();
            conn.execute(
                "UPDATE models SET status = ?2, last_validated_at = ?3, updated_at = ?3 \
                 WHERE model_id = ?1",
                params![id.as_str(), status.as_str(), now],
            )
            .map_err(db_err)?;
        }
        Ok(status)
    }

    /// Locate and register an existing model file on disk (RFC-012 §8
    /// "locate existing model"). This is explicit — no silent downloads.
    pub fn locate(
        &self,
        path: &str,
        role: ModelRole,
        name: &str,
        version: &str,
        dimension: Option<u32>,
    ) -> OrbokResult<ModelRecord> {
        let size_bytes = std::fs::metadata(path).map(|m| m.len()).ok();
        let record = self.insert(NewModel {
            role,
            model_name: name.to_string(),
            model_version: version.to_string(),
            local_path: Some(path.to_string()),
            license_summary: None,
            size_bytes,
            backend: None,
            dimension,
            status: if size_bytes.is_some() {
                ModelStatus::Available
            } else {
                ModelStatus::Missing
            },
        })?;
        Ok(record)
    }

    /// When an embedding model changes, mark all embeddings from the old
    /// model as stale (RFC-012 §14).
    pub fn mark_embedding_dependents_stale(&self, model_id: &ModelId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n = conn
            .execute(
                "UPDATE embeddings SET status = 'stale', updated_at = ?2 WHERE model_id = ?1",
                params![model_id.as_str(), now_iso8601()],
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelRecord> {
    Ok(ModelRecord {
        model_id: ModelId::from_string(row.get::<_, String>(0)?),
        role: {
            let s: String = row.get(1)?;
            ModelRole::parse(&s).unwrap_or(ModelRole::Embedding)
        },
        model_name: row.get(2)?,
        model_version: row.get(3)?,
        local_path: row.get(4)?,
        license_summary: row.get(5)?,
        size_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
        backend: row.get(7)?,
        dimension: row.get::<_, Option<i64>>(8)?.map(|v| v as u32),
        status: {
            let s: String = row.get(9)?;
            ModelStatus::parse(&s).unwrap_or(ModelStatus::Missing)
        },
        last_validated_at: row.get(10)?,
    })
}

/// Verify a model file's SHA-256 against an expected hash
/// (RFC-029 §5 integrity checking).
///
/// `expected_hash` is a lowercase hex SHA-256 string (64 chars).
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, `Err` on I/O
/// error. The file path is not logged (NFR-014).
pub fn verify_model_sha256(path: &str, expected_hash: &str) -> OrbokResult<bool> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(orbok_core::OrbokError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(orbok_core::OrbokError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(actual == expected_hash)
}
