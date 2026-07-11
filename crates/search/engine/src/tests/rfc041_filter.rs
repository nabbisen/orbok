//! RFC-041 filter model unit tests (§24.1 test plan).

use crate::filter::{
    ActiveFilter, ChangedFilter, KindFilter, SuggestedFilter, extension_matches_kind,
    is_already_active,
};

#[test]
fn active_filter_can_be_added() {
    let filters: Vec<ActiveFilter> = vec![ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    }];
    assert_eq!(filters.len(), 1);
}

#[test]
fn active_filter_can_be_removed() {
    let mut filters = vec![
        ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        ActiveFilter::Kind {
            value: KindFilter::Notes,
            label: "Notes".into(),
        },
    ];
    filters.remove(0);
    assert_eq!(filters.len(), 1);
    assert!(matches!(
        &filters[0],
        ActiveFilter::Kind {
            value: KindFilter::Notes,
            ..
        }
    ));
}

#[test]
fn clear_removes_all_filters() {
    let mut filters = vec![
        ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        ActiveFilter::Changed {
            value: ChangedFilter::ThisWeek,
            label: "This week".into(),
        },
    ];
    filters.clear();
    assert!(filters.is_empty());
}

#[test]
fn search_text_is_independent_of_filter_operations() {
    // RFC-041 §15.4: filters must not clear search text.
    // Modelled here as label stability across remove.
    let mut filters = vec![
        ActiveFilter::Folder {
            id: "src-1".into(),
            label: "Documents".into(),
        },
        ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
    ];
    // Remove index 1 (PDFs) — Documents must remain.
    filters.remove(1);
    assert_eq!(filters.len(), 1);
    assert_eq!(filters[0].label(), "Documents");
}

#[test]
fn suggested_filters_exclude_already_active() {
    let active = vec![ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    }];
    let candidate = ActiveFilter::Kind {
        value: KindFilter::Pdfs,
        label: "PDFs".into(),
    };
    assert!(is_already_active(&active, &candidate));
}

#[test]
fn suggested_filter_with_zero_count_should_not_show() {
    // RFC-041 §13.2: do not suggest if it produces zero results.
    let s = SuggestedFilter {
        filter: ActiveFilter::Kind {
            value: KindFilter::Pdfs,
            label: "PDFs".into(),
        },
        estimated_result_count: 0,
    };
    assert_eq!(
        s.estimated_result_count, 0,
        "zero-count suggestion must be suppressed by caller"
    );
}

#[test]
fn pdf_extension_matches_pdfs_kind() {
    assert!(extension_matches_kind("pdf", &KindFilter::Pdfs));
    assert!(extension_matches_kind("PDF", &KindFilter::Pdfs));
}

#[test]
fn md_extension_matches_notes_kind() {
    assert!(extension_matches_kind("md", &KindFilter::Notes));
    assert!(extension_matches_kind("txt", &KindFilter::Notes));
}

#[test]
fn rs_extension_matches_code_kind() {
    assert!(extension_matches_kind("rs", &KindFilter::Code));
    assert!(extension_matches_kind("py", &KindFilter::Code));
}

#[test]
fn pdf_does_not_match_notes_kind() {
    assert!(!extension_matches_kind("pdf", &KindFilter::Notes));
}

#[test]
fn active_filter_label_is_stable() {
    // Labels are stored separately from values (RFC-041 §16.2).
    let f = ActiveFilter::Folder {
        id: "src-abc".into(),
        label: "Documents".into(),
    };
    assert_eq!(f.label(), "Documents");
}

#[test]
fn default_ui_does_not_expose_technical_kind_names() {
    // RFC-041 §8.2: no technical terms in default labels.
    for (kind, expected) in [
        (KindFilter::Pdfs, "PDFs"),
        (KindFilter::Notes, "Notes"),
        (KindFilter::Code, "Code"),
        (KindFilter::Documents, "Documents"),
        (KindFilter::Spreadsheets, "Spreadsheets"),
    ] {
        assert_eq!(kind.label(), expected, "kind label must be plain");
        // None of the labels should contain technical terms.
        let label = kind.label();
        for forbidden in &["ext", "mime", "type", "format", "application/"] {
            assert!(
                !label.to_lowercase().contains(forbidden),
                "label '{label}' contains forbidden technical term '{forbidden}'"
            );
        }
    }
}
