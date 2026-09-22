//! Page view functions (GUI external design §7, §8–§12 wireframes).
//!
//! Styling (RFC-032/035): sizes come from `state.tokens` via [`crate::theme`]
//! scaled by `state.text_scale`. No literal sizes, paddings, or colours.
//!
//! Primitives (RFC-033): cards/buttons/badges/progress via [`crate::components`].
//!
//! Formatting (RFC-035): user-facing numbers and sizes via [`crate::i18n`].

pub mod startup_failure;
pub mod wizard;
pub use wizard::wizard_view;

use crate::components::{
    self, health_cell, hrow, icon_text, job_progress, result_card, source_card,
};
use crate::i18n::{
    Locale, MessageKey, files_ready_for_search, fmt_gib, fmt_label_value, fmt_mib_bucket,
    fmt_query, fmt_storage_row, preparing_folder_for_search, search_location_chip,
    search_result_count, source_summary, tr,
};
use crate::state::{AppState, Message, ResultTrustDisplay, SearchFolderScope};
use crate::theme::{self, TextScale, Theme};
use iced::widget::{button, column, container, scrollable, text, text_input, tooltip};
use iced::{Element, Length, Padding};
use orbok_models::SearchCapability;
use orbok_search::{ResultRecoveryAction, ResultTrustState, ResultWarningSummary};
use snora::design::Tokens;
use snora::design::style::color::to_iced_color;
use snora::lucide;

// ── Recent searches panel (RFC-042 §11.3) ─────────────────────────────────

/// Recent searches list. Collapsed to a single "Recent searches" button when
/// closed (shown only if entries exist); expands to a panel of entries each
/// with a "Search again" action and a "Clear recent searches" footer.
///
/// "Less is more": no entry counts, no tabs, no technical labels.
fn recent_searches_panel<'a>(state: &'a AppState) -> Element<'a, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    if !state.search_ui.history_panel_open {
        if state.search_ui.history.is_empty() {
            return column![].into();
        }
        return hrow![
            button(
                text(tr(locale, MessageKey::OpenRecentSearches)).size(theme::meta_s(tokens, sc))
            )
            .on_press(Message::OpenRecentSearches)
        ]
        .into();
    }

    let mut entries = column![].spacing(tokens.spacing.sm);

    if state.search_ui.history.is_empty() {
        entries = entries.push(
            text(tr(locale, MessageKey::NoRecentSearches))
                .size(theme::meta_s(tokens, sc))
                .color(to_iced_color(tokens.palette.text_secondary)),
        );
    } else {
        for entry in &state.search_ui.history {
            let filter_summary = entry
                .filters
                .iter()
                .map(|f| f.label())
                .collect::<Vec<_>>()
                .join(" · ");

            // The user's own past query, unbounded length -- wraps like any
            // other prose (Task 028 §2).
            let mut entry_col = column![
                text(&entry.search_text)
                    .size(theme::body_s(tokens, sc))
                    .line_height(theme::body_lh(tokens))
            ]
            .spacing(tokens.spacing.xs);

            if !filter_summary.is_empty() {
                entry_col = entry_col.push(
                    text(filter_summary)
                        .size(theme::meta_s(tokens, sc))
                        .color(to_iced_color(tokens.palette.text_secondary)),
                );
            }

            let search_again = button(
                text(tr(locale, MessageKey::SearchAgainButton)).size(theme::meta_s(tokens, sc)),
            )
            .on_press(Message::SearchAgain(entry.id.clone()));

            entries = entries.push(column![entry_col, search_again].spacing(tokens.spacing.xs));
        }

        entries = entries.push(
            button(
                text(tr(locale, MessageKey::ClearRecentSearches)).size(theme::meta_s(tokens, sc)),
            )
            .on_press(Message::AskClearRecentSearches),
        );
    }

    column![
        hrow![
            text(tr(locale, MessageKey::RecentSearchesLabel)).size(theme::label_s(tokens, sc)),
            // Task 072: an icon-only control keeps its label as a tooltip.
            tooltip(
                button(icon_text(
                    char::from(lucide::X),
                    theme::meta_s(tokens, sc).0
                ))
                .on_press(Message::CloseRecentSearches),
                text(tr(locale, MessageKey::NoticeDismiss)).size(theme::meta_s(tokens, sc)),
                tooltip::Position::Bottom,
            ),
        ]
        .spacing(tokens.spacing.sm),
        scrollable(entries).height(Length::Shrink),
    ]
    .spacing(tokens.spacing.sm)
    .into()
}

