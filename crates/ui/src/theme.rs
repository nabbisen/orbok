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

/// User-selectable UI theme. `System` is resolved to a concrete variant by
/// [`Theme::resolved`], continuously, from the OS's own detected light/dark
/// mode (Task 138); the other four map directly to the built-in Snora
/// Design presets and ignore the OS entirely.
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
    /// `System` falls back to Light here, since resolving it for real needs
    /// the OS's detected light/dark mode, which this method has no way to
    /// take -- call [`Theme::resolved`] first wherever that mode is known
    /// (`AppState::tokens`'s own construction, Task 138), and only read
    /// `tokens()` directly where a theme is already concrete, or where a
    /// placeholder for `System` is genuinely wanted (a Settings preview, for
    /// example, that shows what each named option looks like on its own).
    ///
    /// Task 140 §1a.2/§1a.4: Light and Dark use snora's AAA presets
    /// (`Tokens::light_aaa`/`dark_aaa` -- every declared text pair at
    /// 7:1, not light/dark's own AA floor), and every preset's
    /// `targets.min` is set to the house 44 px minimum here, in this one
    /// function, so every snora design control measures at least 44x44
    /// regardless of which theme is active; `AppState::tokens` and the
    /// scaled `snora_tokens` copy derived from it (`theme::snora_tokens`)
    /// both carry it, since the copy clones from this return value.
    pub fn tokens(self) -> Tokens {
        let mut tokens = match self {
            Theme::Light | Theme::System => Tokens::light_aaa(),
            Theme::Dark => Tokens::dark_aaa(),
            Theme::HighContrastLight => Tokens::high_contrast_light(),
            Theme::HighContrastDark => Tokens::high_contrast_dark(),
        };
        tokens.targets.min = 44.0;
        tokens
    }

    /// The concrete theme this resolves to right now (Task 138, RFC-032
    /// §5.5). Every variant but `System` ignores `system_mode` and returns
    /// itself unchanged -- a concrete theme never reacts to the OS. `System`
    /// resolves to [`Theme::from_env`]'s override when one is set (unchanged
    /// behaviour, still an explicit override for tests and diagnostics), else
    /// to `system_mode`: `Dark` → [`Theme::Dark`], `Light` or `None` (no
    /// preference reported) → [`Theme::Light`] -- one documented fallback,
    /// here, not duplicated at each caller.
    pub fn resolved(self, system_mode: iced::theme::Mode) -> Theme {
        match self {
            Theme::System => Theme::from_env().unwrap_or(match system_mode {
                iced::theme::Mode::Dark => Theme::Dark,
                iced::theme::Mode::Light | iced::theme::Mode::None => Theme::Light,
            }),
            concrete => concrete,
        }
    }

    /// An explicit override for tests and diagnostics, read from the
    /// `ORBOK_THEME` env var. Takes priority over the OS's own detected
    /// mode in [`Theme::resolved`]. Returns `None` when the var is unset, or
    /// set to something other than a concrete theme name (including
    /// `"system"` itself, which would otherwise be a no-op override).
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
/// Four discrete steps ("less is more"): predictable for layout testing and
/// avoids unbounded-growth risks of a continuous slider. Scrollable page
/// wrappers absorb reflow at `Larger`/`Largest`.
///
/// Task 140 §1 (RFC-066 §3.2): `Largest` (×2.0) is new; `Large` and
/// `Larger` grow from their original 1.15x/1.3x to 1.25x/1.5x, a step
/// RFC-066 asked for so the four steps read as one even progression
/// rather than three small ones and a jump. `large`/`larger` keep their
/// saved setting names -- only the factor each name maps to changes, so
/// a settings file saved before this task still parses, just scaled
/// differently once loaded (Task 117's "a file that cannot be parsed is
/// kept, otherwise it's trusted" salvage behaviour is about a value this
/// enum doesn't recognize at all, which is unaffected: every string
/// `parse` already accepted still parses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextScale {
    /// Default token sizes (1x).
    #[default]
    Default,
    /// 25 % larger (1.25x).
    Large,
    /// 50 % larger (1.5x).
    Larger,
    /// 100% larger (2.0x).
    Largest,
}

