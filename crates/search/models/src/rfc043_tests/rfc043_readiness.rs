//! RFC-043 model readiness unit tests (§22.1 test plan).

use super::TEST_MANIFEST;
use crate::readiness::{
    LocalFileIntegrity, LocalFileStatus, ModelProvenance, ModelReadiness,
    check_app_managed_model_readiness_against, check_model_readiness,
};

#[test]
fn missing_model_dir_reports_needs_download() {
    let dir = tempfile::tempdir().unwrap();
    // Empty dir — no files at all.
    let report = check_model_readiness(dir.path());
    assert_eq!(report.overall(), ModelReadiness::NeedsDownload);
    assert!(
        report
            .files()
            .iter()
            .all(|f| f.status() == LocalFileStatus::Missing)
    );
}

#[test]
fn complete_valid_files_report_ready() {
    let dir = tempfile::tempdir().unwrap();
    // Create all required files as non-empty.
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\x00\x01\x02\x03").unwrap();
    let report = check_model_readiness(dir.path());
    assert_eq!(report.overall(), ModelReadiness::Ready);
    assert_eq!(report.provenance(), ModelProvenance::UserSupplied);
    assert!(
        report
            .files()
            .iter()
            .all(|f| f.status() == LocalFileStatus::Ready)
    );
    assert!(
        report
            .files()
            .iter()
            .all(|f| f.integrity() == LocalFileIntegrity::Unverified)
    );
}

#[test]
fn app_managed_files_require_exact_trusted_bytes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\0").unwrap();

    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    assert_eq!(report.provenance(), ModelProvenance::AppManaged);
    assert_eq!(report.overall(), ModelReadiness::Ready);
    assert!(
        report
            .files()
            .iter()
            .all(|file| file.integrity() == LocalFileIntegrity::TrustedDigest)
    );

    std::fs::write(dir.path().join("tokenizer.json"), b"[]").unwrap();
    let report = check_app_managed_model_readiness_against(dir.path(), &TEST_MANIFEST);
    let tokenizer = report
        .files()
        .iter()
        .find(|file| file.logical_name() == "tokenizer")
        .unwrap();
    assert_eq!(tokenizer.status(), LocalFileStatus::Invalid);
    assert_eq!(tokenizer.integrity(), LocalFileIntegrity::Mismatch);
    assert_eq!(report.overall(), ModelReadiness::NeedsRepair);
}

#[test]
fn partial_file_reports_partial_status() {
    let dir = tempfile::tempdir().unwrap();
    // Create a .part file but not the final file.
    std::fs::write(dir.path().join("tokenizer.json.part"), b"partial").unwrap();
    let report = check_model_readiness(dir.path());
    let tokenizer = report
        .files()
        .iter()
        .find(|f| f.logical_name() == "tokenizer")
        .unwrap();
    assert_eq!(tokenizer.status(), LocalFileStatus::Partial);
}

#[test]
fn empty_file_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    // Create an empty file.
    std::fs::write(dir.path().join("tokenizer.json"), b"").unwrap();
    std::fs::create_dir_all(dir.path().join("onnx")).unwrap();
    std::fs::write(dir.path().join("onnx/model.onnx"), b"\x00").unwrap();
    let report = check_model_readiness(dir.path());
    let tokenizer = report
        .files()
        .iter()
        .find(|f| f.logical_name() == "tokenizer")
        .unwrap();
    assert_eq!(tokenizer.status(), LocalFileStatus::Invalid);
    assert_eq!(report.overall(), ModelReadiness::NeedsRepair);
}

#[test]
fn ready_count_reflects_valid_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
    // Only tokenizer present; model missing.
    let report = check_model_readiness(dir.path());
    assert_eq!(report.ready_count(), 1);
    assert_eq!(report.total_count(), 2);
}

#[test]
fn all_labels_avoid_technical_terms() {
    let forbidden = ["index", "cache", "mtime", "blob", "vector", "sqlite"];
    for status in [
        LocalFileStatus::Ready,
        LocalFileStatus::Missing,
        LocalFileStatus::Partial,
        LocalFileStatus::Invalid,
        LocalFileStatus::CannotCheck,
    ] {
        let label = status.user_label().to_lowercase();
        for term in &forbidden {
            assert!(
                !label.contains(term),
                "label '{}' contains forbidden term '{term}'",
                status.user_label()
            );
        }
    }
}

#[test]
fn needs_work_is_correct() {
    assert!(!LocalFileStatus::Ready.needs_work());
    assert!(LocalFileStatus::Missing.needs_work());
    assert!(LocalFileStatus::Partial.needs_work());
    assert!(LocalFileStatus::Invalid.needs_work());
    assert!(LocalFileStatus::CannotCheck.needs_work());
}