/// Settings control for clearing recent searches (RFC-042 §11.6). Renders a
/// single "Clear recent searches" button, or an inline confirmation
/// (title + body + Cancel/Clear) when `confirm_clear_history` is set.
/// Confirmation focuses Cancel-equivalent first by listing it first.
fn recent_searches_clear_control<'a>(state: &'a AppState) -> Element<'a, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    if state.visible_confirmation() == Some(crate::state::Confirmation::ClearRecentSearches) {
        column![
            text(tr(locale, MessageKey::ClearRecentSearchesConfirmTitle))
                .size(theme::body_s(tokens, sc)),
            text(tr(locale, MessageKey::ClearRecentSearchesConfirmBody))
                .size(theme::meta_s(tokens, sc))
                .line_height(theme::meta_lh(tokens))
                .color(to_iced_color(tokens.palette.text_secondary)),
            hrow![
                button(text(tr(locale, MessageKey::Cancel)).size(theme::meta_s(tokens, sc)))
                    .on_press(Message::CancelClearRecentSearches),
                button(
                    text(tr(locale, MessageKey::ClearRecentSearches))
                        .size(theme::meta_s(tokens, sc))
                )
                .on_press(Message::ConfirmClearRecentSearches),
            ]
            .spacing(tokens.spacing.sm),
        ]
        .spacing(tokens.spacing.xs)
        .into()
    } else {
        button(text(tr(locale, MessageKey::ClearRecentSearches)).size(theme::meta_s(tokens, sc)))
            .on_press(Message::AskClearRecentSearches)
            .into()
    }
}

// ── Search location row ───────────────────────────────────────────────────

/// "Search in: [Folder and subfolders ×] [Change]" row (RFC-045 §7.3, §11).
///
/// When no folder is selected, renders a passive prompt ("Choose a folder").
/// When a folder is selected, renders a removable chip with a scope selector.
/// The scope toggle is shown only when a folder is selected (progressive
/// disclosure — RFC §2.3 / "less is more").
fn search_location_row<'a>(state: &'a AppState) -> Element<'a, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    match &state.search_location.selected {
        None => {
            // First-run / no-folder state: passive one-line prompt (RFC-045 §7.1).
            hrow![
                text(tr(locale, MessageKey::SearchInLabel)).size(theme::meta_s(tokens, sc)),
                text(tr(locale, MessageKey::SearchChooseFolder))
                    .size(theme::meta_s(tokens, sc))
                    .color(to_iced_color(tokens.palette.text_secondary)),
            ]
            .spacing(tokens.spacing.xs)
            .into()
        }
        Some(location) => {
            let scope = location.scope();
            let chip_label = search_location_chip(locale, location.display_name(), scope);

            // Scope toggle: "and subfolders" / "only" (RFC-045 §11.2).
            let (other_scope, other_label_key) = match scope {
                SearchFolderScope::FolderAndSubfolders => {
                    (SearchFolderScope::FolderOnly, MessageKey::SearchScopeOnly)
                }
                SearchFolderScope::FolderOnly => (
                    SearchFolderScope::FolderAndSubfolders,
                    MessageKey::SearchScopeSubfolders,
                ),
            };

            hrow![
                text(tr(locale, MessageKey::SearchInLabel)).size(theme::meta_s(tokens, sc)),
                // Folder chip with an X to remove — keyboard removable
                // (RFC-045 §20).
                components::chip(
                    tokens,
                    sc,
                    None,
                    &chip_label,
                    Some(char::from(lucide::X)),
                    Message::SearchLocationCleared,
                ),
                // Scope toggle: ArrowUpDown says "switch to the other scope",
                // which is what pressing it does.
                components::chip(
                    tokens,
                    sc,
                    Some(char::from(lucide::ArrowUpDown)),
                    tr(locale, other_label_key),
                    None,
                    Message::SearchScopeChanged(other_scope),
                ),
            ]
            .spacing(tokens.spacing.xs)
            .into()
        }
    }
}

/// The one notice renderer (Task 064: called only from the shell, above
/// every view and the wizard).
pub(crate) fn friendly_notice<'a>(
    tokens: &'a Tokens,
    locale: Locale,
    notice: &crate::notice::UserNotice,
    has_action: bool,
) -> Element<'a, Message> {
    use snora::design::notice::Notice;
    let mut builder = Notice::new(tokens, notice.tone(), notice.body(locale).to_string())
        .title(notice.title(locale).to_string());
    // Task 060: a labelled action renders only when its raise site stored the
    // concrete retry. Task 072: dismiss is always offered, beside the action
    // when there is one (Task 060 §1).
    if let (Some(action_label), true) = (notice.action(locale), has_action) {
        builder = builder.action(action_label.to_string(), Message::NoticeActionPressed);
    }
    builder.dismiss(Message::ClearNotice).render()
}

fn page<'a>(tokens: &Tokens, content: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    container(
        iced::widget::scrollable(
            container(content.spacing(tokens.spacing.md))
                .padding(Padding::from([tokens.spacing.xl, tokens.spacing.xxl]))
                .width(Length::Fill),
        )
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn heading<'a>(tokens: &Tokens, sc: TextScale, label: &'a str) -> iced::widget::Text<'a> {
    text(label.to_string()).size(theme::heading_s(tokens, sc))
}

// ── Search view ──────────────────────────────────────────────────────────

