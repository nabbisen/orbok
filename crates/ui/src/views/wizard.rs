//! Wizard views: model setup, download progress, file-check, and ready pages.
//!
//! Design (GUI spec §6 and RFC-012): The wizard runs at every launch when the
//! embedding model is missing or invalid. It has four pages:
//!
//! 1. **Setup** — shown on `NotConfigured` or `FileMissing`. Primary action is
//!    "Download from HuggingFace"; secondary is "Locate existing files".
//! 2. **Downloading** — progress bar while the model is being fetched.
//! 3. **Checked** — shows per-file ✓/✗ after the user locates files manually.
//! 4. **Ready** — confirmation that the model is loaded; wizard dismisses.
//!
//! Styling (RFC-032): sizes/paddings/spacing come from `state.tokens` via the
//! [`crate::theme`] helpers and the token spacing scale; icon glyph dimensions
//! stay explicit.

use crate::i18n::{
    Locale, MessageKey, model_exact_size, model_file_position, model_transfer_progress, tr,
};
use crate::state::{
    AppState, Message, ModelArtifact, ModelDeliveryFailure, ModelDownloadConsent,
    ModelPersistenceState, ModelProvenance, ModelTrustPresentation, WizardFileCheck, WizardState,
};
use crate::theme;
use iced::widget::{button, column, container, progress_bar, row, text, text_input};
use iced::{Element, Length, Padding};
use snora::design::Tokens;
use snora::lucide;

fn icon_text<'a>(glyph: char, size: f32) -> iced::widget::Text<'a> {
    iced::widget::text(glyph.to_string())
        .font(iced::Font::with_name("lucide"))
        .size(size)
}

