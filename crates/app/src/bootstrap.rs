//! Backend bootstrap: data-directory resolution, catalog open, settings
//! load, model verification, and initial view-model population.
//!
//! Startup sequence (RFC-027, design §startup):
//! 1. resolve data directory (env > portable flag > platform dir)
//! 2. open catalog and run migrations
//! 3. run startup recovery (RFC-018)
//! 4. load `OrbokSettings` from platform config dir
//! 5. verify embedding model files (design §startup-verify)
//! 6. build initial `AppState` (wizard active if model missing)

use crate::settings::OrbokSettings;
use orbok::runtime_context::{
    AllowRuntimePathProbe, PlatformRuntimePaths, RuntimeContext, RuntimeMode, RuntimePathProbe,
    RuntimeSelection,
};
#[cfg(test)]
use orbok::runtime_storage::ProfileModelStore;

mod cleanup;
pub(crate) mod embedding_resolution;
mod model_resolution;
mod preferences;
mod search;
mod sources;
mod startup;
#[cfg(test)]
mod tests;

pub use cleanup::{clean_search_cache, clean_snippets, reset_catalog};
pub use preferences::{
    persist_locale, persist_model_dir, persist_reduced_motion, persist_text_scale, persist_theme,
    remove_managed_model_dir_setting, resolve_os_reduced_motion,
};
// `_with` variants below take an explicit `RuntimePathProbe`; the only
// caller that needs one directly (rather than through the `AllowRuntimePathProbe`
// default the plain wrapper uses) is `runtime_isolation_tests.rs`'s
// `RecordingProbe`. Re-exporting them unconditionally would make this a
// dead `pub(crate) use` outside test builds and fail the `-D warnings` gate.
#[cfg(test)]
pub(crate) use preferences::{persist_model_dir_with, persist_theme_with};
pub(crate) use search::run_search;
#[cfg(test)]
pub(crate) use search::run_search_with;
pub use sources::{
    add_source, check_and_refresh_source, find_source_by_canonical_path, remove_source,
    scan_and_index_source,
};
// `get_sources` re-exported since Task 035: `Message::SourceRefreshRequested`'s
// handler in `main.rs` needs to re-fetch the sources list after a refresh
// (the status it may have just changed), the same reach `get_health` has
// long had for the equivalent reason.
pub use startup::{get_health, get_sources, load_initial_state, run_check};
#[cfg(test)]
pub(crate) use startup::{load_initial_state_with, run_check_with};

/// Capture process inputs once and construct the immutable RFC-049 context.
pub fn resolve_runtime_context(
    portable: bool,
) -> Result<RuntimeContext, Box<dyn std::error::Error>> {
    let startup_dir = std::env::current_dir()?;
    let data_override = std::env::var_os("ORBOK_DATA_DIR");
    let standard_data_dir = dirs::data_local_dir().map(|directory| directory.join("orbok"));
    // RFC-055 §3/§4.2: capture stays unconditional -- resolve the platform
    // settings directory here regardless of mode, and record absence as
    // `None` rather than branching on mode before resolving. `.ok()`
    // discards the specific `ConfigError` (`for_app`'s only fallible step --
    // in practice always `ConfigError::Platform`, since "orbok" is a fixed,
    // always-valid path component) without matching on it, so a future
    // upstream variant addition needs no change here.
    let standard_settings_dir = crate::settings::standard_settings_dir().ok();
    let selection = RuntimeSelection::resolve(portable, data_override)?;
    let context = RuntimeContext::resolve(
        selection,
        &startup_dir,
        PlatformRuntimePaths {
            standard_data_dir: standard_data_dir.as_deref(),
            standard_settings_dir: standard_settings_dir.as_deref(),
        },
    )?;
    if context.mode() == RuntimeMode::Portable {
        orbok::physical_identity::validate_physical_profile_separation(
            &context,
            standard_data_dir.as_deref(),
            standard_settings_dir.as_deref(),
        )?;
    }
    Ok(context)
}

// RFC-049 Slice 2: the storage boundary (`RuntimeStorage`, `ProfileCache`,
// `ProfileModelStore`) lives in `orbok::runtime_storage`, in the library
// crate alongside `RuntimeContext`, because `RuntimeContext`'s path
// accessors are `pub(crate)` there. This binary crate can only reach a
// profile resource through that sealed API. The functions below are thin,
// `OrbokSettings`-typed wrappers around that generic API for this file's
// call sites.
pub use orbok::runtime_storage::cache as cache_service;
pub use orbok::runtime_storage::{model_store, open_catalog};

pub fn load_runtime_settings(context: &RuntimeContext) -> std::io::Result<OrbokSettings> {
    runtime_settings_with(context, &AllowRuntimePathProbe)
}

pub(crate) fn runtime_settings_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> std::io::Result<OrbokSettings> {
    orbok::runtime_storage::load_settings_with(context, probe)
}

pub fn save_runtime_settings(
    context: &RuntimeContext,
    settings: &OrbokSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    save_runtime_settings_with(context, &AllowRuntimePathProbe, settings)
}

pub(crate) fn save_runtime_settings_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    settings: &OrbokSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    orbok::runtime_storage::save_settings_with(context, probe, settings)
        .map_err(|error| format!("settings save failed: {error:?}").into())
}

#[cfg(test)]
pub fn ensure_default_model_store<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> std::io::Result<ProfileModelStore> {
    orbok::runtime_storage::RuntimeStorage::new(context, probe).model_store()
}