pub fn search_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    let input = text_input(tr(locale, MessageKey::SearchPlaceholder), &state.query)
        .on_input(Message::QueryChanged)
        .on_submit(Message::SubmitSearch)
        // Task 072: the same text size and vertical padding as the Search
        // button beside it, so the two are the same height.
        .size(theme::body(tokens))
        .padding(components::input_padding(tokens));

    let submit = components::icon_primary(
        tokens,
        char::from(lucide::Search),
        13.0,
        tr(locale, MessageKey::SearchButton),
        (!state.search_running).then_some(Message::SubmitSearch),
    );

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::NavSearch)),
        hrow![container(input).width(Length::Fill), submit].spacing(tokens.spacing.sm),
        // RFC-045: "Search in" location row.
        search_location_row(state),
    ];

    // RFC-045 §7.4: recent / remembered folder quick-select chips.
    // Shown only when there are remembered folders and no folder is already
    // selected (they disappear once a choice is made — progressive disclosure).
    if !state.search_location.recent_locations.is_empty()
        && state.search_location.selected.is_none()
    {
        let mut chips = hrow![
            text(tr(locale, MessageKey::SearchRecentFoldersLabel))
                .size(theme::meta_s(tokens, sc))
                .color(to_iced_color(tokens.palette.text_secondary)),
        ]
        .spacing(tokens.spacing.xs);
        for summary in &state.search_location.recent_locations {
            chips = chips.push(components::chip(
                tokens,
                sc,
                Some(char::from(lucide::Folder)),
                &summary.display_name,
                None,
                Message::RecentFolderSelected(summary.source_id.clone()),
            ));
        }
        content = content.push(chips);
    }

    // RFC-042: Recent searches (collapsed button or expanded panel).
    content = content.push(recent_searches_panel(state));

    // RFC-036 §14.2 (RFC-056 Slice 4): a reminder that search already
    // works on prepared files while background work continues. Shown
    // only while something is actually queued, so a fully-prepared
    // profile's search view stays uncluttered -- the same "skip
    // rendering once Ready" precedent `result_trust_badge` (RFC-038
    // §6.1) already established.
    if state.health.queued > 0 {
        content = content.push(
            column![
                text(tr(locale, MessageKey::SearchFilesStillPreparing))
                    .size(theme::meta_s(tokens, sc))
                    .line_height(theme::meta_lh(tokens)),
                text(tr(locale, MessageKey::SearchResultsWillImprove))
                    .size(theme::meta_s(tokens, sc))
                    .line_height(theme::meta_lh(tokens)),
            ]
            .spacing(tokens.spacing.xs),
        );
    }

    if state.show_advanced {
        content = content.push(
            hrow![
                text(tr(locale, MessageKey::SearchModeLabel)).size(theme::meta_s(tokens, sc)),
                button(
                    text(tr(locale, MessageKey::SearchModeAuto)).size(theme::meta_s(tokens, sc))
                )
                .on_press(Message::SetSearchMode(orbok_search::SearchMode::Auto)),
                button(
                    text(tr(locale, MessageKey::SearchModeExact)).size(theme::meta_s(tokens, sc))
                )
                .on_press(Message::SetSearchMode(orbok_search::SearchMode::Exact)),
                button(
                    text(tr(locale, MessageKey::SearchModeConceptual))
                        .size(theme::meta_s(tokens, sc))
                )
                // Task 053: Conceptual has no keyword half, so without a
                // model it can only return nothing. Disabled rather than
                // hidden, so all three options stay discoverable.
                .on_press_maybe(
                    (state.capability != SearchCapability::KeywordOnly)
                        .then_some(Message::SetSearchMode(orbok_search::SearchMode::Conceptual)),
                ),
            ]
            .spacing(tokens.spacing.xs),
        );
    }

    if state.sources.is_empty() {
        content = content.push(
            column![
                text(tr(locale, MessageKey::SearchNoSourcesTitle)).size(theme::title_s(tokens, sc)),
                text(tr(locale, MessageKey::SearchNoSourcesBody))
                    .size(theme::body_s(tokens, sc))
                    .line_height(theme::body_lh(tokens)),
                components::primary(
                    tokens,
                    tr(locale, MessageKey::SearchAddSource),
                    Some(Message::Switch(crate::state::ViewId::Sources)),
                ),
            ]
            .spacing(tokens.spacing.sm),
        );
    } else {
        if state.capability == SearchCapability::KeywordOnly {
            content = content.push(
                text(tr(locale, MessageKey::SearchKeywordOnlyNotice))
                    .size(theme::meta_s(tokens, sc))
                    .line_height(theme::meta_lh(tokens)),
            );
        }
        if state.search_running {
            content = content
                .push(text(tr(locale, MessageKey::SearchRunning)).size(theme::body_s(tokens, sc)));
        } else if let Some(last) = &state.last_query {
            if state.search_results.is_empty() {
                content = content.push(
                    column![
                        text(tr(locale, MessageKey::SearchNoResults))
                            .size(theme::body_s(tokens, sc)),
                        // Echoes the user's own query text back -- unbounded
                        // length, same reasoning as recent-search entries.
                        text(fmt_query(locale, last))
                            .size(theme::meta_s(tokens, sc))
                            .line_height(theme::meta_lh(tokens)),
                    ]
                    .spacing(tokens.spacing.xs),
                );
            } else {
                content = content.push(
                    text(search_result_count(locale, state.search_results.len()))
                        .size(theme::meta_s(tokens, sc)),
                );
                for (i, result) in state.search_results.iter().enumerate() {
                    let is_selected = state.selected_result == Some(i);
                    let title_raw = result.title.as_deref().unwrap_or(&result.display_path);
                    let title_str = title_raw.to_string();
                    let snippet = result
                        .snippet
                        .as_deref()
                        .unwrap_or(tr(locale, MessageKey::SearchSnippetUnavailable));
                    let heading_str = result.heading_path.as_deref().unwrap_or("");
                    content = content.push(result_card(
                        tokens,
                        locale,
                        title_str,
                        result.display_path.clone(),
                        heading_str.to_string(),
                        snippet.to_string(),
                        &result.badges,
                        result.trust.state,
                        state.show_advanced,
                        is_selected,
                        Message::SelectResult(i),
                    ));
                    // HANDOFF-038: what the user can do about a result that is
                    // not fully ready, and the detail behind its badge.
                    if let Some(recovery) = trust_recovery(state, i, result) {
                        content = content.push(recovery);
                    }
                    // HANDOFF-041 §3: until a preview pane exists, the
                    // selected result carries its two file actions.
                    if is_selected {
                        content = content.push(
                            hrow![
                                components::secondary(
                                    tokens,
                                    tr(locale, MessageKey::SearchResultOpenFile),
                                    Some(Message::OpenResult(i)),
                                ),
                                components::secondary(
                                    tokens,
                                    tr(locale, MessageKey::SearchResultShowInFolder),
                                    Some(Message::RevealResult(i)),
                                ),
                            ]
                            .spacing(tokens.spacing.sm),
                        );
                    }
                }
            }
        }
    }
    page(tokens, content)
}