/// Standard wizard page wrapper: token page padding, fills the window.
fn wizard_page<'a>(
    tokens: &Tokens,
    col: iced::widget::Column<'a, Message>,
) -> Element<'a, Message> {
    container(col.spacing(tokens.spacing.md))
        .padding(Padding::from([tokens.spacing.xxl, tokens.spacing.xxl]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Dispatch to the correct wizard page.
pub fn wizard_view(state: &AppState) -> Element<'_, Message> {
    let locale = state.locale;
    let tokens = &state.tokens;
    let sc = state.text_scale;
    match state
        .wizard
        .as_ref()
        .expect("wizard_view called without active wizard")
    {
        WizardState::NotConfigured => page_setup(locale, state, None),
        WizardState::FileMissing {
            previous_dir,
            checks,
        } => page_setup(
            locale,
            state,
            Some((previous_dir.as_str(), checks.as_slice())),
        ),
        WizardState::DownloadConsent { presentation, .. } => {
            page_download_consent(tokens, sc, locale, presentation)
        }
        WizardState::Downloading {
            presentation,
            current_artifact,
            bytes,
            total,
            files_done,
            files_total,
            ..
        } => page_downloading(
            tokens,
            sc,
            locale,
            presentation,
            *current_artifact,
            *bytes,
            *total,
            *files_done,
            *files_total,
        ),
        WizardState::Checked {
            model_dir,
            checks,
            all_ok,
        } => page_checked(locale, state, model_dir, checks, *all_ok),
        WizardState::Ready {
            model_dir,
            provenance,
            persistence,
            ..
        } => page_ready(tokens, sc, locale, model_dir, *provenance, *persistence),
        WizardState::DownloadFailed { failure, .. } => {
            page_download_failed(tokens, sc, locale, *failure)
        }
    }
}

// ── Page: setup ──────────────────────────────────────────────────────

fn page_setup<'a>(
    locale: Locale,
    state: &'a AppState,
    missing: Option<(&'a str, &'a [WizardFileCheck])>,
) -> Element<'a, Message> {
    let tokens = &state.tokens;
    let sc = state.text_scale;
    let mut col = column![
        text(tr(locale, MessageKey::WizardTitleNotConfigured)).size(theme::title_s(tokens, sc)),
        text(tr(locale, MessageKey::WizardBodyNotConfigured)).size(theme::body_s(tokens, sc)),
    ]
    .spacing(tokens.spacing.sm);

    // ── Primary action: Download ──────────────────────────────────────
    let download_card = container(
        column![
            row![
                icon_text(char::from(lucide::Download), 16.0),
                text(tr(locale, MessageKey::WizardDownloadAction)).size(theme::body_s(tokens, sc)),
            ]
            .spacing(tokens.spacing.sm),
            text(
                state
                    .model_download_consent
                    .as_ref()
                    .map_or("multilingual-e5-small", |offer| offer.model_name),
            )
            .size(theme::meta_s(tokens, sc)),
            button(
                row![
                    icon_text(char::from(lucide::Download), 13.0),
                    text(tr(locale, MessageKey::WizardDownloadAction))
                        .size(theme::body_s(tokens, sc)),
                ]
                .spacing(tokens.spacing.xs),
            )
            .on_press(Message::DownloadModel),
        ]
        .spacing(tokens.spacing.sm),
    )
    .padding(tokens.spacing.md);
    col = col.push(download_card);

    // ── Separator ────────────────────────────────────────────────────
    col = col.push(
        text(format!("— {} —", tr(locale, MessageKey::WizardOr))).size(theme::meta_s(tokens, sc)),
    );

    // ── Secondary action: locate existing files ───────────────────────
    col = col
        .push(text(tr(locale, MessageKey::WizardBodyFileMissing)).size(theme::meta_s(tokens, sc)));

    // Show previous path hint when files were missing.
    if let Some((prev_dir, checks)) = missing {
        col = col.push(text(prev_dir).size(theme::meta_s(tokens, sc)));
        for fc in checks {
            let (icon, note) = if fc.found {
                ("✓", String::new())
            } else {
                (
                    "✗",
                    format!("  ← {}", tr(locale, MessageKey::WizardMissingMarker)),
                )
            };
            col = col.push(
                text(format!("{icon}  {}{note}", fc.relative_path)).size(theme::meta_s(tokens, sc)),
            );
        }
    }

    let path_input = text_input(
        tr(locale, MessageKey::WizardPathPlaceholder),
        &state.wizard_path_input,
    )
    .on_input(Message::WizardPathChanged)
    .on_submit(Message::WizardValidate)
    .padding(tokens.spacing.sm);

    col = col.push(
        row![
            container(path_input).width(Length::Fill),
            button(
                row![
                    icon_text(char::from(lucide::FolderOpen), 13.0),
                    text(tr(locale, MessageKey::WizardActionValidate))
                        .size(theme::body_s(tokens, sc)),
                ]
                .spacing(tokens.spacing.xs),
            )
            .on_press(Message::WizardValidate),
        ]
        .spacing(tokens.spacing.sm),
    );

    // ── Tertiary action: skip ─────────────────────────────────────────
    col = col.push(
        button(text(tr(locale, MessageKey::WizardActionSkip)).size(theme::meta_s(tokens, sc)))
            .on_press(Message::WizardSkip),
    );

    wizard_page(tokens, col)
}

// ── Page: explicit download consent ─────────────────────────────────

fn page_download_consent<'a>(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    locale: Locale,
    presentation: &'a ModelDownloadConsent,
) -> Element<'a, Message> {
    let trust = match presentation.trust {
        ModelTrustPresentation::AppWillVerify => tr(locale, MessageKey::ModelTrustAppWillVerify),
        ModelTrustPresentation::AppVerified => tr(locale, MessageKey::ModelTrustAppVerified),
        ModelTrustPresentation::UserSupplied => tr(locale, MessageKey::ModelTrustUserSupplied),
    };
    let col = column![
        text(tr(locale, MessageKey::ModelConsentTitle)).size(theme::title_s(tokens, sc)),
        text(presentation.model_name).size(theme::body_s(tokens, sc)),
        text(tr(locale, MessageKey::ModelConsentBody)).size(theme::body_s(tokens, sc)),
        text(tr(locale, MessageKey::ModelConsentPrivacy)).size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentProvider),
            presentation.provider
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentSource),
            presentation.source
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentRevision),
            presentation.immutable_revision
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentExactSize),
            model_exact_size(locale, presentation.exact_size_bytes)
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentLicense),
            presentation.license
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {}",
            tr(locale, MessageKey::ModelConsentLocation),
            presentation.destination
        ))
        .size(theme::body_s(tokens, sc)),
        text(format!(
            "{}: {trust}",
            tr(locale, MessageKey::ModelConsentVerification)
        ))
        .size(theme::body_s(tokens, sc)),
        row![
            button(
                text(tr(locale, MessageKey::ModelConsentConfirm)).size(theme::body_s(tokens, sc)),
            )
            .on_press(Message::ConfirmModelDownload),
            button(
                text(tr(locale, MessageKey::ModelConsentCancel)).size(theme::body_s(tokens, sc)),
            )
            .on_press(Message::CancelModelDownload),
        ]
        .spacing(tokens.spacing.sm),
    ]
    .spacing(tokens.spacing.sm);

    wizard_page(tokens, col)
}

