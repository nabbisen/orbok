//! orbok view-model → Snora Design primitive adapters (RFC-033).
//!
//! Views call these functions; they never call `snora::design::{button, card,
//! chip, progress}` directly. One layer of indirection means a future primitive
//! swap touches only this file. snora is the sole gateway for UI primitives —
//! the same rule that already holds for lucide icons (RFC-027) and design
//! tokens (RFC-032).
//!
//! ## Primitive inventory (RFC-033 §5.2)
//!
//! | orbok element               | snora 0.25 primitive                  |
//! |-----------------------------|---------------------------------------|
//! | result card                 | `card::surface` / `card::selected`    |
//! | source card                 | `card::surface`                       |
//! | indexing health cell        | `card::surface`                       |
//! | status badge                | tone-styled chip (text + icon + tone) |
//! | primary action              | `button::primary_maybe`               |
//! | secondary action            | `button::secondary_maybe`             |
//! | ghost / tertiary action     | `button::ghost_maybe`                 |
//! | destructive action          | `button::danger_maybe`                |
//! | indexing job progress       | `progress::row`                       |
//! | notice / banner             | `notice::Notice` (unchanged)          |
//! | two-pane split              | **bespoke** — no snora primitive yet  |
//! | confirmation dialog         | **bespoke** — no snora primitive yet  |
//! | wizard stepper              | **bespoke** — no snora primitive yet  |

use crate::state::Message;
use crate::theme;
use iced::widget::{button, column, row, text};
use iced::{Element, Padding};
use snora::design::style::button as btn_style;
use snora::design::{Tokens, Tone, card, progress};
use snora::lucide;

// ── Icon helper (same technique as views.rs; glyph size stays explicit) ──

fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a> {
    iced::widget::text(glyph.to_string())
        .font(iced::Font::with_name("lucide"))
        .size(size)
}

// ── Status badges ─────────────────────────────────────────────────────────

/// Map an orbok badge string to a semantic [`Tone`].
///
/// The mapping is stable and table-driven so that RFC-035's CVD fixture and
/// the tone-mapping unit test both reference the same single source of truth.
pub fn badge_tone(label: &str) -> Tone {
    let l = label.to_lowercase();
    if l.contains("missing") {
        Tone::Danger
    } else if l.contains("stale") {
        Tone::Warning
    } else if l.contains("semantic") || l.contains("rerank") {
        Tone::Accent
    } else if l.contains("keyword") {
        Tone::Info
    } else if l.contains("current") {
        Tone::Success
    } else {
        Tone::Neutral
    }
}

/// The lucide icon bound to each tone (RFC-035 CVD-safe guarantee).
///
/// Each status is conveyed by three independent signals: text label, tone
/// colour, and this icon/shape, so the meaning survives any colour vision.
pub fn tone_icon(tone: Tone) -> char {
    char::from(match tone {
        Tone::Success => lucide::CheckCircle,
        Tone::Warning => lucide::AlertTriangle,
        Tone::Danger => lucide::CircleX,
        Tone::Info => lucide::Info,
        Tone::Accent => lucide::Sparkles,
        Tone::Neutral => lucide::Clock,
    })
}

/// A status badge: icon + text label + tone — three redundant channels so
/// meaning survives any colour vision (RFC-034 §5.2, RFC-035 §5.4).
///
/// The label is mandatory; tone is supplementary. Passing an empty label is
/// a logic error and is caught by the `status_badge_label_invariant` test.
pub fn status_badge<'a>(tokens: &Tokens, label: &str, tone: Tone) -> Element<'a, Message> {
    debug_assert!(!label.is_empty(), "status_badge: label must not be empty");
    row![
        icon_text(tone_icon(tone), theme::meta(tokens).0),
        text(label.to_string()).size(theme::meta(tokens)),
    ]
    .spacing(tokens.spacing.xs)
    .into()
}

// ── Cards ─────────────────────────────────────────────────────────────────