// ── Result trust recovery (HANDOFF-038) ──────────────────────────────────

/// The label of a recovery action orbok handles itself, or `None` for the
/// two that open something outside orbok. `OpenAnyway` and `ShowInFolder`
/// are never rendered here (HANDOFF-038 §3): the selected result's own
/// Open file and Show in folder buttons are the way to open a file.
fn recovery_label(action: ResultRecoveryAction) -> Option<MessageKey> {
    match action {
        ResultRecoveryAction::PrepareAgain => Some(MessageKey::TrustActionPrepareAgain),
        ResultRecoveryAction::CheckFolder => Some(MessageKey::TrustActionCheckFolder),
        ResultRecoveryAction::RemoveFromResults => Some(MessageKey::TrustActionRemoveFromResults),
        ResultRecoveryAction::ViewDetails => Some(MessageKey::TrustActionViewDetails),
        ResultRecoveryAction::OpenAnyway | ResultRecoveryAction::ShowInFolder => None,
    }
}

/// The plain-language lines behind a result's badge (RFC-038 §14): what the
/// state means, then what each extraction warning means.
fn trust_detail_keys(trust: &ResultTrustDisplay) -> Vec<MessageKey> {
    let mut keys = Vec::new();
    match trust.state {
        ResultTrustState::NeedsUpdate => keys.push(MessageKey::TrustFileChangedDetail),
        ResultTrustState::FileNotFound => keys.push(MessageKey::TrustFileNotFoundDetail),
        ResultTrustState::PartlyPrepared => keys.push(MessageKey::TrustPartlyPreparedDetail),
        ResultTrustState::CannotOpen => keys.push(MessageKey::TrustCannotOpenDetail),
        ResultTrustState::Ready | ResultTrustState::StillBeingPrepared => {}
    }
    for warning in &trust.warnings {
        match warning {
            ResultWarningSummary::PossiblyScannedPdf => {
                keys.push(MessageKey::TrustScannedPdfDetail)
            }
            ResultWarningSummary::SomePagesUnreadable => {
                keys.push(MessageKey::TrustSomePagesDetail)
            }
            ResultWarningSummary::SizeLimitReached => keys.push(MessageKey::TrustSizeLimitDetail),
            // No default copy: they change how a result is located, not
            // whether it can be trusted.
            ResultWarningSummary::UnsupportedDocumentPart
            | ResultWarningSummary::ApproximateLocation => {}
        }
    }
    keys
}

