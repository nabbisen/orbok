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
use iced::{Alignment, Background, Border, Color, Element, Padding, Shadow};
use orbok_search::MatchBadge;
use snora::design::style::button as btn_style;
use snora::design::style::color::to_iced_color;
use snora::design::{Tokens, Tone, card, progress};
use snora::lucide;

// ── Rows (Task 072) ───────────────────────────────────────────────────────

/// A horizontal row whose children share a centre line. iced aligns a row's
/// children to the top by default, which puts an icon above its label and
/// hangs a button below the input beside it, so views use this instead of
/// `row!`. A deliberate top-aligned row writes
/// `row![..].align_y(Alignment::Start)` with a comment saying why;
/// `scripts/check-design-tokens.sh` fails on a `row!` with neither.
///
/// **A row that holds a control wraps** (Task 106). The main window has no
/// minimum size and narrow windows are routine, so a row wider than the window
/// would push its last button or input past the edge, out of reach. Every
/// `hrow![..]` whose children include a button, an input or a chip therefore
/// ends in `.wrap()`, after its `.spacing(..)` (`Row::wrap` returns a
/// `Wrapping`, which has no `.push`): `hrow![a, b].spacing(s).wrap()`. A
/// builder (`let mut x = hrow![..]; x = x.push(..)`) is wrapped where it is
/// used (`x.wrap()`). A row of text only is out of the rule; a row that must
/// not wrap says why in a `// no-wrap: <reason>` comment on the line above.
/// `check-design-tokens.sh` enforces this, and `tests/task106_narrow_window.rs`
/// asserts that the listed controls lie inside a 450 px window.
macro_rules! hrow {
    () => {
        iced::widget::row![].align_y(iced::Alignment::Center)
    };
    ($($child:expr),+ $(,)?) => {
        iced::widget::row![$($child),+].align_y(iced::Alignment::Center)
    };
}
pub(crate) use hrow;

// ── How a setting is shown (Task 118) ────────────────────────────────────
//
// **Two standards, and only two.** A user is never left to work out what is
// set:
//
// 1. **A choice** (one of several: language, theme, text size, search mode,
//    folder coverage, search scope) is [`choice`]. Every option is a visible,
//    pressable button. The **chosen** one is filled and carries a check icon, so
//    the state never rests on colour alone (RFC-035); the others are outlined;
//    pressing the chosen one does nothing (`Message::AlreadyChosen`) and it never
//    looks disabled. The disabled look is used for one thing only: an option that
//    is not available (search by meaning without a model), and the reason stays
//    beside it. A choice wraps as whole options (Task 106).
// 2. **An on/off setting** is [`switch`]: iced's `toggler` with its label part of
//    the same widget, so a wrap never separates the label from the switch. The
//    position of the switch is the state; there is no "On"/"Off" button.
//
// No other way of showing a setting is used: not a button that is disabled when
// it is current, not a button labelled with the *other* value, not a
// check-marked button standing in for a switch. `check-design-tokens.sh` catches
// the first shape (a button whose press is decided by comparing against the
// current value) in `views.rs`; it cannot see a hand-built shape it has not been
// taught, so the standard is also this paragraph.

// ── What a fill means (Task 118 follow-up, Review 296 §2) ────────────────
//
// **Filled** is for the chosen option of a choice and for the page's single
// main action (Search; the empty state's Add folder). **Destructive** actions
// keep the danger style (Remove from orbok). **Every other action is outlined.**
//
// A filled control beside a choice is read as "this one is chosen", so a fill
// is never spent on an ordinary action. Every button a view builds picks one of
// [`filled`], [`outlined`] or [`destructive`] with `.style(..)`; a button with
// no style takes iced's default, which is filled, and so is a mistake.
// `check-design-tokens.sh` fails on a raw button with no `.style` call; it cannot
// say whether the look picked is the right one, so the rule above is the standard.

/// The filled look: a chosen option, or the page's single main action.
pub fn filled(tokens: &Tokens) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    let t = tokens.clone();
    move |_theme, status| btn_style::primary(&t, status)
}

/// The outlined look: every action that is not the page's main action.
pub fn outlined(
    tokens: &Tokens,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    let t = tokens.clone();
    move |_theme, status| btn_style::secondary(&t, status)
}

