//! Fixture: one planted violation per pattern, plus one allowlisted literal
//! that must NOT be flagged (proves the allowlist suppresses correctly,
//! not just that the patterns fire).

fn violation_view(locale: Locale, tokens: &Tokens) -> Element<'_, Message> {
    // Pattern 1a: raw literal in text().
    let a = text("Searching…").size(theme::body_s(tokens));
    // Pattern 1b: raw literal in text_input().
    let b = text_input("Or type a path manually…", &state.source_path_input);
    // Pattern 1c: raw literal in .set_title().
    let c = rfd::FileDialog::new().set_title("Select folder to search");
    // Pattern 1d: raw literal in job_progress().
    let d = job_progress(tokens, "Indexing…", None);
    // Pattern 2: raw literal as unwrap_or fallback.
    let e = result.snippet.as_deref().unwrap_or("(source unavailable)");
    // Pattern 3: multi-word literal in a struct field assignment.
    let f = ResultsStatus::Problem {
        friendly_message: "Search did not finish. Please try again.".into(),
    };
    // Pattern 4: ad-hoc "{}: {value}" label/value concatenation.
    let g = format!(
        "{}: {}",
        tr(locale, MessageKey::ModelConsentProvider),
        presentation.provider
    );
    // Allowlisted multi-word literal (matches Pattern 3 but is a
    // documented exemption) — must NOT be flagged.
    let h = model_store
        .contains(path)
        .expect("reviewed default-model file sizes must fit in u64");
    a
}
