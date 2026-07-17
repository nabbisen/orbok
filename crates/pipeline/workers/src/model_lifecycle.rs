//! Serialized managed-model startup recovery and cleanup (RFC-050 Phase 3).

#[cfg(unix)]
use crate::model_delivery::sync_directory;
use crate::model_durability::{ModelDurabilityError, durable_rename, preflight_managed_store};
use orbok_db::Catalog;
use orbok_db::repo::ManagedGenerationRepository;
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_models::{
    DEFAULT_TRUSTED_MODEL, ManagedGenerationId, ManagedGenerationState, ManagedModelStore,
    ModelReadiness, ModelStoreLockError, check_app_managed_model_readiness,
};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const STAGING_DIR: &str = ".staging";
const QUARANTINE_DIR: &str = ".quarantine";
const GENERATIONS_DIR: &str = "generations";
const TRUSTED_MANIFEST_FILE: &str = "trusted-manifest.json";
const COMPLETE_FILE: &str = "COMPLETE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedModelStartupOutcome {
    pub startup_epoch: u64,
    pub current_generation_id: Option<ManagedGenerationId>,
    pub recovered_inactive: usize,
    pub quarantined_staging: usize,
    pub quarantined_generations: usize,
    pub rolled_back: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedModelCleanupOutcome {
    pub removed_generations: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelLifecycleError {
    #[error("the model store is unavailable")]
    StoreUnavailable,
    #[error("the model store is busy")]
    StoreBusy,
    #[error("managed model recovery failed")]
    Filesystem,
    #[error("managed model catalog recovery failed")]
    Catalog,
}

/// Advance the durable startup epoch, recover filesystem state, and validate
/// the current generation before normal runtime resolution can observe it.
pub fn run_managed_model_startup(
    catalog: &Catalog,
    store: &ManagedModelStore,
) -> Result<ManagedModelStartupOutcome, ModelLifecycleError> {
    run_managed_model_startup_with(catalog, store, DEFAULT_TRUSTED_MODEL.manifest_id, |path| {
        trusted_generation_loads(store.models_dir(), path)
    })
}

#[cfg_attr(test, allow(clippy::needless_pass_by_value))]
pub(crate) fn run_managed_model_startup_with(
    catalog: &Catalog,
    store: &ManagedModelStore,
    manifest_id: &str,
    validate_and_load: impl Fn(&Path) -> bool,
) -> Result<ManagedModelStartupOutcome, ModelLifecycleError> {
    preflight_managed_store(store.models_dir()).map_err(map_durability_store_error)?;
    #[cfg(unix)]
    if !is_real_directory_from_store_root(store.models_dir(), store.models_dir()) {
        return Err(ModelLifecycleError::StoreUnavailable);
    }
    let guard = store
        .acquire_exclusive(LOCK_TIMEOUT)
        .map_err(map_lock_error)?;
    let repository = ManagedGenerationRepository::new(catalog);
    let advanced = repository
        .advance_startup_epoch(&guard)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    let startup_epoch = advanced.profile.startup_epoch.get();

    let staging_parent = store.models_dir().join(STAGING_DIR);
    let quarantine_parent = store.models_dir().join(QUARANTINE_DIR);
    let generations_parent = store.models_dir().join(GENERATIONS_DIR);
    create_and_sync_parent(store.models_dir(), &staging_parent)?;
    create_and_sync_parent(store.models_dir(), &quarantine_parent)?;
    create_and_sync_parent(store.models_dir(), &generations_parent)?;

    let mut recovered_inactive = 0;
    let mut quarantined_staging = 0;
    let mut quarantined_generations = 0;
    for entry in read_entries(&staging_parent)? {
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|_| ModelLifecycleError::Filesystem)?
            .is_dir()
        {
            quarantine(
                store.models_dir(),
                &path,
                &quarantine_parent,
                &staging_parent,
            )?;
            quarantined_staging += 1;
            continue;
        }
        let parsed_id = entry
            .file_name()
            .to_str()
            .and_then(|name| ManagedGenerationId::parse(name.to_owned()).ok());
        if let Some(generation_id) = parsed_id.filter(|_| validate_and_load(&path)) {
            let promoted = generations_parent.join(generation_id.as_str());
            if promoted.exists() {
                quarantine(
                    store.models_dir(),
                    &path,
                    &quarantine_parent,
                    &staging_parent,
                )?;
                quarantined_staging += 1;
                continue;
            }
            lifecycle_crash_point("before_recovery_durable_rename");
            durable_rename(store.models_dir(), &path, &promoted)
                .map_err(map_durability_filesystem_error)?;
            lifecycle_crash_point("after_recovery_durable_rename");
            #[cfg(unix)]
            {
                lifecycle_crash_point("before_recovery_source_parent_sync");
                sync_directory(&staging_parent).map_err(|_| ModelLifecycleError::Filesystem)?;
                lifecycle_crash_point("after_recovery_source_parent_sync");
                lifecycle_crash_point("before_recovery_destination_parent_sync");
                sync_directory(&generations_parent).map_err(|_| ModelLifecycleError::Filesystem)?;
                lifecycle_crash_point("after_recovery_destination_parent_sync");
                lifecycle_crash_point("before_recovery_model_root_sync");
                sync_directory(store.models_dir()).map_err(|_| ModelLifecycleError::Filesystem)?;
                lifecycle_crash_point("after_recovery_model_root_sync");
            }
            register_inactive_if_unreferenced(&repository, &guard, generation_id, manifest_id)?;
            recovered_inactive += 1;
        } else {
            quarantine(
                store.models_dir(),
                &path,
                &quarantine_parent,
                &staging_parent,
            )?;
            quarantined_staging += 1;
        }
    }

    let referenced = repository
        .load_exclusive(&guard)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    for entry in read_entries(&generations_parent)? {
        let path = entry.path();
        let generation_id = entry
            .file_name()
            .to_str()
            .and_then(|name| ManagedGenerationId::parse(name.to_owned()).ok());
        if generation_id
            .as_ref()
            .is_some_and(|id| referenced.generations.contains_key(id))
        {
            continue;
        }
        let is_directory = entry
            .file_type()
            .map_err(|_| ModelLifecycleError::Filesystem)?
            .is_dir();
        if let Some(generation_id) = generation_id.filter(|_| {
            is_directory
                && is_real_directory_from_store_root(store.models_dir(), &path)
                && validate_and_load(&path)
        }) {
            register_inactive_if_unreferenced(&repository, &guard, generation_id, manifest_id)?;
            recovered_inactive += 1;
        } else {
            quarantine(
                store.models_dir(),
                &path,
                &quarantine_parent,
                &generations_parent,
            )?;
            quarantined_generations += 1;
        }
    }

    let snapshot = repository
        .load_exclusive(&guard)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    let Some(current_id) = snapshot.profile.current_generation_id.clone() else {
        return Ok(ManagedModelStartupOutcome {
            startup_epoch,
            current_generation_id: None,
            recovered_inactive,
            quarantined_staging,
            quarantined_generations,
            rolled_back: false,
        });
    };
    let current_valid = record_and_generation_load(
        store,
        &snapshot,
        &current_id,
        manifest_id,
        &validate_and_load,
    );
    if current_valid {
        let evidence = snapshot
            .observe_current_for_startup_validation(&current_id)
            .map_err(|_| ModelLifecycleError::Catalog)?;
        let validated = repository
            .validate_current_after_startup(&guard, &evidence)
            .map_err(|_| ModelLifecycleError::Catalog)?;
        lifecycle_crash_point("after_current_validation_transaction");
        let validated = if validated.profile.previous_generation_id.is_some() {
            lifecycle_crash_point("before_previous_release_transaction");
            let released = repository
                .release_previous_after_startup_validation(&guard)
                .map_err(|_| ModelLifecycleError::Catalog)?;
            lifecycle_crash_point("after_previous_release_transaction");
            released
        } else {
            validated
        };
        return Ok(ManagedModelStartupOutcome {
            startup_epoch,
            current_generation_id: validated.profile.current_generation_id,
            recovered_inactive,
            quarantined_staging,
            quarantined_generations,
            rolled_back: false,
        });
    }

    let previous_verified = snapshot
        .profile
        .previous_generation_id
        .as_ref()
        .is_some_and(|id| {
            record_and_generation_load(store, &snapshot, id, manifest_id, &validate_and_load)
        });
    let rolled_back = repository
        .rollback_invalid_current(&guard, previous_verified)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    Ok(ManagedModelStartupOutcome {
        startup_epoch,
        current_generation_id: rolled_back.profile.current_generation_id,
        recovered_inactive,
        quarantined_staging,
        quarantined_generations,
        rolled_back: true,
    })
}

