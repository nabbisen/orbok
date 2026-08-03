//! Safe diagnostics and redacted support bundle (RFC-040).
//!
//! Manual export flow — never uploaded automatically. Default bundle is
//! redacted: no document contents, no search text, no raw paths. Each
//! optional inclusion requires an explicit opt-in.

// Public API for RFC-040 §11.2's pre-export bundle preview. Not currently
// wired into any UI view or `orbok update()` handler -- the previous
// comment here claimed wiring that does not exist (Phase 1 inventory,
// RFC-052 review request 132 §2c).
#![allow(dead_code)]

use orbok_core::DiagnosticsPolicy;
use orbok_ui::i18n::Locale;
use std::collections::HashMap;

// ── Manifest ──────────────────────────────────────────────────────────

/// Bundle manifest records exactly what was included (RFC-040 §10).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsManifest {
    pub app: &'static str,
    pub bundle_version: u32,
    pub created_at: String,
    pub privacy_mode: String,
    pub redacted: bool,
    pub includes_document_contents: bool,
    pub includes_search_text: bool,
    pub includes_raw_paths: bool,
    pub includes_folder_names: bool,
    pub includes_recent_searches: bool,
}

impl DiagnosticsManifest {
    pub fn from_policy(policy: &DiagnosticsPolicy) -> Self {
        Self {
            app: "orbok",
            bundle_version: 1,
            created_at: orbok_core::now_iso8601(),
            privacy_mode: policy.privacy_mode.as_str().to_string(),
            redacted: true,
            includes_document_contents: false,
            includes_search_text: false,
            includes_raw_paths: policy.include_raw_paths,
            includes_folder_names: policy.include_folder_names,
            includes_recent_searches: policy.include_recent_searches,
        }
    }
}

// ── Section kind ──────────────────────────────────────────────────────

/// Diagnostics section labels (RFC-040 §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticsSectionKind {
    App,
    Platform,
    Settings,
    Storage,
    Sources,
    Indexing,
    Extraction,
    Models,
    Scheduler,
    Errors,
    Logs,
}

impl DiagnosticsSectionKind {
    pub fn filename(&self) -> &'static str {
        match self {
            DiagnosticsSectionKind::App => "app.json",
            DiagnosticsSectionKind::Platform => "platform.json",
            DiagnosticsSectionKind::Settings => "settings-redacted.json",
            DiagnosticsSectionKind::Storage => "storage-summary.json",
            DiagnosticsSectionKind::Sources => "sources-summary.json",
            DiagnosticsSectionKind::Indexing => "indexing-summary.json",
            DiagnosticsSectionKind::Extraction => "extraction-summary.json",
            DiagnosticsSectionKind::Models => "models-summary.json",
            DiagnosticsSectionKind::Scheduler => "scheduler-summary.json",
            DiagnosticsSectionKind::Errors => "recent-errors.json",
            DiagnosticsSectionKind::Logs => "logs-redacted.txt",
        }
    }
}

// ── Redaction engine ──────────────────────────────────────────────────

/// Redact sensitive patterns from a log or text string (RFC-040 §8, §17).
///
/// Rules (all default-on):
/// - absolute paths → `<folder:N>/filename`
/// - search text markers → `<redacted search text>`
/// - URL query tokens → `<redacted query>`
/// - home-directory prefix → `<home>/...`
pub fn redact_text(input: &str, policy: &DiagnosticsPolicy) -> String {
    if policy.include_raw_paths {
        return input.to_string();
    }

    let mut out = input.to_string();

    // Redact home directory paths (Unix and Windows).
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_str = home.to_string_lossy();
        out = out.replace(home_str.as_ref(), "<home>");
    }

    // Redact absolute paths: /some/path/file.ext → <folder>/file.ext
    out = redact_absolute_paths(&out);

    // Redact URL query strings (e.g. ?token=...).
    out = redact_url_queries(&out);

    out
}

fn redact_absolute_paths(s: &str) -> String {
    // Simple heuristic: replace runs of /word/word/.../file with <folder>/file
    // This keeps the filename for debuggability while hiding directory structure.
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/'
            && chars
                .peek()
                .map(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                .unwrap_or(false)
        {
            // Consume the path until whitespace or end.
            let mut path = String::from("/");
            for pc in chars.by_ref() {
                if pc.is_whitespace() {
                    // We're done with the path; emit redacted form then the space.
                    let filename = path.rsplit('/').next().unwrap_or("").to_string();
                    result.push_str(&format!("<folder>/{filename}"));
                    result.push(pc);
                    break;
                }
                path.push(pc);
            }
            // If we consumed to end without whitespace:
            if path.len() > 1 && !result.ends_with('>') {
                let filename = path.rsplit('/').next().unwrap_or("").to_string();
                result.push_str(&format!("<folder>/{filename}"));
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn redact_url_queries(s: &str) -> String {
    // Replace ?key=value&... with ?<redacted query>
    let mut result = s.to_string();
    while let Some(q_pos) = result.find('?') {
        let after = &result[q_pos + 1..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .map(|i| q_pos + 1 + i)
            .unwrap_or(result.len());
        if end > q_pos + 1 {
            result.replace_range(q_pos..end, "?<redacted query>");
        } else {
            break;
        }
    }
    result
}

// ── App info collector ────────────────────────────────────────────────

/// Collect safe app-level diagnostics (RFC-040 §6.1).
pub fn collect_app_info() -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    m.insert("app", "orbok".to_string());
    m.insert("version", env!("CARGO_PKG_VERSION").to_string());
    m.insert(
        "build_profile",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .to_string(),
    );
    m
}

/// Collect safe platform diagnostics (RFC-040 §6.1).
pub fn collect_platform_info() -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    m.insert("os", std::env::consts::OS.to_string());
    m.insert("arch", std::env::consts::ARCH.to_string());
    m.insert("family", std::env::consts::FAMILY.to_string());
    m
}

// ── Bundle summary ────────────────────────────────────────────────────

/// Human-readable summary of what the bundle includes and excludes
/// (RFC-040 §11.2 preview).
pub fn bundle_preview_text(locale: Locale, policy: &DiagnosticsPolicy) -> String {
    let labels = orbok_ui::i18n::diagnostics_bundle_labels(locale);
    let included = [
        labels.app_version,
        labels.platform_summary,
        labels.folder_status_counts,
        labels.search_preparation_status,
        labels.model_readiness,
        labels.redacted_logs,
    ];
    let excluded = [
        labels.documents,
        labels.search_words,
        labels.raw_folder_paths,
    ];
    let mut lines = vec![labels.included_heading.to_string()];
    for item in &included {
        lines.push(format!("  ✓ {item}"));
    }
    if policy.include_folder_names {
        lines.push(format!("  ✓ {}", labels.folder_names_opted_in));
    }
    lines.push(String::new());
    lines.push(labels.not_included_heading.to_string());
    for item in &excluded {
        lines.push(format!("  × {item}"));
    }
    lines.join("\n")
}
