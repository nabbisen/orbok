//! Guard-ordered catalog persistence for RFC-050 managed generations.

use crate::catalog::Catalog;
use orbok_core::now_iso8601;
use orbok_models::{
    ExclusiveAccess, GenerationTransitionError, ManagedGenerationId, ManagedGenerationRecord,
    ManagedGenerationSnapshot, ManagedGenerationState, ManagedProfileState,
    ModelStoreMutationGuard, ModelStoreProfileId, SharedAccess, StartupEpoch,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;
use std::fmt;

pub struct ManagedGenerationRepository<'a> {
    catalog: &'a Catalog,
}

impl<'a> ManagedGenerationRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Read while holding shared model-store ownership. The guard is acquired
    /// before the catalog mutex by construction.
    pub fn load_shared(
        &self,
        guard: &ModelStoreMutationGuard<SharedAccess>,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        let conn = self.catalog.lock();
        load_snapshot(&conn, guard.profile_id())
    }

    /// Read from an exclusive mutation operation without changing lock order.
    pub fn load_exclusive(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        let conn = self.catalog.lock();
        load_snapshot(&conn, guard.profile_id())
    }

    pub fn register_inactive(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        generation_id: ManagedGenerationId,
        manifest_id: impl Into<String>,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        let manifest_id = manifest_id.into();
        self.transition(guard, |current| {
            current.register_inactive(generation_id, manifest_id)
        })
    }

    pub fn advance_startup_epoch(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        self.transition(guard, ManagedGenerationSnapshot::advance_startup_epoch)
    }

    pub fn activate(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        generation_id: &ManagedGenerationId,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        self.transition(guard, |current| current.activate(generation_id))
    }

    /// Record later-startup validation only if the loaded identity is still the
    /// current generation when the guarded catalog transition is applied.
    pub fn validate_current_after_startup(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        loaded_generation_id: &ManagedGenerationId,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        self.transition(guard, |current| {
            current.validate_current_after_startup(loaded_generation_id)
        })
    }

    pub fn rollback_invalid_current(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        previous_verified: bool,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        self.transition(guard, |current| {
            current.rollback_invalid_current(previous_verified)
        })
    }

    fn transition(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        build: impl FnOnce(
            &ManagedGenerationSnapshot,
        ) -> Result<ManagedGenerationSnapshot, GenerationTransitionError>,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        let current = self.load_exclusive(guard)?;
        let expected_revision = current.profile.state_revision;
        let next = build(&current).map_err(GenerationCatalogError::InvalidTransition)?;
        self.apply_snapshot(guard, expected_revision, &next)
    }

    /// Persist one already-validated pure state transition atomically.
    ///
    /// The exclusive guard parameter makes the required ordering explicit:
    /// callers cannot enter this repository mutation and then wait for the
    /// model-store guard while holding the catalog connection.
    fn apply_snapshot(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        expected_revision: u64,
        next: &ManagedGenerationSnapshot,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        if guard.profile_id() != &next.profile.profile_id {
            return Err(GenerationCatalogError::GuardProfileMismatch);
        }
        next.validate()
            .map_err(GenerationCatalogError::InvalidTransition)?;
        let required_revision = expected_revision
            .checked_add(1)
            .ok_or(GenerationCatalogError::RevisionOutOfRange)?;
        if next.profile.state_revision != required_revision {
            return Err(GenerationCatalogError::StateConflict);
        }

        let mut conn = self.catalog.lock();
        let tx = conn
            .transaction()
            .map_err(GenerationCatalogError::database)?;
        let current = load_snapshot(&tx, guard.profile_id())?;
        if current.profile.state_revision != expected_revision {
            return Err(GenerationCatalogError::StateConflict);
        }
        if current.profile.startup_epoch > next.profile.startup_epoch
            || current
                .generations
                .keys()
                .any(|id| !next.generations.contains_key(id))
        {
            return Err(GenerationCatalogError::InvalidTransition(
                GenerationTransitionError::PointerStateMismatch,
            ));
        }
        for (id, existing) in &current.generations {
            let replacement = next
                .generations
                .get(id)
                .ok_or(GenerationCatalogError::StateConflict)?;
            if replacement.manifest_id != existing.manifest_id {
                return Err(GenerationCatalogError::ImmutableIdentityChanged);
            }
        }

        let now = now_iso8601();
        tx.execute(
            "INSERT OR IGNORE INTO managed_model_profiles \
             (profile_id, startup_epoch, current_generation_id, previous_generation_id, \
              state_revision, updated_at) VALUES (?1, 0, NULL, NULL, 0, ?2)",
            params![guard.profile_id().as_str(), now],
        )
        .map_err(GenerationCatalogError::database)?;

        // Clear pointers before changing lifecycle labels, then restore the
        // coherent pair before commit. No intermediate state escapes SQLite.
        tx.execute(
            "UPDATE managed_model_profiles SET current_generation_id = NULL, \
             previous_generation_id = NULL WHERE profile_id = ?1",
            [guard.profile_id().as_str()],
        )
        .map_err(GenerationCatalogError::database)?;
        tx.execute(
            "UPDATE managed_model_generations SET lifecycle_state = 'inactive', \
             updated_at = ?2 WHERE profile_id = ?1",
            params![guard.profile_id().as_str(), now],
        )
        .map_err(GenerationCatalogError::database)?;

        for record in next.generations.values() {
            let changed = tx
                .execute(
                    "INSERT INTO managed_model_generations \
                 (generation_id, profile_id, manifest_id, lifecycle_state, activation_epoch, \
                  validated_startup_epoch, created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?7) \
                 ON CONFLICT(generation_id) DO UPDATE SET \
                  lifecycle_state = excluded.lifecycle_state, \
                  activation_epoch = excluded.activation_epoch, \
                  validated_startup_epoch = excluded.validated_startup_epoch, \
                  updated_at = excluded.updated_at \
                 WHERE managed_model_generations.profile_id = excluded.profile_id \
                   AND managed_model_generations.manifest_id = excluded.manifest_id",
                    params![
                        record.generation_id.as_str(),
                        guard.profile_id().as_str(),
                        record.manifest_id,
                        record.state.as_str(),
                        optional_epoch_to_i64(record.activation_epoch)?,
                        optional_epoch_to_i64(record.validated_startup_epoch)?,
                        now,
                    ],
                )
                .map_err(GenerationCatalogError::database)?;
            if changed != 1 {
                return Err(GenerationCatalogError::ImmutableIdentityChanged);
            }
        }

        let updated = tx
            .execute(
                "UPDATE managed_model_profiles SET startup_epoch = ?2, \
                 current_generation_id = ?3, previous_generation_id = ?4, \
                 state_revision = ?5, updated_at = ?6 WHERE profile_id = ?1",
                params![
                    guard.profile_id().as_str(),
                    u64_to_i64(next.profile.startup_epoch.get())?,
                    next.profile
                        .current_generation_id
                        .as_ref()
                        .map(|id| id.as_str()),
                    next.profile
                        .previous_generation_id
                        .as_ref()
                        .map(|id| id.as_str()),
                    u64_to_i64(next.profile.state_revision)?,
                    now,
                ],
            )
            .map_err(GenerationCatalogError::database)?;
        if updated != 1 {
            return Err(GenerationCatalogError::StateConflict);
        }
        tx.commit().map_err(GenerationCatalogError::database)?;
        drop(conn);
        self.load_exclusive(guard)
    }

    #[cfg(test)]
    pub(crate) fn apply_snapshot_for_test(
        &self,
        guard: &ModelStoreMutationGuard<ExclusiveAccess>,
        expected_revision: u64,
        next: &ManagedGenerationSnapshot,
    ) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
        self.apply_snapshot(guard, expected_revision, next)
    }
}