/// Remove only catalog-known inactive/invalid generations while rechecking
/// current and previous protection under the exclusive model-store guard.
pub fn cleanup_managed_model_generations(
    catalog: &Catalog,
    store: &ManagedModelStore,
) -> Result<ManagedModelCleanupOutcome, ModelLifecycleError> {
    preflight_managed_store(store.models_dir()).map_err(map_durability_store_error)?;
    let guard = store
        .acquire_exclusive(LOCK_TIMEOUT)
        .map_err(map_lock_error)?;
    let snapshot = ManagedGenerationRepository::new(catalog)
        .load_exclusive(&guard)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    let protected = [
        snapshot.profile.current_generation_id.as_ref(),
        snapshot.profile.previous_generation_id.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect::<BTreeSet<_>>();
    let mut removed_generations = 0;
    for record in snapshot.generations.values() {
        if protected.contains(&record.generation_id)
            || !matches!(
                record.state,
                ManagedGenerationState::Inactive | ManagedGenerationState::Invalid
            )
        {
            continue;
        }
        let path = store
            .models_dir()
            .join(GENERATIONS_DIR)
            .join(record.generation_id.as_str());
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed_generations += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ModelLifecycleError::Filesystem),
        }
    }
    if removed_generations > 0 {
        #[cfg(unix)]
        {
            sync_directory(&store.models_dir().join(GENERATIONS_DIR))
                .map_err(|_| ModelLifecycleError::Filesystem)?;
            sync_directory(store.models_dir()).map_err(|_| ModelLifecycleError::Filesystem)?;
        }
        #[cfg(not(any(unix, windows)))]
        return Err(ModelLifecycleError::Filesystem);
    }
    Ok(ManagedModelCleanupOutcome {
        removed_generations,
    })
}