/// The danger look: an action that removes or erases something.
pub fn destructive(
    tokens: &Tokens,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    let t = tokens.clone();
    move |_theme, status| btn_style::danger(&t, status)
}

/// One option of a [`choice`].
pub struct ChoiceOption {
    pub label: String,
    pub chosen: bool,
    /// `false` draws the disabled look and sends nothing.
    pub available: bool,
    /// Pressed when the option is available and not already chosen.
    pub on_press: Message,
}

/// The one way to show a choice of several (see the block above).
pub fn choice<'a>(
    tokens: &Tokens,
    size: iced::Pixels,
    options: Vec<ChoiceOption>,
) -> Element<'a, Message> {
    let mut row = hrow![].spacing(tokens.spacing.xs);
    for option in options {
        let t = tokens.clone();
        let chosen = option.chosen;
        // no-wrap: the inside of one option's button, not a row of controls
        let mut content = hrow![].spacing(tokens.spacing.xs);
        if chosen {
            content = content.push(icon_text(char::from(lucide::Check), size.0));
        }
        content = content.push(text(option.label).size(size));
        let mut b = button(content)
            .padding(Padding::from([tokens.spacing.xs, tokens.spacing.md]))
            .style(move |_theme, status| {
                if chosen {
                    btn_style::primary(&t, status)
                } else {
                    btn_style::secondary(&t, status)
                }
            });
        if option.available {
            b = b.on_press(if chosen {
                Message::AlreadyChosen
            } else {
                option.on_press
            });
        }
        row = row.push(b);
    }
    row.wrap().into()
}

/// The widget id of the switch named `name`, so a test (or an operation) can
/// find the whole switch: `toggler`'s label is drawn by the widget itself and is
/// not a text element a selector can see.
pub fn switch_id(name: &'static str) -> iced::widget::Id {
    iced::widget::Id::new(name)
}

/// The one way to show an on/off setting (see the block above): the label and
/// the switch are one widget.
pub fn switch<'a>(
    tokens: &Tokens,
    size: iced::Pixels,
    name: &'static str,
    label: &str,
    is_on: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    let t = tokens.clone();
    iced::widget::container(
        iced::widget::toggler(is_on)
            .label(label.to_string())
            .on_toggle(on_toggle)
            .size(size.0 * 1.4)
            .text_size(size)
            .spacing(tokens.spacing.sm)
            .style(move |_theme, status| switch_style(&t, is_on, status)),
    )
    .id(switch_id(name))
    .into()
}

/// The switch's colours, from the tokens: an accent track with the accent's own
/// text colour for the knob when on; the secondary text colour for the track
/// with the surface colour for the knob when off. Both pairs are contrast-tested
/// (`tests/task118_choice_and_switch.rs`).
pub(crate) fn switch_style(
    tokens: &Tokens,
    is_on: bool,
    status: iced::widget::toggler::Status,
) -> iced::widget::toggler::Style {
    let (track, knob) = if is_on {
        (tokens.palette.accent, tokens.palette.accent_text)
    } else {
        (tokens.palette.text_secondary, tokens.palette.surface)
    };
    let hovered = matches!(status, iced::widget::toggler::Status::Hovered { .. });
    let track = to_iced_color(track);
    iced::widget::toggler::Style {
        background: Background::Color(if hovered {
            Color {
                r: (track.r - 0.06).max(0.0),
                g: (track.g - 0.06).max(0.0),
                b: (track.b - 0.06).max(0.0),
                a: track.a,
            }
        } else {
            track
        }),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: Background::Color(to_iced_color(knob)),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(to_iced_color(tokens.palette.text_primary)),
        border_radius: None,
        padding_ratio: 0.2,
    }
}

// ── Control heights (Task 072) ────────────────────────────────────────────

/// Padding for a button that sits beside a text input. Its vertical padding
/// is [`input_padding`]'s, so with the same text size both controls are the
/// same height.
pub fn control_padding(tokens: &Tokens) -> Padding {
    Padding::from([tokens.spacing.sm, tokens.spacing.lg])
}