/// Below a non-ready result's card: its detail (when Advanced view is on, or
/// the user asked for it), and a button for each recovery action orbok can
/// do. `None` for a Ready result, which stays uncluttered (RFC-038 §6.1).
fn trust_recovery<'a>(
    state: &'a AppState,
    index: usize,
    result: &'a crate::state::SearchResultDisplay,
) -> Option<Element<'a, Message>> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;
    if result.trust.state == ResultTrustState::Ready {
        return None;
    }
    let detail_shown = state.show_advanced
        || state
            .search_ui
            .trust_details_open
            .contains(&result.canonical_path);
    let mut block = column![].spacing(tokens.spacing.xs);
    let mut anything = false;
    if detail_shown {
        for key in trust_detail_keys(&result.trust) {
            anything = true;
            block = block.push(
                text(tr(locale, key))
                    .size(theme::meta_s(tokens, sc))
                    .line_height(theme::meta_lh(tokens)),
            );
        }
    }
    let mut buttons = hrow![].spacing(tokens.spacing.sm);
    let mut any_button = false;
    for action in &result.trust.recovery_actions {
        let Some(key) = recovery_label(*action) else {
            continue;
        };
        // Once shown, View details has nothing left to do.
        if *action == ResultRecoveryAction::ViewDetails && detail_shown {
            continue;
        }
        any_button = true;
        buttons = buttons.push(components::secondary(
            tokens,
            tr(locale, key),
            Some(Message::TrustRecoveryAction {
                result_idx: index,
                action: *action,
            }),
        ));
    }
    if any_button {
        anything = true;
        block = block.push(buttons);
    }
    anything.then(|| block.into())
}

// ── Sources view ─────────────────────────────────────────────────────────

pub fn sources_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    // Task 062: the removal confirmation, laid out like the reset one.
    // Task 073: the same `removal_target` lookup `visible_confirmation` uses.
    if let Some(folder) = (state.visible_confirmation()
        == Some(crate::state::Confirmation::RemoveSource))
    .then(|| state.removal_target())
    .flatten()
    {
        let content = column![
            text(crate::i18n::fmt_remove_source_title(
                locale,
                &folder.display_name
            ))
            .size(theme::title_s(tokens, sc)),
            text(tr(locale, MessageKey::SourceRemoveConfirmBody))
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
            hrow![
                components::ghost(
                    tokens,
                    tr(locale, MessageKey::Cancel),
                    Some(Message::CancelRemoveSource)
                ),
                components::danger(
                    tokens,
                    tr(locale, MessageKey::SourceRemoveConfirm),
                    Some(Message::ConfirmRemoveSource)
                ),
            ]
            .spacing(tokens.spacing.md),
        ]
        .spacing(tokens.spacing.lg);
        return page(tokens, content);
    }

    // Task 047: no second add-folder dialog while one is open.
    let add_folder = (!state.add_source_picker_in_progress).then_some(Message::RequestAddSource);
    let add_btn = components::icon_secondary(
        tokens,
        char::from(lucide::FolderPlus),
        13.0,
        tr(locale, MessageKey::SourcesAddFolder),
        add_folder.clone(),
    );
    let add_input = text_input(
        tr(locale, MessageKey::SourcesPathInputPlaceholder),
        &state.source_path_input,
    )
    .on_input(Message::SourcePathChanged)
    .on_submit_maybe(add_folder)
    // Task 072: matches the Add Folder button beside it.
    .size(theme::body(tokens))
    .padding(components::input_padding(tokens));

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::SourcesTitle)),
        hrow![add_btn, container(add_input).width(Length::Fill)].spacing(tokens.spacing.sm),
        text(tr(locale, MessageKey::SourcesRecursiveHint))
            .size(theme::meta_s(tokens, sc))
            .line_height(theme::meta_lh(tokens)),
    ];

    if state.sources.is_empty() {
        content = content.push(
            column![
                text(tr(locale, MessageKey::SourcesEmptyTitle)).size(theme::title_s(tokens, sc)),
                text(tr(locale, MessageKey::SourcesEmptyBody))
                    .size(theme::body_s(tokens, sc))
                    .line_height(theme::body_lh(tokens)),
            ]
            .spacing(tokens.spacing.sm),
        );
    } else {
        for (i, card) in state.sources.iter().enumerate() {
            // RFC-037 §7/§17 (Task 035): the source's persisted status
            // decides the label and which refresh action applies.
            // `NeedsUpdate` has no catalog column of its own (§7.3 is
            // UI-derived, from `stale`) -- Active-with-stale-files reads
            // as "Needs update" rather than "Ready", the one place this
            // card's already-present `stale` count changes which label an
            // Active source gets.
            use orbok_core::SourceStatus;
            let (status_label, refresh_action) = match card.status {
                SourceStatus::Active if card.stale > 0 => (
                    tr(locale, MessageKey::SourceStateNeedsUpdate),
                    Some(MessageKey::SourceActionPrepareAgain),
                ),
                SourceStatus::Active => (
                    tr(locale, MessageKey::SourceStateReady),
                    Some(MessageKey::SourceActionPrepareAgain),
                ),
                SourceStatus::Paused => (tr(locale, MessageKey::SourceStatePaused), None),
                SourceStatus::Missing => (
                    tr(locale, MessageKey::SourceStateFolderNotFound),
                    Some(MessageKey::SourceActionCheckAgain),
                ),
                SourceStatus::PermissionDenied => (
                    tr(locale, MessageKey::SourceStateCannotOpen),
                    Some(MessageKey::SourceActionCheckAgain),
                ),
                // Removed sources are deleted from the catalog outright
                // (`remove_source`), never listed here.
                SourceStatus::Removed => (tr(locale, MessageKey::SourceStateRemoved), None),
            };
            let refresh_action = refresh_action.map(|key| {
                (
                    tr(locale, key),
                    Message::SourceRefreshRequested(card.source_id.clone()),
                )
            });
            // RFC-037 §17.3 (Task 035): the "Folder not found" wireframe's
            // explanatory line -- shown only for Missing, matching §17's
            // other cards (17.1/17.2/17.4), which carry no such line.
            let detail = matches!(card.status, SourceStatus::Missing)
                .then(|| tr(locale, MessageKey::SourceFolderNotFoundDetail));
            let summary = source_summary(
                locale,
                card.indexed,
                card.stale,
                card.failed,
                card.no_text_found,
            );
            content = content.push(source_card(
                tokens,
                card.display_name.clone(),
                card.display_path.clone(),
                summary,
                status_label,
                detail,
                refresh_action,
                state.selected_source == Some(i),
                // Task 062: the button was unlabelled, and removed directly.
                tr(locale, MessageKey::SourceActionRemoveFromOrbok),
                Message::AskRemoveSource(card.source_id.clone()),
            ));
        }
    }
    page(tokens, content)
}

