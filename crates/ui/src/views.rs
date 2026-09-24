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
    fmt_query, fmt_rebuild_prepares, fmt_reset_removes, fmt_storage_row,
    preparing_folder_for_search, search_location_chip, search_result_count, source_summary, tr,
};
use crate::state::{AppState, FileCountState, Message, ResultTrustDisplay, SearchFolderScope};
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
            // First-run / no-folder state: the one-line prompt, and its
            // second half is the control that opens the picker (Task 105,
            // RFC-045 §7.1 Amendment): after a cancelled picker the next
            // step is something to press, not only text.
            hrow![
                text(tr(locale, MessageKey::SearchInLabel)).size(theme::meta_s(tokens, sc)),
                components::ghost(
                    tokens,
                    tr(locale, MessageKey::SearchChooseFolder),
                    (!state.search_location.picker_in_progress)
                        .then_some(Message::ChooseSearchFolder),
                ),
            ]
            .spacing(tokens.spacing.xs)
            .align_y(iced::Alignment::Center)
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

    // Task 110: the private-folder question, when the search picker asked it.
    if state.visible_confirmation() == Some(crate::state::Confirmation::AddSensitiveFolderOnSearch)
    {
        return private_folder_dialog(state);
    }

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
                    tr(locale, MessageKey::SourcesAddFolder),
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