/// Padding for a text input that sits beside a button; see
/// [`control_padding`].
pub fn input_padding(tokens: &Tokens) -> Padding {
    Padding::from([tokens.spacing.sm, tokens.spacing.sm])
}

// ── Icon helper (same technique as views.rs; glyph size stays explicit) ──

pub(crate) fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a> {
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
    hrow![
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
    trust: orbok_search::ResultTrustState,
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

    // HANDOFF-038: the trust badge sits beside the match badges, first
    // because it is the one that says whether to rely on the result.
    let trust_badge = result_trust_badge(tokens, trust, locale);
    let badge_row: Element<'a, Message> = if shown_badges.is_empty() && trust_badge.is_none() {
        text("").size(theme::meta(tokens)).into()
    } else {
        // no-wrap: badges, not controls
        let mut r = hrow![].spacing(tokens.spacing.sm);
        if let Some(badge) = trust_badge {
            r = r.push(badge);
        }
        for b in shown_badges {
            let label = tr(locale, badge_message_key(b));
            r = r.push(status_badge(tokens, label, badge_tone(b)));
        }
        r.into()
    };

    // A result's title -- often a document heading, not guaranteed to fit
    // one line at the card's bounded width (Task 028 §2).
    let title_text = text(title)
        .size(theme::body(tokens))
        .line_height(theme::body_lh(tokens));
    // Task 072: the selected result's non-colour marker (RFC-034 §5.2) is a
    // lucide chevron, not a text "▶".
    let title_line: Element<'a, Message> = if is_selected {
        // Top-aligned: the title can wrap, and the marker belongs beside its
        // first line.
        row![
            icon_text(char::from(lucide::ChevronRight), theme::body(tokens).0),
            title_text,
        ]
        .spacing(tokens.spacing.xs)
        .align_y(Alignment::Start)
        .into()
    } else {
        title_text.into()
    };

    let body = column![
        title_line,
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
    // RFC-037 §17.3 (Task 035): the explanatory line under "Folder not
    // found" ("This can happen if a drive is disconnected or the folder
    // was moved.") -- `None` for every other state, which §17's other
    // wireframes (17.1/17.2/17.4) draw with no such line.
    detail: Option<&'a str>,
    // Task 118: what the folder covers, drawn as a `choice` -- both labels, the
    // current one chosen (built by the caller, which knows the messages).
    coverage: Element<'a, Message>,
    // RFC-037 §10.2/§17 (Task 035): `[Check again]` for a missing/
    // permission-denied source, `[Prepare again]` for an active one --
    // `None` for a source with nothing to refresh (Paused; RFC-037 §7.4
    // treats a pause as covering exactly this kind of preparation work).
    refresh_action: Option<(&'a str, Message)>,
    is_selected: bool,
    remove_label: &'a str,
    on_remove: Message,
) -> Element<'a, Message> {
    let mut actions =
        hrow![text(status_label.to_string()).size(theme::meta(tokens))].spacing(tokens.spacing.sm);
    if let Some((label, on_refresh)) = refresh_action {
        actions = actions.push(secondary(tokens, label, Some(on_refresh)));
    }
    actions = actions.push(danger(tokens, remove_label, Some(on_remove)));

    let mut body = column![
        text(display_name).size(theme::body(tokens)),
        text(display_path).size(theme::meta(tokens)),
        text(summary).size(theme::meta(tokens)),
    ]
    .spacing(tokens.spacing.xs);
    if let Some(detail) = detail {
        body = body.push(
            text(detail.to_string())
                .size(theme::meta(tokens))
                .line_height(theme::meta_lh(tokens)),
        );
    }
    body = body.push(coverage);
    body = body.push(actions.wrap());
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
    let content = hrow![
        icon_text(glyph, icon_size),
        text(label.to_string()).size(theme::body(tokens)),
    ]
    .spacing(tokens.spacing.sm);
    let mut b = button(content)
        .padding(control_padding(tokens))
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
    let content = hrow![
        icon_text(glyph, icon_size),
        text(label.to_string()).size(theme::body(tokens)),
    ]
    .spacing(tokens.spacing.sm);
    let mut b = button(content)
        .padding(control_padding(tokens))
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
    let mut r = hrow![].spacing(tokens.spacing.sm);
    for (label, msg) in actions {
        r = r.push(
            button(text(label.to_string()).size(theme::body(tokens)))
                .padding(Padding::from([tokens.spacing.md, tokens.spacing.lg]))
                .style(outlined(tokens))
                .on_press(msg),
        );
    }
    r.wrap().into()
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
/// When `selected` is true the chip shows a trailing lucide `X`, the
/// remove affordance, beside its label. Color must not be the only
/// signal of the active state (RFC-034 §5.2), so the icon carries it too.
pub fn filter_chip<'a>(
    tokens: &Tokens,
    label: &str,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    chip(
        tokens,
        crate::theme::TextScale::default(),
        None,
        label,
        selected.then_some(char::from(lucide::X)),
        on_press,
    )
}

