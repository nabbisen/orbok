//! RFC-050-aware resolution of the embedding worker's model + lease, for
//! the scheduler host (RFC-056 Slice 2).

use super::model_resolution::resolve_model_dir;
use crate::settings::OrbokSettings;
use orbok::runtime_context::{RuntimeContext, RuntimePathProbe};
use orbok_core::{ModelId, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::{ModelRecord, ModelRepository, ModelRole, ModelStatus, NewModel};
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_models::{EmbeddingModel, EmbeddingModelConfig, ModelStoreMutationGuard, SharedAccess};

/// Everything the scheduler host needs to run a real `EmbeddingWorker`,
/// resolved once at startup: a loaded model, the catalog-registered
/// `ModelId` embeddings should be written under, and (if the model came
/// from the managed store) the RFC-050 lease guard that must outlive the
/// scheduler loop -- dropping it early would let a model swap collect the
/// generation directory out from under an in-flight embed.
pub(crate) struct EmbeddingWorkerParts {
    pub(crate) model: Box<dyn EmbeddingModel>,
    pub(crate) model_id: ModelId,
    #[allow(dead_code)] // held for its Drop impl only (RFC-050 lease)
    _guard: Option<ModelStoreMutationGuard<SharedAccess>>,
}

impl EmbeddingWorkerParts {
    /// Build parts directly from a model + id, with no RFC-050 lease --
    /// for `scheduler_host`'s tests, which need to wire a deterministic
    /// test double (e.g. one that always fails) through the real dispatch
    /// path without a managed model store to resolve against.
    #[cfg(test)]
    pub(crate) fn for_test(model: Box<dyn EmbeddingModel>, model_id: ModelId) -> Self {
        Self {
            model,
            model_id,
            _guard: None,
        }
    }
}

/// Resolve the active-profile embedding model into `EmbeddingWorkerParts`,
/// or `None` when no model directory is configured or the backend fails to
/// load -- mirrors `bootstrap::search::run_search_with`'s own fallback.
/// The scheduler host treats `None` here as `model_missing` (RFC-008 §15).
/// Deliberately does not surface `ResolvedModelDir`'s own type outside
/// `bootstrap`: `model_resolution` stays a private module, and this is the
/// seam that lets `scheduler_host.rs` (a sibling top-level module, outside
/// `bootstrap`'s tree) reach RFC-050 resolution without that module's
/// visibility being widened.
pub(crate) fn resolve_embedding_worker_parts<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    catalog: &Catalog,
    settings: &OrbokSettings,
) -> Option<EmbeddingWorkerParts> {
    let resolved = match resolve_model_dir(context, probe, catalog, settings) {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(category = %error, "managed model resolution failed closed");
            return None;
        }
    };
    let dir = resolved.path.as_ref()?;
    let config = recommended_config_from_model_dir(dir);
    let model = match create_embedding_model(&config) {
        Ok(model) => model,
        Err(error) => {
            tracing::warn!(error = %error, "embedding model failed to load");
            return None;
        }
    };
    let model_id = match ensure_embedding_model_registered(catalog, &config) {
        Ok(id) => id,
        Err(error) => {
            tracing::warn!(error = %error, "embedding model registration failed");
            return None;
        }
    };
    Some(EmbeddingWorkerParts {
        model,
        model_id,
        _guard: resolved._guard,
    })
}

/// Find-or-register the recommended embedding model in the catalog's
/// `models` table, so repeated startups accumulate embeddings under one
/// stable `model_id` instead of a fresh row every launch. Duplicated from
/// the RFC-050 measurement test's own helper of the same name
/// (`bootstrap/tests/embedding_blocking_measurement.rs`) -- that copy
/// stays test-only; this is the one the running app now uses.
fn ensure_embedding_model_registered(
    catalog: &Catalog,
    config: &EmbeddingModelConfig,
) -> OrbokResult<ModelId> {
    let repo = ModelRepository::new(catalog);
    if let Some(existing) =
        repo.list_by_role(ModelRole::Embedding)?
            .into_iter()
            .find(|record: &ModelRecord| {
                record.model_name == config.model_name
                    && record.model_version == config.model_version
            })
    {
        return Ok(existing.model_id);
    }
    let record = repo.insert(NewModel {
        role: ModelRole::Embedding,
        model_name: config.model_name.clone(),
        model_version: config.model_version.clone(),
        local_path: None,
        license_summary: None,
        size_bytes: None,
        backend: Some("tract".to_string()),
        dimension: Some(config.dimension),
        status: ModelStatus::Available,
    })?;
    Ok(record.model_id)
}
