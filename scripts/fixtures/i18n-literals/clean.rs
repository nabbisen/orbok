//! Fixture: fully catalog-driven, must produce zero findings.

fn clean_view(locale: Locale, tokens: &Tokens) -> Element<'_, Message> {
    let label = text(tr(locale, MessageKey::SearchPlaceholder)).size(theme::body_s(tokens));
    let input = text_input(tr(locale, MessageKey::SearchPlaceholder), &state.query);
    let fallback = result.snippet.as_deref().unwrap_or_default();
    let dialog = rfd::FileDialog::new().set_title(tr(locale, MessageKey::DialogChooseFolder));
    let status = job_progress(tokens, tr(locale, MessageKey::IndexingIdle), None);
    let summary = source_summary(locale, indexed, stale, failed);
    label
}
