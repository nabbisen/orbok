//! RFC-043 download plan unit tests (§22.2 test plan).

use crate::download_plan::{
    DEFAULT_MODEL_DOWNLOAD_CONCURRENCY, DownloadAction, FriendlyDownloadProblem,
    build_download_plan,
};
use crate::readiness::check_model_readiness;

#[test]
fn plan_skips_ready_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\x00").unwrap();
    let report = check_model_readiness(dir.path());
    let plan = build_download_plan(&report);
    assert!(!plan.has_work());
    assert!(plan.files_to_download().is_empty());
    assert_eq!(plan.files_to_skip().len(), 2);
}

#[test]
fn plan_downloads_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_model_readiness(dir.path());
    let plan = build_download_plan(&report);
    assert!(plan.has_work());
    assert_eq!(plan.files_to_download().len(), 2);
    for f in plan.files_to_download() {
        assert_eq!(f.action, DownloadAction::Download);
    }
}

#[test]
fn plan_retries_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json.part"), b"partial").unwrap();
    let report = check_model_readiness(dir.path());
    let plan = build_download_plan(&report);
    let tokenizer = plan
        .files
        .iter()
        .find(|f| f.logical_name == "tokenizer")
        .unwrap();
    assert_eq!(tokenizer.action, DownloadAction::Retry);
}

#[test]
fn plan_replaces_invalid_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"").unwrap(); // empty = invalid
    let report = check_model_readiness(dir.path());
    let plan = build_download_plan(&report);
    let tokenizer = plan
        .files
        .iter()
        .find(|f| f.logical_name == "tokenizer")
        .unwrap();
    assert_eq!(tokenizer.action, DownloadAction::Replace);
}

#[test]
fn concurrency_does_not_exceed_maximum() {
    // RFC-043 §11.1: bounded concurrency.
    const { assert!(DEFAULT_MODEL_DOWNLOAD_CONCURRENCY <= 2) };
}

#[test]
fn temp_path_has_part_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_model_readiness(dir.path());
    let plan = build_download_plan(&report);
    for f in &plan.files {
        let temp = f.temp_path(dir.path());
        assert!(
            temp.to_string_lossy().ends_with(".part"),
            "temp path must end with .part"
        );
    }
}

#[test]
fn friendly_problem_messages_avoid_technical_terms() {
    let forbidden = [
        "http", "tcp", "socket", "dns", "tls", "url", "errno", "enospc",
    ];
    for prob in [
        FriendlyDownloadProblem::NetworkUnavailable,
        FriendlyDownloadProblem::ServerBusy,
        FriendlyDownloadProblem::NotEnoughSpace,
        FriendlyDownloadProblem::CannotWriteFiles,
        FriendlyDownloadProblem::CannotCheckFiles,
        FriendlyDownloadProblem::ValidationFailed,
        FriendlyDownloadProblem::Unexpected,
    ] {
        let msg = prob.user_message().to_lowercase();
        for term in &forbidden {
            assert!(
                !msg.contains(term),
                "message '{msg}' contains technical term '{term}'"
            );
        }
        // Must end with period (RFC-043 §20: all copy ends with punctuation).
        assert!(
            msg.trim_end().ends_with('.'),
            "message '{msg}' must end with a period"
        );
    }
}
