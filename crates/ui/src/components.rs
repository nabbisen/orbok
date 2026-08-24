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
//! | result card                 | `card::surface` / bespoke `selection_ring` (Task 031, `tokens.focus`) |
//! | source card                 | `card::surface` / bespoke `selection_ring` (Task 031, `tokens.focus`) |
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

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::Message;
use crate::theme;
use iced::widget::{button, column, container, row, text};
use iced::{Border, Element, Padding, Shadow};
use orbok_search::MatchBadge;
use snora::design::style::button as btn_style;
use snora::design::style::color::to_iced_color;
use snora::design::{Tokens, Tone, card, progress};
use snora::lucide;

// ── Icon helper (same technique as views.rs; glyph size stays explicit) ──

fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a> {
    iced::widget::text(glyph.to_string())
        .font(iced::Font::with_name("lucide"))
        .size(size)
}

// ── Status badges ─────────────────────────────────────────────────────────

/// Map a [`MatchBadge`] to a semantic [`Tone`].
///
/// Matches the typed variant directly rather than the rendered (and
/// localized) label, so translating badge text can never silently change
/// or collapse the colour-coding (RFC-052 §3).
pub fn badge_tone(badge: MatchBadge) -> Tone {
    match badge {
        MatchBadge::SourceStale => Tone::Warning,
        MatchBadge::Semantic | MatchBadge::Reranked => Tone::Accent,
        MatchBadge::Keyword => Tone::Info,
    }
}

/// Map a [`MatchBadge`] to its catalog key (RFC-052 §3).
pub(crate) fn badge_message_key(badge: MatchBadge) -> MessageKey {
    match badge {
        MatchBadge::Keyword => MessageKey::BadgeKeyword,
        MatchBadge::Semantic => MessageKey::BadgeSemantic,
        MatchBadge::Reranked => MessageKey::BadgeReranked,
        MatchBadge::SourceStale => MessageKey::BadgeSourceStale,
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

/// A card with the selected-state ring driven by `tokens.focus`
/// (RFC-034 §2.4.7, Task 031) rather than snora's own fixed
/// `card::selected` accent border. `card::selected` takes only `&Tokens`
/// — no parameter for a caller-supplied border — so this mirrors its
/// exact non-border styling
/// (`snora-style::container::card_selected`: `surface` background,
/// `radius.lg`, default shadow, primary text colour, `md` padding) and
/// substitutes only the border, reading `ring_color`/`ring_width` from
/// `tokens.focus` instead of a hardcoded `accent`/`2.0`. This is why the
/// high-contrast presets now render correctly: they widen the ring
/// `2.0 -> 3.0` and change its colour, a distinction `card::selected`'s
/// fixed border cannot express.
///
/// **Inset ring; `ring_offset` is not expressed.** `iced::Border` has no
/// offset field, so a ring drawn *outside* the card's edge isn't
/// expressible as a container border alone
/// (`FocusTokens::ring_offset`'s own doc comment names padding or a
/// nested container as the way to honour it). `ring_offset` is `2.0` in
/// all four built-in presets today — no accessibility signal varies by
/// preset on that field — so the ring stays inset, matching orbok's
/// prior visual behaviour exactly apart from colour/width. Accepting the
/// inset deliberately rather than restructuring card layout for a
/// dimension that doesn't yet vary.
fn selection_ring<'a, Message: 'a>(
    tokens: &Tokens,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let style = selection_ring_style(tokens);
    container(content)
        .padding(tokens.spacing.md)
        .style(move |_theme| style)
        .into()
}

/// The style computation [`selection_ring`] applies, pulled out as a pure
/// function of `Tokens` so it is directly testable (Task 031 §4) --
/// `iced_test::Simulator` finds text, it does not inspect a rendered
/// container's border, the same limit `line_height_helpers_track_tokens_not_constants`
/// (Task 028) worked around the same way.
pub(crate) fn selection_ring_style(tokens: &Tokens) -> iced::widget::container::Style {
    iced::widget::container::Style {
        text_color: Some(to_iced_color(tokens.palette.text_primary)),
        background: Some(to_iced_color(tokens.palette.surface).into()),
        border: Border::default()
            .rounded(tokens.radius.lg)
            .color(to_iced_color(tokens.focus.ring_color))
            .width(tokens.focus.ring_width),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// A search result card.
///
/// Uses [`selection_ring`] (focus-token-driven border) when this result is
/// the active selection, `card::surface` otherwise. Wrapped in an
/// invisible button so the whole card surface is clickable and
/// keyboard-reachable.
#[allow(clippy::too_many_arguments)]
pub fn result_card<'a>(
    tokens: &'a Tokens,
    locale: Locale,
    title: String,
    display_path: String,
    heading_str: String,
    snippet: String,
    badges: &'a [MatchBadge],
    show_advanced: bool,
    is_selected: bool,
    on_select: Message,
) -> Element<'a, Message> {
    let shown_badges: Vec<MatchBadge> = if show_advanced {
        badges.to_vec()
    } else {
        badges
            .iter()
            .filter(|b| matches!(b, MatchBadge::SourceStale))
            .cloned()
            .collect()
    };

    let badge_row: Element<'a, Message> = if shown_badges.is_empty() {
        text("").size(theme::meta(tokens)).into()
    } else {
        let mut r = row![].spacing(tokens.spacing.sm);
        for b in shown_badges {
            let label = tr(locale, badge_message_key(b));
            r = r.push(status_badge(tokens, label, badge_tone(b)));
        }
        r.into()
    };

    let body = column![
        // A result's title -- often a document heading, not guaranteed
        // to fit one line at the card's bounded width (Task 028 §2).
        text(title)
            .size(theme::body(tokens))
            .line_height(theme::body_lh(tokens)),
        text(display_path).size(theme::meta(tokens)),
        if !heading_str.is_empty() {
            text(heading_str).size(theme::meta(tokens))
        } else {
            text("").size(theme::meta(tokens))
        },
        // A genuine excerpt, meant to give context across more than one
        // line -- the wrapping-prose case this task exists for.
        text(snippet.chars().take(120).collect::<String>())
            .size(theme::meta(tokens))
            .line_height(theme::meta_lh(tokens)),
        badge_row,
    ]
    .spacing(tokens.spacing.xs);

    let inner = if is_selected {
        selection_ring(tokens, body)
    } else {
        card::surface(tokens, body)
    };

    button(inner)
        .on_press(on_select)
        .style(|_t, _s| iced::widget::button::Style::default())
        .into()
}

/// A source card: name, path, summary stats, status, and a remove action.
///
/// `is_selected` uses [`selection_ring`] exactly like `result_card` --
/// RFC-034 (Task 024)'s keyboard selection for the Sources view reuses
/// the same visible-selection mitigation for 2.4.7's absence, not a
/// second convention.
#[allow(clippy::too_many_arguments)]
pub fn source_card<'a>(
    tokens: &'a Tokens,
    display_name: String,
    display_path: String,
    summary: String,
    status_label: &'a str,
    is_selected: bool,
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
    if is_selected {
        selection_ring(tokens, body)
    } else {
        card::surface(tokens, body)
    }
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
    // button::danger_maybe already handles padding via snora; no extra
    // container padding is needed (RFC-052 §5 -- the prior zero padding
    // here was redundant and has been removed, not replaced with a helper).
    let btn = snora::design::button::danger_maybe(tokens, label, on);
    iced::widget::container(btn).into()
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
