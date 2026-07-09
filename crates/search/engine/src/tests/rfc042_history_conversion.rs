//! RFC-042 — `From<&ActiveFilter> for StoredSearchFilter` conversion.
//!
//! The history layer stores a self-contained mirror of the live filter
//! model. These tests pin the variant-to-variant mapping and the label
//! pass-through so a stored entry can be displayed without rehydration.

use crate::filter::{
    ActiveFilter, ChangedFilter, KindFilter, LanguageFilter, ReadyFilter, SearchStyle,
};
use orbok_core::{
    StoredChangedFilter, StoredKindFilter, StoredLanguageFilter, StoredReadyFilter,
    StoredSearchFilter, StoredSearchStyle,
};

#[test]
fn folder_filter_maps_id_and_label() {
    let active = ActiveFilter::Folder {
        id: "src_42".into(),
        label: "Documents".into(),
    };
    let stored = StoredSearchFilter::from(&active);
    match stored {
        StoredSearchFilter::Folder { id, label } => {
            assert_eq!(id, "src_42");
            assert_eq!(label, "Documents");
        }
        other => panic!("expected Folder, got {other:?}"),
    }
}

#[test]
fn kind_filter_maps_value_and_label() {
    let active = ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    };
    match StoredSearchFilter::from(&active) {
        StoredSearchFilter::Kind { value, label } => {
            assert_eq!(value, StoredKindFilter::Pdfs);
            assert_eq!(label, "PDFs");
        }
        other => panic!("expected Kind, got {other:?}"),
    }
}

#[test]
fn changed_ready_style_language_map_values() {
    assert!(matches!(
        StoredSearchFilter::from(&ActiveFilter::Changed {
            value: ChangedFilter::ThisWeek,
            label: "This week".into()
        }),
        StoredSearchFilter::Changed {
            value: StoredChangedFilter::ThisWeek,
            ..
        }
    ));
    assert!(matches!(
        StoredSearchFilter::from(&ActiveFilter::ReadyStatus {
            value: ReadyFilter::NeedsUpdate,
            label: "Needs update".into()
        }),
        StoredSearchFilter::ReadyStatus {
            value: StoredReadyFilter::NeedsUpdate,
            ..
        }
    ));
    assert!(matches!(
        StoredSearchFilter::from(&ActiveFilter::SearchStyle {
            value: SearchStyle::Meaning,
            label: "Meaning".into()
        }),
        StoredSearchFilter::SearchStyle {
            value: StoredSearchStyle::Meaning,
            ..
        }
    ));
    assert!(matches!(
        StoredSearchFilter::from(&ActiveFilter::Language {
            value: LanguageFilter::Japanese,
            label: "Japanese".into()
        }),
        StoredSearchFilter::Language {
            value: StoredLanguageFilter::Japanese,
            ..
        }
    ));
}
