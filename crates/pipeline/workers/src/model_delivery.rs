//! Serialized trusted model-generation delivery (RFC-050 Phase 2).

use crate::model_durability::{ModelDurabilityError, durable_rename, preflight_managed_store};
use futures::{SinkExt as _, StreamExt as _};
use orbok_db::Catalog;
use orbok_db::repo::ManagedGenerationRepository;
use orbok_models::{
    DEFAULT_TRUSTED_MODEL, DownloadAction, DownloadPlan, ManagedGenerationId,
    ManagedGenerationSnapshot, ManagedModelStore, ModelReadiness, ModelStoreLockError,
    TrustedModelManifest, build_download_plan, check_app_managed_model_readiness,
    validate_initial_url, validate_redirect_url,
};
use sha2::Digest as _;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const STAGING_DIR: &str = ".staging";
const GENERATIONS_DIR: &str = "generations";
const TRUSTED_MANIFEST_FILE: &str = "trusted-manifest.json";
const COMPLETE_FILE: &str = "COMPLETE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDeliveryEvent {
    FileProgress {
        logical_name: &'static str,
        bytes: u64,
        total: u64,
        files_done: u32,
        files_total: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDeliveryOutcome {
    pub generation_id: ManagedGenerationId,
    pub generation_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelDeliveryError {
    #[error("the model store is unavailable")]
    StoreUnavailable,
    #[error("the model store is busy")]
    StoreBusy,
    #[error("trusted model policy validation failed")]
    TrustPolicy,
    #[error("model readiness could not be planned")]
    Plan,
    #[error("model download failed")]
    Network,
    #[error("downloaded model data exceeded its trusted limit")]
    TransferLimit,
    #[error("downloaded model data did not match trusted metadata")]
    Integrity,
    #[error("model files could not be written safely")]
    Filesystem,
    #[error("model generation catalog update failed")]
    Catalog,
    #[error("the active model generation could not be confirmed")]
    FinalCheck,
}

/// Install or repair the reviewed default model as one immutable generation.
///
/// The model-store root must already exist. The exclusive guard is acquired
/// before readiness, filesystem mutation, or catalog access and remains held
/// through final active-generation confirmation.
pub async fn install_default_model(
    catalog: &Catalog,
    store: &ManagedModelStore,
    events: futures::channel::mpsc::Sender<ModelDeliveryEvent>,
) -> Result<ModelDeliveryOutcome, ModelDeliveryError> {
    DEFAULT_TRUSTED_MODEL
        .validate()
        .map_err(|_| ModelDeliveryError::TrustPolicy)?;
    preflight_managed_store(store.models_dir()).map_err(map_durability_store_error)?;
    let guard = store
        .acquire_exclusive(LOCK_TIMEOUT)
        .map_err(map_lock_error)?;
    let repository = ManagedGenerationRepository::new(catalog);
    let snapshot = repository
        .load_exclusive(&guard)
        .map_err(|_| ModelDeliveryError::Catalog)?;
    let source_dir = snapshot
        .profile
        .current_generation_id
        .as_ref()
        .map(|id| store.models_dir().join(GENERATIONS_DIR).join(id.as_str()))
        .unwrap_or_else(|| store.models_dir().to_path_buf());
    let report = check_app_managed_model_readiness(&source_dir);
    let plan = build_download_plan(&report).map_err(|_| ModelDeliveryError::Plan)?;

    if report.overall() == ModelReadiness::Ready {
        if let Some(current_id) = snapshot.profile.current_generation_id.clone() {
            return verify_ready_current(
                &snapshot,
                current_id,
                source_dir,
                &plan,
                &DEFAULT_TRUSTED_MODEL,
            )
            .await;
        }
    }

    let client = production_client(&DEFAULT_TRUSTED_MODEL)?;
    execute_generation(
        store,
        &guard,
        &repository,
        &source_dir,
        &plan,
        &DEFAULT_TRUSTED_MODEL,
        &client,
        events,
        |_| {},
        |_| {},
    )
    .await
}

async fn verify_ready_current(
    snapshot: &ManagedGenerationSnapshot,
    generation_id: ManagedGenerationId,
    generation_dir: PathBuf,
    plan: &DownloadPlan,
    manifest: &TrustedModelManifest,
) -> Result<ModelDeliveryOutcome, ModelDeliveryError> {
    let record = snapshot
        .generations
        .get(&generation_id)
        .ok_or(ModelDeliveryError::Integrity)?;
    verify_generation_validity(&generation_dir, plan, manifest, &record.manifest_id).await?;
    Ok(ModelDeliveryOutcome {
        generation_id,
        generation_dir,
    })
}

#[allow(clippy::too_many_arguments)]
async fn execute_generation<B, A>(
    store: &ManagedModelStore,
    guard: &orbok_models::ModelStoreMutationGuard<orbok_models::ExclusiveAccess>,
    repository: &ManagedGenerationRepository<'_>,
    source_dir: &Path,
    plan: &DownloadPlan,
    manifest: &'static TrustedModelManifest,
    client: &reqwest::Client,
    events: futures::channel::mpsc::Sender<ModelDeliveryEvent>,
    before_promotion: B,
    after_activation: A,
) -> Result<ModelDeliveryOutcome, ModelDeliveryError>
where
    B: FnOnce(&Path),
    A: FnOnce(&Path),
{
    if plan.manifest_id != manifest.manifest_id
        || plan.max_concurrent == 0
        || plan.max_concurrent > 2
    {
        return Err(ModelDeliveryError::Plan);
    }
    preflight_managed_store(store.models_dir()).map_err(map_durability_store_error)?;
    let generation_id = ManagedGenerationId::generate();
    let staging_parent = store.models_dir().join(STAGING_DIR);
    let generations_parent = store.models_dir().join(GENERATIONS_DIR);
    let staging = staging_parent.join(generation_id.as_str());
    let promoted = generations_parent.join(generation_id.as_str());

    std::fs::create_dir_all(&staging_parent).map_err(|_| ModelDeliveryError::Filesystem)?;
    std::fs::create_dir_all(&generations_parent).map_err(|_| ModelDeliveryError::Filesystem)?;
    std::fs::create_dir(&staging).map_err(|_| ModelDeliveryError::Filesystem)?;
    #[cfg(unix)]
    {
        sync_directory(&staging_parent)?;
        sync_directory(&generations_parent)?;
        sync_directory(store.models_dir())?;
    }
    #[cfg(not(any(unix, windows)))]
    return Err(ModelDeliveryError::Filesystem);

    let result = stage_files(
        store.models_dir(),
        source_dir,
        &staging,
        plan,
        client,
        events,
    )
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    verify_payload_files(&staging, plan).await?;
    #[cfg(unix)]
    sync_staged_tree(&staging, plan)?;
    #[cfg(not(any(unix, windows)))]
    return Err(ModelDeliveryError::Filesystem);
    write_metadata(&staging, manifest)?;
    verify_generation_validity(&staging, plan, manifest, manifest.manifest_id).await?;

    before_promotion(&promoted);
    crash_point("before_generation_promotion_durable_rename");
    durable_rename_async(store.models_dir(), &staging, &promoted).await?;
    crash_point("after_generation_promotion_durable_rename");
    #[cfg(unix)]
    {
        crash_point("before_staging_parent_sync");
        sync_directory(&staging_parent)?;
        crash_point("after_staging_parent_sync");
        crash_point("before_generations_parent_sync");
        sync_directory(&generations_parent)?;
        crash_point("after_generations_parent_sync");
        crash_point("before_model_root_sync");
        sync_directory(store.models_dir())?;
        crash_point("after_model_root_sync");
    }
    verify_generation_validity(&promoted, plan, manifest, manifest.manifest_id).await?;

    crash_point("before_inactive_registration_transaction");
    repository
        .register_inactive(guard, generation_id.clone(), manifest.manifest_id)
        .map_err(|_| ModelDeliveryError::Catalog)?;
    crash_point("after_inactive_registration_transaction");
    crash_point("before_activation_transaction");
    repository
        .activate(guard, &generation_id)
        .map_err(|_| ModelDeliveryError::Catalog)?;
    crash_point("after_activation_transaction");
    after_activation(&promoted);
    let final_check =
        confirm_active_generation(repository, guard, &generation_id, &promoted, plan, manifest)
            .await;
    if final_check.is_err() {
        let previous_verified =
            verify_previous_for_rollback(store, repository, guard, plan, manifest).await;
        repository
            .rollback_invalid_current(guard, previous_verified)
            .map_err(|_| ModelDeliveryError::Catalog)?;
        return Err(ModelDeliveryError::FinalCheck);
    }

    Ok(ModelDeliveryOutcome {
        generation_id,
        generation_dir: promoted,
    })
}

async fn verify_previous_for_rollback(
    store: &ManagedModelStore,
    repository: &ManagedGenerationRepository<'_>,
    guard: &orbok_models::ModelStoreMutationGuard<orbok_models::ExclusiveAccess>,
    plan: &DownloadPlan,
    manifest: &TrustedModelManifest,
) -> bool {
    let Ok(snapshot) = repository.load_exclusive(guard) else {
        return false;
    };
    let Some(previous_id) = snapshot.profile.previous_generation_id else {
        return false;
    };
    let Some(record) = snapshot.generations.get(&previous_id) else {
        return false;
    };
    let previous_dir = store
        .models_dir()
        .join(GENERATIONS_DIR)
        .join(previous_id.as_str());
    verify_generation_validity(&previous_dir, plan, manifest, &record.manifest_id)
        .await
        .is_ok()
}

async fn stage_files(
    managed_root: &Path,
    source_dir: &Path,
    staging: &Path,
    plan: &DownloadPlan,
    client: &reqwest::Client,
    events: futures::channel::mpsc::Sender<ModelDeliveryEvent>,
) -> Result<(), ModelDeliveryError> {
    for file in plan
        .files
        .iter()
        .filter(|file| file.action == DownloadAction::Skip)
    {
        let destination = file.final_path(staging);
        create_parent(&destination)?;
        let part = file.temp_path(staging);
        tokio::fs::copy(file.final_path(source_dir), &part)
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
        crash_point(&format!("before_file_sync:{}", file.logical_name));
        let part_file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&part)
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
        part_file
            .sync_all()
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
        close_tokio_file(part_file).await;
        crash_point(&format!("after_file_sync:{}", file.logical_name));
        crash_point(&format!(
            "before_staged_file_durable_rename:{}",
            file.logical_name
        ));
        durable_rename_async(managed_root, &part, &destination).await?;
        crash_point(&format!(
            "after_staged_file_durable_rename:{}",
            file.logical_name
        ));
    }

    let files_total = plan.files.len() as u32;
    let downloads = plan
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.action.requires_download())
        .map(|(index, file)| (index, file.clone()))
        .collect::<Vec<_>>();
    let staging = staging.to_path_buf();
    let managed_root = managed_root.to_path_buf();
    let client = client.clone();
    let mut downloads = downloads.into_iter().map(|(index, file)| {
        let mut tx = events.clone();
        let staging = staging.clone();
        let managed_root = managed_root.clone();
        let client = client.clone();
        async move {
            let destination = file.final_path(&staging);
            create_parent(&destination)?;
            download_file(
                &client,
                &file,
                &managed_root,
                &staging,
                index as u32,
                files_total,
                &mut tx,
            )
            .await
        }
    });
    let mut in_flight = futures::stream::FuturesUnordered::new();
    for _ in 0..plan.max_concurrent {
        if let Some(download) = downloads.next() {
            in_flight.push(download);
        }
    }
    let mut first_error = None;
    while let Some(result) = in_flight.next().await {
        match result {
            Ok(()) if first_error.is_none() => {
                if let Some(download) = downloads.next() {
                    in_flight.push(download);
                }
            }
            Ok(()) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(())
}

async fn download_file(
    client: &reqwest::Client,
    file: &orbok_models::ModelFilePlan,
    managed_root: &Path,
    staging: &Path,
    files_done: u32,
    files_total: u32,
    events: &mut futures::channel::mpsc::Sender<ModelDeliveryEvent>,
) -> Result<(), ModelDeliveryError> {
    let response = client
        .get(file.remote_url)
        .send()
        .await
        .map_err(|_| ModelDeliveryError::Network)?;
    if !response.status().is_success() {
        return Err(ModelDeliveryError::Network);
    }
    if response
        .content_length()
        .is_some_and(|length| length != file.exact_size_bytes)
    {
        return Err(ModelDeliveryError::Integrity);
    }

    let part = file.temp_path(staging);
    let final_path = file.final_path(staging);
    let mut output = tokio::fs::File::create(&part)
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ModelDeliveryError::Network)?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or(ModelDeliveryError::TransferLimit)?;
        if downloaded > file.max_transfer_bytes || downloaded > file.exact_size_bytes {
            return Err(ModelDeliveryError::TransferLimit);
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
        let _ = events
            .send(ModelDeliveryEvent::FileProgress {
                logical_name: file.logical_name,
                bytes: downloaded,
                total: file.exact_size_bytes,
                files_done,
                files_total,
            })
            .await;
    }
    crash_point(&format!("before_file_sync:{}", file.logical_name));
    output
        .flush()
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    output
        .sync_all()
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    close_tokio_file(output).await;
    crash_point(&format!("after_file_sync:{}", file.logical_name));
    if downloaded != file.exact_size_bytes || sha256_file(&part).await? != file.expected_sha256 {
        return Err(ModelDeliveryError::Integrity);
    }
    crash_point(&format!(
        "before_staged_file_durable_rename:{}",
        file.logical_name
    ));
    durable_rename_async(managed_root, &part, &final_path).await?;
    crash_point(&format!(
        "after_staged_file_durable_rename:{}",
        file.logical_name
    ));
    Ok(())
}

async fn durable_rename_async(
    managed_root: &Path,
    source: &Path,
    destination: &Path,
) -> Result<(), ModelDeliveryError> {
    let managed_root = managed_root.to_path_buf();
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || durable_rename(&managed_root, &source, &destination))
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?
        .map_err(map_durability_filesystem_error)
}