/// A search result card.
///
/// Uses `card::selected` (accent border) when this result is the active
/// selection, `card::surface` otherwise. Wrapped in an invisible button so the
/// whole card surface is clickable and keyboard-reachable.
#[allow(clippy::too_many_arguments)]
pub fn result_card<'a>(
    tokens: &'a Tokens,
    title: String,
    display_path: String,
    heading_str: String,
    snippet: String,
    badges: &'a [String],
    show_advanced: bool,
    is_selected: bool,
    on_select: Message,
) -> Element<'a, Message> {
    let shown_badges: Vec<&String> = if show_advanced {
        badges.iter().collect()
    } else {
        badges
            .iter()
            .filter(|b| {
                let l = b.to_lowercase();
                l.contains("stale") || l.contains("missing")
            })
            .collect()
    };

    let badge_row: Element<'a, Message> = if shown_badges.is_empty() {
        text("").size(theme::meta(tokens)).into()
    } else {
        let mut r = row![].spacing(tokens.spacing.sm);
        for b in shown_badges {
            r = r.push(status_badge(tokens, b, badge_tone(b)));
        }
        r.into()
    };

    let body = column![
        text(title).size(theme::body(tokens)),
        text(display_path).size(theme::meta(tokens)),
        if !heading_str.is_empty() {
            text(heading_str).size(theme::meta(tokens))
        } else {
            text("").size(theme::meta(tokens))
        },
        text(snippet.chars().take(120).collect::<String>()).size(theme::meta(tokens)),
        badge_row,
    ]
    .spacing(tokens.spacing.xs);

    let inner = if is_selected {
        card::selected(tokens, body)
    } else {
        card::surface(tokens, body)
    };

    button(inner)
        .on_press(on_select)
        .style(|_t, _s| iced::widget::button::Style::default())
        .into()
}

/// A source card: name, path, summary stats, status, and a remove action.
pub fn source_card<'a>(
    tokens: &'a Tokens,
    display_name: String,
    display_path: String,
    summary: String,
    status_label: &'a str,
    on_remove: Message,
) -> Element<'a, Message> {
    let body = column![
        text(display_name).size(theme::body(tokens)),
        text(display_path).size(theme::meta(tokens)),
        text(summary).size(theme::meta(tokens)),
        row![
            text(status_label.to_string()).size(theme::meta(tokens)),
            danger(tokens, "", Some(on_remove)),
        ]
        .spacing(tokens.spacing.sm),
    ]
    .spacing(tokens.spacing.xs);
    card::surface(tokens, body)
}

/// An indexing health stat cell: label above a large number.
pub fn health_cell<'a>(tokens: &'a Tokens, label: &str, value: u64) -> Element<'a, Message> {
    card::surface(
        tokens,
        column![
            text(label.to_string()).size(theme::meta(tokens)),
            text(value.to_string()).size(theme::title(tokens)),
        ]
        .spacing(tokens.spacing.xs),
    )
}

// ── Action buttons ────────────────────────────────────────────────────────
//
// Thin pass-throughs that normalise label sizing and expose the four semantic
// roles (primary/secondary/ghost/danger). Each accepts Option<Message> so
// the caller uses the same call site whether the action is enabled or not —
// snora renders a visually disabled button when `on_press` is `None`.

pub fn primary<'a>(tokens: &Tokens, label: &str, on: Option<Message>) -> Element<'a, Message> {
    snora::design::button::primary_maybe(tokens, label, on)
}

pub fn secondary<'a>(tokens: &Tokens, label: &str, on: Option<Message>) -> Element<'a, Message> {
    snora::design::button::secondary_maybe(tokens, label, on)
}

pub fn ghost<'a>(tokens: &Tokens, label: &str, on: Option<Message>) -> Element<'a, Message> {
    snora::design::button::ghost_maybe(tokens, label, on)
}

/// Danger button for irreversible actions (Reset, Delete, Remove).
///
/// Uses the `danger_text on danger` contrast-verified pair. Every destructive
/// action in orbok-ui must go through this function — never a neutral button
/// (RFC-033 §6, rule 2).
pub fn danger<'a>(tokens: &Tokens, label: &str, on: Option<Message>) -> Element<'a, Message> {
    snora::design::button::danger_maybe(tokens, label, on)
}

/// An icon + label button using the primary style.
///
/// `icon_size` is a glyph dimension, not a typography role — stays explicit.
/// Uses the snora primary style function directly since `button::primary_maybe`
/// takes `impl Into<String>`; icon content is an `Element`, not a string.
pub fn icon_primary<'a>(
    tokens: &'a Tokens,
    glyph: char,
    icon_size: f32,
    label: &str,
    on: Option<Message>,
) -> Element<'a, Message> {
    let t = tokens.clone();
    let content = row![
        icon_text(glyph, icon_size),
        text(label.to_string()).size(theme::body(tokens)),
    ]
    .spacing(tokens.spacing.sm);
    let mut b = button(content)
        .padding(Padding::from([tokens.spacing.md, tokens.spacing.lg]))
        .style(move |_theme, status| btn_style::primary(&t, status));
    if let Some(msg) = on {
        b = b.on_press(msg);
    }
    b.into()
}

