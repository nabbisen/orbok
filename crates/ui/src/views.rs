//! Page view functions (GUI external design §7, §8–§12 wireframes).
//!
//! Styling (RFC-032/035): sizes come from `state.tokens` via [`crate::theme`]
//! scaled by `state.text_scale`. No literal sizes, paddings, or colours.
//!
//! Primitives (RFC-033): cards/buttons/badges/progress via [`crate::components`].
//!
//! Formatting (RFC-035): user-facing numbers and sizes via [`crate::i18n`].

pub mod wizard;
pub use wizard::wizard_view;

use crate::components::{self, health_cell, job_progress, result_card, source_card};
use crate::i18n::{
    Locale, MessageKey, files_indexed, fmt_gib, fmt_mib_bucket, fmt_query, fmt_storage_row,
    search_location_chip, search_result_count, source_summary, tr,
};
use crate::state::{AppState, Message, SearchFolderScope};
use crate::theme::{self, TextScale, Theme};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Padding};
use orbok_models::SearchCapability;
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
        return row![
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

            let mut entry_col = column![text(&entry.search_text).size(theme::body_s(tokens, sc))]
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
        row![
            text(tr(locale, MessageKey::RecentSearchesLabel)).size(theme::label_s(tokens, sc)),
            button(text("✕").size(theme::meta_s(tokens, sc)))
                .on_press(Message::CloseRecentSearches),
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

    if state.confirm_clear_history {
        column![
            text(tr(locale, MessageKey::ClearRecentSearchesConfirmTitle))
                .size(theme::body_s(tokens, sc)),
            text(tr(locale, MessageKey::ClearRecentSearchesConfirmBody))
                .size(theme::meta_s(tokens, sc))
                .color(to_iced_color(tokens.palette.text_secondary)),
            row![
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
            row![
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

            row![
                text(tr(locale, MessageKey::SearchInLabel)).size(theme::meta_s(tokens, sc)),
                // Folder chip with ✕ remove — keyboard removable (RFC-045 §20).
                button(text(format!("{chip_label}  ✕")).size(theme::meta_s(tokens, sc)))
                    .on_press(Message::SearchLocationCleared),
                // Scope toggle button.
                button(
                    text(format!("↕ {}", tr(locale, other_label_key)))
                        .size(theme::meta_s(tokens, sc)),
                )
                .on_press(Message::SearchScopeChanged(other_scope)),
            ]
            .spacing(tokens.spacing.xs)
            .into()
        }
    }
}

fn friendly_notice<'a>(
    tokens: &'a Tokens,
    locale: Locale,
    notice: &crate::notice::UserNotice,
) -> Element<'a, Message> {
    use snora::design::notice::Notice;
    let mut builder = Notice::new(tokens, notice.tone(), notice.body(locale).to_string())
        .title(notice.title(locale).to_string());
    if let Some(action_label) = notice.action(locale) {
        builder = builder.action(action_label.to_string(), Message::ClearNotice);
    } else {
        builder = builder.dismiss(Message::ClearNotice);
    }
    builder.render()
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
        .padding(tokens.spacing.sm);

    let submit = components::icon_primary(
        tokens,
        char::from(lucide::Search),
        13.0,
        tr(locale, MessageKey::SearchButton),
        (!state.search_running).then_some(Message::SubmitSearch),
    );

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::NavSearch)),
        row![container(input).width(Length::Fill), submit].spacing(tokens.spacing.sm),
        // RFC-045: "Search in" location row.
        search_location_row(state),
    ];

    // RFC-045 §7.4: recent / remembered folder quick-select chips.
    // Shown only when there are remembered folders and no folder is already
    // selected (they disappear once a choice is made — progressive disclosure).
    if !state.search_location.recent_locations.is_empty()
        && state.search_location.selected.is_none()
    {
        let mut chips = row![
            text(tr(locale, MessageKey::SearchRecentFoldersLabel))
                .size(theme::meta_s(tokens, sc))
                .color(to_iced_color(tokens.palette.text_secondary)),
        ]
        .spacing(tokens.spacing.xs);
        for summary in &state.search_location.recent_locations {
            chips = chips.push(
                button(text(&summary.display_name).size(theme::meta_s(tokens, sc)))
                    .on_press(Message::RecentFolderSelected(summary.source_id.clone())),
            );
        }
        content = content.push(chips);
    }

    // RFC-042: Recent searches (collapsed button or expanded panel).
    content = content.push(recent_searches_panel(state));

    if let Some(notice) = &state.notice {
        content = content.push(friendly_notice(tokens, locale, notice));
    }

    if state.show_advanced {
        content = content.push(
            row![
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
                .on_press(Message::SetSearchMode(orbok_search::SearchMode::Conceptual)),
            ]
            .spacing(tokens.spacing.xs),
        );
    }

    if state.sources.is_empty() {
        content = content.push(
            column![
                text(tr(locale, MessageKey::SearchNoSourcesTitle)).size(theme::title_s(tokens, sc)),
                text(tr(locale, MessageKey::SearchNoSourcesBody)).size(theme::body_s(tokens, sc)),
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
                    .size(theme::meta_s(tokens, sc)),
            );
        }
        if state.search_running {
            content = content.push(text("Searching…").size(theme::body_s(tokens, sc)));
        } else if let Some(last) = &state.last_query {
            if state.search_results.is_empty() {
                content = content.push(
                    column![
                        text(tr(locale, MessageKey::SearchNoResults))
                            .size(theme::body_s(tokens, sc)),
                        text(fmt_query(locale, last)).size(theme::meta_s(tokens, sc)),
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
                    let title_str = if is_selected {
                        format!("▶  {title_raw}")
                    } else {
                        title_raw.to_string()
                    };
                    let snippet = result.snippet.as_deref().unwrap_or("(source unavailable)");
                    let heading_str = result.heading_path.as_deref().unwrap_or("");
                    content = content.push(result_card(
                        tokens,
                        title_str,
                        result.display_path.clone(),
                        heading_str.to_string(),
                        snippet.to_string(),
                        &result.badges,
                        state.show_advanced,
                        is_selected,
                        Message::SelectResult(i),
                    ));
                }
            }
        }
    }
    page(tokens, content)
}