fn trusted_generation_loads(store_root: &Path, generation_dir: &Path) -> bool {
    if !is_real_directory_from_store_root(store_root, generation_dir) {
        return false;
    }
    if !trusted_generation_bytes_valid(generation_dir) {
        return false;
    }
    embedding_generation_loads(generation_dir, DEFAULT_TRUSTED_MODEL.model.dimension)
}

fn embedding_generation_loads(generation_dir: &Path, expected_dimension: u32) -> bool {
    let mut config = recommended_config_from_model_dir(generation_dir);
    config.dimension = expected_dimension;
    let Ok(model) = create_embedding_model(&config) else {
        return false;
    };
    if model.dimension() != expected_dimension {
        return false;
    }
    model
        .embed_batch(&["query: startup validation"])
        .ok()
        .is_some_and(|vectors| {
            vectors.len() == 1 && vectors[0].len() == expected_dimension as usize
        })
}

fn trusted_generation_bytes_valid(generation_dir: &Path) -> bool {
    for file in DEFAULT_TRUSTED_MODEL.files {
        let path = generation_dir.join(file.relative_path);
        if !is_regular_file_without_symlink_ancestors(generation_dir, &path) {
            return false;
        }
    }
    if check_app_managed_model_readiness(generation_dir).overall() != ModelReadiness::Ready {
        return false;
    }
    for relative in [TRUSTED_MANIFEST_FILE, COMPLETE_FILE] {
        if !is_regular_file_without_symlink_ancestors(
            generation_dir,
            &generation_dir.join(relative),
        ) {
            return false;
        }
    }
    let Ok(expected_manifest) = serde_json::to_vec_pretty(&DEFAULT_TRUSTED_MODEL) else {
        return false;
    };
    std::fs::read(generation_dir.join(TRUSTED_MANIFEST_FILE))
        .ok()
        .as_deref()
        == Some(expected_manifest.as_slice())
        && std::fs::read(generation_dir.join(COMPLETE_FILE))
            .ok()
            .as_deref()
            == Some(b"complete\n")
}

#[cfg(windows)]
fn is_real_directory_from_store_root(store_root: &Path, directory: &Path) -> bool {
    directory.strip_prefix(store_root).is_ok() && preflight_managed_store(directory).is_ok()
}

#[cfg(not(windows))]
fn is_real_directory_from_store_root(store_root: &Path, directory: &Path) -> bool {
    let Ok(relative) = directory.strip_prefix(store_root) else {
        return false;
    };
    let Ok(root_metadata) = std::fs::symlink_metadata(store_root) else {
        return false;
    };
    if !root_metadata.file_type().is_dir() || is_symlink_or_reparse_point(&root_metadata) {
        return false;
    }
    let mut current = store_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        if !metadata.file_type().is_dir() || is_symlink_or_reparse_point(&metadata) {
            return false;
        }
    }
    true
}

fn is_regular_file_without_symlink_ancestors(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        if is_symlink_or_reparse_point(&metadata) {
            return false;
        }
    }
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && !is_symlink_or_reparse_point(&metadata)
    })
}

#[cfg(windows)]
fn is_symlink_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_symlink_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn record_and_generation_load(
    store: &ManagedModelStore,
    snapshot: &orbok_models::ManagedGenerationSnapshot,
    generation_id: &ManagedGenerationId,
    manifest_id: &str,
    validate_and_load: &impl Fn(&Path) -> bool,
) -> bool {
    snapshot
        .generations
        .get(generation_id)
        .is_some_and(|record| record.manifest_id == manifest_id)
        && validate_and_load(
            &store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(generation_id.as_str()),
        )
}

fn register_inactive_if_unreferenced(
    repository: &ManagedGenerationRepository<'_>,
    guard: &orbok_models::ModelStoreMutationGuard<orbok_models::ExclusiveAccess>,
    generation_id: ManagedGenerationId,
    manifest_id: &str,
) -> Result<(), ModelLifecycleError> {
    let snapshot = repository
        .load_exclusive(guard)
        .map_err(|_| ModelLifecycleError::Catalog)?;
    if !snapshot.generations.contains_key(&generation_id) {
        repository
            .register_inactive(guard, generation_id, manifest_id)
            .map_err(|_| ModelLifecycleError::Catalog)?;
    }
    Ok(())
}