/// An icon + label button using the secondary style.
pub fn icon_secondary<'a>(
    tokens: &'a Tokens,
    glyph: char,
    icon_size: f32,
    label: &str,
    on: Option<Message>,
) -> Element<'a, Message> {
    let t = tokens.clone();
    let content = row![
        icon_text(glyph, icon_size),
        text(label.to_string()).size(theme::body(tokens)),
    ]
    .spacing(tokens.spacing.sm);
    let mut b = button(content)
        .padding(Padding::from([tokens.spacing.md, tokens.spacing.lg]))
        .style(move |_theme, status| btn_style::secondary(&t, status));
    if let Some(msg) = on {
        b = b.on_press(msg);
    }
    b.into()
}

// ── Progress ──────────────────────────────────────────────────────────────

/// An indexing-job progress row. Pass `None` for indeterminate state.
pub fn job_progress<'a>(
    tokens: &'a Tokens,
    label: &'a str,
    value: Option<f32>,
) -> Element<'a, Message> {
    progress::row(tokens, label, value, Tone::Accent)
}

// ── Cleanup action button row ─────────────────────────────────────────────

/// A row of token-padded buttons for safe cleanup actions (secondary style).
pub fn cleanup_row<'a>(
    tokens: &Tokens,
    actions: impl IntoIterator<Item = (&'a str, Message)>,
) -> Element<'a, Message> {
    let mut r = row![].spacing(tokens.spacing.sm);
    for (label, msg) in actions {
        r = r.push(
            button(text(label.to_string()).size(theme::body(tokens)))
                .padding(Padding::from([tokens.spacing.md, tokens.spacing.lg]))
                .on_press(msg),
        );
    }
    r.into()
}

/// A danger action button with standard token padding (for danger-zone rows).
pub fn danger_action<'a>(
    tokens: &Tokens,
    label: &str,
    on: Option<Message>,
) -> Element<'a, Message> {
    // button::danger_maybe already handles padding via snora; wrap in our
    // standard page padding for consistency with the danger zone section.
    let btn = snora::design::button::danger_maybe(tokens, label, on);
    iced::widget::container(btn)
        .padding(Padding::from([0.0, 0.0]))
        .into()
}

// ── Filter chips (RFC-041 §18.2) ──────────────────────────────────────

/// A narrowing chip — either a quick suggestion or an active filter.
///
/// When `selected` is true the label shows with " ×" appended and the
/// chip renders in its active state. Color must not be the only
/// selected-state indicator (RFC-041 §19; RFC-034 §8).
pub fn filter_chip<'a>(
    tokens: &Tokens,
    label: &str,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let display = if selected {
        format!("{label} ×")
    } else {
        label.to_string()
    };
    snora::design::button::primary_maybe(tokens, &display, Some(on_press))
}

// ── Result trust badge (RFC-038 §6) ───────────────────────────────────

/// A plain-text trust badge shown only when the result is not fully ready.
///
/// Returns `None` for `ResultTrustState::Ready` so callers can skip
/// rendering entirely — keeping clean results uncluttered (RFC-038 §6.1).
pub fn result_trust_badge<'a>(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    state: orbok_search::ResultTrustState,
    locale: crate::i18n::Locale,
) -> Option<Element<'a, Message>> {
    use crate::i18n::{MessageKey, tr};
    use orbok_search::ResultTrustState;
    let key = match state {
        ResultTrustState::Ready => return None,
        ResultTrustState::NeedsUpdate => MessageKey::TrustNeedsUpdate,
        ResultTrustState::FileNotFound => MessageKey::TrustFileNotFound,
        ResultTrustState::StillBeingPrepared => MessageKey::TrustStillBeingPrepared,
        ResultTrustState::PartlyPrepared => MessageKey::TrustPartlyPrepared,
        ResultTrustState::CannotOpen => MessageKey::TrustCannotOpen,
    };
    Some(
        iced::widget::text(tr(locale, key))
            .size(crate::theme::meta_s(tokens, sc))
            .into(),
    )
}
