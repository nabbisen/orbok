//! Locale, theme, and accessibility/model-directory preference persistence.

use orbok::runtime_context::{AllowRuntimePathProbe, RuntimeContext, RuntimePathProbe};
use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_db::repo::SettingsRepository;
use orbok_ui::i18n::Locale;
use orbok_ui::theme::{TextScale, Theme};

/// Persist locale to the catalog (called when the user changes language).
pub fn persist_locale(catalog: &Catalog, locale: &Locale) -> OrbokResult<()> {
    SettingsRepository::new(catalog).set("ui.locale", &locale.as_str().to_string())
}

/// Persist the selected UI theme to `OrbokSettings` (RFC-032).
pub fn persist_theme(
    context: &RuntimeContext,
    theme: Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_theme_with(context, &AllowRuntimePathProbe, theme)
}

pub(crate) fn persist_theme_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    theme: Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = super::runtime_settings_with(context, probe)?;
    settings.theme = theme.as_str().to_string();
    super::save_runtime_settings_with(context, probe, &settings)
}

/// Persist the text scale to `OrbokSettings` (RFC-035).
pub fn persist_text_scale(
    context: &RuntimeContext,
    scale: TextScale,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = super::runtime_settings_with(context, &AllowRuntimePathProbe)?;
    settings.text_scale = scale.as_str().to_string();
    super::save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)
}

/// Persist the reduced-motion preference to `OrbokSettings` (RFC-035).
pub fn persist_reduced_motion(
    context: &RuntimeContext,
    val: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = super::runtime_settings_with(context, &AllowRuntimePathProbe)?;
    settings.reduced_motion = val;
    super::save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)
}

/// Best-effort OS reduced-motion probe (RFC-035).
///
/// Checks `ORBOK_REDUCE_MOTION=1` env var (override / test hook). A
/// richer per-platform probe (portal, SPI_GETCLIENTAREAANIMATION, NSWorkspace)
/// is a tracked follow-up — returns `false` when unknown.
pub fn resolve_os_reduced_motion() -> bool {
    std::env::var("ORBOK_REDUCE_MOTION")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Persist the validated model directory to `OrbokSettings` (called when
/// the user completes the wizard and accepts a model folder).
pub fn persist_model_dir(
    context: &RuntimeContext,
    model_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_model_dir_with(context, &AllowRuntimePathProbe, model_dir)
}

pub(crate) fn persist_model_dir_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    model_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = super::runtime_settings_with(context, probe)?;
    settings.embedding_model_dir = Some(model_dir.to_string());
    super::save_runtime_settings_with(context, probe, &settings)
}

pub fn remove_managed_model_dir_setting(
    context: &RuntimeContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = super::runtime_settings_with(context, &AllowRuntimePathProbe)?;
    let store = super::model_store(context)?;
    if settings
        .embedding_model_dir
        .as_ref()
        .is_some_and(|path| store.contains(std::path::Path::new(path)))
    {
        settings.embedding_model_dir = None;
        super::save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)?;
    }
    Ok(())
}