fn create_and_sync_parent(root: &Path, path: &Path) -> Result<(), ModelLifecycleError> {
    if path.exists() {
        return is_real_directory_from_store_root(root, path)
            .then_some(())
            .ok_or(ModelLifecycleError::Filesystem);
    }
    std::fs::create_dir(path).map_err(|_| ModelLifecycleError::Filesystem)?;
    #[cfg(windows)]
    let _ = root;
    #[cfg(unix)]
    sync_directory(root).map_err(|_| ModelLifecycleError::Filesystem)?;
    #[cfg(not(any(unix, windows)))]
    return Err(ModelLifecycleError::Filesystem);
    Ok(())
}

fn read_entries(path: &Path) -> Result<Vec<std::fs::DirEntry>, ModelLifecycleError> {
    std::fs::read_dir(path)
        .map_err(|_| ModelLifecycleError::Filesystem)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ModelLifecycleError::Filesystem)
}

fn quarantine(
    managed_root: &Path,
    path: &Path,
    quarantine_parent: &Path,
    source_parent: &Path,
) -> Result<(), ModelLifecycleError> {
    let original = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let unique = ManagedGenerationId::generate();
    let target = quarantine_parent.join(format!("{original}-{}", unique.as_str()));
    lifecycle_crash_point("before_quarantine_durable_rename");
    durable_rename(managed_root, path, &target).map_err(map_durability_filesystem_error)?;
    lifecycle_crash_point("after_quarantine_durable_rename");
    #[cfg(windows)]
    let _ = source_parent;
    #[cfg(unix)]
    {
        lifecycle_crash_point("before_quarantine_source_parent_sync");
        sync_directory(source_parent).map_err(|_| ModelLifecycleError::Filesystem)?;
        lifecycle_crash_point("after_quarantine_source_parent_sync");
        lifecycle_crash_point("before_quarantine_destination_parent_sync");
        sync_directory(quarantine_parent).map_err(|_| ModelLifecycleError::Filesystem)?;
        lifecycle_crash_point("after_quarantine_destination_parent_sync");
        lifecycle_crash_point("before_quarantine_model_root_sync");
        sync_directory(managed_root).map_err(|_| ModelLifecycleError::Filesystem)?;
        lifecycle_crash_point("after_quarantine_model_root_sync");
    }
    #[cfg(not(any(unix, windows)))]
    return Err(ModelLifecycleError::Filesystem);
    Ok(())
}

fn map_lock_error(error: ModelStoreLockError) -> ModelLifecycleError {
    match error {
        ModelStoreLockError::Timeout => ModelLifecycleError::StoreBusy,
        ModelStoreLockError::Io(_) | ModelStoreLockError::UnsupportedTarget => {
            ModelLifecycleError::StoreUnavailable
        }
    }
}

fn map_durability_store_error(error: ModelDurabilityError) -> ModelLifecycleError {
    tracing::warn!(durability_error = %error, "managed model durability preflight failed");
    ModelLifecycleError::StoreUnavailable
}

fn map_durability_filesystem_error(error: ModelDurabilityError) -> ModelLifecycleError {
    tracing::warn!(durability_error = %error, "managed model durability operation failed");
    ModelLifecycleError::Filesystem
}

#[cfg(test)]
fn lifecycle_crash_point(point: &str) {
    if std::env::var("ORBOK_RFC050_LIFECYCLE_CRASH_POINT").as_deref() == Ok(point) {
        std::process::abort();
    }
}

