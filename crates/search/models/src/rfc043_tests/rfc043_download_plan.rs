//! RFC-043 download plan unit tests (§22.2 test plan).

use super::TEST_MANIFEST;
use crate::download_plan::{
    DEFAULT_MODEL_DOWNLOAD_CONCURRENCY, DownloadAction, DownloadPlanError, FriendlyDownloadProblem,
    build_download_plan, build_download_plan_against,
};
use crate::readiness::{
    LocalFileIntegrity, LocalFileStatus, check_app_managed_model_readiness,
    check_app_managed_model_readiness_against, check_model_readiness,
};

#[test]
fn plan_skips_ready_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\x00").unwrap();
    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    let plan = build_download_plan_against(&report, &TEST_MANIFEST).unwrap();
    assert!(!plan.has_work());
    assert!(plan.files_to_download().is_empty());
    assert_eq!(plan.files_to_skip().len(), 2);
}

#[test]
fn plan_downloads_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_app_managed_model_readiness(dir.path());
    let plan = build_download_plan(&report).unwrap();
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
    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    let plan = build_download_plan_against(&report, &TEST_MANIFEST).unwrap();
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
    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    let plan = build_download_plan_against(&report, &TEST_MANIFEST).unwrap();
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
    let report = check_app_managed_model_readiness(dir.path());
    let plan = build_download_plan(&report).unwrap();
    for f in &plan.files {
        let temp = f.temp_path(dir.path());
        assert!(
            temp.to_string_lossy().ends_with(".part"),
            "temp path must end with .part"
        );
    }
}

#[test]
fn plan_uses_only_pinned_manifest_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_app_managed_model_readiness(dir.path());
    let plan = build_download_plan(&report).unwrap();
    assert_eq!(plan.manifest_id, "multilingual-e5-small-hf-614241f6");
    for file in &plan.files {
        assert!(!file.remote_url.contains("/resolve/main/"));
        assert!(
            file.remote_url
                .contains("614241f622f53c4eeff9890bdc4f31cfecc418b3")
        );
        assert_eq!(file.expected_sha256.len(), 64);
        assert!(file.exact_size_bytes <= file.max_transfer_bytes);
    }
}

#[test]
fn trusted_plan_refuses_user_supplied_folder() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_model_readiness(dir.path());
    assert!(matches!(
        build_download_plan(&report),
        Err(DownloadPlanError::UserSuppliedFolder)
    ));
}

#[test]
fn trusted_plan_rejects_cross_manifest_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    assert!(matches!(
        build_download_plan(&report),
        Err(DownloadPlanError::TrustRootMismatch)
    ));
}

#[test]
fn trusted_plan_rejects_forged_ready_integrity() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\0").unwrap();

    for integrity in [LocalFileIntegrity::Unverified, LocalFileIntegrity::Mismatch] {
        let mut report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
        report.set_file_state_for_test("tokenizer.json", LocalFileStatus::Ready, integrity);
        assert!(matches!(
            build_download_plan_against(&report, &TEST_MANIFEST),
            Err(DownloadPlanError::IncoherentReadiness)
        ));
    }
}

#[test]
fn trusted_plan_fails_closed_when_files_cannot_be_checked() {
    let dir = tempfile::tempdir().unwrap();
    let mut report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    report.set_file_state_for_test(
        "tokenizer.json",
        LocalFileStatus::CannotCheck,
        LocalFileIntegrity::NotAvailable,
    );
    assert!(matches!(
        build_download_plan_against(&report, &TEST_MANIFEST),
        Err(DownloadPlanError::CannotCheckFiles)
    ));
}

#[test]
fn trusted_plan_rejects_stale_or_incomplete_file_sets() {
    let dir = tempfile::tempdir().unwrap();
    let mut report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    report.remove_file_for_test("onnx/model.onnx");
    assert!(matches!(
        build_download_plan_against(&report, &TEST_MANIFEST),
        Err(DownloadPlanError::ReadinessManifestMismatch)
    ));
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