async fn close_tokio_file(file: tokio::fs::File) {
    drop(file.into_std().await);
}

async fn verify_payload_files(
    generation_dir: &Path,
    plan: &DownloadPlan,
) -> Result<(), ModelDeliveryError> {
    for file in &plan.files {
        let path = file.final_path(generation_dir);
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| ModelDeliveryError::Integrity)?;
        if !metadata.file_type().is_file()
            || metadata.len() != file.exact_size_bytes
            || sha256_file(&path).await? != file.expected_sha256
        {
            return Err(ModelDeliveryError::Integrity);
        }
    }
    Ok(())
}

async fn verify_generation_validity(
    generation_dir: &Path,
    plan: &DownloadPlan,
    manifest: &TrustedModelManifest,
    catalog_manifest_id: &str,
) -> Result<(), ModelDeliveryError> {
    if plan.manifest_id != manifest.manifest_id || catalog_manifest_id != manifest.manifest_id {
        return Err(ModelDeliveryError::Integrity);
    }
    verify_payload_files(generation_dir, plan).await?;
    verify_generation_metadata(generation_dir, manifest)
}

async fn confirm_active_generation(
    repository: &ManagedGenerationRepository<'_>,
    guard: &orbok_models::ModelStoreMutationGuard<orbok_models::ExclusiveAccess>,
    generation_id: &ManagedGenerationId,
    generation_dir: &Path,
    plan: &DownloadPlan,
    manifest: &TrustedModelManifest,
) -> Result<(), ModelDeliveryError> {
    let final_snapshot = repository
        .load_exclusive(guard)
        .map_err(|_| ModelDeliveryError::FinalCheck)?;
    if final_snapshot.profile.current_generation_id.as_ref() != Some(generation_id) {
        return Err(ModelDeliveryError::FinalCheck);
    }
    let record = final_snapshot
        .generations
        .get(generation_id)
        .ok_or(ModelDeliveryError::FinalCheck)?;
    verify_generation_validity(generation_dir, plan, manifest, &record.manifest_id)
        .await
        .map_err(|_| ModelDeliveryError::FinalCheck)
}