#[cfg(not(test))]
fn lifecycle_crash_point(_point: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_models::ManagedGenerationState;
    use prost::Message as _;
    use std::process::Command;

    fn setup() -> (tempfile::TempDir, Catalog, ManagedModelStore) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("models");
        std::fs::create_dir(&root).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        let store = ManagedModelStore::default_embedding(root);
        (temp, catalog, store)
    }

    fn marker_validator(path: &Path) -> bool {
        path.join("VALID").is_file()
    }

    #[cfg(unix)]
    fn symlink_directory(source: &Path, target: &Path) {
        std::os::unix::fs::symlink(source, target).unwrap();
    }

    #[cfg(windows)]
    fn symlink_directory(source: &Path, target: &Path) {
        std::os::windows::fs::symlink_dir(source, target).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn startup_rejects_current_generation_root_symlink_and_clears_pointer() {
        let (_temp, catalog, store) = setup();
        let generation_id = ManagedGenerationId::generate();
        let external = store.models_dir().parent().unwrap().join("external-model");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(external.join("VALID"), b"valid").unwrap();
        let generations = store.models_dir().join(GENERATIONS_DIR);
        std::fs::create_dir(&generations).unwrap();
        symlink_directory(&external, &generations.join(generation_id.as_str()));
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(
                    &guard,
                    generation_id.clone(),
                    DEFAULT_TRUSTED_MODEL.manifest_id,
                )
                .unwrap();
            repository.activate(&guard, &generation_id).unwrap();
        }

        let outcome = run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            |path| {
                is_real_directory_from_store_root(store.models_dir(), path)
                    && marker_validator(path)
            },
        )
        .unwrap();

        assert!(outcome.rolled_back);
        assert_eq!(outcome.current_generation_id, None);
        assert!(external.join("VALID").exists());
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(
            snapshot.generations[&generation_id].state,
            ManagedGenerationState::Invalid
        );
    }

    #[test]
    fn real_tokenizer_onnx_load_and_output_dimension_are_checked() {
        use tract_onnx::pb::tensor_proto::DataType;
        use tract_onnx::pb::tensor_shape_proto::dimension::Value as DimensionValue;
        use tract_onnx::pb::type_proto::{Tensor, Value as TypeValue};
        use tract_onnx::pb::{
            GraphProto, ModelProto, OperatorSetIdProto, TensorProto, TensorShapeProto, TypeProto,
            ValueInfoProto,
        };

        fn value_info(name: &str, datum: DataType, dimensions: &[i64]) -> ValueInfoProto {
            ValueInfoProto {
                name: name.to_owned(),
                r#type: Some(TypeProto {
                    denotation: String::new(),
                    value: Some(TypeValue::TensorType(Tensor {
                        elem_type: datum as i32,
                        shape: Some(TensorShapeProto {
                            dim: dimensions
                                .iter()
                                .map(|dimension| tract_onnx::pb::tensor_shape_proto::Dimension {
                                    denotation: String::new(),
                                    value: Some(DimensionValue::DimValue(*dimension)),
                                })
                                .collect(),
                        }),
                    })),
                }),
                doc_string: String::new(),
            }
        }

        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("onnx")).unwrap();
        std::fs::write(
            temp.path().join("tokenizer.json"),
            br#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"query":1,"startup":2,"validation":3},"unk_token":"[UNK]"}}"#,
        )
        .unwrap();
        let model = ModelProto {
            ir_version: 8,
            opset_import: vec![OperatorSetIdProto {
                domain: String::new(),
                version: 13,
            }],
            producer_name: "orbok-test".into(),
            graph: Some(GraphProto {
                name: "constant-embedding".into(),
                initializer: vec![TensorProto {
                    dims: vec![1, 2],
                    data_type: DataType::Float as i32,
                    float_data: vec![0.25, 0.75],
                    name: "embedding".into(),
                    ..Default::default()
                }],
                input: vec![value_info("input_ids", DataType::Int64, &[1, 512])],
                output: vec![value_info("embedding", DataType::Float, &[1, 2])],
                ..Default::default()
            }),
            ..Default::default()
        };
        std::fs::write(temp.path().join("onnx/model.onnx"), model.encode_to_vec()).unwrap();

        let mut config = recommended_config_from_model_dir(temp.path());
        config.dimension = 2;
        let loaded = create_embedding_model(&config).unwrap();
        let vectors = loaded.embed_batch(&["query: startup validation"]).unwrap();
        assert_eq!(vectors[0].len(), 2);
        assert!(embedding_generation_loads(temp.path(), 2));
        assert!(!embedding_generation_loads(temp.path(), 3));
    }

    fn create_generation(root: &Path, parent: &str, id: &ManagedGenerationId, valid: bool) {
        let path = root.join(parent).join(id.as_str());
        std::fs::create_dir_all(&path).unwrap();
        if valid {
            std::fs::write(path.join("VALID"), b"valid").unwrap();
        }
    }

    #[test]
    fn startup_quarantines_incomplete_staging_and_retains_complete_as_inactive() {
        let (_temp, catalog, store) = setup();
        let complete_staged = ManagedGenerationId::generate();
        let incomplete_staged = ManagedGenerationId::generate();
        let complete_unreferenced = ManagedGenerationId::generate();
        let invalid_unreferenced = ManagedGenerationId::generate();
        create_generation(store.models_dir(), STAGING_DIR, &complete_staged, true);
        create_generation(store.models_dir(), STAGING_DIR, &incomplete_staged, false);
        create_generation(
            store.models_dir(),
            GENERATIONS_DIR,
            &complete_unreferenced,
            true,
        );
        create_generation(
            store.models_dir(),
            GENERATIONS_DIR,
            &invalid_unreferenced,
            false,
        );
        std::fs::write(
            store.models_dir().join(GENERATIONS_DIR).join("malformed"),
            b"invalid",
        )
        .unwrap();

        let outcome = run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert_eq!(outcome.startup_epoch, 1);
        assert_eq!(outcome.current_generation_id, None);
        assert_eq!(outcome.recovered_inactive, 2);
        assert_eq!(outcome.quarantined_staging, 1);
        assert_eq!(outcome.quarantined_generations, 2);
        assert!(
            store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(complete_staged.as_str())
                .is_dir()
        );
        assert!(
            std::fs::read_dir(store.models_dir().join(STAGING_DIR))
                .unwrap()
                .next()
                .is_none()
        );
        assert_eq!(
            std::fs::read_dir(store.models_dir().join(QUARANTINE_DIR))
                .unwrap()
                .count(),
            3
        );

        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(
            snapshot.generations[&complete_staged].state,
            ManagedGenerationState::Inactive
        );
        assert_eq!(
            snapshot.generations[&complete_unreferenced].state,
            ManagedGenerationState::Inactive
        );
    }

    #[test]
    fn recovery_and_quarantine_durability_boundaries_recover_coherently() {
        let rename_crash_cases = [
            ("before_recovery_durable_rename", true),
            ("after_recovery_durable_rename", true),
            ("before_quarantine_durable_rename", false),
            ("after_quarantine_durable_rename", false),
        ];
        #[cfg(unix)]
        let crash_cases = rename_crash_cases
            .into_iter()
            .chain([
                ("before_recovery_source_parent_sync", true),
                ("after_recovery_source_parent_sync", true),
                ("before_recovery_destination_parent_sync", true),
                ("after_recovery_destination_parent_sync", true),
                ("before_recovery_model_root_sync", true),
                ("after_recovery_model_root_sync", true),
                ("before_quarantine_source_parent_sync", false),
                ("after_quarantine_source_parent_sync", false),
                ("before_quarantine_destination_parent_sync", false),
                ("after_quarantine_destination_parent_sync", false),
                ("before_quarantine_model_root_sync", false),
                ("after_quarantine_model_root_sync", false),
            ])
            .collect::<Vec<_>>();
        #[cfg(not(unix))]
        let crash_cases = rename_crash_cases.to_vec();

        for (crash_point, valid) in crash_cases {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("model-store");
            std::fs::create_dir(&root).unwrap();
            let catalog_path = temp.path().join("catalog.sqlite3");
            drop(Catalog::open(&catalog_path).unwrap());
            let store = ManagedModelStore::default_embedding(&root);
            let generation_id = ManagedGenerationId::generate();
            create_generation(store.models_dir(), STAGING_DIR, &generation_id, valid);

            let output = lifecycle_child_command(
                &std::env::current_exe().unwrap(),
                &root,
                &catalog_path,
                "recovery",
            )
            .env("ORBOK_RFC050_LIFECYCLE_CRASH_POINT", crash_point)
            .output()
            .unwrap();
            assert!(
                !output.status.success(),
                "lifecycle failpoint {crash_point} did not abort"
            );

            let catalog = Catalog::open(&catalog_path).unwrap();
            let outcome = run_managed_model_startup_with(
                &catalog,
                &store,
                DEFAULT_TRUSTED_MODEL.manifest_id,
                marker_validator,
            )
            .unwrap();
            let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
            let snapshot = ManagedGenerationRepository::new(&catalog)
                .load_shared(&guard)
                .unwrap();
            snapshot.validate().unwrap();

            if valid {
                assert_eq!(outcome.recovered_inactive, 1, "{crash_point}");
                assert_eq!(
                    snapshot.generations[&generation_id].state,
                    ManagedGenerationState::Inactive,
                    "{crash_point}"
                );
                assert!(
                    root.join(GENERATIONS_DIR)
                        .join(generation_id.as_str())
                        .is_dir(),
                    "{crash_point}"
                );
            } else {
                assert!(snapshot.generations.is_empty(), "{crash_point}");
                assert_eq!(
                    std::fs::read_dir(root.join(QUARANTINE_DIR))
                        .unwrap()
                        .count(),
                    1,
                    "{crash_point}"
                );
            }
            assert!(
                std::fs::read_dir(root.join(STAGING_DIR))
                    .unwrap()
                    .next()
                    .is_none(),
                "staging was not classified after {crash_point}"
            );
        }
    }

    #[test]
    fn later_startup_validation_is_durable_and_invalid_current_rolls_back() {
        let (_temp, catalog, store) = setup();
        let a = ManagedGenerationId::generate();
        let b = ManagedGenerationId::generate();
        create_generation(store.models_dir(), GENERATIONS_DIR, &a, true);
        create_generation(store.models_dir(), GENERATIONS_DIR, &b, false);
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, a.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository.activate(&guard, &a).unwrap();
        }

        let first = run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert_eq!(first.current_generation_id, Some(a.clone()));
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, b.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository.activate(&guard, &b).unwrap();
        }

        let second = run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert!(second.rolled_back);
        assert_eq!(second.current_generation_id, Some(a.clone()));
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(snapshot.profile.startup_epoch.get(), 2);
        assert_eq!(snapshot.profile.previous_generation_id, None);
        assert_eq!(
            snapshot.generations[&a].state,
            ManagedGenerationState::Current
        );
        assert_eq!(
            snapshot.generations[&b].state,
            ManagedGenerationState::Invalid
        );
    }

    #[test]
    fn two_invalid_generations_clear_both_pointers() {
        let (_temp, catalog, store) = setup();
        let a = ManagedGenerationId::generate();
        let b = ManagedGenerationId::generate();
        create_generation(store.models_dir(), GENERATIONS_DIR, &a, true);
        create_generation(store.models_dir(), GENERATIONS_DIR, &b, false);
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, a.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository.activate(&guard, &a).unwrap();
        }
        run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, b.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository.activate(&guard, &b).unwrap();
            std::fs::remove_file(
                store
                    .models_dir()
                    .join(GENERATIONS_DIR)
                    .join(a.as_str())
                    .join("VALID"),
            )
            .unwrap();
        }

        let outcome = run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert!(outcome.rolled_back);
        assert_eq!(outcome.current_generation_id, None);
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(snapshot.profile.previous_generation_id, None);
        assert_eq!(
            snapshot.generations[&a].state,
            ManagedGenerationState::Invalid
        );
        assert_eq!(
            snapshot.generations[&b].state,
            ManagedGenerationState::Invalid
        );
    }

    #[test]
    fn cleanup_rechecks_and_preserves_current_and_previous() {
        let (_temp, catalog, store) = setup();
        let a = ManagedGenerationId::generate();
        let b = ManagedGenerationId::generate();
        let inactive = ManagedGenerationId::generate();
        for id in [&a, &b, &inactive] {
            create_generation(store.models_dir(), GENERATIONS_DIR, id, true);
        }
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            for id in [&a, &b, &inactive] {
                repository
                    .register_inactive(&guard, id.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                    .unwrap();
            }
            repository.activate(&guard, &a).unwrap();
        }
        run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            ManagedGenerationRepository::new(&catalog)
                .activate(&guard, &b)
                .unwrap();
        }

        let outcome = cleanup_managed_model_generations(&catalog, &store).unwrap();
        assert_eq!(outcome.removed_generations, 1);
        assert!(
            store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(a.as_str())
                .exists()
        );
        assert!(
            store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(b.as_str())
                .exists()
        );
        assert!(
            !store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(inactive.as_str())
                .exists()
        );

        // Simulate a process dying after B's validation transaction but before
        // the separate previous-release transaction. Cleanup must over-retain A.
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            let later = repository.advance_startup_epoch(&guard).unwrap();
            let evidence = later.observe_current_for_startup_validation(&b).unwrap();
            repository
                .validate_current_after_startup(&guard, &evidence)
                .unwrap();
        }
        assert_eq!(
            cleanup_managed_model_generations(&catalog, &store)
                .unwrap()
                .removed_generations,
            0
        );
        assert!(
            store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(a.as_str())
                .exists()
        );

        // The next startup repeats validation safely, releases A atomically,
        // and makes only A cleanup-eligible while B remains current.
        run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert_eq!(
            cleanup_managed_model_generations(&catalog, &store)
                .unwrap()
                .removed_generations,
            1
        );
        assert!(
            !store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(a.as_str())
                .exists()
        );
        assert!(
            store
                .models_dir()
                .join(GENERATIONS_DIR)
                .join(b.as_str())
                .exists()
        );
    }

    #[test]
    fn separate_process_installer_recovery_rollback_and_cleanup_share_one_guard() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let catalog_path = temp.path().join("catalog.sqlite3");
        let catalog = Catalog::open(&catalog_path).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let invalid_current = ManagedGenerationId::generate();
        create_generation(store.models_dir(), GENERATIONS_DIR, &invalid_current, false);
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(
                    &guard,
                    invalid_current.clone(),
                    DEFAULT_TRUSTED_MODEL.manifest_id,
                )
                .unwrap();
            repository.activate(&guard, &invalid_current).unwrap();
        }
        drop(catalog);

        let ready = temp.path().join("installer-ready");
        let installer_id = ManagedGenerationId::generate();
        let executable = std::env::current_exe().unwrap();
        let mut installer = lifecycle_child_command(&executable, &root, &catalog_path, "installer")
            .env("ORBOK_RFC050_READY", &ready)
            .env("ORBOK_RFC050_INSTALLER_ID", installer_id.as_str())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !ready.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready.exists(), "installer child did not acquire the guard");

        let mut contenders = ["recovery", "rollback", "cleanup"].map(|action| {
            lifecycle_child_command(&executable, &root, &catalog_path, action)
                .spawn()
                .unwrap()
        });
        std::thread::sleep(Duration::from_millis(50));
        for contender in &mut contenders {
            assert!(
                contender.try_wait().unwrap().is_none(),
                "a lifecycle contender escaped the installer's exclusive guard"
            );
        }
        assert!(
            root.join(GENERATIONS_DIR)
                .join(installer_id.as_str())
                .is_dir(),
            "cleanup removed the promoted pre-registration generation"
        );
        assert!(installer.wait().unwrap().success());
        for contender in &mut contenders {
            assert!(contender.wait().unwrap().success());
        }

        let catalog = Catalog::open(&catalog_path).unwrap();
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        snapshot.validate().unwrap();
        assert_eq!(snapshot.profile.startup_epoch.get(), 2);
        assert_eq!(snapshot.profile.current_generation_id, None);
        assert_eq!(snapshot.profile.previous_generation_id, None);
        assert_eq!(
            snapshot.generations[&invalid_current].state,
            ManagedGenerationState::Invalid
        );
        assert_eq!(
            snapshot.generations[&installer_id].state,
            ManagedGenerationState::Inactive
        );
    }

    #[test]
    fn abrupt_exit_between_validation_and_previous_release_over_retains_until_restart() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let catalog_path = temp.path().join("catalog.sqlite3");
        let catalog = Catalog::open(&catalog_path).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let a = ManagedGenerationId::generate();
        let b = ManagedGenerationId::generate();
        create_generation(store.models_dir(), GENERATIONS_DIR, &a, true);
        create_generation(store.models_dir(), GENERATIONS_DIR, &b, true);
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, a.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository
                .register_inactive(&guard, b.clone(), DEFAULT_TRUSTED_MODEL.manifest_id)
                .unwrap();
            repository.activate(&guard, &a).unwrap();
        }
        run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            ManagedGenerationRepository::new(&catalog)
                .activate(&guard, &b)
                .unwrap();
        }
        drop(catalog);

        let output = lifecycle_child_command(
            &std::env::current_exe().unwrap(),
            &root,
            &catalog_path,
            "recovery",
        )
        .env(
            "ORBOK_RFC050_LIFECYCLE_CRASH_POINT",
            "after_current_validation_transaction",
        )
        .output()
        .unwrap();
        assert!(!output.status.success(), "lifecycle child did not abort");

        let catalog = Catalog::open(&catalog_path).unwrap();
        {
            let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
            let snapshot = ManagedGenerationRepository::new(&catalog)
                .load_shared(&guard)
                .unwrap();
            assert_eq!(snapshot.profile.current_generation_id, Some(b.clone()));
            assert_eq!(snapshot.profile.previous_generation_id, Some(a.clone()));
            assert!(snapshot.generations[&b].validated_startup_epoch.is_some());
        }
        assert_eq!(
            cleanup_managed_model_generations(&catalog, &store)
                .unwrap()
                .removed_generations,
            0
        );

        run_managed_model_startup_with(
            &catalog,
            &store,
            DEFAULT_TRUSTED_MODEL.manifest_id,
            marker_validator,
        )
        .unwrap();
        assert_eq!(
            cleanup_managed_model_generations(&catalog, &store)
                .unwrap()
                .removed_generations,
            1
        );
        assert!(!root.join(GENERATIONS_DIR).join(a.as_str()).exists());
        assert!(root.join(GENERATIONS_DIR).join(b.as_str()).exists());
    }

    fn lifecycle_child_command(
        executable: &Path,
        root: &Path,
        catalog_path: &Path,
        action: &str,
    ) -> Command {
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "model_lifecycle::tests::lifecycle_interleaving_child",
                "--ignored",
            ])
            .env("ORBOK_RFC050_LIFECYCLE_ACTION", action)
            .env("ORBOK_RFC050_TEST_STORE", root)
            .env("ORBOK_RFC050_TEST_CATALOG", catalog_path);
        command
    }

    #[test]
    #[ignore = "separate-process helper"]
    fn lifecycle_interleaving_child() {
        let action = std::env::var("ORBOK_RFC050_LIFECYCLE_ACTION").unwrap();
        let root = std::path::PathBuf::from(std::env::var_os("ORBOK_RFC050_TEST_STORE").unwrap());
        let catalog_path =
            std::path::PathBuf::from(std::env::var_os("ORBOK_RFC050_TEST_CATALOG").unwrap());
        let catalog = Catalog::open(catalog_path).unwrap();
        let store = ManagedModelStore::default_embedding(root);
        match action.as_str() {
            "installer" => {
                let guard = store.acquire_exclusive(Duration::from_secs(5)).unwrap();
                let id =
                    ManagedGenerationId::parse(std::env::var("ORBOK_RFC050_INSTALLER_ID").unwrap())
                        .unwrap();
                create_generation(store.models_dir(), GENERATIONS_DIR, &id, true);
                std::fs::write(std::env::var_os("ORBOK_RFC050_READY").unwrap(), b"ready").unwrap();
                std::thread::sleep(Duration::from_millis(250));
                ManagedGenerationRepository::new(&catalog)
                    .register_inactive(&guard, id, DEFAULT_TRUSTED_MODEL.manifest_id)
                    .unwrap();
            }
            "recovery" | "rollback" => {
                run_managed_model_startup_with(
                    &catalog,
                    &store,
                    DEFAULT_TRUSTED_MODEL.manifest_id,
                    marker_validator,
                )
                .unwrap();
            }
            "cleanup" => {
                cleanup_managed_model_generations(&catalog, &store).unwrap();
            }
            _ => panic!("unknown lifecycle action"),
        }
    }
}
