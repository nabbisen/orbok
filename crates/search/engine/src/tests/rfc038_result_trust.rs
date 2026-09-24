//! RFC-038 result trust unit tests (§15.1 test plan).

use crate::result_trust::{ResultRecoveryAction, ResultTrustState, SearchResultTrust};
use orbok_extract::ExtractWarning;

// ── Trust state computation ───────────────────────────────────────────

#[test]
fn indexed_file_with_no_warnings_is_ready() {
    let trust = SearchResultTrust::from_catalog("indexed", &[]);
    assert_eq!(trust.state, ResultTrustState::Ready);
    assert!(trust.warnings.is_empty());
    assert!(trust.recovery_actions.is_empty());
}

#[test]
fn stale_file_is_needs_update() {
    let trust = SearchResultTrust::from_catalog("stale", &[]);
    assert_eq!(trust.state, ResultTrustState::NeedsUpdate);
    assert!(
        trust
            .recovery_actions
            .contains(&ResultRecoveryAction::PrepareAgain)
    );
    assert!(
        trust
            .recovery_actions
            .contains(&ResultRecoveryAction::OpenAnyway)
    );
}

#[test]
fn missing_file_is_file_not_found() {
    let trust = SearchResultTrust::from_catalog("missing", &[]);
    assert_eq!(trust.state, ResultTrustState::FileNotFound);
    assert!(
        trust
            .recovery_actions
            .contains(&ResultRecoveryAction::CheckFolder)
    );
}

#[test]
fn deleted_file_is_file_not_found() {
    let trust = SearchResultTrust::from_catalog("deleted", &[]);
    assert_eq!(trust.state, ResultTrustState::FileNotFound);
}

#[test]
fn discovered_file_is_still_being_prepared() {
    let trust = SearchResultTrust::from_catalog("discovered", &[]);
    assert_eq!(trust.state, ResultTrustState::StillBeingPrepared);
}

#[test]
fn permission_denied_file_is_cannot_open() {
    let trust = SearchResultTrust::from_catalog("permission_denied", &[]);
    assert_eq!(trust.state, ResultTrustState::CannotOpen);
    assert!(
        trust
            .recovery_actions
            .contains(&ResultRecoveryAction::ShowInFolder)
    );
}

// ── Extraction warning → trust state ─────────────────────────────────

#[test]
fn scanned_pdf_warning_makes_partly_prepared() {
    let trust = SearchResultTrust::from_catalog("indexed", &[ExtractWarning::PossiblyScannedPdf]);
    assert_eq!(trust.state, ResultTrustState::PartlyPrepared);
}

#[test]
fn unreadable_pages_makes_partly_prepared() {
    let trust = SearchResultTrust::from_catalog(
        "indexed",
        &[ExtractWarning::SomePagesUnreadable { pages: vec![1, 2] }],
    );
    assert_eq!(trust.state, ResultTrustState::PartlyPrepared);
}

#[test]
fn size_limit_warning_makes_partly_prepared() {
    let trust = SearchResultTrust::from_catalog(
        "indexed",
        &[ExtractWarning::SizeLimitReached {
            limit_name: "max_extracted_chars".into(),
        }],
    );
    assert_eq!(trust.state, ResultTrustState::PartlyPrepared);
}

// ── Badge display rules ───────────────────────────────────────────────

#[test]
fn ready_result_has_no_badge_by_default() {
    // RFC-038 §6.1: clean results must not be cluttered.
    assert!(!ResultTrustState::Ready.show_badge_by_default());
}

#[test]
fn non_ready_states_show_badge() {
    for state in [
        ResultTrustState::NeedsUpdate,
        ResultTrustState::FileNotFound,
        ResultTrustState::StillBeingPrepared,
        ResultTrustState::PartlyPrepared,
        ResultTrustState::CannotOpen,
    ] {
        assert!(state.show_badge_by_default(), "{state:?} must show badge");
    }
}

// ── Recovery actions ──────────────────────────────────────────────────

#[test]
fn every_non_ready_result_has_a_recovery_action() {
    // RFC-038 §16: every non-ready result should have a safe next step.
    let statuses = ["stale", "missing", "deleted", "permission_denied"];
    for status in statuses {
        let trust = SearchResultTrust::from_catalog(status, &[]);
        assert!(
            !trust.recovery_actions.is_empty(),
            "status '{status}' must have at least one recovery action"
        );
    }
}

#[test]
fn ready_trust_has_no_recovery_actions() {
    let trust = SearchResultTrust::ready();
    assert!(trust.recovery_actions.is_empty());
    assert!(trust.warnings.is_empty());
}

// ── Copy compliance ───────────────────────────────────────────────────

#[test]
fn trust_state_does_not_expose_catalog_terms() {
    // Verify via the i18n layer that no badge copy contains catalog terms.
    // This tests the contract, not the strings directly; the actual copy
    // is validated in orbok-ui i18n tests.
    let technical = ["stale", "index", "catalog", "cache", "mtime"];
    // The state itself is an enum — no strings at this layer.
    // This test documents the RFC-038 §4 intent as a structural check.
    for term in &technical {
        assert!(
            !format!("{:?}", ResultTrustState::NeedsUpdate)
                .to_lowercase()
                .contains(term),
            "trust state must not contain technical term '{term}'"
        );
    }
}