// ── Indexing view ────────────────────────────────────────────────────────

pub fn indexing_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;
    let h = state.health;

    let mut cells = hrow![health_cell(
        tokens,
        tr(locale, MessageKey::IndexingHealthIndexed),
        h.indexed
    )]
    .spacing(tokens.spacing.sm);
    if h.queued > 0 || state.show_advanced {
        cells = cells.push(health_cell(
            tokens,
            tr(locale, MessageKey::IndexingHealthQueued),
            h.queued,
        ));
    }
    if h.stale > 0 || state.show_advanced {
        cells = cells.push(health_cell(
            tokens,
            tr(locale, MessageKey::IndexingHealthStale),
            h.stale,
        ));
    }
    if h.failed > 0 || state.show_advanced {
        cells = cells.push(health_cell(
            tokens,
            tr(locale, MessageKey::IndexingHealthFailed),
            h.failed,
        ));
    }

    // RFC-036 §14.1 (RFC-056 Slice 4): the literal "preparing"/"ready"
    // copy, not RFC-041's abandoned `SearchPreparingFolder`/
    // `SearchPartialReadiness` (removed -- see `i18n.rs`). Named to a
    // specific folder only when exactly one source exists: `SourceCard`'s
    // per-source counts are never updated after creation, so with more
    // than one source there is no honest way to say *which* is still
    // preparing.
    let status = if h.queued == 0 {
        files_ready_for_search(locale, h.indexed)
    } else {
        match state.sources.as_slice() {
            [only] => preparing_folder_for_search(locale, &only.display_name),
            _ => tr(locale, MessageKey::IndexingRunning).to_string(),
        }
    };

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::IndexingTitle)),
        cells,
        // Review 189 §2: two of the three possible contents can wrap --
        // the "ready" branch is two sentences, and the "preparing"
        // branch embeds a user-supplied, unbounded folder name, the same
        // reasoning already applied to recent-search entries and
        // fmt_query above. Harmless on the short "Indexing…" branch.
        text(status)
            .size(theme::body_s(tokens, sc))
            .line_height(theme::body_lh(tokens)),
    ];

    if h.queued > 0 {
        content = content.push(job_progress(
            tokens,
            tr(locale, MessageKey::IndexingRunning),
            None,
        ));
    }

    page(tokens, content)
}

// ── Storage view ─────────────────────────────────────────────────────────