#[derive(Debug)]
pub enum GenerationCatalogError {
    Database(String),
    InvalidCatalogValue { column: &'static str, value: String },
    InvalidTransition(GenerationTransitionError),
    GuardProfileMismatch,
    StateConflict,
    ImmutableIdentityChanged,
    RevisionOutOfRange,
}

impl GenerationCatalogError {
    fn database(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl fmt::Display for GenerationCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(message) => write!(formatter, "generation catalog error: {message}"),
            Self::InvalidCatalogValue { column, value } => {
                write!(
                    formatter,
                    "invalid generation catalog value in {column}: {value}"
                )
            }
            Self::InvalidTransition(error) => error.fmt(formatter),
            Self::GuardProfileMismatch => formatter.write_str("model-store guard profile mismatch"),
            Self::StateConflict => formatter.write_str("managed-generation state conflict"),
            Self::ImmutableIdentityChanged => {
                formatter.write_str("managed-generation immutable identity changed")
            }
            Self::RevisionOutOfRange => {
                formatter.write_str("managed-generation revision is out of range")
            }
        }
    }
}

impl std::error::Error for GenerationCatalogError {}

fn load_snapshot(
    conn: &Connection,
    profile_id: &ModelStoreProfileId,
) -> Result<ManagedGenerationSnapshot, GenerationCatalogError> {
    let profile_row = conn
        .query_row(
            "SELECT startup_epoch, current_generation_id, previous_generation_id, state_revision \
             FROM managed_model_profiles WHERE profile_id = ?1",
            [profile_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(GenerationCatalogError::database)?;
    let Some((startup_epoch, current_id, previous_id, state_revision)) = profile_row else {
        return Ok(ManagedGenerationSnapshot::empty(profile_id.clone()));
    };

    let profile = ManagedProfileState {
        profile_id: profile_id.clone(),
        startup_epoch: parse_epoch("startup_epoch", startup_epoch)?,
        current_generation_id: parse_optional_id("current_generation_id", current_id)?,
        previous_generation_id: parse_optional_id("previous_generation_id", previous_id)?,
        state_revision: parse_u64("state_revision", state_revision)?,
    };
    let mut statement = conn
        .prepare(
            "SELECT generation_id, manifest_id, lifecycle_state, activation_epoch, \
             validated_startup_epoch FROM managed_model_generations \
             WHERE profile_id = ?1 ORDER BY generation_id",
        )
        .map_err(GenerationCatalogError::database)?;
    let rows = statement
        .query_map([profile_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(GenerationCatalogError::database)?;
    let mut generations = BTreeMap::new();
    for row in rows {
        let (id, manifest_id, state, activation, validated) =
            row.map_err(GenerationCatalogError::database)?;
        let generation_id = ManagedGenerationId::parse(id.clone()).map_err(|_| {
            GenerationCatalogError::InvalidCatalogValue {
                column: "generation_id",
                value: id,
            }
        })?;
        let state = ManagedGenerationState::parse(&state).map_err(|_| {
            GenerationCatalogError::InvalidCatalogValue {
                column: "lifecycle_state",
                value: state,
            }
        })?;
        let record = ManagedGenerationRecord {
            generation_id: generation_id.clone(),
            manifest_id,
            state,
            activation_epoch: parse_optional_epoch("activation_epoch", activation)?,
            validated_startup_epoch: parse_optional_epoch("validated_startup_epoch", validated)?,
        };
        generations.insert(generation_id, record);
    }
    let snapshot = ManagedGenerationSnapshot {
        profile,
        generations,
    };
    snapshot
        .validate()
        .map_err(GenerationCatalogError::InvalidTransition)?;
    Ok(snapshot)
}

fn parse_optional_id(
    column: &'static str,
    value: Option<String>,
) -> Result<Option<ManagedGenerationId>, GenerationCatalogError> {
    value
        .map(|value| {
            ManagedGenerationId::parse(value.clone())
                .map_err(|_| GenerationCatalogError::InvalidCatalogValue { column, value })
        })
        .transpose()
}

fn parse_epoch(column: &'static str, value: i64) -> Result<StartupEpoch, GenerationCatalogError> {
    StartupEpoch::new(parse_u64(column, value)?).map_err(|_| {
        GenerationCatalogError::InvalidCatalogValue {
            column,
            value: value.to_string(),
        }
    })
}

fn parse_optional_epoch(
    column: &'static str,
    value: Option<i64>,
) -> Result<Option<StartupEpoch>, GenerationCatalogError> {
    value.map(|value| parse_epoch(column, value)).transpose()
}

fn parse_u64(column: &'static str, value: i64) -> Result<u64, GenerationCatalogError> {
    u64::try_from(value).map_err(|_| GenerationCatalogError::InvalidCatalogValue {
        column,
        value: value.to_string(),
    })
}

fn optional_epoch_to_i64(
    value: Option<StartupEpoch>,
) -> Result<Option<i64>, GenerationCatalogError> {
    value.map(|value| u64_to_i64(value.get())).transpose()
}

fn u64_to_i64(value: u64) -> Result<i64, GenerationCatalogError> {
    i64::try_from(value).map_err(|_| GenerationCatalogError::RevisionOutOfRange)
}
