//! Safe FTS5 MATCH expression building (RFC-015 §13: user input is
//! data, not query syntax).
//!
//! Each whitespace-separated term becomes a double-quoted phrase with
//! embedded quotes doubled, joined with implicit AND. FTS5 operators in
//! user input (`OR`, `NEAR`, `-`, `^`, column filters) are thereby
//! neutralized into literal phrases.

/// Build an FTS5 MATCH expression from a raw user query. Returns `None`
/// when the query contains no searchable terms.
pub fn build_match_expression(raw: &str) -> Option<String> {
    let terms = cleaned_terms(raw);
    if terms.is_empty() {
        None
    } else {
        Some(
            terms
                .into_iter()
                .map(|term| quote(&term))
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

/// Build a safe FTS5 expression for long conceptual queries.
///
/// The first adjacent term pair becomes a quoted phrase:
/// `embedding model cosine similarity` becomes `"embedding model"`.
pub fn build_match_pair_expression(raw: &str) -> Option<String> {
    let terms = cleaned_terms(raw);
    if terms.len() < 4 {
        return build_match_expression(raw);
    }
    Some(quote(&terms[..2].join(" ")))
}

fn cleaned_terms(raw: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    for term in raw.split_whitespace() {
        let cleaned = term.replace('"', "\"\"");
        if cleaned.is_empty() {
            continue;
        }
        phrases.push(cleaned);
    }
    phrases
}

fn quote(term: &str) -> String {
    format!("\"{term}\"")
}
