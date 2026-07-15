//! Serialized trusted model-generation delivery (RFC-050 Phase 2).

use futures::{SinkExt as _, StreamExt as _};
use orbok_db::Catalog;
use orbok_db::repo::ManagedGenerationRepository;
use orbok_models::{
    DEFAULT_TRUSTED_MODEL, DownloadAction, DownloadPlan, ManagedGenerationId, ManagedModelStore,
    ModelReadiness, ModelStoreLockError, TrustedModelManifest, build_download_plan,
    check_app_managed_model_readiness, validate_initial_url, validate_redirect_url,
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
    if !store.models_dir().is_dir() {
        return Err(ModelDeliveryError::StoreUnavailable);
    }
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
        if let Some(current_id) = snapshot.profile.current_generation_id {
            return Ok(ModelDeliveryOutcome {
                generation_dir: source_dir,
                generation_id: current_id,
            });
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
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_generation(
    store: &ManagedModelStore,
    guard: &orbok_models::ModelStoreMutationGuard<orbok_models::ExclusiveAccess>,
    repository: &ManagedGenerationRepository<'_>,
    source_dir: &Path,
    plan: &DownloadPlan,
    manifest: &'static TrustedModelManifest,
    client: &reqwest::Client,
    events: futures::channel::mpsc::Sender<ModelDeliveryEvent>,
) -> Result<ModelDeliveryOutcome, ModelDeliveryError> {
    if plan.manifest_id != manifest.manifest_id || plan.max_concurrent > 2 {
        return Err(ModelDeliveryError::Plan);
    }
    let generation_id = ManagedGenerationId::generate();
    let staging_parent = store.models_dir().join(STAGING_DIR);
    let generations_parent = store.models_dir().join(GENERATIONS_DIR);
    let staging = staging_parent.join(generation_id.as_str());
    let promoted = generations_parent.join(generation_id.as_str());

    std::fs::create_dir_all(&staging_parent).map_err(|_| ModelDeliveryError::Filesystem)?;
    std::fs::create_dir_all(&generations_parent).map_err(|_| ModelDeliveryError::Filesystem)?;
    std::fs::create_dir(&staging).map_err(|_| ModelDeliveryError::Filesystem)?;
    sync_directory(&staging_parent)?;
    sync_directory(&generations_parent)?;
    sync_directory(store.models_dir())?;

    let result = stage_files(source_dir, &staging, plan, client, events).await;
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    verify_complete_set(&staging, plan).await?;
    sync_staged_tree(&staging, plan)?;
    write_metadata(&staging, manifest)?;
    verify_generation_metadata(&staging, manifest)?;

    std::fs::rename(&staging, &promoted).map_err(|_| ModelDeliveryError::Filesystem)?;
    sync_directory(&staging_parent)?;
    sync_directory(&generations_parent)?;
    sync_directory(store.models_dir())?;
    verify_complete_set(&promoted, plan).await?;
    verify_generation_metadata(&promoted, manifest)?;

    repository
        .register_inactive(guard, generation_id.clone(), manifest.manifest_id)
        .map_err(|_| ModelDeliveryError::Catalog)?;
    repository
        .activate(guard, &generation_id)
        .map_err(|_| ModelDeliveryError::Catalog)?;
    let final_snapshot = repository
        .load_exclusive(guard)
        .map_err(|_| ModelDeliveryError::FinalCheck)?;
    if final_snapshot.profile.current_generation_id.as_ref() != Some(&generation_id) {
        return Err(ModelDeliveryError::FinalCheck);
    }
    verify_complete_set(&promoted, plan)
        .await
        .map_err(|_| ModelDeliveryError::FinalCheck)?;

    Ok(ModelDeliveryOutcome {
        generation_id,
        generation_dir: promoted,
    })
}

async fn stage_files(
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
        tokio::fs::copy(file.final_path(source_dir), &destination)
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
        tokio::fs::File::open(&destination)
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?
            .sync_all()
            .await
            .map_err(|_| ModelDeliveryError::Filesystem)?;
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
    let client = client.clone();
    let downloads = downloads.into_iter().map(|(index, file)| {
        let mut tx = events.clone();
        let staging = staging.clone();
        let client = client.clone();
        async move {
            let destination = file.final_path(&staging);
            create_parent(&destination)?;
            download_file(&client, &file, &staging, index as u32, files_total, &mut tx).await
        }
    });
    let mut results = futures::stream::iter(downloads).buffer_unordered(plan.max_concurrent);
    while let Some(result) = results.next().await {
        result?;
    }
    Ok(())
}

async fn download_file(
    client: &reqwest::Client,
    file: &orbok_models::ModelFilePlan,
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
    output
        .flush()
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    output
        .sync_all()
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    drop(output);
    if downloaded != file.exact_size_bytes || sha256_file(&part).await? != file.expected_sha256 {
        return Err(ModelDeliveryError::Integrity);
    }
    tokio::fs::rename(part, final_path)
        .await
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    Ok(())
}

async fn verify_complete_set(
    generation_dir: &Path,
    plan: &DownloadPlan,
) -> Result<(), ModelDeliveryError> {
    for file in &plan.files {
        let path = file.final_path(generation_dir);
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| ModelDeliveryError::Integrity)?;
        if metadata.len() != file.exact_size_bytes
            || sha256_file(&path).await? != file.expected_sha256
        {
            return Err(ModelDeliveryError::Integrity);
        }
    }
    Ok(())
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
        sync_directory(&parent)?;
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
    sync_directory(staging)
}

fn verify_generation_metadata(
    generation_dir: &Path,
    manifest: &TrustedModelManifest,
) -> Result<(), ModelDeliveryError> {
    let expected =
        serde_json::to_vec_pretty(manifest).map_err(|_| ModelDeliveryError::Integrity)?;
    let actual = std::fs::read(generation_dir.join(TRUSTED_MANIFEST_FILE))
        .map_err(|_| ModelDeliveryError::Integrity)?;
    let complete = std::fs::read(generation_dir.join(COMPLETE_FILE))
        .map_err(|_| ModelDeliveryError::Integrity)?;
    if actual != expected || complete != b"complete\n" {
        return Err(ModelDeliveryError::Integrity);
    }
    Ok(())
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> Result<(), ModelDeliveryError> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path).map_err(|_| ModelDeliveryError::Filesystem)?;
    file.write_all(bytes)
        .map_err(|_| ModelDeliveryError::Filesystem)?;
    file.sync_all().map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ModelDeliveryError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<(), ModelDeliveryError> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| ModelDeliveryError::Filesystem)
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(_path: &Path) -> Result<(), ModelDeliveryError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_models::{
        DownloadAction, LocalFileStatus, ModelFilePlan, TrustedModelFile, TrustedModelIdentity,
        TrustedTransportPolicy,
    };
    use std::collections::HashMap;
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mock_server_install_promotes_complete_generation_and_activates_it() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server, tokenizer, model, None);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("model-store");
        std::fs::create_dir(&root).unwrap();
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
        )
        .await
        .unwrap();
        let max_active = server.max_active.clone();
        server.finish().await;

        assert_eq!(max_active.load(Ordering::SeqCst), 2);
        assert!(outcome.generation_dir.join(COMPLETE_FILE).is_file());
        assert!(outcome.generation_dir.join(TRUSTED_MANIFEST_FILE).is_file());
        assert!(!outcome.generation_dir.join("tokenizer.json.part").exists());
        assert_eq!(
            repository
                .load_exclusive(&guard)
                .unwrap()
                .profile
                .current_generation_id,
            Some(outcome.generation_id)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn checksum_failure_never_promotes_or_activates() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"corrupt-model".to_vec();
        let server =
            MockServer::start([("/tokenizer", tokenizer.clone()), ("/model", model.clone())]).await;
        let fixture = fixture(&server, tokenizer, model, Some(leak("0".repeat(64))));
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
        )
        .await;
        server.finish().await;

        assert!(matches!(result, Err(ModelDeliveryError::Integrity)));
        assert!(
            repository
                .load_exclusive(&guard)
                .unwrap()
                .profile
                .current_generation_id
                .is_none()
        );
        assert_eq!(
            std::fs::read_dir(root.join(GENERATIONS_DIR))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn trusted_skip_is_copied_without_a_network_request() {
        let tokenizer = b"trusted-tokenizer".to_vec();
        let model = b"trusted-onnx-model".to_vec();
        let server = MockServer::start([("/model", model.clone())]).await;
        let mut fixture = fixture(&server, tokenizer.clone(), model, None);
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
        )
        .await
        .unwrap();
        server.finish().await;

        assert_eq!(
            std::fs::read(outcome.generation_dir.join("tokenizer.json")).unwrap(),
            b"trusted-tokenizer"
        );
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

    struct Fixture {
        plan: DownloadPlan,
        manifest: &'static TrustedModelManifest,
    }

    fn fixture(
        server: &MockServer,
        tokenizer: Vec<u8>,
        model: Vec<u8>,
        model_digest_override: Option<&'static str>,
    ) -> Fixture {
        let tokenizer_url = leak(format!("{}/tokenizer", server.base_url));
        let model_url = leak(format!("{}/model", server.base_url));
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

    fn leak(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    struct MockServer {
        base_url: String,
        max_active: Arc<AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }

    impl MockServer {
        async fn start<const N: usize>(responses: [(&'static str, Vec<u8>); N]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let responses = Arc::new(responses.into_iter().collect::<HashMap<_, _>>());
            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let active_task = active.clone();
            let max_task = max_active.clone();
            let task = tokio::spawn(async move {
                let mut children = Vec::new();
                for _ in 0..N {
                    let (stream, _) = listener.accept().await.unwrap();
                    let responses = responses.clone();
                    let active = active_task.clone();
                    let max_active = max_task.clone();
                    children.push(tokio::spawn(async move {
                        serve(stream, responses, active, max_active).await;
                    }));
                }
                for child in children {
                    child.await.unwrap();
                }
            });
            Self {
                base_url: format!("http://{address}"),
                max_active,
                task,
            }
        }

        async fn finish(self) {
            self.task.await.unwrap();
        }
    }

    async fn serve(
        mut stream: TcpStream,
        responses: Arc<HashMap<&'static str, Vec<u8>>>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    ) {
        let mut request = vec![0_u8; 2048];
        let read = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        let path = request.split_whitespace().nth(1).unwrap();
        let body = responses.get(path).unwrap();
        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_active.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        stream.shutdown().await.unwrap();
        active.fetch_sub(1, Ordering::SeqCst);
    }
}
