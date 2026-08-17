//! Theme selection, text scale, and token-reading helpers (RFC-032, RFC-035).
//!
//! This module is the single place the UI reads design *values*. View code
//! never uses literal font sizes, paddings, or colors; it calls these helpers,
//! which read the active [`snora::design::Tokens`] bundle from
//! [`crate::state::AppState::tokens`], optionally scaled by
//! [`TextScale`] from [`crate::state::AppState::text_scale`].
//!
//! snora remains the sole gateway to the design vocabulary — these helpers
//! wrap the `snora::design` style bridge and never invent values.

use iced::Pixels;
use serde::{Deserialize, Serialize};
use snora::design::Tokens;

use crate::i18n::MessageKey;

// ── Theme ─────────────────────────────────────────────────────────────────

/// User-selectable UI theme. `System` is resolved to a concrete variant at
/// startup (in `orbok`) via [`Theme::from_env`]; the other four map
/// directly to the built-in Snora Design presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the operating system preference (resolved at startup).
    #[default]
    System,
    /// Calm light theme.
    Light,
    /// Low-glare dark theme.
    Dark,
    /// High-contrast light theme (accessibility).
    HighContrastLight,
    /// High-contrast dark theme (accessibility).
    HighContrastDark,
}

impl Theme {
    /// All selectable themes, in display order.
    pub const ALL: &'static [Theme] = &[
        Theme::System,
        Theme::Light,
        Theme::Dark,
        Theme::HighContrastLight,
        Theme::HighContrastDark,
    ];

    /// Setting string stored in `OrbokSettings`.
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::HighContrastLight => "high_contrast_light",
            Theme::HighContrastDark => "high_contrast_dark",
        }
    }

    /// Parse a stored setting string back into a [`Theme`].
    pub fn parse(s: &str) -> Option<Theme> {
        Some(match s {
            "system" => Theme::System,
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            "high_contrast_light" => Theme::HighContrastLight,
            "high_contrast_dark" => Theme::HighContrastDark,
            _ => return None,
        })
    }

    /// The i18n key for this theme's display name (Settings picker).
    pub fn label_key(self) -> MessageKey {
        match self {
            Theme::System => MessageKey::ThemeSystem,
            Theme::Light => MessageKey::ThemeLight,
            Theme::Dark => MessageKey::ThemeDark,
            Theme::HighContrastLight => MessageKey::ThemeHighContrastLight,
            Theme::HighContrastDark => MessageKey::ThemeHighContrastDark,
        }
    }

    /// The concrete Snora Design token bundle for this theme.
    ///
    /// `System` falls back to Light here; the real OS resolution happens once
    /// at startup in `orbok` via [`Theme::from_env`]. Selecting `System`
    /// at runtime therefore applies Light until the next launch (RFC-032 §5.5).
    pub fn tokens(self) -> Tokens {
        match self {
            Theme::Light | Theme::System => Tokens::light(),
            Theme::Dark => Tokens::dark(),
            Theme::HighContrastLight => Tokens::high_contrast_light(),
            Theme::HighContrastDark => Tokens::high_contrast_dark(),
        }
    }

    /// Best-effort OS colour-scheme probe for the `System` theme.
    ///
    /// Checks `ORBOK_THEME` env var first (concrete override). Returns `None`
    /// when the OS preference is unknown, leaving the caller to fall back to
    /// `Light`. A richer per-platform probe is a tracked follow-up (RFC-032 §9).
    pub fn from_env() -> Option<Theme> {
        let raw = std::env::var("ORBOK_THEME").ok()?;
        match Theme::parse(raw.trim()) {
            Some(Theme::System) | None => None,
            concrete => concrete,
        }
    }
}

// ── Text scale (RFC-035) ──────────────────────────────────────────────────

/// User-selectable text scale applied uniformly to all typography roles.
///
/// Three discrete steps ("less is more"): predictable for layout testing and
/// avoids unbounded-growth risks of a continuous slider. Scrollable page
/// wrappers absorb reflow at `Larger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextScale {
    /// Default token sizes (1×).
    #[default]
    Default,
    /// 15 % larger (1.15×).
    Large,
    /// 30 % larger (1.3×).
    Larger,
}

impl TextScale {
    pub const ALL: &'static [TextScale] =
        &[TextScale::Default, TextScale::Large, TextScale::Larger];

    pub fn factor(self) -> f32 {
        match self {
            TextScale::Default => 1.0,
            TextScale::Large => 1.15,
            TextScale::Larger => 1.3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TextScale::Default => "default",
            TextScale::Large => "large",
            TextScale::Larger => "larger",
        }
    }

    pub fn parse(s: &str) -> Option<TextScale> {
        Some(match s {
            "default" => TextScale::Default,
            "large" => TextScale::Large,
            "larger" => TextScale::Larger,
            _ => return None,
        })
    }

    pub fn label_key(self) -> MessageKey {
        match self {
            TextScale::Default => MessageKey::TextScaleDefault,
            TextScale::Large => MessageKey::TextScaleLarge,
            TextScale::Larger => MessageKey::TextScaleLarger,
        }
    }
}

// ── Typography helpers ────────────────────────────────────────────────────
//
// Unscaled variants: used by components.rs and wherever token sizes are
// consumed directly (padding, icon sizes, etc.).
//
// Scaled variants: views call these, passing `state.text_scale`, so the
// user's text-size preference propagates everywhere with no per-view change.

fn scale(px: Pixels, s: TextScale) -> Pixels {
    Pixels(px.0 * s.factor())
}

/// Page / section heading size (unscaled).
pub fn heading(t: &Tokens) -> Pixels {
    snora::design::style::text::heading_size(t)
}
/// Heading scaled by user preference.
pub fn heading_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(heading(t), s)
}

/// Card / dialog / metric title size (unscaled).
pub fn title(t: &Tokens) -> Pixels {
    snora::design::style::text::title_size(t)
}
/// Title scaled.
pub fn title_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(title(t), s)
}

/// Ordinary body text size (unscaled).
pub fn body(t: &Tokens) -> Pixels {
    snora::design::style::text::body_size(t)
}
/// Body scaled.
pub fn body_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(body(t), s)
}

/// Line-height multiplier for wrapping body prose (snora `body`, 1.4 by
/// default). `LineHeight::Relative` is a multiplier of the font size, and
/// the size is already scaled by [`TextScale`] wherever `body_s` is used
/// -- so unlike the `_s` size helpers, this needs no separate scaled
/// variant (RFC-032, Task 028 §3). Only for text that actually wraps to a
/// second line; a single-line site gains only unused vertical space from
/// this (Task 028 §2).
pub fn body_lh(t: &Tokens) -> iced::widget::text::LineHeight {
    iced::widget::text::LineHeight::Relative(t.typography.body.line_height)
}

/// Secondary metadata / compact help size (unscaled).
pub fn meta(t: &Tokens) -> Pixels {
    snora::design::style::text::body_small_size(t)
}
/// Meta scaled.
pub fn meta_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(meta(t), s)
}

/// Line-height multiplier for wrapping secondary text (snora `body_small`,
/// 1.35 by default). See [`body_lh`]'s own comment for why there is no
/// scaled variant and why this is not for single-line sites.
pub fn meta_lh(t: &Tokens) -> iced::widget::text::LineHeight {
    iced::widget::text::LineHeight::Relative(t.typography.body_small.line_height)
}

/// Button / chip / control label size (unscaled).
pub fn label(t: &Tokens) -> Pixels {
    snora::design::style::text::label_size(t)
}
/// Label scaled.
pub fn label_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(label(t), s)
}