pub fn storage_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    if state.visible_confirmation() == Some(crate::state::Confirmation::ResetCatalog) {
        let content = column![
            text(tr(locale, MessageKey::StorageResetCatalog)).size(theme::title_s(tokens, sc)),
            text(tr(locale, MessageKey::StorageResetWarning))
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
            hrow![
                components::ghost(
                    tokens,
                    tr(locale, MessageKey::Cancel),
                    Some(Message::CancelResetCatalog)
                ),
                components::danger(
                    tokens,
                    tr(locale, MessageKey::StorageResetCatalog),
                    Some(Message::ConfirmResetCatalog)
                ),
            ]
            .spacing(tokens.spacing.md),
        ]
        .spacing(tokens.spacing.lg);
        return page(tokens, content);
    }

    let total_bytes: u64 = state.storage_rows.iter().map(|(_, b, _)| b).sum();
    let gib = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let mut breakdown = column![
        text(tr(locale, MessageKey::StorageTitle)).size(theme::heading_s(tokens, sc)),
        text(tr(locale, MessageKey::StorageIntro))
            .size(theme::body_s(tokens, sc))
            .line_height(theme::body_lh(tokens)),
        text(fmt_gib(locale, gib)).size(theme::title_s(tokens, sc)),
    ]
    .spacing(tokens.spacing.xs);

    if !state.storage_rows.is_empty() {
        if state.show_advanced {
            for (category, bytes, count) in &state.storage_rows {
                if *bytes > 0 || *count > 0 {
                    let mib = *bytes as f64 / (1024.0 * 1024.0);
                    breakdown = breakdown.push(
                        text(fmt_storage_row(locale, category, mib, *count))
                            .size(theme::meta_s(tokens, sc)),
                    );
                }
            }
        } else {
            let mut search_index = 0u64;
            let mut ai_models = 0u64;
            let mut caches = 0u64;
            for (category, bytes, _) in &state.storage_rows {
                match category.as_str() {
                    "keyword_index" | "vector_index" => search_index += bytes,
                    "model_files" => ai_models += bytes,
                    "snippet_cache" | "search_cache" | "temporary_extraction" => caches += bytes,
                    _ => {}
                }
            }
            let mib = |b: u64| b as f64 / (1024.0 * 1024.0);
            for (label, bytes) in [
                (
                    tr(locale, MessageKey::StorageGroupSearchIndex),
                    search_index,
                ),
                (tr(locale, MessageKey::StorageGroupModels), ai_models),
                (tr(locale, MessageKey::StorageGroupCaches), caches),
            ] {
                if bytes > 0 {
                    breakdown = breakdown.push(
                        text(fmt_mib_bucket(locale, label, mib(bytes)))
                            .size(theme::body_s(tokens, sc)),
                    );
                }
            }
        }
    }

    let content = column![
        breakdown,
        text(tr(locale, MessageKey::StorageSafeCleanupHeading)).size(theme::body_s(tokens, sc)),
        hrow![
            components::secondary(
                tokens,
                tr(locale, MessageKey::StorageClearSnippets),
                Some(Message::CleanSnippets)
            ),
            components::secondary(
                tokens,
                tr(locale, MessageKey::StorageClearSearchCache),
                Some(Message::CleanSearchCache)
            ),
            components::secondary(
                tokens,
                tr(locale, MessageKey::StorageClearTemporaryExtraction),
                Some(Message::CleanTemporaryExtraction)
            ),
            components::secondary(
                tokens,
                tr(locale, MessageKey::StorageRemoveReplacedStaleIndexes),
                Some(Message::RemoveReplacedStaleIndexes)
            ),
        ]
        .spacing(tokens.spacing.sm)
        .wrap(),
        text(tr(locale, MessageKey::StorageDangerHeading)).size(theme::body_s(tokens, sc)),
        components::danger(
            tokens,
            tr(locale, MessageKey::StorageResetCatalog),
            Some(Message::AskResetCatalog)
        ),
        text(tr(locale, MessageKey::StorageResetWarning))
            .size(theme::meta_s(tokens, sc))
            .line_height(theme::meta_lh(tokens)),
    ];
    page(tokens, content)
}

// ── Models view ──────────────────────────────────────────────────────────

pub fn models_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;
    let available = tr(locale, MessageKey::ModelsStatusAvailable);
    let missing = tr(locale, MessageKey::ModelsStatusMissing);
    let (embedding, reranker) = match state.capability {
        SearchCapability::KeywordOnly => (missing, missing),
        SearchCapability::Hybrid => (available, missing),
        SearchCapability::HybridWithRerank => (available, available),
    };
    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::ModelsTitle)),
        text(fmt_label_value(
            locale,
            tr(locale, MessageKey::ModelsEmbeddingRole),
            embedding
        ))
        .size(theme::body_s(tokens, sc)),
        text(fmt_label_value(
            locale,
            tr(locale, MessageKey::ModelsRerankerRole),
            reranker
        ))
        .size(theme::body_s(tokens, sc)),
    ];
    if state.capability == SearchCapability::KeywordOnly {
        content = content.push(
            text(tr(locale, MessageKey::ModelsKeywordOnlyHint))
                .size(theme::meta_s(tokens, sc))
                .line_height(theme::meta_lh(tokens)),
        );
    }
    if let Some(provenance) = state.active_model_provenance {
        let status = match provenance {
            crate::state::ModelProvenance::AppManaged => {
                tr(locale, MessageKey::ModelTrustAppVerified)
            }
            crate::state::ModelProvenance::UserSupplied => {
                tr(locale, MessageKey::ModelTrustUserSupplied)
            }
        };
        content = content.push(
            text(fmt_label_value(
                locale,
                tr(locale, MessageKey::ModelsVerification),
                status,
            ))
            .size(theme::body_s(tokens, sc)),
        );
    }
    page(tokens, content)
}

