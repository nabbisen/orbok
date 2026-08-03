//! Active-profile embedding model directory resolution.

use crate::settings::OrbokSettings;
use orbok::runtime_context::{RuntimeContext, RuntimePathProbe};
use orbok::runtime_storage::RuntimeStorage;
use orbok_db::Catalog;
use orbok_models::{ModelStoreLockError, ModelStoreMutationGuard, SharedAccess};
use orbok_ui::state::ModelProvenance;
use std::time::Duration;

#[derive(Debug)]
pub(crate) enum ManagedModelResolutionError {
    Store(std::io::Error),
    StoreLock(ModelStoreLockError),
    Catalog,
}

impl std::fmt::Display for ManagedModelResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "managed model store is unavailable: {error}"),
            Self::StoreLock(error) => {
                write!(formatter, "managed model store is unavailable: {error}")
            }
            Self::Catalog => formatter.write_str("managed model catalog state is unavailable"),
        }
    }
}

impl std::error::Error for ManagedModelResolutionError {}

pub(crate) struct ResolvedModelDir {
    pub(crate) _guard: Option<ModelStoreMutationGuard<SharedAccess>>,
    pub(crate) path: Option<String>,
    pub(crate) provenance: Option<ModelProvenance>,
}

/// Resolve the active-profile model directory: the current managed
/// generation if the catalog records one, else a manually-configured
/// directory outside the managed store. The managed store is always
/// obtained from the active `RuntimeContext` (via `RuntimeStorage`), never
/// re-derived from `catalog.path()` — a `Catalog` handle obtained by any
/// other route must not silently determine which profile's model store this
/// resolves against (Review 113 F1).
pub(crate) fn resolve_model_dir<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    catalog: &Catalog,
    settings: &OrbokSettings,
) -> Result<ResolvedModelDir, ManagedModelResolutionError> {
    resolve_model_dir_with_timeout(context, probe, catalog, settings, Duration::from_secs(5))
}

pub(crate) fn resolve_model_dir_with_timeout<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    catalog: &Catalog,
    settings: &OrbokSettings,
    timeout: Duration,
) -> Result<ResolvedModelDir, ManagedModelResolutionError> {
    let model_store = RuntimeStorage::new(context, probe)
        .model_store()
        .map_err(ManagedModelResolutionError::Store)?;
    let current = model_store
        .current_generation_dir(catalog, timeout)
        .map_err(|error| match error {
            orbok::runtime_storage::ManagedGenerationLookupError::StoreLock(inner) => {
                ManagedModelResolutionError::StoreLock(inner)
            }
            orbok::runtime_storage::ManagedGenerationLookupError::Catalog => {
                ManagedModelResolutionError::Catalog
            }
        })?;
    if let Some((guard, path)) = current {
        return Ok(ResolvedModelDir {
            _guard: Some(guard),
            path: Some(path.to_string_lossy().into_owned()),
            provenance: Some(ModelProvenance::AppManaged),
        });
    }
    let manual = settings
        .embedding_model_dir
        .as_ref()
        .filter(|path| !model_store.contains(std::path::Path::new(path)))
        .cloned();
    Ok(ResolvedModelDir {
        _guard: None,
        provenance: manual.as_ref().map(|_| ModelProvenance::UserSupplied),
        path: manual,
    })
}