// ── Page: download progress ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn page_downloading<'a>(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    locale: Locale,
    presentation: &'a ModelDownloadConsent,
    current_artifact: Option<ModelArtifact>,
    bytes: u64,
    total: u64,
    files_done: u32,
    files_total: u32,
) -> Element<'a, Message> {
    let overall_label = model_file_position(locale, files_done, files_total);

    // Progress fraction for the current file (0.0 – 1.0).
    let frac: f32 = if total > 0 {
        (bytes as f32 / total as f32).min(1.0)
    } else {
        0.0
    };
    let artifact_label = match current_artifact {
        Some(ModelArtifact::Tokenizer) => tr(locale, MessageKey::ModelArtifactTokenizer),
        Some(ModelArtifact::OnnxModel) => tr(locale, MessageKey::ModelArtifactOnnx),
        None => tr(locale, MessageKey::ModelDownloadingWhatNeeded),
    };
    let transfer_label = model_transfer_progress(locale, bytes, total);

    let col = column![
        row![
            icon_text(char::from(lucide::Download), 16.0),
            text(tr(locale, MessageKey::WizardDownloadProgress)).size(theme::title_s(tokens, sc)),
        ]
        .spacing(tokens.spacing.sm),
        text(format!(
            "{} · {}",
            presentation.model_name, presentation.license
        ))
        .size(theme::meta_s(tokens, sc)),
        text(overall_label).size(theme::meta_s(tokens, sc)),
        text(format!("↓  {artifact_label}")).size(theme::body_s(tokens, sc)),
        progress_bar(0.0..=1.0, frac),
        text(transfer_label).size(theme::meta_s(tokens, sc)),
    ]
    .spacing(tokens.spacing.md);

    wizard_page(tokens, col)
}

fn page_download_failed(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    locale: Locale,
    failure: ModelDeliveryFailure,
) -> Element<'static, Message> {
    let detail = match failure {
        ModelDeliveryFailure::StoreUnavailable => {
            tr(locale, MessageKey::ModelDeliveryStoreUnavailable)
        }
        ModelDeliveryFailure::Connection => tr(locale, MessageKey::ModelDeliveryConnection),
        ModelDeliveryFailure::Verification => tr(locale, MessageKey::ModelDeliveryVerification),
        ModelDeliveryFailure::LocalStorage => tr(locale, MessageKey::ModelDeliveryLocalStorage),
        ModelDeliveryFailure::InternalState => tr(locale, MessageKey::ModelDeliveryInternalState),
    };
    let col = column![
        text(tr(locale, MessageKey::ModelDownloadFailed)).size(theme::title_s(tokens, sc)),
        text(detail).size(theme::body_s(tokens, sc)),
        button(text(tr(locale, MessageKey::ModelDownloadRetry)).size(theme::body_s(tokens, sc)),)
            .on_press(Message::RetryModelDownload),
        button(text(tr(locale, MessageKey::WizardActionSkip)).size(theme::meta_s(tokens, sc)),)
            .on_press(Message::WizardSkip),
    ]
    .spacing(tokens.spacing.md);
    wizard_page(tokens, col)
}

// ── Page: file check results ─────────────────────────────────────────

