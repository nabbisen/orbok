use crate::Catalog;
use crate::repo::{GenerationCatalogError, ManagedGenerationRepository};
use orbok_models::{
    ManagedGenerationId, ManagedGenerationState, ModelStoreMutationGuard, ModelStoreProfileId,
};
use std::time::Duration;

#[test]
fn migration_enforces_pointer_epoch_and_unique_role_constraints() {
    let catalog = Catalog::open_in_memory().unwrap();
    let conn = catalog.lock();
    assert!(
        conn.execute(
            "INSERT INTO managed_model_profiles \
             (profile_id, startup_epoch, current_generation_id, previous_generation_id, \
              state_revision, updated_at) VALUES ('p',0,'same','same',0,'t')",
            [],
        )
        .is_err()
    );
    let error = conn
        .execute(
            "INSERT INTO managed_model_profiles \
             (profile_id, startup_epoch, current_generation_id, previous_generation_id, \
              state_revision, updated_at) VALUES ('insert-pointer',0,'missing',NULL,0,'t')",
            [],
        )
        .unwrap_err();
    assert!(error.to_string().contains("pointer/state mismatch"));
    conn.execute(
        "INSERT INTO managed_model_profiles \
         (profile_id, startup_epoch, current_generation_id, previous_generation_id, \
          state_revision, updated_at) VALUES ('p',0,NULL,NULL,0,'t')",
        [],
    )
    .unwrap();
    assert!(
        conn.execute(
            "UPDATE managed_model_profiles SET previous_generation_id = 'missing' \
             WHERE profile_id = 'p'",
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "UPDATE managed_model_profiles SET current_generation_id = 'missing' \
             WHERE profile_id = 'p'",
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO managed_model_generations \
             (generation_id, profile_id, manifest_id, lifecycle_state, activation_epoch, \
              validated_startup_epoch, created_at, updated_at) \
             VALUES ('bad','p','m','current',1,1,'t','t')",
            [],
        )
        .is_err()
    );
    conn.execute(
        "INSERT INTO managed_model_generations \
         (generation_id, profile_id, manifest_id, lifecycle_state, activation_epoch, \
          validated_startup_epoch, created_at, updated_at) \
         VALUES ('one','p','m','current',0,NULL,'t','t')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE managed_model_profiles SET current_generation_id = 'one' WHERE profile_id = 'p'",
        [],
    )
    .unwrap();
    assert!(
        conn.execute(
            "UPDATE managed_model_generations SET lifecycle_state = 'inactive' \
             WHERE generation_id = 'one'",
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO managed_model_generations \
             (generation_id, profile_id, manifest_id, lifecycle_state, activation_epoch, \
              validated_startup_epoch, created_at, updated_at) \
             VALUES ('two','p','m','current',0,NULL,'t','t')",
            [],
        )
        .is_err()
    );
}

#[test]
fn repository_round_trips_guarded_state_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ModelStoreProfileId::default_embedding();
    let catalog = Catalog::open_in_memory().unwrap();
    let repository = ManagedGenerationRepository::new(&catalog);
    let guard = ModelStoreMutationGuard::acquire_exclusive(
        temp.path(),
        profile.clone(),
        Duration::from_secs(1),
    )
    .unwrap();

    let a = ManagedGenerationId::generate();
    repository
        .register_inactive(&guard, a.clone(), "manifest")
        .unwrap();
    let stored = repository.activate(&guard, &a).unwrap();
    assert_eq!(stored.profile.current_generation_id, Some(a.clone()));
    assert_eq!(
        stored.generations[&a].state,
        ManagedGenerationState::Current
    );

    repository.advance_startup_epoch(&guard).unwrap();
    let stored = repository
        .validate_current_after_startup(&guard, &a)
        .unwrap();
    assert!(stored.generations[&a].validated_startup_epoch.is_some());
}

#[test]
fn repository_persists_exact_activation_and_rollback_pairs() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ModelStoreProfileId::default_embedding();
    let catalog = Catalog::open_in_memory().unwrap();
    let repository = ManagedGenerationRepository::new(&catalog);
    let guard =
        ModelStoreMutationGuard::acquire_exclusive(temp.path(), profile, Duration::from_secs(1))
            .unwrap();
    let a = ManagedGenerationId::generate();
    let b = ManagedGenerationId::generate();
    repository
        .register_inactive(&guard, a.clone(), "manifest")
        .unwrap();
    repository.activate(&guard, &a).unwrap();
    repository.advance_startup_epoch(&guard).unwrap();
    repository
        .validate_current_after_startup(&guard, &a)
        .unwrap();
    repository
        .register_inactive(&guard, b.clone(), "manifest")
        .unwrap();
    let active_b = repository.activate(&guard, &b).unwrap();
    assert_eq!(active_b.profile.current_generation_id, Some(b.clone()));
    assert_eq!(active_b.profile.previous_generation_id, Some(a.clone()));

    let rolled_back = repository.rollback_invalid_current(&guard, true).unwrap();
    assert_eq!(rolled_back.profile.current_generation_id, Some(a.clone()));
    assert_eq!(rolled_back.profile.previous_generation_id, None);
    assert_eq!(
        rolled_back.generations[&a].state,
        ManagedGenerationState::Current
    );
    assert_eq!(
        rolled_back.generations[&b].state,
        ManagedGenerationState::Invalid
    );
}

#[test]
fn repository_rejects_stale_revision_and_wrong_profile_guard() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ModelStoreProfileId::default_embedding();
    let other_profile = ModelStoreProfileId::parse("other-profile").unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let repository = ManagedGenerationRepository::new(&catalog);
    let guard = ModelStoreMutationGuard::acquire_exclusive(
        temp.path(),
        profile.clone(),
        Duration::from_secs(1),
    )
    .unwrap();

    let next = repository
        .load_exclusive(&guard)
        .unwrap()
        .register_inactive(ManagedGenerationId::generate(), "manifest")
        .unwrap();
    repository
        .apply_snapshot_for_test(&guard, 0, &next)
        .unwrap();
    assert!(matches!(
        repository.apply_snapshot_for_test(&guard, 0, &next),
        Err(GenerationCatalogError::StateConflict)
    ));

    let wrong_profile_state = orbok_models::ManagedGenerationSnapshot::empty(other_profile)
        .register_inactive(ManagedGenerationId::generate(), "manifest")
        .unwrap();
    assert!(matches!(
        repository.apply_snapshot_for_test(&guard, 0, &wrong_profile_state),
        Err(GenerationCatalogError::GuardProfileMismatch)
    ));
}

#[test]
fn shared_guard_reads_after_exclusive_mutation_finishes() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ModelStoreProfileId::default_embedding();
    let catalog = Catalog::open_in_memory().unwrap();
    let repository = ManagedGenerationRepository::new(&catalog);
    let exclusive = ModelStoreMutationGuard::acquire_exclusive(
        temp.path(),
        profile.clone(),
        Duration::from_secs(1),
    )
    .unwrap();
    let next = repository
        .register_inactive(&exclusive, ManagedGenerationId::generate(), "manifest")
        .unwrap();
    drop(exclusive);

    let shared =
        ModelStoreMutationGuard::acquire_shared(temp.path(), profile, Duration::from_secs(1))
            .unwrap();
    assert_eq!(repository.load_shared(&shared).unwrap(), next);
}
