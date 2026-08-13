//! i18n catalog completeness, locale detection, and parameterized message tests.

use crate::i18n::{
    ALL_KEYS, Locale, files_ready_for_search, fmt_label_value, model_exact_size,
    model_file_position, model_transfer_progress, preparing_folder_for_search, source_summary, tr,
    wizard_file_size_mb,
};

// RFC-031 §9: every key resolves to a non-empty string in every locale.
#[test]
fn all_messages_non_empty_in_all_locales() {
    for locale in Locale::ALL {
        for key in ALL_KEYS {
            assert!(!tr(*locale, *key).is_empty(), "{locale:?} {key:?} is empty");
        }
    }
}

#[test]
fn exact_model_size_localizes_the_unit_without_rounding_the_byte_count() {
    assert_eq!(
        model_exact_size(Locale::En, 487_351_240),
        "487351240 bytes (487.4 MB)"
    );
    assert_eq!(
        model_exact_size(Locale::Ja, 487_351_240),
        "487351240 バイト (487.4 MB)"
    );
}

#[test]
fn model_progress_formatters_cover_zero_completed_and_locale_edges() {
    assert_eq!(model_file_position(Locale::En, 0, 0), "Preparing files");
    assert_eq!(model_file_position(Locale::Ja, 0, 0), "ファイルを準備中");
    assert_eq!(model_file_position(Locale::En, 0, 2), "File 1 of 2");
    assert_eq!(model_file_position(Locale::En, 2, 2), "File 2 of 2");
    assert_eq!(model_file_position(Locale::Ja, u32::MAX, 2), "ファイル 2/2");

    assert_eq!(model_transfer_progress(Locale::En, 0, 0), "0 B");
    assert_eq!(model_transfer_progress(Locale::Ja, 12, 0), "12 バイト");
    assert_eq!(
        model_transfer_progress(Locale::En, 500, 1_000),
        "500 B / 1 KB (50%)"
    );
    assert_eq!(
        model_transfer_progress(Locale::Ja, 2_000, 1_000),
        "2 KB / 1 KB (100%)"
    );
}

// RFC-031 §9: locales actually differ (a copy-pasted catalog is a bug).
#[test]
fn locales_differ_for_translatable_keys() {
    let differing = ALL_KEYS
        .iter()
        .filter(|k| tr(Locale::En, **k) != tr(Locale::Ja, **k))
        .count();
    assert!(
        differing > 10,
        "expected >10 keys to differ between locales, got {differing}; \
         the Japanese catalog may be a copy-paste of English"
    );
}

// RFC-031 §5.3: parameterized messages format correctly.
#[test]
fn parameterized_messages_localize() {
    // RFC-036 §14.1 (RFC-056 Slice 4): the folder name is interpolated
    // into the string, not left as literal placeholder text (the defect
    // `SearchPreparingFolder`/`SearchPartialReadiness` had before removal).
    assert_eq!(
        preparing_folder_for_search(Locale::En, "Documents"),
        "Preparing \"Documents\" for search"
    );
    assert!(preparing_folder_for_search(Locale::Ja, "Documents").contains("Documents"));
    assert_eq!(
        files_ready_for_search(Locale::En, 124),
        "124 files ready. You can search now."
    );
    assert!(files_ready_for_search(Locale::Ja, 124).contains("124"));

    // source_summary
    let s = source_summary(Locale::En, 10, 2, 1);
    assert!(
        s.contains("10") || s.contains("2") || s.contains("1"),
        "source_summary should include counts: {s}"
    );
}

// RFC-052 §4 rule 3: the shared label/value formatter replaces ten ad-hoc
// `format!("{}: {value}")` call sites; both locales share its shape today.
#[test]
fn fmt_label_value_joins_label_and_value_in_both_locales() {
    assert_eq!(
        fmt_label_value(Locale::En, "Provider", "Hugging Face"),
        "Provider: Hugging Face"
    );
    assert_eq!(
        fmt_label_value(Locale::Ja, "プロバイダー", "Hugging Face"),
        "プロバイダー: Hugging Face"
    );
    // Accepts any Display value, not just &str (e.g. an already-localized String).
    assert_eq!(
        fmt_label_value(Locale::En, "Exact size", 42u64),
        "Exact size: 42"
    );
}

// RFC-052 §4 rule 3: replaces the ad-hoc `format!("  ({m} MB)")` in the
// wizard's file-check list; "MB" itself is not localized today (see
// model_exact_size/model_bytes, which share the same unlocalized unit), but
// this takes `locale` like fmt_label_value so a future convention change is
// a one-line fix (Review 135 §3).
#[test]
fn wizard_file_size_mb_rounds_to_one_decimal() {
    assert_eq!(wizard_file_size_mb(Locale::En, 2.5), "(2.5 MB)");
    assert_eq!(wizard_file_size_mb(Locale::Ja, 0.0), "(0.0 MB)");
    assert_eq!(wizard_file_size_mb(Locale::En, 487.351_24), "(487.4 MB)");
}

// RFC-031 §3: locale persistence round-trip.
#[test]
fn locale_setting_round_trip() {
    for locale in Locale::ALL {
        assert_eq!(Locale::parse(locale.as_str()), Some(*locale));
    }
}

// RFC-031 §3: OS locale detection — Japanese.
#[test]
fn locale_from_env_detects_japanese() {
    let detected = Locale::from_env_values(Some("ja_JP.UTF-8"), None);
    assert_eq!(detected, Some(Locale::Ja));
}

// RFC-031 §3: non-Japanese LANG falls through to English.
#[test]
fn locale_from_env_english_fallback() {
    let detected = Locale::from_env_values(Some("en_US.UTF-8"), None);
    assert_eq!(detected, Some(Locale::En));
}
