//! RFC-033 component adapter tests: tone mapping, badge invariant, smoke builds.

use crate::components::{badge_tone, status_badge, tone_icon};
use crate::i18n::Locale;
use crate::state::Message;
use orbok_search::MatchBadge;
use snora::design::{Tokens, Tone};

// RFC-033 §5.2 + RFC-052 §3: the badge_tone mapping is stable and keyed off
// the typed MatchBadge variant, not its (localized) rendered label.
#[test]
fn badge_tone_mapping() {
    let cases: &[(MatchBadge, Tone)] = &[
        (MatchBadge::SourceStale, Tone::Warning),
        (MatchBadge::Semantic, Tone::Accent),
        (MatchBadge::Reranked, Tone::Accent),
        (MatchBadge::Keyword, Tone::Info),
    ];
    for (badge, expected) in cases {
        assert_eq!(badge_tone(*badge), *expected, "badge_tone({badge:?})");
    }
}

// RFC-033 + RFC-034 §5.2: each tone maps to a non-null icon glyph; badges build
// without panicking for representative labels (text + icon invariant).
#[test]
fn status_badge_label_and_icon_invariant() {
    for tone in [
        Tone::Success,
        Tone::Warning,
        Tone::Danger,
        Tone::Info,
        Tone::Accent,
        Tone::Neutral,
    ] {
        assert_ne!(
            tone_icon(tone) as u32,
            0,
            "tone_icon for {tone:?} must be non-null"
        );
    }
    let tokens = Tokens::light();
    for badge in [
        MatchBadge::SourceStale,
        MatchBadge::Keyword,
        MatchBadge::Semantic,
        MatchBadge::Reranked,
    ] {
        let label = crate::i18n::tr(Locale::En, crate::components::badge_message_key(badge));
        let _ = status_badge(&tokens, label, badge_tone(badge));
    }
}

// RFC-033 §8: adapters build Elements for normal and edge cases.
#[test]
fn component_smoke_result_card() {
    let tokens = Tokens::light();
    // Normal unselected card.
    let _ = crate::components::result_card(
        &tokens,
        Locale::En,
        "My document.md".to_string(),
        "/home/user/My document.md".to_string(),
        "Section heading".to_string(),
        "A short snippet of content…".to_string(),
        &[MatchBadge::SourceStale, MatchBadge::Keyword],
        false,
        false,
        Message::SelectResult(0),
    );
    // Selected card with no heading and empty badges.
    let _ = crate::components::result_card(
        &tokens,
        Locale::Ja,
        "▶  selected.pdf".to_string(),
        "/docs/selected.pdf".to_string(),
        String::new(),
        "(source unavailable)".to_string(),
        &[],
        false,
        true,
        Message::SelectResult(1),
    );
}

#[test]
fn component_smoke_source_card() {
    let tokens = Tokens::light();
    let _ = crate::components::source_card(
        &tokens,
        "Documents".to_string(),
        "/home/user/Documents".to_string(),
        "812 indexed · 0 stale".to_string(),
        "Active",
        false,
        Message::SourceRemoved("src-1".to_string()),
    );
}

#[test]
fn component_smoke_health_cell() {
    let tokens = Tokens::light();
    let _ = crate::components::health_cell(&tokens, "Indexed", 812);
    let _ = crate::components::health_cell(&tokens, "Failed", 0);
}

#[test]
fn component_smoke_action_buttons() {
    let tokens = Tokens::light();
    let _ = crate::components::primary(&tokens, "Save", Some(Message::ToggleAdvanced));
    let _ = crate::components::primary(&tokens, "Save", None);
    let _ = crate::components::secondary(&tokens, "Cancel", Some(Message::ClearNotice));
    let _ = crate::components::ghost(&tokens, "Details", None);
    let _ = crate::components::danger(&tokens, "Delete", Some(Message::AskResetCatalog));
    let _ = crate::components::danger(&tokens, "Delete", None);
}

#[test]
fn component_smoke_progress() {
    let tokens = Tokens::light();
    let _ = crate::components::job_progress(&tokens, "Indexing...", Some(0.42));
    let _ = crate::components::job_progress(&tokens, "Queued", None);
}