// ── Sources view ─────────────────────────────────────────────────────────

pub fn sources_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    let add_btn = components::icon_secondary(
        tokens,
        char::from(lucide::FolderPlus),
        13.0,
        tr(locale, MessageKey::SourcesAddFolder),
        Some(Message::RequestAddSource),
    );
    let add_input = text_input("Or type a path manually…", &state.source_path_input)
        .on_input(Message::SourcePathChanged)
        .on_submit(Message::RequestAddSource)
        .padding(tokens.spacing.sm);

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::SourcesTitle)),
        row![add_btn, container(add_input).width(Length::Fill)].spacing(tokens.spacing.sm),
        text("All sub-folders are scanned recursively.").size(theme::meta_s(tokens, sc)),
    ];

    if let Some(notice) = &state.notice {
        content = content.push(friendly_notice(tokens, locale, notice));
    }
    if state.sources.is_empty() {
        content = content.push(
            column![
                text(tr(locale, MessageKey::SourcesEmptyTitle)).size(theme::title_s(tokens, sc)),
                text(tr(locale, MessageKey::SourcesEmptyBody)).size(theme::body_s(tokens, sc)),
            ]
            .spacing(tokens.spacing.sm),
        );
    } else {
        for card in &state.sources {
            let status_label = if card.active {
                tr(locale, MessageKey::SourcesStatusActive)
            } else {
                tr(locale, MessageKey::SourcesStatusPaused)
            };
            let summary = source_summary(locale, card.indexed, card.stale, card.failed);
            content = content.push(source_card(
                tokens,
                card.display_name.clone(),
                card.display_path.clone(),
                summary,
                status_label,
                Message::SourceRemoved(card.source_id.clone()),
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

    let mut cells = row![health_cell(
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

    let mut content = column![
        heading(tokens, sc, tr(locale, MessageKey::IndexingTitle)),
        cells,
        text(if h.queued == 0 {
            tr(locale, MessageKey::IndexingIdle).to_string()
        } else {
            files_indexed(locale, h.indexed)
        })
        .size(theme::body_s(tokens, sc)),
    ];

    if h.queued > 0 {
        content = content.push(job_progress(tokens, "Indexing…", None));
    }

    page(tokens, content)
}

// ── Storage view ─────────────────────────────────────────────────────────

pub fn storage_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;

    if state.confirm_reset {
        let content = column![
            text(tr(locale, MessageKey::StorageResetCatalog)).size(theme::title_s(tokens, sc)),
            text(tr(locale, MessageKey::StorageResetWarning)).size(theme::body_s(tokens, sc)),
            row![
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
        text(tr(locale, MessageKey::StorageIntro)).size(theme::body_s(tokens, sc)),
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
        row![
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
        ]
        .spacing(tokens.spacing.sm),
        text(tr(locale, MessageKey::StorageDangerHeading)).size(theme::body_s(tokens, sc)),
        components::danger(
            tokens,
            tr(locale, MessageKey::StorageResetCatalog),
            Some(Message::AskResetCatalog)
        ),
        text(tr(locale, MessageKey::StorageResetWarning)).size(theme::meta_s(tokens, sc)),
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
        text(format!(
            "{}: {embedding}",
            tr(locale, MessageKey::ModelsEmbeddingRole)
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {reranker}",
            tr(locale, MessageKey::ModelsRerankerRole)
        ))
        .size(theme::body_s(tokens, sc)),
    ];
    if state.capability == SearchCapability::KeywordOnly {
        content = content.push(
            text(tr(locale, MessageKey::ModelsKeywordOnlyHint)).size(theme::meta_s(tokens, sc)),
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
    let mut language_row = row![].spacing(tokens.spacing.sm);
    for candidate in Locale::ALL {
        let label = text(candidate.display_name()).size(theme::body_s(tokens, sc));
        let mut b = button(label).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
        if *candidate != locale {
            b = b.on_press(Message::SetLocale(*candidate));
        }
        language_row = language_row.push(b);
    }

    // Theme picker
    let mut theme_row = row![].spacing(tokens.spacing.sm);
    for candidate in Theme::ALL {
        let label = text(tr(locale, candidate.label_key())).size(theme::body_s(tokens, sc));
        let mut b = button(label).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
        if *candidate != state.theme {
            b = b.on_press(Message::SetTheme(*candidate));
        }
        theme_row = theme_row.push(b);
    }

    // Text size picker (RFC-035)
    let mut scale_row = row![].spacing(tokens.spacing.sm);
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
        button(text(format!("✓  {motion_label}")).size(theme::body_s(tokens, sc)))
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
        row![
            motion_btn,
            text(tr(locale, MessageKey::SettingsReduceMotionHint)).size(theme::meta_s(tokens, sc)),
        ]
        .spacing(tokens.spacing.sm),
        // CVD note (always-on — informational, not a toggle)
        text(tr(locale, MessageKey::SettingsCvdNote)).size(theme::meta_s(tokens, sc)),
        // Privacy
        text(tr(locale, MessageKey::SettingsPrivacyHeading)).size(theme::body_s(tokens, sc)),
        text(tr(locale, MessageKey::SettingsPrivacyLocalOnly)).size(theme::body_s(tokens, sc)),
        // RFC-042: Remember recent searches toggle + note.
        row![
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
        text(tr(locale, MessageKey::RecentSearchesPrivacyNote)).size(theme::meta_s(tokens, sc)),
        recent_searches_clear_control(state),
        // Advanced
        text(tr(locale, MessageKey::SettingsAdvancedHeading)).size(theme::body_s(tokens, sc)),
        row![
            button(
                text(if state.show_advanced {
                    tr(locale, MessageKey::SettingsAdvancedOn)
                } else {
                    tr(locale, MessageKey::SettingsAdvancedOff)
                })
                .size(theme::body_s(tokens, sc)),
            )
            .on_press(Message::ToggleAdvanced),
            text(tr(locale, MessageKey::SettingsAdvancedHint)).size(theme::meta_s(tokens, sc)),
        ]
        .spacing(tokens.spacing.sm),
    ];
    page(tokens, content)
}
