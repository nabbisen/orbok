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
//! The config directory identity is the explicit literal `"orbok"`
//! (`ConfigManager::for_app("orbok")`, RFC-055 §3), not derived from the
//! running executable's name. The crate package and binary happen to
//! also be named `orbok`, but that is no longer load-bearing: renaming
//! the binary cannot change the settings location, and an executable
//! name that failed to resolve cannot silently fall back to a shared
//! literal (RFC-055 §2.4).

use app_json_settings::ConfigManager;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

/// All persistent user preferences.
///
/// `Serialize` and `Deserialize` are written out below around the derived
/// ones (`remote = "Self"`) for one reason: a `settings.json` an older orbok
/// wrote may carry `"privacy_mode": "strict"`, a field this struct no longer
/// has (RFC-039 amendment, Task 115), and that value must still turn recent
/// searches off, once, as it did.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(remote = "Self")]
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

    /// UI locale code — `"en"` | `"ja"` | `"auto"` (RFC-031 §48). `"auto"`
    /// is not a `Locale` variant; `Locale::parse` returns `None` for it by
    /// design, which is what lets the startup priority chain fall through
    /// to OS detection (RFC-031 §130, §166) -- see the default below.
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

    /// Skip the embedding queue while on battery power (RFC-036 §13.2,
    /// RFC-057 §4.3a). Renamed from `pause_on_battery` (RFC-057 §4.4): the
    /// old name promised more than it did even once wired -- reading it,
    /// a user would reasonably expect indexing to stop entirely on
    /// battery, but only embedding ever does; files keep being scanned,
    /// extracted, chunked and made keyword-searchable, only vectors wait.
    /// `#[serde(alias)]` keeps a profile's existing `settings.json`
    /// honoring its saved preference across the rename with no migration
    /// step -- old files are read under the new name and, once saved
    /// again, are written under it too.
    #[serde(alias = "pause_on_battery")]
    pub pause_embedding_on_battery: bool,

    /// Whether to persist recent search queries (RFC-039 §10): the one
    /// privacy control, and the whole truth about whether a search is
    /// recorded.
    pub remember_recent_searches: bool,

    /// Whether to clear temporary previews on app exit (RFC-039 §11).
    pub clear_temporary_previews_on_exit: bool,
}

impl Default for OrbokSettings {
    fn default() -> Self {
        Self {
            embedding_model_dir: None,
            reranker_model_dir: None,
            index_mode: "balanced".into(),
            // RFC-031 §48/§166, Task 009: "auto" is load-bearing, not "en".
            // A fresh profile must reach OS locale detection; "en" as the
            // literal default would satisfy Locale::parse on the first
            // priority-chain step and never fall through. Existing profiles
            // that already have "en" written to disk (every profile that
            // has ever launched orbok, per RFC-049 C4) are unaffected --
            // this default applies to new profiles only.
            locale: "auto".into(),
            theme: "system".into(),
            text_scale: "default".into(),
            reduced_motion: false,
            rerank_enabled: false,
            background_indexing: true,
            pause_embedding_on_battery: true,
            remember_recent_searches: true,
            clear_temporary_previews_on_exit: false,
        }
    }
}

/// The standard profile's settings **directory** under the platform config
/// directory, or an error if that directory cannot be resolved (RFC-055
/// §3 -- reported, never substituted). Path derivation only: settings I/O
/// goes through `orbok::runtime_storage`'s RFC-049 boundary, never through
/// this crate's own `load`/`save`/`load_or_default`.
///
/// Directory, not file: the sole production caller
/// (`bootstrap::resolve_runtime_context`) only ever wanted the directory --
/// `RuntimeContext` re-derives the file path itself as
/// `settings_dir.join(SETTINGS_FILE)`. Asking `app-json-settings` for the
/// file via `try_with_filename` and then immediately discarding the
/// filename with `.parent()` (Task 019) was based on an incorrect belief
/// that `folder_path()` required keeping the `ConfigManager` alive beyond
/// this call, which RFC-049 forbids -- it does not: `folder_path()` borrows
/// from the manager, but `.to_path_buf()` copies out within the same
/// expression, and the manager drops at the end of the statement.
/// `try_with_filename` is not called at all here, which is stronger than
/// Task 008's checked-setter fix (`rfcs/done/055-settings-path-fail-closed.md`
/// §12): a filename that is never passed cannot be the wrong one.
pub fn standard_settings_dir() -> app_json_settings::Result<PathBuf> {
    Ok(ConfigManager::<OrbokSettings>::for_app("orbok")?
        .folder_path()
        .to_path_buf())
}

/// Test-fixture only: production settings load/save now goes through
/// `orbok::runtime_storage`'s generic, boundary-mediated implementation.
/// These remain for tests that deliberately write/read a known profile path
/// directly to seed or verify fixtures, bypassing the app's own boundary on
/// purpose (Correction Request 111 §4 C1).
#[cfg(test)]
pub fn load_settings(path: &Path) -> OrbokSettings {
    let Ok(bytes) = std::fs::read(path) else {
        return OrbokSettings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

#[cfg(test)]
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

// A separate file, not an inline `mod tests { ... }`, deliberately: the
// production-boundary scan in `runtime_isolation_tests.rs` reads this
// file's raw source text via `include_str!` and asserts `for_app("orbok")`
// appears exactly once and `new()` appears nowhere in it. A test that
// calls both (RFC-055 §9.1's compatibility measurement, below) belongs
// outside that text, exactly as `runtime_context.rs` already keeps its
// `tests` module in `runtime_context/tests.rs` rather than inline.
#[cfg(test)]
mod tests;

impl serde::Serialize for OrbokSettings {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> serde::Deserialize<'de> for OrbokSettings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        // Compatibility (Task 115 §1.4): only `"strict"` means anything. It is
        // read once, as the toggle Off; the next save no longer writes the
        // field, and the file then says the same thing itself. Any other
        // value, or none, changes nothing.
        let strict = value
            .get("privacy_mode")
            .and_then(serde_json::Value::as_str)
            == Some("strict");
        let mut settings = Self::deserialize(value).map_err(D::Error::custom)?;
        if strict {
            settings.remember_recent_searches = false;
        }
        Ok(settings)
    }
}

impl OrbokSettings {
    /// Build effective [`PrivacySettings`] from the persisted values.
    pub fn privacy_settings(&self) -> orbok_core::PrivacySettings {
        orbok_core::PrivacySettings {
            remember_recent_searches: self.remember_recent_searches,
            clear_temporary_previews_on_exit: self.clear_temporary_previews_on_exit,
            diagnostics_include_paths: false,
            diagnostics_include_recent_searches: false,
        }
    }

    /// Effective search-history settings (RFC-042 §7.3).
    pub fn history_settings(&self) -> orbok_core::SearchHistorySettings {
        orbok_core::SearchHistorySettings {
            remember_recent_searches: self.remember_recent_searches,
            ..Default::default()
        }
    }
}