// ── Settings view ────────────────────────────────────────────────────────

pub fn settings_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    // Language picker
    let mut language_row = hrow![].spacing(tokens.spacing.sm);
    for candidate in Locale::ALL {
        let label = text(candidate.display_name()).size(theme::body_s(tokens, sc));
        let mut b = button(label).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
        if *candidate != locale {
            b = b.on_press(Message::SetLocale(*candidate));
        }
        language_row = language_row.push(b);
    }

    // Theme picker
    let mut theme_row = hrow![].spacing(tokens.spacing.sm);
    for candidate in Theme::ALL {
        let label = text(tr(locale, candidate.label_key())).size(theme::body_s(tokens, sc));
        let mut b = button(label).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
        if *candidate != state.theme {
            b = b.on_press(Message::SetTheme(*candidate));
        }
        theme_row = theme_row.push(b);
    }

    // Text size picker (RFC-035)
    let mut scale_row = hrow![].spacing(tokens.spacing.sm);
    for candidate in TextScale::ALL {
        let label = text(tr(locale, candidate.label_key())).size(theme::body_s(tokens, sc));
        let mut b = button(label).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
        if *candidate != sc {
            b = b.on_press(Message::SetTextScale(*candidate));
        }
        scale_row = scale_row.push(b);
    }

    // Reduce motion toggle (RFC-035) — checkbox-style button
    let motion_label = tr(locale, MessageKey::SettingsReduceMotion);
    let motion_btn = if state.reduced_motion {
        button(
            hrow![
                icon_text(char::from(lucide::Check), theme::body_s(tokens, sc).0),
                text(motion_label.to_string()).size(theme::body_s(tokens, sc)),
            ]
            .spacing(tokens.spacing.xs),
        )
        .padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]))
        .on_press(Message::SetReducedMotion(false))
    } else {
        button(text(motion_label.to_string()).size(theme::body_s(tokens, sc)))
            .padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]))
            .on_press(Message::SetReducedMotion(true))
    };

    let content = column![
        heading(tokens, sc, tr(locale, MessageKey::SettingsTitle)),
        // Language
        text(tr(locale, MessageKey::SettingsLanguageHeading)).size(theme::body_s(tokens, sc)),
        language_row,
        // Theme
        text(tr(locale, MessageKey::SettingsThemeHeading)).size(theme::body_s(tokens, sc)),
        theme_row,
        // Text size
        text(tr(locale, MessageKey::SettingsTextScaleHeading)).size(theme::body_s(tokens, sc)),
        scale_row,
        // Accessibility
        hrow![
            motion_btn,
            text(tr(locale, MessageKey::SettingsReduceMotionHint))
                .size(theme::meta_s(tokens, sc))
                .line_height(theme::meta_lh(tokens)),
        ]
        .spacing(tokens.spacing.sm),
        // CVD note (always-on — informational, not a toggle)
        text(tr(locale, MessageKey::SettingsCvdNote))
            .size(theme::meta_s(tokens, sc))
            .line_height(theme::meta_lh(tokens)),
        // Privacy
        text(tr(locale, MessageKey::SettingsPrivacyHeading)).size(theme::body_s(tokens, sc)),
        text(tr(locale, MessageKey::SettingsPrivacyLocalOnly))
            .size(theme::body_s(tokens, sc))
            .line_height(theme::body_lh(tokens)),
        // RFC-042: Remember recent searches toggle + note.
        hrow![
            button(
                text(if state.remember_recent_searches {
                    tr(locale, MessageKey::SettingsAdvancedOn)
                } else {
                    tr(locale, MessageKey::SettingsAdvancedOff)
                })
                .size(theme::body_s(tokens, sc)),
            )
            .on_press(Message::ToggleRememberRecentSearches(
                !state.remember_recent_searches
            )),
            text(tr(locale, MessageKey::RememberRecentSearches)).size(theme::body_s(tokens, sc)),
        ]
        .spacing(tokens.spacing.sm),
        text(tr(locale, MessageKey::RecentSearchesPrivacyNote))
            .size(theme::meta_s(tokens, sc))
            .line_height(theme::meta_lh(tokens)),
        recent_searches_clear_control(state),
        // Advanced
        text(tr(locale, MessageKey::SettingsAdvancedHeading)).size(theme::body_s(tokens, sc)),
        hrow![
            button(
                text(if state.show_advanced {
                    tr(locale, MessageKey::SettingsAdvancedOn)
                } else {
                    tr(locale, MessageKey::SettingsAdvancedOff)
                })
                .size(theme::body_s(tokens, sc)),
            )
            .on_press(Message::ToggleAdvanced),
            text(tr(locale, MessageKey::SettingsAdvancedHint))
                .size(theme::meta_s(tokens, sc))
                .line_height(theme::meta_lh(tokens)),
        ]
        .spacing(tokens.spacing.sm),
    ];
    page(tokens, content)
}
