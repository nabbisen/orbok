use crate::bootstrap::model_resolution::{
    ManagedModelResolutionError, resolve_model_dir, resolve_model_dir_with_timeout,
};
use crate::bootstrap::{ensure_default_model_store, load_initial_state, open_catalog};
use crate::settings::OrbokSettings;
use orbok::runtime_context::{
    AllowRuntimePathProbe, PlatformRuntimePaths, RuntimeContext, RuntimeSelection,
};
use orbok_models::{ManagedModelStore, ModelStoreLockError};
use orbok_ui::state::ModelProvenance;
use orbok_workers::VerifyOutcome;
use std::path::PathBuf;
use std::time::Duration;

fn test_context(data_dir: &std::path::Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
        data_dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(data_dir),
            standard_settings_dir: Some(data_dir),
        },
    )
    .unwrap()
}

/// Duplicates `default_model_store_root`'s naming convention rather than
/// reaching for the (now lib-crate-internal) function itself — tests
/// rebuild expected paths from roots they control, per Correction
/// Request 111 §4 C1.
fn test_model_store_root(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("models").join("multilingual-e5-small")
}

#[test]
fn initial_state_advances_managed_model_startup_epoch() {
    let temp = tempfile::tempdir().unwrap();

    let context = test_context(temp.path());
    let _state = load_initial_state(&context).unwrap();

    let catalog = open_catalog(&context).unwrap();
    let store = ManagedModelStore::default_embedding(test_model_store_root(temp.path()));
    let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
    let snapshot = orbok_db::repo::ManagedGenerationRepository::new(&catalog)
        .load_shared(&guard)
        .unwrap();
    assert_eq!(snapshot.profile.startup_epoch.get(), 1);
}

#[test]
fn exclusive_owner_prevents_managed_path_from_falling_back_as_manual() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path();
    let context = test_context(data_dir);
    let model_store = ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
    let _exclusive = model_store
        .acquire_exclusive(Duration::from_secs(1))
        .unwrap();
    let catalog = open_catalog(&context).unwrap();
    let settings = OrbokSettings {
        embedding_model_dir: Some(
            test_model_store_root(data_dir)
                .join("generations")
                .join("persisted-managed-path")
                .to_string_lossy()
                .into_owned(),
        ),
        ..OrbokSettings::default()
    };

    let result = resolve_model_dir_with_timeout(
        &context,
        &AllowRuntimePathProbe,
        &catalog,
        &settings,
        Duration::from_millis(20),
    );

    assert!(matches!(
        result,
        Err(ManagedModelResolutionError::StoreLock(
            ModelStoreLockError::Timeout
        ))
    ));
}

#[test]
fn managed_setting_is_not_treated_as_manual_without_a_catalog_current() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
    let catalog = open_catalog(&context).unwrap();
    let managed_path = test_model_store_root(temp.path())
        .join("generations")
        .join("old-managed-path");
    let settings = OrbokSettings {
        embedding_model_dir: Some(managed_path.to_string_lossy().into_owned()),
        ..OrbokSettings::default()
    };

    let resolved =
        resolve_model_dir(&context, &AllowRuntimePathProbe, &catalog, &settings).unwrap();

    assert!(resolved.path.is_none());
    assert_eq!(resolved.provenance, None);
    assert!(resolved._guard.is_none());
}

#[test]
fn genuine_manual_setting_remains_available_when_no_managed_current_exists() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
    let catalog = open_catalog(&context).unwrap();
    let manual = temp.path().join("user-model");
    let settings = OrbokSettings {
        embedding_model_dir: Some(manual.to_string_lossy().into_owned()),
        ..OrbokSettings::default()
    };

    let resolved =
        resolve_model_dir(&context, &AllowRuntimePathProbe, &catalog, &settings).unwrap();

    assert_eq!(resolved.path.as_deref(), manual.to_str());
    assert_eq!(resolved.provenance, Some(ModelProvenance::UserSupplied));
    assert!(resolved._guard.is_none());
}