/// The search row's folder chip (RFC-045 §7.3, Review 299 §3.1): two controls,
/// as snora draws a removable chip. The **label** (the folder's name) changes the
/// folder (`on_change`, RFC-045's [Change]); the **`×`** clears it (`on_clear`,
/// §11.3) and has a visual tooltip. Always outlined -- a chip is not a choice, so
/// it never takes the chosen look (Task 118). snora sizes the `×` target to at
/// least 24 × 24 (WCAG 2.5.8), which the tests measure.
///
/// The tooltip is a visual tooltip, not an accessible name (snora, Task 121).
pub fn removable_chip<'a>(
    tokens: &Tokens,
    label: &str,
    on_change: Option<Message>,
    on_clear: Message,
    tooltip: &str,
) -> Element<'a, Message> {
    // `selected` is false: the outlined look, whatever the state.
    snora::design::chip::removable_with_tooltip(
        tokens,
        label.to_string(),
        false,
        on_change,
        Some(on_clear),
        tooltip.to_string(),
    )
}

/// Task 072: one chip primitive -- token-styled (outlined: a chip is an action,
/// never a chosen option), with an optional
/// leading and trailing lucide icon sharing the label's centre line. The
/// label is the chip's accessible text: the whole chip is the control, so
/// finding and pressing the label presses the chip. Icons are sized to the
/// label's text size, so no glyph dimension is chosen here.
pub fn chip<'a>(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    leading: Option<char>,
    label: &str,
    trailing: Option<char>,
    on_press: Message,
) -> Element<'a, Message> {
    let size = theme::meta_s(tokens, sc);
    // no-wrap: the inside of one chip's button, not a row of controls
    let mut content = hrow![].spacing(tokens.spacing.xs);
    if let Some(glyph) = leading {
        content = content.push(icon_text(glyph, size.0));
    }
    content = content.push(text(label.to_string()).size(size));
    if let Some(glyph) = trailing {
        content = content.push(icon_text(glyph, size.0));
    }
    let t = tokens.clone();
    button(content)
        .padding(Padding::from([tokens.spacing.xs, tokens.spacing.sm]))
        .style(move |_theme, status| btn_style::secondary(&t, status))
        .on_press(on_press)
        .into()
}

// ── Result trust badge (RFC-038 §6) ───────────────────────────────────

/// The tone (and so the icon and colour) each trust state reinforces its
/// text label with (RFC-038 §6.3: text, never colour alone).
pub fn trust_tone(state: orbok_search::ResultTrustState) -> Tone {
    use orbok_search::ResultTrustState;
    match state {
        ResultTrustState::Ready => Tone::Success,
        ResultTrustState::NeedsUpdate | ResultTrustState::PartlyPrepared => Tone::Warning,
        ResultTrustState::FileNotFound | ResultTrustState::CannotOpen => Tone::Danger,
        ResultTrustState::StillBeingPrepared => Tone::Neutral,
    }
}

/// The trust badge for a result: icon, text label and tone, three redundant
/// channels (RFC-038 §6.3, criterion 8), shown only when the result is not
/// fully ready.
///
/// Returns `None` for `ResultTrustState::Ready` so callers can skip
/// rendering entirely — keeping clean results uncluttered (RFC-038 §6.1).
pub fn result_trust_badge<'a>(
    tokens: &Tokens,
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
    Some(status_badge(tokens, tr(locale, key), trust_tone(state)))
}
