//! i18n catalog completeness, locale detection, and parameterized message tests.

use crate::i18n::{
    Locale, files_indexed, model_exact_size, model_file_position, model_transfer_progress,
    source_summary, tr,
};
use crate::tests::ALL_KEYS;

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
    // files_indexed
    assert!(!files_indexed(Locale::En, 1).is_empty());
    assert!(!files_indexed(Locale::Ja, 100).is_empty());

    // source_summary
    let s = source_summary(Locale::En, 10, 2, 1);
    assert!(
        s.contains("10") || s.contains("2") || s.contains("1"),
        "source_summary should include counts: {s}"
    );
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