/// F1 regression: the managed-generation directory must be computed
/// from the *context's* store root, never re-derived from
/// `Catalog::path()`. The catalog's own database record of the active
/// generation is legitimately whatever that catalog contains regardless
/// of which context resolves it — that part is unaffected by this test.
/// What must not happen: the *filesystem path* returned for that
/// generation silently follows the catalog's own profile directory
/// instead of the context passed to `resolve_model_dir`.
#[test]
fn resolution_follows_the_context_not_the_catalog_path() {
    let catalog_temp = tempfile::tempdir().unwrap();
    let catalog_context = test_context(catalog_temp.path());
    let catalog_store =
        ensure_default_model_store(&catalog_context, &AllowRuntimePathProbe).unwrap();
    let catalog = open_catalog(&catalog_context).unwrap();
    let generation_id = orbok_models::ManagedGenerationId::generate();
    {
        let guard = catalog_store
            .acquire_exclusive(Duration::from_secs(1))
            .unwrap();
        let repository = orbok_db::repo::ManagedGenerationRepository::new(&catalog);
        repository
            .register_inactive(
                &guard,
                generation_id.clone(),
                orbok_models::trust::DEFAULT_TRUSTED_MODEL.manifest_id,
            )
            .unwrap();
        repository.activate(&guard, &generation_id).unwrap();
    }

    // A different profile's context. The catalog above still reports
    // this generation as active (that is a property of the catalog's
    // own database, independent of which context resolves it) — the
    // defect this guards against is the *directory* computed for it
    // silently anchoring under the catalog's profile instead of the
    // context actually passed in.
    let other_temp = tempfile::tempdir().unwrap();
    let other_context = test_context(other_temp.path());
    ensure_default_model_store(&other_context, &AllowRuntimePathProbe).unwrap();

    let resolved = resolve_model_dir(
        &other_context,
        &AllowRuntimePathProbe,
        &catalog,
        &OrbokSettings::default(),
    )
    .unwrap();

    let resolved_path = resolved.path.expect("catalog reports an active generation");
    let expected_root = test_model_store_root(other_temp.path());
    let forbidden_root = test_model_store_root(catalog_temp.path());
    assert!(
        resolved_path.starts_with(expected_root.to_str().unwrap()),
        "resolved path {resolved_path} must anchor under the context's own store root {}",
        expected_root.display()
    );
    assert!(
        !resolved_path.starts_with(forbidden_root.to_str().unwrap()),
        "resolved path {resolved_path} must not follow the catalog's profile root {}",
        forbidden_root.display()
    );
    assert_eq!(resolved.provenance, Some(ModelProvenance::AppManaged));
}

#[test]
fn ready_startup_distinguishes_managed_and_manual_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let model_store = ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
    let catalog = open_catalog(&context).unwrap();
    let generation_id = orbok_models::ManagedGenerationId::generate();
    {
        let guard = model_store
            .acquire_exclusive(Duration::from_secs(1))
            .unwrap();
        let repository = orbok_db::repo::ManagedGenerationRepository::new(&catalog);
        repository
            .register_inactive(
                &guard,
                generation_id.clone(),
                orbok_models::trust::DEFAULT_TRUSTED_MODEL.manifest_id,
            )
            .unwrap();
        repository.activate(&guard, &generation_id).unwrap();
    }

    let managed = resolve_model_dir(
        &context,
        &AllowRuntimePathProbe,
        &catalog,
        &OrbokSettings::default(),
    )
    .unwrap();
    assert_eq!(
        crate::model_flow::project_startup(VerifyOutcome::Ready, managed.provenance)
            .active_provenance,
        Some(ModelProvenance::AppManaged)
    );

    let manual_temp = tempfile::tempdir().unwrap();
    let manual_context = test_context(manual_temp.path());
    ensure_default_model_store(&manual_context, &AllowRuntimePathProbe).unwrap();
    let manual_catalog = open_catalog(&manual_context).unwrap();
    let manual_path = manual_temp.path().join("user-model");
    let manual_settings = OrbokSettings {
        embedding_model_dir: Some(manual_path.to_string_lossy().into_owned()),
        ..OrbokSettings::default()
    };
    let manual = resolve_model_dir(
        &manual_context,
        &AllowRuntimePathProbe,
        &manual_catalog,
        &manual_settings,
    )
    .unwrap();
    assert_eq!(
        crate::model_flow::project_startup(VerifyOutcome::Ready, manual.provenance)
            .active_provenance,
        Some(ModelProvenance::UserSupplied)
    );
    assert_eq!(
        crate::model_flow::project_startup(
            VerifyOutcome::FilesInvalid {
                model_dir: manual_path.to_string_lossy().into_owned(),
                issues: Vec::new(),
            },
            manual.provenance,
        )
        .active_provenance,
        None
    );
}
