-- RFC-050 §3B: managed model-generation lifecycle state.
-- Filesystem mutation is serialized separately by the per-profile model-store
-- guard. These tables persist only generation identity, pointers, and epochs.

CREATE TABLE managed_model_profiles (
    profile_id TEXT PRIMARY KEY,
    startup_epoch INTEGER NOT NULL DEFAULT 0 CHECK (startup_epoch >= 0),
    current_generation_id TEXT,
    previous_generation_id TEXT,
    state_revision INTEGER NOT NULL DEFAULT 0 CHECK (state_revision >= 0),
    updated_at TEXT NOT NULL,
    CHECK (
        current_generation_id IS NULL OR
        previous_generation_id IS NULL OR
        current_generation_id <> previous_generation_id
    ),
    CHECK (previous_generation_id IS NULL OR current_generation_id IS NOT NULL),
    FOREIGN KEY (profile_id, current_generation_id)
        REFERENCES managed_model_generations(profile_id, generation_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (profile_id, previous_generation_id)
        REFERENCES managed_model_generations(profile_id, generation_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE managed_model_generations (
    generation_id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES managed_model_profiles(profile_id) ON DELETE CASCADE,
    manifest_id TEXT NOT NULL CHECK (length(manifest_id) > 0),
    lifecycle_state TEXT NOT NULL CHECK (
        lifecycle_state IN ('inactive', 'current', 'previous', 'invalid')
    ),
    activation_epoch INTEGER CHECK (activation_epoch IS NULL OR activation_epoch >= 0),
    validated_startup_epoch INTEGER CHECK (
        validated_startup_epoch IS NULL OR (
            activation_epoch IS NOT NULL AND
            validated_startup_epoch > activation_epoch
        )
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(profile_id, generation_id),
    CHECK (
        lifecycle_state NOT IN ('current', 'previous') OR
        activation_epoch IS NOT NULL
    )
);

CREATE UNIQUE INDEX idx_managed_generation_one_current
ON managed_model_generations(profile_id)
WHERE lifecycle_state = 'current';

CREATE UNIQUE INDEX idx_managed_generation_one_previous
ON managed_model_generations(profile_id)
WHERE lifecycle_state = 'previous';

CREATE INDEX idx_managed_generation_profile_state
ON managed_model_generations(profile_id, lifecycle_state);

CREATE TRIGGER managed_profile_insert_current_must_match_state
BEFORE INSERT ON managed_model_profiles
WHEN NEW.current_generation_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM managed_model_generations
    WHERE profile_id = NEW.profile_id
      AND generation_id = NEW.current_generation_id
      AND lifecycle_state = 'current'
)
BEGIN
    SELECT RAISE(ABORT, 'current generation pointer/state mismatch');
END;

CREATE TRIGGER managed_profile_insert_previous_must_match_state
BEFORE INSERT ON managed_model_profiles
WHEN NEW.previous_generation_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM managed_model_generations
    WHERE profile_id = NEW.profile_id
      AND generation_id = NEW.previous_generation_id
      AND lifecycle_state = 'previous'
)
BEGIN
    SELECT RAISE(ABORT, 'previous generation pointer/state mismatch');
END;

CREATE TRIGGER managed_profile_current_must_match_state
BEFORE UPDATE OF current_generation_id ON managed_model_profiles
WHEN NEW.current_generation_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM managed_model_generations
    WHERE profile_id = NEW.profile_id
      AND generation_id = NEW.current_generation_id
      AND lifecycle_state = 'current'
)
BEGIN
    SELECT RAISE(ABORT, 'current generation pointer/state mismatch');
END;

CREATE TRIGGER managed_profile_previous_must_match_state
BEFORE UPDATE OF previous_generation_id ON managed_model_profiles
WHEN NEW.previous_generation_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM managed_model_generations
    WHERE profile_id = NEW.profile_id
      AND generation_id = NEW.previous_generation_id
      AND lifecycle_state = 'previous'
)
BEGIN
    SELECT RAISE(ABORT, 'previous generation pointer/state mismatch');
END;

CREATE TRIGGER managed_generation_preserve_pointer_state
BEFORE UPDATE OF lifecycle_state ON managed_model_generations
WHEN (
    EXISTS (
        SELECT 1 FROM managed_model_profiles
        WHERE profile_id = OLD.profile_id
          AND current_generation_id = OLD.generation_id
    ) AND NEW.lifecycle_state <> 'current'
) OR (
    EXISTS (
        SELECT 1 FROM managed_model_profiles
        WHERE profile_id = OLD.profile_id
          AND previous_generation_id = OLD.generation_id
    ) AND NEW.lifecycle_state <> 'previous'
)
BEGIN
    SELECT RAISE(ABORT, 'referenced generation state mismatch');
END;