async fn sha256_file(path: &Path) -> Result<String, ModelDeliveryError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ModelDeliveryError::Integrity)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|_| ModelDeliveryError::Integrity)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    close_tokio_file(file).await;
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}

fn production_client(
    manifest: &'static TrustedModelManifest,
) -> Result<reqwest::Client, ModelDeliveryError> {
    for file in manifest.files {
        validate_initial_url(manifest, file, file.url)
            .map_err(|_| ModelDeliveryError::TrustPolicy)?;
    }
    base_client_builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let redirect_number = u8::try_from(attempt.previous().len()).unwrap_or(u8::MAX);
            if validate_redirect_url(manifest, attempt.url().as_str(), redirect_number).is_ok() {
                attempt.follow()
            } else {
                attempt.error("redirect rejected by trusted model policy")
            }
        }))
        .build()
        .map_err(|_| ModelDeliveryError::TrustPolicy)
}

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .referer(false)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
}

fn create_parent(path: &Path) -> Result<(), ModelDeliveryError> {
    let parent = path.parent().ok_or(ModelDeliveryError::Filesystem)?;
    std::fs::create_dir_all(parent).map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(unix)]
fn sync_staged_tree(staging: &Path, plan: &DownloadPlan) -> Result<(), ModelDeliveryError> {
    let mut parents = plan
        .files
        .iter()
        .filter_map(|file| file.final_path(staging).parent().map(Path::to_path_buf))
        .filter(|parent| parent != staging)
        .collect::<Vec<_>>();
    parents.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    parents.dedup();
    for parent in parents {
        crash_point("before_nested_directory_sync");
        sync_directory(&parent)?;
        crash_point("after_nested_directory_sync");
    }
    sync_directory(staging)
}

fn write_metadata(
    staging: &Path,
    manifest: &TrustedModelManifest,
) -> Result<(), ModelDeliveryError> {
    let manifest_file = staging.join(TRUSTED_MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| ModelDeliveryError::Filesystem)?;
    write_and_sync(&manifest_file, &bytes)?;
    write_and_sync(&staging.join(COMPLETE_FILE), b"complete\n")?;
    #[cfg(unix)]
    sync_directory(staging)?;
    #[cfg(not(any(unix, windows)))]
    return Err(ModelDeliveryError::Filesystem);
    Ok(())
}

fn verify_generation_metadata(
    generation_dir: &Path,
    manifest: &TrustedModelManifest,
) -> Result<(), ModelDeliveryError> {
    let expected =
        serde_json::to_vec_pretty(manifest).map_err(|_| ModelDeliveryError::Integrity)?;
    let actual = read_regular_file(&generation_dir.join(TRUSTED_MANIFEST_FILE))?;
    let complete = read_regular_file(&generation_dir.join(COMPLETE_FILE))?;
    if actual != expected || complete != b"complete\n" {
        return Err(ModelDeliveryError::Integrity);
    }
    Ok(())
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, ModelDeliveryError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ModelDeliveryError::Integrity)?;
    if !metadata.file_type().is_file() {
        return Err(ModelDeliveryError::Integrity);
    }
    std::fs::read(path).map_err(|_| ModelDeliveryError::Integrity)
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> Result<(), ModelDeliveryError> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path).map_err(|_| ModelDeliveryError::Filesystem)?;
    file.write_all(bytes)
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    file.sync_all().map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<(), ModelDeliveryError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), ModelDeliveryError> {
    Err(ModelDeliveryError::Filesystem)
}

fn map_lock_error(error: ModelStoreLockError) -> ModelDeliveryError {
    match error {
        ModelStoreLockError::Timeout => ModelDeliveryError::StoreBusy,
        ModelStoreLockError::Io(_) | ModelStoreLockError::UnsupportedTarget => {
            ModelDeliveryError::StoreUnavailable
        }
    }
}

fn map_durability_store_error(error: ModelDurabilityError) -> ModelDeliveryError {
    tracing::warn!(durability_error = %error, "managed model durability preflight failed");
    ModelDeliveryError::StoreUnavailable
}

fn map_durability_filesystem_error(error: ModelDurabilityError) -> ModelDeliveryError {
    #[cfg(test)]
    eprintln!("managed model durability operation failed: {error}");
    tracing::warn!(durability_error = %error, "managed model durability operation failed");
    ModelDeliveryError::Filesystem
}

#[cfg(test)]
fn crash_point(point: &str) {
    if std::env::var("ORBOK_RFC050_CRASH_POINT").as_deref() == Ok(point) {
        std::process::abort();
    }
}