impl TextScale {
    pub const ALL: &'static [TextScale] = &[
        TextScale::Default,
        TextScale::Large,
        TextScale::Larger,
        TextScale::Largest,
    ];

    pub fn factor(self) -> f32 {
        match self {
            TextScale::Default => 1.0,
            TextScale::Large => 1.25,
            TextScale::Larger => 1.5,
            TextScale::Largest => 2.0,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TextScale::Default => "default",
            TextScale::Large => "large",
            TextScale::Larger => "larger",
            TextScale::Largest => "largest",
        }
    }

    pub fn parse(s: &str) -> Option<TextScale> {
        Some(match s {
            "default" => TextScale::Default,
            "large" => TextScale::Large,
            "larger" => TextScale::Larger,
            "largest" => TextScale::Largest,
            _ => return None,
        })
    }

    pub fn label_key(self) -> MessageKey {
        match self {
            TextScale::Default => MessageKey::TextScaleDefault,
            TextScale::Large => MessageKey::TextScaleLarge,
            TextScale::Larger => MessageKey::TextScaleLarger,
            TextScale::Largest => MessageKey::TextScaleLargest,
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

// ── snora tokens (Task 127, Review 305 §2) ─────────────────────────────────
//
// snora 0.52's `design` widgets read `Typography` for their own text instead
// of a fixed literal. **The rule this file now follows, on both sides:**
//
//   Everything snora draws takes the scaled tokens ([`snora_tokens`]).
//   Everything orbok draws takes the base tokens (`AppState::tokens`) times
//   `sc`, through `body_s`/`title_s`/`heading_s` below.
//
// One scale, applied once, on each side. [`snora_tokens`] is the *only*
// place the scaled copy is built, and it must never reach `body_s` and its
// siblings -- they already multiply by `s` themselves, so a scaled input
// would scale twice (`orbok_s_own_text_is_scaled_once_not_twice`, the UI
// crate's own test, proves the arithmetic both ways).
//
// Every snora-drawn surface orbok renders takes this: the sidebar and its
// tooltip, the tab bar (`shell::view`), the notice (`views::friendly_notice`)
// and the search row's folder chip (`components::removable_chip`, called
// from `views::search_location_row`).

/// Tokens scaled for every snora-drawn surface (Task 136). The *only* type
/// a `components`/`shell`/`views` function that hands tokens to a
/// `snora::design`/`snora::design::widget` constructor which renders its
/// own text accepts -- a plain `&Tokens` (the base, unscaled bundle) no
/// longer compiles there, which is the whole point: passing the wrong
/// bundle was previously a silent, correctly-typed mistake (Review 313
/// §2.2).
///
/// Its only public constructor is [`snora_tokens`], so every instance is
/// genuinely scaled; nothing elsewhere in this crate can build one from a
/// base `Tokens` by hand.
///
/// No `Deref` to `Tokens` (Review 314 §3): that would let a `&SnoraTokens`
/// reach `body_s` and its siblings through coercion alone, scaling twice
/// (1.69×) -- exactly the mistake the module comment above warns against.
/// [`SnoraTokens::for_snora_widget`] is the one, explicit way out, named
/// for the only thing it is for.
#[derive(Debug, Clone)]
pub struct SnoraTokens(Tokens);

impl SnoraTokens {
    /// The scaled bundle, for a `snora::design`/`snora::design::widget`
    /// constructor that renders its own text (or reads a scale-independent
    /// field, such as spacing or palette, it also exposes) -- never for a
    /// `theme::*_s` helper, which would scale what is already scaled.
    pub(crate) fn for_snora_widget(&self) -> &Tokens {
        &self.0
    }
}

/// A copy of `tokens` with every [`snora::design::Tokens::typography`] role's
/// size multiplied by `s`; line heights are untouched (already a multiplier
/// of the size, not a size themselves). Pass this, never the base tokens, to
/// any `snora::design` widget that renders its own text -- see the rule
/// above.
pub fn snora_tokens(tokens: &Tokens, s: TextScale) -> SnoraTokens {
    let mut scaled = tokens.clone();
    let f = s.factor();
    scaled.typography.body.size *= f;
    scaled.typography.body_small.size *= f;
    scaled.typography.label.size *= f;
    scaled.typography.title.size *= f;
    scaled.typography.heading.size *= f;
    scaled.typography.display.size *= f;
    SnoraTokens(scaled)
}

/// Task 140 §4 (RFC-066 §3.5): the one weight a page heading or a card
/// title (a result's own title, a folder card's name) uses; body text
/// stays regular. A system with no semibold/bold face for a given
/// script (checked for Japanese, Task 140's own stop condition) falls
/// back to its nearest available weight on its own -- `iced`'s text
/// renderer (`cosmic-text`) does this without orbok asking for it, the
/// same as any other font request for a glyph a face does not cover.
pub fn semibold() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }
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
/// this (Task 028 §2). Delegates to snora's own helper (RFC-068, snora
/// 0.38.0) rather than reading `t.typography.body.line_height` and
/// constructing `LineHeight::Relative` by hand, as this did before Task
/// 030 -- same module/shape as every other helper in this file, same
/// values, one fewer place duplicating what snora now maintains.
pub fn body_lh(t: &Tokens) -> iced::widget::text::LineHeight {
    snora::design::style::text::body_line_height(t)
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
/// scaled variant, why this is not for single-line sites, and why this
/// delegates to snora's own helper rather than reading the token field
/// directly.
pub fn meta_lh(t: &Tokens) -> iced::widget::text::LineHeight {
    snora::design::style::text::body_small_line_height(t)
}

/// Button / chip / control label size (unscaled).
pub fn label(t: &Tokens) -> Pixels {
    snora::design::style::text::label_size(t)
}
/// Label scaled.
pub fn label_s(t: &Tokens, s: TextScale) -> Pixels {
    scale(label(t), s)
}
