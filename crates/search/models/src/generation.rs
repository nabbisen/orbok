//! Typed managed-generation lifecycle state for RFC-050 §3B.
//!
//! This module is pure: it does not touch SQLite or the filesystem. Later
//! phases may execute only transitions that are valid here and persist them
//! under the model-store mutation guard.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManagedGenerationId(String);

impl ManagedGenerationId {
    pub fn generate() -> Self {
        Self(format!("gen_{}", uuid::Uuid::now_v7()))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, GenerationValueError> {
        let value = value.into();
        let uuid = value
            .strip_prefix("gen_")
            .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
            .filter(|uuid| uuid.get_version() == Some(uuid::Version::SortRand))
            .ok_or(GenerationValueError::InvalidGenerationId)?;
        let canonical = format!("gen_{uuid}");
        if value != canonical {
            return Err(GenerationValueError::InvalidGenerationId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ManagedGenerationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelStoreProfileId(String);

impl ModelStoreProfileId {
    pub fn parse(value: impl Into<String>) -> Result<Self, GenerationValueError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GenerationValueError::InvalidProfileId);
        }
        Ok(Self(value))
    }

    pub fn default_embedding() -> Self {
        Self("default-embedding".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelStoreProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StartupEpoch(u64);

impl StartupEpoch {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Result<Self, GenerationValueError> {
        if value > i64::MAX as u64 {
            return Err(GenerationValueError::EpochOutOfRange);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, GenerationTransitionError> {
        let next = self
            .0
            .checked_add(1)
            .ok_or(GenerationTransitionError::EpochExhausted)?;
        Self::new(next).map_err(|_| GenerationTransitionError::EpochExhausted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedGenerationState {
    Inactive,
    Current,
    Previous,
    Invalid,
}

impl ManagedGenerationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Current => "current",
            Self::Previous => "previous",
            Self::Invalid => "invalid",
        }
    }

    pub fn parse(value: &str) -> Result<Self, GenerationValueError> {
        match value {
            "inactive" => Ok(Self::Inactive),
            "current" => Ok(Self::Current),
            "previous" => Ok(Self::Previous),
            "invalid" => Ok(Self::Invalid),
            _ => Err(GenerationValueError::InvalidLifecycleState),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedGenerationRecord {
    pub generation_id: ManagedGenerationId,
    pub manifest_id: String,
    pub state: ManagedGenerationState,
    pub activation_epoch: Option<StartupEpoch>,
    pub validated_startup_epoch: Option<StartupEpoch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedProfileState {
    pub profile_id: ModelStoreProfileId,
    pub startup_epoch: StartupEpoch,
    pub current_generation_id: Option<ManagedGenerationId>,
    pub previous_generation_id: Option<ManagedGenerationId>,
    pub state_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedGenerationSnapshot {
    pub profile: ManagedProfileState,
    pub generations: BTreeMap<ManagedGenerationId, ManagedGenerationRecord>,
}

impl ManagedGenerationSnapshot {
    pub fn empty(profile_id: ModelStoreProfileId) -> Self {
        Self {
            profile: ManagedProfileState {
                profile_id,
                startup_epoch: StartupEpoch::ZERO,
                current_generation_id: None,
                previous_generation_id: None,
                state_revision: 0,
            },
            generations: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), GenerationTransitionError> {
        if self.profile.current_generation_id == self.profile.previous_generation_id
            && self.profile.current_generation_id.is_some()
        {
            return Err(GenerationTransitionError::EqualCurrentAndPrevious);
        }
        if self.profile.current_generation_id.is_none()
            && self.profile.previous_generation_id.is_some()
        {
            return Err(GenerationTransitionError::PreviousWithoutCurrent);
        }

        let mut current_count = 0;
        let mut previous_count = 0;
        for (id, record) in &self.generations {
            if id != &record.generation_id {
                return Err(GenerationTransitionError::RecordIdentityMismatch);
            }
            if let Some(validated) = record.validated_startup_epoch {
                let activation = record
                    .activation_epoch
                    .ok_or(GenerationTransitionError::InvalidEpochPair)?;
                if validated <= activation {
                    return Err(GenerationTransitionError::InvalidEpochPair);
                }
            }
            match record.state {
                ManagedGenerationState::Current => {
                    current_count += 1;
                    if self.profile.current_generation_id.as_ref() != Some(id)
                        || record.activation_epoch.is_none()
                    {
                        return Err(GenerationTransitionError::PointerStateMismatch);
                    }
                }
                ManagedGenerationState::Previous => {
                    previous_count += 1;
                    if self.profile.previous_generation_id.as_ref() != Some(id)
                        || record.activation_epoch.is_none()
                    {
                        return Err(GenerationTransitionError::PointerStateMismatch);
                    }
                }
                ManagedGenerationState::Inactive | ManagedGenerationState::Invalid => {
                    if self.profile.current_generation_id.as_ref() == Some(id)
                        || self.profile.previous_generation_id.as_ref() == Some(id)
                    {
                        return Err(GenerationTransitionError::PointerStateMismatch);
                    }
                }
            }
        }
        if current_count != usize::from(self.profile.current_generation_id.is_some())
            || previous_count != usize::from(self.profile.previous_generation_id.is_some())
        {
            return Err(GenerationTransitionError::PointerStateMismatch);
        }
        Ok(())
    }

    pub fn register_inactive(
        &self,
        generation_id: ManagedGenerationId,
        manifest_id: impl Into<String>,
    ) -> Result<Self, GenerationTransitionError> {
        if self.generations.contains_key(&generation_id) {
            return Err(GenerationTransitionError::GenerationAlreadyExists);
        }
        let manifest_id = manifest_id.into();
        if manifest_id.is_empty() {
            return Err(GenerationTransitionError::EmptyManifestId);
        }
        let mut next = self.clone();
        next.generations.insert(
            generation_id.clone(),
            ManagedGenerationRecord {
                generation_id,
                manifest_id,
                state: ManagedGenerationState::Inactive,
                activation_epoch: None,
                validated_startup_epoch: None,
            },
        );
        next.bump_revision()?;
        next.validate()?;
        Ok(next)
    }

    pub fn advance_startup_epoch(&self) -> Result<Self, GenerationTransitionError> {
        let mut next = self.clone();
        next.profile.startup_epoch = next.profile.startup_epoch.next()?;
        next.bump_revision()?;
        next.validate()?;
        Ok(next)
    }

    pub fn activate(
        &self,
        generation_id: &ManagedGenerationId,
    ) -> Result<Self, GenerationTransitionError> {
        self.validate()?;
        let candidate = self
            .generations
            .get(generation_id)
            .ok_or(GenerationTransitionError::GenerationNotFound)?;
        if candidate.state != ManagedGenerationState::Inactive {
            return Err(GenerationTransitionError::GenerationNotInactive);
        }
        if let Some(current_id) = &self.profile.current_generation_id {
            let current = self
                .generations
                .get(current_id)
                .ok_or(GenerationTransitionError::PointerStateMismatch)?;
            if current.validated_startup_epoch.is_none() {
                return Err(GenerationTransitionError::CurrentNotStartupValidated);
            }
        }

        let mut next = self.clone();
        if let Some(old_previous) = next.profile.previous_generation_id.take() {
            next.generations
                .get_mut(&old_previous)
                .ok_or(GenerationTransitionError::PointerStateMismatch)?
                .state = ManagedGenerationState::Inactive;
        }
        if let Some(old_current) = next
            .profile
            .current_generation_id
            .replace(generation_id.clone())
        {
            next.generations
                .get_mut(&old_current)
                .ok_or(GenerationTransitionError::PointerStateMismatch)?
                .state = ManagedGenerationState::Previous;
            next.profile.previous_generation_id = Some(old_current);
        }
        let new_current = next
            .generations
            .get_mut(generation_id)
            .ok_or(GenerationTransitionError::GenerationNotFound)?;
        new_current.state = ManagedGenerationState::Current;
        new_current.activation_epoch = Some(next.profile.startup_epoch);
        new_current.validated_startup_epoch = None;
        next.bump_revision()?;
        next.validate()?;
        Ok(next)
    }

    pub fn validate_current_after_startup(
        &self,
        loaded_generation_id: &ManagedGenerationId,
    ) -> Result<Self, GenerationTransitionError> {
        self.validate()?;
        if self.profile.current_generation_id.as_ref() != Some(loaded_generation_id) {
            return Err(GenerationTransitionError::StaleValidationResult);
        }
        let mut next = self.clone();
        let current = next
            .generations
            .get_mut(loaded_generation_id)
            .ok_or(GenerationTransitionError::PointerStateMismatch)?;
        let activation = current
            .activation_epoch
            .ok_or(GenerationTransitionError::InvalidEpochPair)?;
        if next.profile.startup_epoch <= activation {
            return Err(GenerationTransitionError::ValidationNotFromLaterStartup);
        }
        current.validated_startup_epoch = Some(next.profile.startup_epoch);
        next.bump_revision()?;
        next.validate()?;
        Ok(next)
    }

    pub fn rollback_invalid_current(
        &self,
        previous_verified: bool,
    ) -> Result<Self, GenerationTransitionError> {
        self.validate()?;
        let current_id = self
            .profile
            .current_generation_id
            .as_ref()
            .ok_or(GenerationTransitionError::NoCurrentGeneration)?
            .clone();
        let mut next = self.clone();
        next.generations
            .get_mut(&current_id)
            .ok_or(GenerationTransitionError::PointerStateMismatch)?
            .state = ManagedGenerationState::Invalid;
        next.profile.current_generation_id = None;

        if let Some(previous_id) = next.profile.previous_generation_id.take() {
            let previous = next
                .generations
                .get_mut(&previous_id)
                .ok_or(GenerationTransitionError::PointerStateMismatch)?;
            if previous_verified {
                previous.state = ManagedGenerationState::Current;
                next.profile.current_generation_id = Some(previous_id);
            } else {
                previous.state = ManagedGenerationState::Invalid;
            }
        }
        next.bump_revision()?;
        next.validate()?;
        Ok(next)
    }

    fn bump_revision(&mut self) -> Result<(), GenerationTransitionError> {
        self.profile.state_revision = self
            .profile
            .state_revision
            .checked_add(1)
            .ok_or(GenerationTransitionError::RevisionExhausted)?;
        if self.profile.state_revision > i64::MAX as u64 {
            return Err(GenerationTransitionError::RevisionExhausted);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationValueError {
    InvalidGenerationId,
    InvalidProfileId,
    InvalidLifecycleState,
    EpochOutOfRange,
}

impl fmt::Display for GenerationValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid managed-generation value: {self:?}")
    }
}

impl std::error::Error for GenerationValueError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationTransitionError {
    EqualCurrentAndPrevious,
    PreviousWithoutCurrent,
    RecordIdentityMismatch,
    InvalidEpochPair,
    PointerStateMismatch,
    GenerationAlreadyExists,
    GenerationNotFound,
    GenerationNotInactive,
    CurrentNotStartupValidated,
    StaleValidationResult,
    ValidationNotFromLaterStartup,
    NoCurrentGeneration,
    EmptyManifestId,
    EpochExhausted,
    RevisionExhausted,
}

impl fmt::Display for GenerationTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid managed-generation transition: {self:?}")
    }
}

impl std::error::Error for GenerationTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_id() -> ManagedGenerationId {
        ManagedGenerationId::generate()
    }

    #[test]
    fn identifiers_are_path_safe_and_strict() {
        let generated = ManagedGenerationId::generate();
        assert_eq!(
            ManagedGenerationId::parse(generated.as_str()),
            Ok(generated)
        );
        assert!(ManagedGenerationId::parse("gen_../../escape").is_err());
        assert!(ManagedGenerationId::parse("gen_550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(ModelStoreProfileId::parse("default-embedding").is_ok());
        assert!(ModelStoreProfileId::parse("../profile").is_err());
    }

    #[test]
    fn second_activation_requires_later_startup_validation() {
        let a = new_id();
        let b = new_id();
        let state = ManagedGenerationSnapshot::empty(ModelStoreProfileId::default_embedding())
            .register_inactive(a.clone(), "manifest")
            .unwrap()
            .register_inactive(b.clone(), "manifest")
            .unwrap()
            .activate(&a)
            .unwrap();
        assert_eq!(
            state.activate(&b),
            Err(GenerationTransitionError::CurrentNotStartupValidated)
        );
        let state = state
            .advance_startup_epoch()
            .unwrap()
            .validate_current_after_startup(&a)
            .unwrap()
            .activate(&b)
            .unwrap();
        assert_eq!(state.profile.current_generation_id, Some(b));
        assert_eq!(state.profile.previous_generation_id, Some(a));
    }

    #[test]
    fn stale_or_same_startup_validation_is_rejected() {
        let a = new_id();
        let other = new_id();
        let state = ManagedGenerationSnapshot::empty(ModelStoreProfileId::default_embedding())
            .register_inactive(a.clone(), "manifest")
            .unwrap()
            .activate(&a)
            .unwrap();
        assert_eq!(
            state.validate_current_after_startup(&a),
            Err(GenerationTransitionError::ValidationNotFromLaterStartup)
        );
        let later = state.advance_startup_epoch().unwrap();
        assert_eq!(
            later.validate_current_after_startup(&other),
            Err(GenerationTransitionError::StaleValidationResult)
        );
    }

    #[test]
    fn rollback_never_keeps_invalid_current_as_previous() {
        let a = new_id();
        let b = new_id();
        let active_b = ManagedGenerationSnapshot::empty(ModelStoreProfileId::default_embedding())
            .register_inactive(a.clone(), "manifest")
            .unwrap()
            .register_inactive(b.clone(), "manifest")
            .unwrap()
            .activate(&a)
            .unwrap()
            .advance_startup_epoch()
            .unwrap()
            .validate_current_after_startup(&a)
            .unwrap()
            .activate(&b)
            .unwrap();
        let rolled_back = active_b.rollback_invalid_current(true).unwrap();
        assert_eq!(rolled_back.profile.current_generation_id, Some(a.clone()));
        assert_eq!(rolled_back.profile.previous_generation_id, None);
        assert_eq!(
            rolled_back.generations[&b].state,
            ManagedGenerationState::Invalid
        );
        assert_eq!(
            rolled_back.generations[&a].state,
            ManagedGenerationState::Current
        );
    }

    #[test]
    fn rollback_with_two_invalid_generations_clears_both_pointers() {
        let a = new_id();
        let b = new_id();
        let active_b = ManagedGenerationSnapshot::empty(ModelStoreProfileId::default_embedding())
            .register_inactive(a.clone(), "manifest")
            .unwrap()
            .register_inactive(b.clone(), "manifest")
            .unwrap()
            .activate(&a)
            .unwrap()
            .advance_startup_epoch()
            .unwrap()
            .validate_current_after_startup(&a)
            .unwrap()
            .activate(&b)
            .unwrap();
        let rolled_back = active_b.rollback_invalid_current(false).unwrap();
        assert_eq!(rolled_back.profile.current_generation_id, None);
        assert_eq!(rolled_back.profile.previous_generation_id, None);
        assert_eq!(
            rolled_back.generations[&a].state,
            ManagedGenerationState::Invalid
        );
        assert_eq!(
            rolled_back.generations[&b].state,
            ManagedGenerationState::Invalid
        );
    }
}