fn page_checked<'a>(
    locale: Locale,
    state: &'a AppState,
    model_dir: &'a str,
    checks: &'a [WizardFileCheck],
    all_ok: bool,
) -> Element<'a, Message> {
    let tokens = &state.tokens;
    let sc = state.text_scale;
    let mut col = column![
        text(tr(locale, MessageKey::WizardTitleValidating)).size(theme::title_s(tokens, sc)),
        text(model_dir).size(theme::meta_s(tokens, sc)),
    ]
    .spacing(tokens.spacing.sm);

    for fc in checks {
        let (icon, style) = if fc.found {
            ("✓", String::new())
        } else {
            (
                "✗",
                format!("  ← {}", tr(locale, MessageKey::WizardMissingMarker)),
            )
        };
        let size_info = fc
            .size_mb
            .map(|m| format!("  ({m} MB)"))
            .unwrap_or_default();
        col = col.push(
            text(format!("{icon}  {}{size_info}{style}", fc.relative_path))
                .size(theme::meta_s(tokens, sc)),
        );
    }

    if all_ok {
        col = col.push(
            button(
                row![
                    icon_text(char::from(lucide::CheckCircle), 13.0),
                    text(tr(locale, MessageKey::WizardActionUseModel))
                        .size(theme::body_s(tokens, sc)),
                ]
                .spacing(tokens.spacing.xs),
            )
            .on_press(Message::WizardAccept),
        );
    } else {
        col = col.push(
            text(tr(locale, MessageKey::WizardBodyFileMissing)).size(theme::meta_s(tokens, sc)),
        );
        let path_input = text_input(
            tr(locale, MessageKey::WizardPathPlaceholder),
            &state.wizard_path_input,
        )
        .on_input(Message::WizardPathChanged)
        .on_submit(Message::WizardValidate)
        .padding(tokens.spacing.sm);
        col = col.push(
            row![
                container(path_input).width(Length::Fill),
                button(
                    row![
                        icon_text(char::from(lucide::ScanEye), 13.0),
                        text(tr(locale, MessageKey::WizardActionValidate))
                            .size(theme::body_s(tokens, sc)),
                    ]
                    .spacing(tokens.spacing.xs),
                )
                .on_press(Message::WizardValidate),
            ]
            .spacing(tokens.spacing.sm),
        );
    }

    col = col.push(
        row![
            button(
                text(format!("← {}", tr(locale, MessageKey::WizardBack)))
                    .size(theme::meta_s(tokens, sc)),
            )
            .on_press(Message::WizardBack),
            button(text(tr(locale, MessageKey::WizardActionSkip)).size(theme::meta_s(tokens, sc)))
                .on_press(Message::WizardSkip),
        ]
        .spacing(tokens.spacing.sm),
    );

    wizard_page(tokens, col)
}

// ── Page: ready ───────────────────────────────────────────────────────

fn page_ready<'a>(
    tokens: &Tokens,
    sc: crate::theme::TextScale,
    locale: Locale,
    model_dir: &'a str,
    provenance: ModelProvenance,
    persistence: ModelPersistenceState,
) -> Element<'a, Message> {
    let trust = match provenance {
        ModelProvenance::AppManaged => tr(locale, MessageKey::ModelTrustAppVerified),
        ModelProvenance::UserSupplied => tr(locale, MessageKey::ModelTrustUserSupplied),
    };
    let mut col = column![
        row![
            icon_text(char::from(lucide::CheckCircle), 18.0),
            text(tr(locale, MessageKey::WizardTitleReady)).size(theme::title_s(tokens, sc)),
        ]
        .spacing(tokens.spacing.sm),
        text(model_dir).size(theme::meta_s(tokens, sc)),
        text(trust).size(theme::meta_s(tokens, sc)),
        text(tr(locale, MessageKey::WizardReadyBody)).size(theme::body_s(tokens, sc)),
    ]
    .spacing(tokens.spacing.md);

    match persistence {
        ModelPersistenceState::Idle => {
            col = col.push(
                button(
                    row![
                        icon_text(char::from(lucide::CheckCircle), 13.0),
                        text(tr(locale, MessageKey::WizardActionUseModel))
                            .size(theme::body_s(tokens, sc)),
                    ]
                    .spacing(tokens.spacing.xs),
                )
                .on_press(Message::WizardAccept),
            );
        }
        ModelPersistenceState::InFlight(_) => {
            col = col.push(
                text(tr(locale, MessageKey::ModelPersistenceSaving))
                    .size(theme::body_s(tokens, sc)),
            );
        }
        ModelPersistenceState::Failed => {
            col = col
                .push(
                    text(tr(locale, MessageKey::ModelPersistenceFailed))
                        .size(theme::body_s(tokens, sc)),
                )
                .push(
                    button(
                        text(tr(locale, MessageKey::ModelPersistenceRetry))
                            .size(theme::body_s(tokens, sc)),
                    )
                    .on_press(Message::WizardAccept),
                );
        }
    }

    wizard_page(tokens, col)
}