/// The label of a recovery action. HANDOFF-038 §3 held `OpenAnyway` and
/// `ShowInFolder` back, reasoning orbok had no way to open a file at all;
/// Task 041 gave it one, `launch_request` (`result_launch.rs`) already maps
/// both through that same catalog-checked path, and Task 082 lifted the
/// hold on the strength of that (Review Request 254 §4, §6).
fn recovery_label(action: ResultRecoveryAction) -> Option<MessageKey> {
    match action {
        ResultRecoveryAction::PrepareAgain => Some(MessageKey::TrustActionPrepareAgain),
        ResultRecoveryAction::CheckFolder => Some(MessageKey::TrustActionCheckFolder),
        ResultRecoveryAction::RemoveFromResults => Some(MessageKey::TrustActionRemoveFromResults),
        ResultRecoveryAction::ViewDetails => Some(MessageKey::TrustActionViewDetails),
        ResultRecoveryAction::OpenAnyway => Some(MessageKey::TrustActionOpenAnyway),
        ResultRecoveryAction::ShowInFolder => Some(MessageKey::TrustActionShowInFolder),
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

/// Task 110: "Add a folder that may contain private files?" -- Task 062's
/// dialog shape: the title asks, the confirm button names the action, Cancel
/// (and Escape) add nothing. The folder's path is shown so the user knows
/// which one is being asked about. Rendered by whichever page asked
/// (`sources_view`, `search_view`).
fn private_folder_dialog(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;
    let path = state
        .pending_folder_add
        .as_ref()
        .map(|p| p.path.as_str())
        .unwrap_or_default();
    let content = column![
        text(tr(locale, MessageKey::AddSensitiveTitle)).size(theme::title_s(tokens, sc)),
        text(path.to_string()).size(theme::meta_s(tokens, sc)),
        text(crate::i18n::fmt_add_sensitive_body(locale))
            .size(theme::body_s(tokens, sc))
            .line_height(theme::body_lh(tokens)),
        hrow![
            components::ghost(
                tokens,
                tr(locale, MessageKey::Cancel),
                Some(Message::CancelAddSensitiveFolder)
            ),
            components::secondary(
                tokens,
                tr(locale, MessageKey::AddSensitiveConfirm),
                Some(Message::ConfirmAddSensitiveFolder)
            ),
        ]
        .spacing(tokens.spacing.md),
    ]
    .spacing(tokens.spacing.lg);
    page(tokens, content)
}

pub fn sources_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    // Task 110: the private-folder question, when the Folders page asked it.
    if state.visible_confirmation() == Some(crate::state::Confirmation::AddSensitiveFolderOnFolders)
    {
        return private_folder_dialog(state);
    }

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
    .on_submit(Message::SubmitSourcePath)
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
            // decides which refresh action applies. The state label is
            // `SourceCard::state_label_key` (Task 108): unreachable first,
            // then Preparing, Needs update, Ready. "Prepare again" stays
            // while a folder prepares -- asking twice is harmless, and the
            // button should not come and go.
            use orbok_core::SourceStatus;
            let status_label = tr(locale, card.state_label_key());
            let refresh_action = match card.status {
                SourceStatus::Active => Some(MessageKey::SourceActionPrepareAgain),
                SourceStatus::Missing | SourceStatus::PermissionDenied => {
                    Some(MessageKey::SourceActionCheckAgain)
                }
                SourceStatus::Paused | SourceStatus::Removed => None,
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
        tr(locale, FileCountState::Ready.label_key()),
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
            tr(locale, FileCountState::NeedsUpdate.label_key()),
            h.stale,
        ));
    }
    if h.failed > 0 || state.show_advanced {
        cells = cells.push(health_cell(
            tokens,
            tr(locale, FileCountState::Failed.label_key()),
            h.failed,
        ));
    }

    // RFC-036 §14.1 (RFC-056 Slice 4): the literal "preparing"/"ready"
    // copy, not RFC-041's abandoned `SearchPreparingFolder`/
    // `SearchPartialReadiness` (removed -- see `i18n.rs`). Named to a
    // specific folder when exactly one folder has unfinished work (Task 108:
    // the cards follow preparation now, so which one is known).
    let status = if h.queued == 0 {
        files_ready_for_search(locale, h.indexed)
    } else {
        let mut preparing = state.sources.iter().filter(|c| c.is_preparing());
        match (preparing.next(), preparing.next()) {
            (Some(only), None) => preparing_folder_for_search(locale, &only.display_name),
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

/// Task 081: the Advanced-view label for one RFC-011 §11 storage category.
fn storage_category_label(locale: Locale, category: orbok_core::StorageCategory) -> String {
    use orbok_core::StorageCategory::*;
    let key = match category {
        PersistentCatalog => MessageKey::StorageCategoryPersistentCatalog,
        KeywordIndex => MessageKey::StorageCategoryKeywordIndex,
        VectorIndex => MessageKey::StorageCategoryVectorIndex,
        SnippetCache => MessageKey::StorageCategorySnippetCache,
        SearchCache => MessageKey::StorageCategorySearchCache,
        TemporaryExtraction => MessageKey::StorageCategoryTemporaryExtraction,
        ModelFiles => MessageKey::StorageCategoryModelFiles,
        Logs => MessageKey::StorageCategoryLogs,
    };
    tr(locale, key).to_string()
}

pub fn storage_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    if state.visible_confirmation() == Some(crate::state::Confirmation::ResetCatalog) {
        let mut content = column![
            text(tr(locale, MessageKey::StorageResetConfirmTitle)).size(theme::title_s(tokens, sc)),
            text(tr(locale, MessageKey::StorageResetWarning))
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
        ]
        .spacing(tokens.spacing.lg);
        // Task 092/094: counted, never estimated -- absent while the count
        // is in flight or unreadable (`reset_counts` is `None`), never a
        // placeholder zero. Reset itself never waits on this. The history
        // clause needs both a non-zero count and the "Remember recent
        // searches" setting itself -- a count alone cannot tell whether
        // the list showing is one reset is about to clear or one that
        // was already off and stale.
        if let Some(counts) = state.reset_counts {
            let includes_history = state.remember_recent_searches && counts.history > 0;
            content = content.push(
                text(fmt_reset_removes(
                    locale,
                    counts.folders,
                    counts.files,
                    includes_history,
                ))
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
            );
        }
        content = content.push(
            hrow![
                components::ghost(
                    tokens,
                    tr(locale, MessageKey::Cancel),
                    Some(Message::CancelResetCatalog)
                ),
                components::danger(
                    tokens,
                    tr(locale, MessageKey::StorageResetConfirm),
                    Some(Message::ConfirmResetCatalog)
                ),
            ]
            .spacing(tokens.spacing.md),
        );
        return page(tokens, content);
    }

    // Task 099: the two rebuild confirmations share one shape, differing
    // only in title/button key and which message they send -- both read
    // `state.rebuild_file_count`, cleared and re-fetched whenever either
    // opens (`Message::AskDeleteKeywordIndex`/`AskDeleteVectorIndex`).
    for (confirmation, title_key, cancel_msg, confirm_msg) in [
        (
            crate::state::Confirmation::DeleteKeywordIndex,
            MessageKey::RebuildKeywordConfirmTitle,
            Message::CancelDeleteKeywordIndex,
            Message::ConfirmDeleteKeywordIndex,
        ),
        (
            crate::state::Confirmation::DeleteVectorIndex,
            MessageKey::RebuildVectorConfirmTitle,
            Message::CancelDeleteVectorIndex,
            Message::ConfirmDeleteVectorIndex,
        ),
    ] {
        if state.visible_confirmation() != Some(confirmation) {
            continue;
        }
        let mut content = column![
            text(tr(locale, title_key)).size(theme::title_s(tokens, sc)),
            text(tr(locale, MessageKey::RebuildConfirmBody))
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
        ]
        .spacing(tokens.spacing.lg);
        // §2.3: counted, never estimated; no count (in flight, unreadable,
        // or genuinely zero -- `rebuild_file_count` is already `None` for
        // all three), no line.
        if let Some(files) = state.rebuild_file_count {
            content = content.push(
                text(fmt_rebuild_prepares(locale, files))
                    .size(theme::body_s(tokens, sc))
                    .line_height(theme::body_lh(tokens)),
            );
        }
        content = content.push(
            hrow![
                components::ghost(tokens, tr(locale, MessageKey::Cancel), Some(cancel_msg)),
                components::danger(
                    tokens,
                    tr(locale, MessageKey::RebuildConfirm),
                    Some(confirm_msg)
                ),
            ]
            .spacing(tokens.spacing.md),
        );
        return page(tokens, content);
    }

    let mut breakdown = column![
        text(tr(locale, MessageKey::StorageTitle)).size(theme::heading_s(tokens, sc)),
        text(tr(locale, MessageKey::StorageIntro))
            .size(theme::body_s(tokens, sc))
            .line_height(theme::body_lh(tokens)),
    ]
    .spacing(tokens.spacing.xs);

    if state.storage_rows.is_empty() {
        // Task 081 (RFC-011 §13.1): never measured this session -- not a
        // zero, an explicit "ask for it" state, exact owner-approved copy.
        breakdown = breakdown.push(
            text(tr(locale, MessageKey::StorageNotCalculatedYet)).size(theme::body_s(tokens, sc)),
        );
        breakdown = breakdown.push(components::secondary(
            tokens,
            tr(locale, MessageKey::StorageCalculateNow),
            Some(Message::StorageMeasurementRequested),
        ));
    } else {
        // Task 081: the total is the sum of every *measured* category
        // except keyword_index/vector_index, whose bytes already live
        // inside persistent_catalog's own file-size measurement -- summing
        // them too would double-count. An Unknown category is left out of
        // the sum silently, not counted as zero: the total is honestly
        // "at least this many bytes", never claimed complete.
        let total_bytes: u64 = state
            .storage_rows
            .iter()
            .filter(|(cat, _)| {
                !matches!(
                    cat,
                    orbok_core::StorageCategory::KeywordIndex
                        | orbok_core::StorageCategory::VectorIndex
                )
            })
            .filter_map(|(_, m)| match m {
                orbok_core::StorageMeasurement::Measured { bytes, .. } => Some(*bytes),
                orbok_core::StorageMeasurement::Unknown => None,
            })
            .sum();
        let gib = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        breakdown = breakdown.push(text(fmt_gib(locale, gib)).size(theme::title_s(tokens, sc)));
        breakdown = breakdown.push(components::secondary(
            tokens,
            tr(locale, MessageKey::StorageCalculateNow),
            Some(Message::StorageMeasurementRequested),
        ));

        if state.show_advanced {
            for (category, measurement) in &state.storage_rows {
                let label = storage_category_label(locale, *category);
                let line = match measurement {
                    orbok_core::StorageMeasurement::Measured { bytes, items } => {
                        let mib = *bytes as f64 / (1024.0 * 1024.0);
                        fmt_storage_row(locale, &label, mib, *items)
                    }
                    orbok_core::StorageMeasurement::Unknown => format!(
                        "  {}",
                        fmt_label_value(
                            locale,
                            &label,
                            tr(locale, MessageKey::StorageValueUnknown)
                        )
                    ),
                };
                breakdown = breakdown.push(text(line).size(theme::meta_s(tokens, sc)));
            }
            // Task 081 §2: the cache file's own size, shown here since it
            // is a technical detail (several categories' bytes live inside
            // it, and it does not shrink to match them until a VACUUM --
            // Task 079 §2) rather than one of RFC-011's eight categories.
            if let Some(cache_bytes) = state.storage_cache_file_bytes {
                let mib = cache_bytes as f64 / (1024.0 * 1024.0);
                breakdown = breakdown.push(
                    text(format!(
                        "  {}",
                        fmt_label_value(
                            locale,
                            tr(locale, MessageKey::StorageCacheFileSize),
                            format!("{mib:.1} MiB")
                        )
                    ))
                    .size(theme::meta_s(tokens, sc)),
                );
            }
        } else {
            let mut search_index = 0u64;
            let mut ai_models = 0u64;
            let mut caches = 0u64;
            for (category, measurement) in &state.storage_rows {
                let bytes = match measurement {
                    orbok_core::StorageMeasurement::Measured { bytes, .. } => *bytes,
                    orbok_core::StorageMeasurement::Unknown => continue,
                };
                match category {
                    orbok_core::StorageCategory::KeywordIndex
                    | orbok_core::StorageCategory::VectorIndex => search_index += bytes,
                    orbok_core::StorageCategory::ModelFiles => ai_models += bytes,
                    orbok_core::StorageCategory::SnippetCache
                    | orbok_core::StorageCategory::SearchCache
                    | orbok_core::StorageCategory::TemporaryExtraction => caches += bytes,
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

    let mut content = column![
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
    ];

    // Task 099 (RFC-011 §14 criteria 5/6): rebuild actions, Advanced view
    // only -- they force hours of work on a large corpus (§2.1), so they
    // do not belong beside the ordinary Safe cleanup row. Keyword search
    // always works, so its button is unconditional; "search by meaning"
    // only has an index to rebuild when a model is actually configured.
    if state.show_advanced {
        let mut rebuild_row = hrow![components::secondary(
            tokens,
            tr(locale, MessageKey::StorageRebuildKeywordButton),
            Some(Message::AskDeleteKeywordIndex)
        )];
        if state.capability != SearchCapability::KeywordOnly {
            rebuild_row = rebuild_row.push(components::secondary(
                tokens,
                tr(locale, MessageKey::StorageRebuildVectorButton),
                Some(Message::AskDeleteVectorIndex),
            ));
        }
        content = content.push(rebuild_row.spacing(tokens.spacing.sm).wrap());
    }

    content = content
        .push(text(tr(locale, MessageKey::StorageDangerHeading)).size(theme::body_s(tokens, sc)));
    content = content.push(components::danger(
        tokens,
        tr(locale, MessageKey::StorageResetCatalog),
        Some(Message::AskResetCatalog),
    ));
    content = content.push(
        text(tr(locale, MessageKey::StorageResetWarning))
            .size(theme::meta_s(tokens, sc))
            .line_height(theme::meta_lh(tokens)),
    );
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
                    tr(locale, MessageKey::SettingsToggleOn)
                } else {
                    tr(locale, MessageKey::SettingsToggleOff)
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
                    tr(locale, MessageKey::SettingsToggleOn)
                } else {
                    tr(locale, MessageKey::SettingsToggleOff)
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