#[cfg(not(test))]
fn crash_point(_point: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_lifecycle::run_managed_model_startup_with;
    use orbok_models::{
        DownloadAction, LocalFileStatus, ManagedGenerationState, ModelFilePlan, TrustedModelFile,
        TrustedModelIdentity, TrustedTransportPolicy,
    };
    use std::collections::HashMap;
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn synced_tokio_file_is_closed_before_durable_rename() {
        let temp = tempfile::tempdir().unwrap();
        let generation = temp.path().join(STAGING_DIR).join("generation");
        let part = generation.join("onnx/model.onnx.part");
        let destination = generation.join("onnx/model.onnx");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        let mut output = tokio::fs::File::create(&part).await.unwrap();
        output.write_all(b"durable-payload").await.unwrap();
        output.flush().await.unwrap();
        output.sync_all().await.unwrap();
        close_tokio_file(output).await;

        assert_eq!(
            sha256_file(&part).await.unwrap(),
            digest(b"durable-payload")
        );
        durable_rename_async(temp.path(), &part, &destination)
            .await
            .unwrap();

        assert!(!part.exists());
        assert_eq!(std::fs::read(destination).unwrap(), b"durable-payload");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mock_server_install_promotes_complete_generation_and_activates_it() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server.base_url, tokenizer.clone(), model.clone(), None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, receiver) = futures::channel::mpsc::channel(16);

        let outcome = execute_generation(
            &store,
            &guard,
            &repository,
            &root,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |_| {},
        )
        .await
        .unwrap();
        let progress = receiver.collect::<Vec<_>>().await;
        let max_active = server.max_active.clone();
        let requests = server.requests.clone();
        server.finish().await;

        assert_eq!(max_active.load(Ordering::SeqCst), 2);
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        assert!(!progress.is_empty());
        assert!(progress.iter().all(|event| matches!(
            event,
            ModelDeliveryEvent::FileProgress {
                bytes,
                total,
                files_total: 2,
                ..
            } if *bytes > 0 && *bytes == *total
        )));
        assert!(progress.iter().any(|event| matches!(
            event,
            ModelDeliveryEvent::FileProgress {
                logical_name: "tokenizer",
                ..
            }
        )));
        assert!(progress.iter().any(|event| matches!(
            event,
            ModelDeliveryEvent::FileProgress {
                logical_name: "onnx-model",
                ..
            }
        )));
        assert_eq!(
            outcome.generation_dir,
            root.join(GENERATIONS_DIR)
                .join(outcome.generation_id.as_str())
        );
        assert_eq!(
            std::fs::read(outcome.generation_dir.join("tokenizer.json")).unwrap(),
            tokenizer
        );
        assert_eq!(
            std::fs::read(outcome.generation_dir.join("onnx/model.onnx")).unwrap(),
            model
        );
        assert_eq!(
            std::fs::read(outcome.generation_dir.join(COMPLETE_FILE)).unwrap(),
            b"complete\n"
        );
        assert_eq!(
            std::fs::read(outcome.generation_dir.join(TRUSTED_MANIFEST_FILE)).unwrap(),
            serde_json::to_vec_pretty(fixture.manifest).unwrap()
        );
        for file in fixture.manifest.files {
            let bytes = std::fs::read(outcome.generation_dir.join(file.relative_path)).unwrap();
            assert_eq!(digest(&bytes), file.sha256);
        }
        assert_no_part_files(&outcome.generation_dir);
        assert_eq!(
            repository
                .load_exclusive(&guard)
                .unwrap()
                .profile
                .current_generation_id,
            Some(outcome.generation_id.clone())
        );

        let generation_id = outcome.generation_id.clone();
        let generation_dir = outcome.generation_dir.clone();
        drop(guard);
        let startup = run_managed_model_startup_with(
            &catalog,
            &store,
            fixture.manifest.manifest_id,
            |path| {
                fixture_generation_matches(
                    path,
                    &generation_dir,
                    fixture.manifest,
                    &tokenizer,
                    &model,
                )
            },
        )
        .unwrap();
        assert_eq!(startup.current_generation_id, Some(generation_id.clone()));
        assert!(!startup.rolled_back);
        assert!(startup.startup_epoch > 0);
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(snapshot.profile.current_generation_id, Some(generation_id));
        assert!(snapshot.profile.previous_generation_id.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn checksum_failure_never_promotes_or_activates() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let corrupt_model = b"corrupt-onnx-model".to_vec();
        assert_eq!(model.len(), corrupt_model.len());
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", corrupt_model)]).await;
        let fixture = fixture(&server.base_url, tokenizer.clone(), model.clone(), None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let prior = ManagedGenerationId::generate();
        let prior_dir = root.join(GENERATIONS_DIR).join(prior.as_str());
        write_fixture_generation(&prior_dir, fixture.manifest, &tokenizer, &model);
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(&guard, prior.clone(), fixture.manifest.manifest_id)
                .unwrap();
            repository.activate(&guard, &prior).unwrap();
        }
        let startup = run_managed_model_startup_with(
            &catalog,
            &store,
            fixture.manifest.manifest_id,
            |path| {
                fixture_generation_matches(path, &prior_dir, fixture.manifest, &tokenizer, &model)
            },
        )
        .unwrap();
        assert_eq!(startup.current_generation_id, Some(prior.clone()));
        assert!(!startup.rolled_back);
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = execute_generation(
            &store,
            &guard,
            &repository,
            &prior_dir,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |_| {},
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::Integrity)));
        let snapshot = repository.load_exclusive(&guard).unwrap();
        assert_eq!(snapshot.profile.current_generation_id, Some(prior.clone()));
        assert!(snapshot.profile.previous_generation_id.is_none());
        assert_eq!(snapshot.generations.len(), 1);
        assert_eq!(
            snapshot.generations[&prior].state,
            ManagedGenerationState::Current
        );
        let generation_entries = std::fs::read_dir(root.join(GENERATIONS_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            generation_entries,
            vec![std::ffi::OsString::from(prior.as_str())]
        );
        assert_eq!(
            std::fs::read_dir(root.join(STAGING_DIR)).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn concurrent_failure_drains_started_transfer_before_cleanup() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let expected_model = b"trusted-onnx-model".to_vec();
        let server = MockServer::start_delayed([
            ("/tokenizer", tokenizer.clone(), Duration::from_millis(250)),
            ("/model", b"short".to_vec(), Duration::ZERO),
        ])
        .await;
        let fixture = fixture(&server.base_url, tokenizer.clone(), expected_model, None);
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = stage_files(
            temp.path(),
            temp.path(),
            &staging,
            &fixture.plan,
            &base_client_builder().build().unwrap(),
            events,
        )
        .await;

        assert!(matches!(result, Err(ModelDeliveryError::Integrity)));
        assert_eq!(
            std::fs::read(staging.join("tokenizer.json")).unwrap(),
            tokenizer
        );
        server.finish().await;
    }

    #[tokio::test]
    async fn mock_server_finish_tolerates_a_cancelled_pending_request() {
        let server = MockServer::start([
            ("/completed", b"completed".to_vec()),
            ("/cancelled", b"cancelled".to_vec()),
        ])
        .await;

        let response = base_client_builder()
            .build()
            .unwrap()
            .get(format!("{}/completed", server.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(response.bytes().await.unwrap(), b"completed".as_slice());

        tokio::time::timeout(Duration::from_secs(1), server.finish())
            .await
            .expect("mock server shutdown must not wait for a cancelled request");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn promotion_rename_failure_never_registers_or_activates_generation() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server.base_url, tokenizer, model, None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = execute_generation(
            &store,
            &guard,
            &repository,
            &root,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |promoted| {
                std::fs::create_dir(promoted).unwrap();
                std::fs::write(promoted.join("collision"), b"occupied").unwrap();
            },
            |_| {},
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::Filesystem)));
        let snapshot = repository.load_exclusive(&guard).unwrap();
        assert!(snapshot.profile.current_generation_id.is_none());
        assert!(snapshot.generations.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn trusted_skip_is_copied_without_a_network_request() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server = MockServer::start([("/model", model.clone())]).await;
        let mut fixture = fixture(&server.base_url, tokenizer.clone(), model, None);
        fixture.plan.files[0].action = DownloadAction::Skip;
        fixture.plan.files[0].local_status = LocalFileStatus::Ready;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("tokenizer.json"), tokenizer).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let outcome = execute_generation(
            &store,
            &guard,
            &repository,
            &root,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |_| {},
        )
        .await
        .unwrap();
        server.finish().await;

        assert_eq!(
            std::fs::read(outcome.generation_dir.join("tokenizer.json")).unwrap(),
            b"trusted-tokenizer"
        );
        assert!(
            !fixture.plan.files[0]
                .temp_path(&outcome.generation_dir)
                .exists()
        );
    }

    #[tokio::test]
    async fn ready_current_rejects_missing_complete_marker_and_corrupt_manifest() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let fixture = fixture("http://127.0.0.1:1", tokenizer.clone(), model.clone(), None);
        let temp = tempfile::tempdir().unwrap();
        let generation_dir = temp.path().join("generation");
        write_fixture_generation(&generation_dir, fixture.manifest, &tokenizer, &model);
        let generation_id = ManagedGenerationId::generate();
        let snapshot = ManagedGenerationSnapshot::empty(
            orbok_models::ModelStoreProfileId::default_embedding(),
        )
        .register_inactive(generation_id.clone(), fixture.manifest.manifest_id)
        .unwrap()
        .activate(&generation_id)
        .unwrap();

        std::fs::remove_file(generation_dir.join(COMPLETE_FILE)).unwrap();
        let missing_marker = verify_ready_current(
            &snapshot,
            generation_id.clone(),
            generation_dir.clone(),
            &fixture.plan,
            fixture.manifest,
        )
        .await;
        assert!(matches!(missing_marker, Err(ModelDeliveryError::Integrity)));

        write_and_sync(&generation_dir.join(COMPLETE_FILE), b"complete\n").unwrap();
        std::fs::write(generation_dir.join(TRUSTED_MANIFEST_FILE), b"{}").unwrap();
        let corrupt_manifest = verify_ready_current(
            &snapshot,
            generation_id.clone(),
            generation_dir.clone(),
            &fixture.plan,
            fixture.manifest,
        )
        .await;
        assert!(matches!(
            corrupt_manifest,
            Err(ModelDeliveryError::Integrity)
        ));

        write_metadata(&generation_dir, fixture.manifest).unwrap();
        let wrong_catalog_identity = ManagedGenerationSnapshot::empty(
            orbok_models::ModelStoreProfileId::default_embedding(),
        )
        .register_inactive(generation_id.clone(), "different-manifest")
        .unwrap()
        .activate(&generation_id)
        .unwrap();
        let identity_mismatch = verify_ready_current(
            &wrong_catalog_identity,
            generation_id,
            generation_dir,
            &fixture.plan,
            fixture.manifest,
        )
        .await;
        assert!(matches!(
            identity_mismatch,
            Err(ModelDeliveryError::Integrity)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn post_commit_validation_failure_restores_prior_current_and_marks_new_invalid() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server.base_url, tokenizer.clone(), model.clone(), None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let prior = ManagedGenerationId::generate();
        let prior_dir = root.join(GENERATIONS_DIR).join(prior.as_str());
        write_fixture_generation(&prior_dir, fixture.manifest, &tokenizer, &model);
        repository
            .register_inactive(&guard, prior.clone(), fixture.manifest.manifest_id)
            .unwrap();
        repository.activate(&guard, &prior).unwrap();
        repository.advance_startup_epoch(&guard).unwrap();
        let observed = repository.load_exclusive(&guard).unwrap();
        let evidence = observed
            .observe_current_for_startup_validation(&prior)
            .unwrap();
        repository
            .validate_current_after_startup(&guard, &evidence)
            .unwrap();
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = execute_generation(
            &store,
            &guard,
            &repository,
            &prior_dir,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |promoted| std::fs::remove_file(promoted.join(COMPLETE_FILE)).unwrap(),
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::FinalCheck)));
        let final_snapshot = repository.load_exclusive(&guard).unwrap();
        assert_eq!(final_snapshot.profile.current_generation_id, Some(prior));
        assert!(final_snapshot.profile.previous_generation_id.is_none());
        assert_eq!(
            final_snapshot
                .generations
                .values()
                .filter(|record| record.state == ManagedGenerationState::Invalid)
                .count(),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn corrupt_repair_predecessor_is_not_restored_after_post_commit_failure() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server = MockServer::start([("/tokenizer", tokenizer.clone())]).await;
        let mut fixture = fixture(&server.base_url, tokenizer.clone(), model.clone(), None);
        fixture.plan.files[0].action = DownloadAction::Replace;
        fixture.plan.files[0].local_status = LocalFileStatus::Invalid;
        fixture.plan.files[1].action = DownloadAction::Skip;
        fixture.plan.files[1].local_status = LocalFileStatus::Ready;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let prior = ManagedGenerationId::generate();
        let prior_dir = root.join(GENERATIONS_DIR).join(prior.as_str());
        write_fixture_generation(&prior_dir, fixture.manifest, &tokenizer, &model);
        std::fs::write(prior_dir.join("tokenizer.json"), b"corrupt-tokenizer").unwrap();
        repository
            .register_inactive(&guard, prior.clone(), fixture.manifest.manifest_id)
            .unwrap();
        repository.activate(&guard, &prior).unwrap();
        repository.advance_startup_epoch(&guard).unwrap();
        let observed = repository.load_exclusive(&guard).unwrap();
        let evidence = observed
            .observe_current_for_startup_validation(&prior)
            .unwrap();
        repository
            .validate_current_after_startup(&guard, &evidence)
            .unwrap();
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = execute_generation(
            &store,
            &guard,
            &repository,
            &prior_dir,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |promoted| std::fs::remove_file(promoted.join(COMPLETE_FILE)).unwrap(),
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::FinalCheck)));
        let final_snapshot = repository.load_exclusive(&guard).unwrap();
        assert!(final_snapshot.profile.current_generation_id.is_none());
        assert!(final_snapshot.profile.previous_generation_id.is_none());
        assert_eq!(
            final_snapshot
                .generations
                .values()
                .filter(|record| record.state == ManagedGenerationState::Invalid)
                .count(),
            2
        );
        assert_eq!(
            final_snapshot.generations.get(&prior).unwrap().state,
            ManagedGenerationState::Invalid
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn post_commit_manifest_corruption_clears_initial_activation_and_marks_it_invalid() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server.base_url, tokenizer, model, None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open_in_memory().unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, _receiver) = futures::channel::mpsc::channel(16);

        let result = execute_generation(
            &store,
            &guard,
            &repository,
            &root,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |promoted| {
                std::fs::write(promoted.join(TRUSTED_MANIFEST_FILE), b"{}").unwrap();
            },
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::FinalCheck)));
        let final_snapshot = repository.load_exclusive(&guard).unwrap();
        assert!(final_snapshot.profile.current_generation_id.is_none());
        assert!(final_snapshot.profile.previous_generation_id.is_none());
        assert_eq!(
            final_snapshot
                .generations
                .values()
                .filter(|record| record.state == ManagedGenerationState::Invalid)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn exact_size_header_mismatch_is_rejected_before_part_creation() {
        let body = b"trusted-bytes";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len() + 1
        )
        .into_bytes();
        let (url, server) = RawServer::start(vec![(Duration::ZERO, response)]).await;
        let temp = tempfile::tempdir().unwrap();
        let file = test_file_plan(&url, body, body.len() as u64);
        let (mut events, _receiver) = futures::channel::mpsc::channel(4);

        let result = download_file(
            &base_client_builder().build().unwrap(),
            &file,
            temp.path(),
            temp.path(),
            0,
            1,
            &mut events,
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::Integrity)));
        assert!(!file.temp_path(temp.path()).exists());
    }

    #[tokio::test]
    async fn omitted_content_length_streams_and_verifies_exact_bytes() {
        let body = b"trusted-bytes";
        let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        response.extend_from_slice(body);
        let (url, server) = RawServer::start(vec![(Duration::ZERO, response)]).await;
        let temp = tempfile::tempdir().unwrap();
        let file = test_file_plan(&url, body, body.len() as u64);
        let (mut events, _receiver) = futures::channel::mpsc::channel(4);

        download_file(
            &base_client_builder().build().unwrap(),
            &file,
            temp.path(),
            temp.path(),
            0,
            1,
            &mut events,
        )
        .await
        .unwrap();
        server.finish().await;

        assert_eq!(std::fs::read(file.final_path(temp.path())).unwrap(), body);
    }

    #[tokio::test]
    async fn transfer_overflow_without_content_length_is_rejected() {
        let trusted = b"short";
        let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        response.extend_from_slice(b"bytes-longer-than-trusted");
        let (url, server) = RawServer::start(vec![(Duration::ZERO, response)]).await;
        let temp = tempfile::tempdir().unwrap();
        let file = test_file_plan(&url, trusted, trusted.len() as u64);
        let (mut events, _receiver) = futures::channel::mpsc::channel(4);

        let result = download_file(
            &base_client_builder().build().unwrap(),
            &file,
            temp.path(),
            temp.path(),
            0,
            1,
            &mut events,
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::TransferLimit)));
    }

    #[tokio::test]
    async fn timeout_and_midstream_disconnect_fail_closed() {
        let body = b"trusted-bytes";
        let delayed = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        let (timeout_url, timeout_server) =
            RawServer::start(vec![(Duration::from_millis(100), delayed)]).await;
        let temp = tempfile::tempdir().unwrap();
        let timeout_file = test_file_plan(&timeout_url, body, body.len() as u64);
        let (mut events, _receiver) = futures::channel::mpsc::channel(4);
        let timeout = download_file(
            &base_client_builder()
                .timeout(Duration::from_millis(20))
                .build()
                .unwrap(),
            &timeout_file,
            temp.path(),
            temp.path(),
            0,
            1,
            &mut events,
        )
        .await;
        timeout_server.finish().await;
        assert!(matches!(timeout, Err(ModelDeliveryError::Network)));

        let mut partial = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        partial.extend_from_slice(&body[..3]);
        let (disconnect_url, disconnect_server) =
            RawServer::start(vec![(Duration::ZERO, partial)]).await;
        let disconnect_file = test_file_plan(&disconnect_url, body, body.len() as u64);
        let disconnect = download_file(
            &base_client_builder().build().unwrap(),
            &disconnect_file,
            temp.path(),
            temp.path(),
            0,
            1,
            &mut events,
        )
        .await;
        disconnect_server.finish().await;
        assert!(matches!(disconnect, Err(ModelDeliveryError::Network)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn credential_bearing_proxy_environment_cannot_influence_routing() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/direct", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8(request[..read].to_vec()).unwrap()
        });
        let output = tokio::task::spawn_blocking(move || {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "model_delivery::tests::proxy_client_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("ORBOK_PROXY_TEST_URL", url)
                .env("HTTP_PROXY", "http://user:secret@127.0.0.1:9")
                .env("HTTPS_PROXY", "http://user:secret@127.0.0.1:9")
                .env("ALL_PROXY", "http://user:secret@127.0.0.1:9")
                .env("http_proxy", "http://user:secret@127.0.0.1:9")
                .env("https_proxy", "http://user:secret@127.0.0.1:9")
                .env("all_proxy", "http://user:secret@127.0.0.1:9")
                .env("NO_PROXY", "")
                .env("no_proxy", "")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        let request = server.await.unwrap();

        assert!(output.status.success(), "child output: {output:?}");
        assert!(request.starts_with("GET /direct HTTP/1.1\r\n"));
        assert!(
            !request
                .to_ascii_lowercase()
                .contains("proxy-authorization:")
        );
    }

    #[tokio::test]
    #[ignore = "separate-process helper"]
    async fn proxy_client_child() {
        let url = std::env::var("ORBOK_PROXY_TEST_URL").unwrap();
        let response = base_client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "ok");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abrupt_process_exit_at_each_durability_boundary_recovers_coherently() {
        #[cfg(unix)]
        const CRASH_POINTS: &[&str] = &[
            "before_file_sync:tokenizer",
            "after_file_sync:tokenizer",
            "before_file_sync:onnx-model",
            "after_file_sync:onnx-model",
            "before_staged_file_durable_rename:tokenizer",
            "after_staged_file_durable_rename:tokenizer",
            "before_staged_file_durable_rename:onnx-model",
            "after_staged_file_durable_rename:onnx-model",
            "before_nested_directory_sync",
            "after_nested_directory_sync",
            "before_generation_promotion_durable_rename",
            "after_generation_promotion_durable_rename",
            "before_staging_parent_sync",
            "after_staging_parent_sync",
            "before_generations_parent_sync",
            "after_generations_parent_sync",
            "before_model_root_sync",
            "after_model_root_sync",
            "before_inactive_registration_transaction",
            "after_inactive_registration_transaction",
            "before_activation_transaction",
            "after_activation_transaction",
        ];
        #[cfg(windows)]
        const CRASH_POINTS: &[&str] = &[
            "before_file_sync:tokenizer",
            "after_file_sync:tokenizer",
            "before_file_sync:onnx-model",
            "after_file_sync:onnx-model",
            "before_staged_file_durable_rename:tokenizer",
            "after_staged_file_durable_rename:tokenizer",
            "before_staged_file_durable_rename:onnx-model",
            "after_staged_file_durable_rename:onnx-model",
            "before_generation_promotion_durable_rename",
            "after_generation_promotion_durable_rename",
            "before_inactive_registration_transaction",
            "after_inactive_registration_transaction",
            "before_activation_transaction",
            "after_activation_transaction",
        ];

        for crash_point in CRASH_POINTS {
            let tokenizer = b"trusted-tokenizer".to_vec();
            let model = b"trusted-onnx-model".to_vec();
            let server =
                MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())])
                    .await;
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("model-store");
            std::fs::create_dir(&root).unwrap();
            let catalog_path = temp.path().join("catalog.sqlite3");
            drop(Catalog::open(&catalog_path).unwrap());
            let executable = std::env::current_exe().unwrap();
            let base_url = server.base_url.clone();
            let root_for_child = root.clone();
            let catalog_for_child = catalog_path.clone();
            let point = crash_point.to_string();
            let output = tokio::task::spawn_blocking(move || {
                Command::new(executable)
                    .args([
                        "--exact",
                        "model_delivery::tests::crash_injection_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("ORBOK_RFC050_CRASH_POINT", &point)
                    .env("ORBOK_RFC050_TEST_BASE_URL", base_url)
                    .env("ORBOK_RFC050_TEST_STORE", root_for_child)
                    .env("ORBOK_RFC050_TEST_CATALOG", catalog_for_child)
                    .output()
                    .unwrap()
            })
            .await
            .unwrap();
            server.finish().await;
            assert!(
                !output.status.success(),
                "failpoint {crash_point} did not abort"
            );

            let catalog = Catalog::open(&catalog_path).unwrap();
            let store = ManagedModelStore::default_embedding(&root);
            let recovery = crate::model_lifecycle::run_managed_model_startup_with(
                &catalog,
                &store,
                "test-manifest",
                |path| path.join(COMPLETE_FILE).is_file(),
            )
            .unwrap();
            let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
            let snapshot = ManagedGenerationRepository::new(&catalog)
                .load_shared(&guard)
                .unwrap();
            snapshot.validate().unwrap();
            if *crash_point == "after_activation_transaction" {
                assert!(recovery.current_generation_id.is_some());
                assert_eq!(snapshot.generations.len(), 1);
                assert_eq!(
                    snapshot.generations.values().next().unwrap().state,
                    ManagedGenerationState::Current
                );
            } else if matches!(
                *crash_point,
                "before_file_sync:tokenizer"
                    | "after_file_sync:tokenizer"
                    | "before_file_sync:onnx-model"
                    | "after_file_sync:onnx-model"
                    | "before_staged_file_durable_rename:tokenizer"
                    | "after_staged_file_durable_rename:tokenizer"
                    | "before_staged_file_durable_rename:onnx-model"
                    | "after_staged_file_durable_rename:onnx-model"
                    | "before_nested_directory_sync"
                    | "after_nested_directory_sync"
            ) {
                assert_eq!(recovery.current_generation_id, None);
                assert_eq!(recovery.quarantined_staging, 1);
                assert!(snapshot.generations.is_empty());
            } else if matches!(
                *crash_point,
                "after_inactive_registration_transaction" | "before_activation_transaction"
            ) {
                assert_eq!(recovery.current_generation_id, None);
                assert_eq!(recovery.recovered_inactive, 0);
                assert_eq!(snapshot.generations.len(), 1);
                assert_eq!(
                    snapshot.generations.values().next().unwrap().state,
                    ManagedGenerationState::Inactive
                );
            } else {
                assert_eq!(recovery.current_generation_id, None);
                assert_eq!(recovery.recovered_inactive, 1);
                assert_eq!(snapshot.generations.len(), 1);
                assert_eq!(
                    snapshot.generations.values().next().unwrap().state,
                    ManagedGenerationState::Inactive
                );
            }
            assert!(
                std::fs::read_dir(root.join(STAGING_DIR))
                    .unwrap()
                    .next()
                    .is_none(),
                "staging was not fully classified after {crash_point}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "separate-process helper"]
    async fn crash_injection_child() {
        let base_url = std::env::var("ORBOK_RFC050_TEST_BASE_URL").unwrap();
        let root = PathBuf::from(std::env::var_os("ORBOK_RFC050_TEST_STORE").unwrap());
        let catalog_path = PathBuf::from(std::env::var_os("ORBOK_RFC050_TEST_CATALOG").unwrap());
        let fixture = fixture(
            &base_url,
            b"trusted-tokenizer".to_vec(),
            b"trusted-onnx-model".to_vec(),
            None,
        );
        let store = ManagedModelStore::default_embedding(&root);
        let catalog = Catalog::open(catalog_path).unwrap();
        let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let repository = ManagedGenerationRepository::new(&catalog);
        let (events, _receiver) = futures::channel::mpsc::channel(16);
        let _ = execute_generation(
            &store,
            &guard,
            &repository,
            &root,
            &fixture.plan,
            fixture.manifest,
            &base_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            events,
            |_| {},
            |_| {},
        )
        .await;
        panic!("configured crash point was not reached");
    }

    struct Fixture {
        plan: DownloadPlan,
        manifest: &'static TrustedModelManifest,
    }

    fn fixture(
        base_url: &str,
        tokenizer: Vec<u8>,
        model: Vec<u8>,
        model_digest_override: Option<&'static str>,
    ) -> Fixture {
        let tokenizer_url = leak(format!("{base_url}/tokenizer"));
        let model_url = leak(format!("{base_url}/model"));
        let tokenizer_digest = leak(digest(&tokenizer));
        let model_digest = model_digest_override.unwrap_or_else(|| leak(digest(&model)));
        let trusted_files = Box::leak(
            vec![
                TrustedModelFile {
                    logical_name: "tokenizer",
                    relative_path: "tokenizer.json",
                    url: tokenizer_url,
                    sha256: tokenizer_digest,
                    exact_size_bytes: tokenizer.len() as u64,
                    max_transfer_bytes: tokenizer.len() as u64,
                },
                TrustedModelFile {
                    logical_name: "onnx-model",
                    relative_path: "onnx/model.onnx",
                    url: model_url,
                    sha256: model_digest,
                    exact_size_bytes: model.len() as u64,
                    max_transfer_bytes: model.len() as u64,
                },
            ]
            .into_boxed_slice(),
        );
        let manifest = Box::leak(Box::new(TrustedModelManifest {
            schema_version: 1,
            manifest_id: "test-manifest",
            model: TrustedModelIdentity {
                id: "test/model",
                display_name: "test-model",
                revision: "0000000000000000000000000000000000000000",
                role: "embedding",
                dimension: 2,
                license: "MIT",
            },
            transport: TrustedTransportPolicy {
                https_only: true,
                credentials_allowed: false,
                max_redirects: 1,
                initial_host: "example.invalid",
                permitted_redirect_hosts: &["cdn.example.invalid"],
                relative_redirects_allowed: false,
                strip_sensitive_headers_on_redirect: true,
            },
            files: trusted_files,
        }));
        let plan = DownloadPlan {
            manifest_id: manifest.manifest_id,
            max_concurrent: 2,
            files: trusted_files
                .iter()
                .map(|file| ModelFilePlan {
                    logical_name: file.logical_name,
                    relative_path: file.relative_path,
                    remote_url: file.url,
                    expected_sha256: file.sha256,
                    exact_size_bytes: file.exact_size_bytes,
                    max_transfer_bytes: file.max_transfer_bytes,
                    local_status: LocalFileStatus::Missing,
                    action: DownloadAction::Download,
                    temp_path_suffix: ".part",
                })
                .collect(),
        };
        Fixture { plan, manifest }
    }

    fn digest(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut encoded = String::new();
        for byte in sha2::Sha256::digest(bytes) {
            write!(&mut encoded, "{byte:02x}").unwrap();
        }
        encoded
    }

    fn assert_no_part_files(root: &Path) {
        for entry in std::fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                assert_no_part_files(&path);
            } else {
                assert_ne!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("part")
                );
            }
        }
    }

    fn fixture_generation_matches(
        path: &Path,
        expected_path: &Path,
        manifest: &TrustedModelManifest,
        tokenizer: &[u8],
        model: &[u8],
    ) -> bool {
        path == expected_path
            && std::fs::read(path.join("tokenizer.json")).ok().as_deref() == Some(tokenizer)
            && std::fs::read(path.join("onnx/model.onnx")).ok().as_deref() == Some(model)
            && std::fs::read(path.join(COMPLETE_FILE)).ok().as_deref() == Some(b"complete\n")
            && std::fs::read(path.join(TRUSTED_MANIFEST_FILE)).ok()
                == serde_json::to_vec_pretty(manifest).ok()
    }

    fn write_fixture_generation(
        generation_dir: &Path,
        manifest: &TrustedModelManifest,
        tokenizer: &[u8],
        model: &[u8],
    ) {
        std::fs::create_dir_all(generation_dir.join("onnx")).unwrap();
        std::fs::write(generation_dir.join("tokenizer.json"), tokenizer).unwrap();
        std::fs::write(generation_dir.join("onnx/model.onnx"), model).unwrap();
        write_metadata(generation_dir, manifest).unwrap();
    }

    fn test_file_plan(url: &str, trusted_bytes: &[u8], max_transfer_bytes: u64) -> ModelFilePlan {
        ModelFilePlan {
            logical_name: "test-file",
            relative_path: "test.bin",
            remote_url: leak(url.to_string()),
            expected_sha256: leak(digest(trusted_bytes)),
            exact_size_bytes: trusted_bytes.len() as u64,
            max_transfer_bytes,
            local_status: LocalFileStatus::Missing,
            action: DownloadAction::Download,
            temp_path_suffix: ".part",
        }
    }

    fn leak(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    struct MockServer {
        base_url: String,
        max_active: Arc<AtomicUsize>,
        requests: Arc<AtomicUsize>,
        shutdown: oneshot::Sender<()>,
        task: tokio::task::JoinHandle<()>,
    }

    struct MockResponse {
        body: Vec<u8>,
        delay: Duration,
    }

    struct RawServer {
        task: tokio::task::JoinHandle<()>,
    }

    impl RawServer {
        async fn start(chunks: Vec<(Duration, Vec<u8>)>) -> (String, Self) {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 2048];
                let _ = stream.read(&mut request).await;
                for (delay, chunk) in chunks {
                    tokio::time::sleep(delay).await;
                    if stream.write_all(&chunk).await.is_err() {
                        break;
                    }
                }
                let _ = stream.shutdown().await;
            });
            (format!("http://{address}/file"), Self { task })
        }

        async fn finish(self) {
            self.task.await.unwrap();
        }
    }

    impl MockServer {
        async fn start<const N: usize>(responses: [(&'static str, Vec<u8>); N]) -> Self {
            Self::start_delayed(
                responses.map(|(path, body)| (path, body, Duration::from_millis(50))),
            )
            .await
        }

        async fn start_delayed<const N: usize>(
            responses: [(&'static str, Vec<u8>, Duration); N],
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let responses = Arc::new(
                responses
                    .into_iter()
                    .map(|(path, body, delay)| (path, MockResponse { body, delay }))
                    .collect::<HashMap<_, _>>(),
            );
            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(AtomicUsize::new(0));
            let active_task = active.clone();
            let max_task = max_active.clone();
            let requests_task = requests.clone();
            let (shutdown, mut shutdown_requested) = oneshot::channel();
            let task = tokio::spawn(async move {
                let mut children = Vec::new();
                for _ in 0..N {
                    let accepted = tokio::select! {
                        result = listener.accept() => Some(result.unwrap()),
                        _ = &mut shutdown_requested => None,
                    };
                    let Some((stream, _)) = accepted else {
                        break;
                    };
                    let responses = responses.clone();
                    let active = active_task.clone();
                    let max_active = max_task.clone();
                    let requests = requests_task.clone();
                    children.push(tokio::spawn(async move {
                        serve(stream, responses, active, max_active, requests).await;
                    }));
                }
                for child in children {
                    child.await.unwrap();
                }
            });
            Self {
                base_url: format!("http://{address}"),
                max_active,
                requests,
                shutdown,
                task,
            }
        }

        async fn finish(self) {
            let _ = self.shutdown.send(());
            self.task.await.unwrap();
        }
    }

    async fn serve(
        mut stream: TcpStream,
        responses: Arc<HashMap<&'static str, MockResponse>>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        requests: Arc<AtomicUsize>,
    ) {
        let mut request = vec![0_u8; 2048];
        let Ok(read) = stream.read(&mut request).await else {
            return;
        };
        let request = String::from_utf8_lossy(&request[..read]);
        let Some(path) = request.split_whitespace().nth(1) else {
            return;
        };
        let Some(response) = responses.get(path) else {
            return;
        };
        requests.fetch_add(1, Ordering::SeqCst);
        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_active.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(response.delay).await;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.body.len()
        );
        if stream.write_all(header.as_bytes()).await.is_ok() {
            let _ = stream.write_all(&response.body).await;
        }
        let _ = stream.shutdown().await;
        active.fetch_sub(1, Ordering::SeqCst);
    }
}
