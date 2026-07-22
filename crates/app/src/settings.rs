//! Persistent user settings (orbok layer).
//!
//! [`OrbokSettings`] is the single source of truth for user-configurable
//! values that outlive a session. It is persisted as `settings.json`
//! at the explicit path captured in the immutable runtime context.
//!
//! The most important field is [`OrbokSettings::embedding_model_dir`]:
//! the startup wizard writes it when the user successfully locates an
//! embedding model folder. All other fields have safe `Default` values
//! that work out of the box.
//!
//! ## Note for the crate author
//!
//! `ConfigManager::new()` derives the config directory from the binary name.
//! The crate package and binary are both named `orbok` (resolved in v0.20.1),
//! so config paths are now stable.

use app_json_settings::ConfigManager;
use std::path::{Path, PathBuf};

/// All persistent user preferences.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OrbokSettings {
    /// Path to the folder containing `onnx/model.onnx` and
    /// `tokenizer.json` for the embedding model. Set by the startup
    /// wizard (RFC-021). `None` means semantic search has never been
    /// configured.
    pub embedding_model_dir: Option<String>,

    /// Path to the reranker model folder (optional, RFC-010).
    pub reranker_model_dir: Option<String>,

    /// Indexing quality mode (RFC-013).
    /// One of: `"balanced"` | `"high_accuracy"` | `"space_saving"`.
    pub index_mode: String,

    /// UI locale code — `"en"` or `"ja"` (RFC-031).
    pub locale: String,

    /// UI theme (RFC-032). One of: `"system"` | `"light"` | `"dark"` |
    /// `"high_contrast_light"` | `"high_contrast_dark"`. `"system"` is
    /// resolved to a concrete preset at startup.
    pub theme: String,

    /// UI text scale (RFC-035). One of: `"default"` | `"large"` | `"larger"`.
    pub text_scale: String,

    /// Whether to reduce motion (RFC-035). `true` suppresses non-essential
    /// animations. Defaults from OS signal; user can override.
    pub reduced_motion: bool,

    /// Whether reranking is enabled (RFC-010). Requires reranker model.
    pub rerank_enabled: bool,

    /// Whether background indexing is allowed (RFC-019).
    pub background_indexing: bool,

    /// Pause background indexing when on battery power.
    pub pause_on_battery: bool,

    /// Privacy mode (RFC-039 §5). One of: "standard" | "strict" | "portable".
    pub privacy_mode: String,

    /// Whether to persist recent search queries (RFC-039 §10).
    /// Forced off in Strict mode.
    pub remember_recent_searches: bool,

    /// Whether to cache result snippets across sessions (RFC-039 §11).
    pub persist_snippets: bool,

    /// Whether to clear temporary previews on app exit (RFC-039 §11).
    pub clear_temporary_previews_on_exit: bool,
}

impl Default for OrbokSettings {
    fn default() -> Self {
        Self {
            embedding_model_dir: None,
            reranker_model_dir: None,
            index_mode: "balanced".into(),
            locale: "en".into(),
            theme: "system".into(),
            text_scale: "default".into(),
            reduced_motion: false,
            rerank_enabled: false,
            background_indexing: true,
            pause_on_battery: true,
            privacy_mode: "standard".into(),
            remember_recent_searches: true,
            persist_snippets: true,
            clear_temporary_previews_on_exit: false,
        }
    }
}

/// Load settings from the platform config directory, or return defaults
/// if the file does not exist yet.
pub fn standard_settings_file() -> PathBuf {
    ConfigManager::<OrbokSettings>::new()
        .with_filename("settings.json")
        .path()
}

pub fn load_settings(path: &Path) -> OrbokSettings {
    let Ok(bytes) = std::fs::read(path) else {
        return OrbokSettings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Persist settings to the selected runtime profile.
pub fn save_settings(path: &Path, settings: &OrbokSettings) -> std::io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "settings file has no parent directory",
        )
    })?;
    std::fs::create_dir_all(directory)?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path, bytes)
}

impl OrbokSettings {
    /// Build effective [`PrivacySettings`] from the persisted strings,
    /// applying strict-mode overrides (RFC-039 §9, RFC-042 §14).
    pub fn privacy_settings(&self) -> orbok_core::PrivacySettings {
        orbok_core::PrivacySettings {
            mode: orbok_core::PrivacyMode::parse(&self.privacy_mode),
            remember_recent_searches: self.remember_recent_searches,
            persist_snippets: self.persist_snippets,
            clear_temporary_previews_on_exit: self.clear_temporary_previews_on_exit,
            diagnostics_include_paths: false,
            diagnostics_include_recent_searches: false,
        }
        .with_mode_applied()
    }

    /// Effective search-history settings (RFC-042 §7.3).
    pub fn history_settings(&self) -> orbok_core::SearchHistorySettings {
        orbok_core::SearchHistorySettings {
            remember_recent_searches: self.remember_recent_searches,
            ..Default::default()
        }
    }
}
