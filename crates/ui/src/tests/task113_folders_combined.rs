//! Task 113: the two notices' copy (RFC-064 §4, owner-approved), and what the
//! window does when folders become part of another.

use crate::i18n::Locale;
use crate::notice::UserNotice;
use crate::state::{
    AppState, CombinedFolder, FoldersCombined, Message, SearchLocation, SearchLocationSummary,
    SourceCard,
};
use orbok_core::{SourceId, SourceStatus};

fn card(id: &str, name: &str) -> SourceCard {
    SourceCard {
        display_name: name.into(),
        display_path: format!("/docs/{name}"),
        indexed: 0,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: SourceStatus::Active,
        source_id: id.into(),
    }
}

fn combined(parent: &str, folders: &[(&str, &str)]) -> Message {
    Message::FoldersCombined(FoldersCombined {
        parent_id: parent.into(),
        parent_name: "Docs".into(),
        folders: folders
            .iter()
            .map(|(id, name)| CombinedFolder {
                source_id: (*id).into(),
                display_name: (*name).into(),
                canonical_path: format!("/docs/{name}"),
            })
            .collect(),
    })
}

#[test]
fn the_notices_read_as_approved_in_both_locales() {
    let already = UserNotice::FolderAlreadyIncluded {
        folder: "Notes".into(),
        parent: "Docs".into(),
    };
    let one = UserNotice::FoldersCombined {
        folders: vec!["Notes".into()],
        parent: "Docs".into(),
    };
    let two = UserNotice::FoldersCombined {
        folders: vec!["Notes".into(), "Drafts".into()],
        parent: "Docs".into(),
    };
    let three = UserNotice::FoldersCombined {
        folders: vec!["A".into(), "B".into(), "C".into()],
        parent: "Docs".into(),
    };
    assert_eq!(already.title(Locale::En), "Folder already included");
    assert_eq!(already.body(Locale::En), "Notes is already part of Docs.");
    assert_eq!(
        already.title(Locale::Ja),
        "フォルダーはすでに含まれています"
    );
    assert_eq!(already.body(Locale::Ja), "Notes は Docs に含まれています。");

    assert_eq!(one.title(Locale::En), "Folders combined");
    assert_eq!(one.body(Locale::En), "Notes is now part of Docs.");
    assert_eq!(
        two.body(Locale::En),
        "Notes and Drafts are now part of Docs."
    );
    assert_eq!(three.body(Locale::En), "A, B and C are now part of Docs.");
    assert_eq!(one.title(Locale::Ja), "フォルダーをまとめました");
    assert_eq!(
        one.body(Locale::Ja),
        "Notes は Docs に含まれるようになりました。"
    );
    assert_eq!(
        two.body(Locale::Ja),
        "Notes、Drafts は Docs に含まれるようになりました。"
    );
    for notice in [&already, &one] {
        assert!(!notice.is_problem(), "both are information, not problems");
        assert!(notice.action(Locale::En).is_none(), "and have no button");
    }
}

#[test]
fn combined_folders_leave_the_list_and_the_notice_names_them() {
    let mut state = AppState {
        sources: vec![
            card("s_docs", "Docs"),
            card("s_notes", "Notes"),
            card("s_other", "Other"),
        ],
        selected_source: Some(1),
        confirm_remove_source: Some("s_notes".into()),
        ..AppState::default()
    };
    state.update(&combined("s_docs", &[("s_notes", "Notes")]));

    let ids: Vec<_> = state.sources.iter().map(|c| c.source_id.as_str()).collect();
    assert_eq!(ids, ["s_docs", "s_other"]);
    assert_eq!(state.selected_source, None);
    assert_eq!(
        state.confirm_remove_source, None,
        "no question about a folder that is gone"
    );
    assert_eq!(
        state.notice,
        Some(UserNotice::FoldersCombined {
            folders: vec!["Notes".into()],
            parent: "Docs".into(),
        })
    );
}

/// A removal question about some *other* folder is not disturbed.
#[test]
fn a_removal_question_about_another_folder_stays_open() {
    let mut state = AppState {
        sources: vec![
            card("s_docs", "Docs"),
            card("s_notes", "Notes"),
            card("s_other", "Other"),
        ],
        confirm_remove_source: Some("s_other".into()),
        ..AppState::default()
    };
    state.update(&combined("s_docs", &[("s_notes", "Notes")]));
    assert_eq!(state.confirm_remove_source.as_deref(), Some("s_other"));
}

/// The search that was looking at a combined folder keeps looking at the same
/// files, now as the folder that holds them, limited to it; the choice of
/// "only" is kept.
#[test]
fn a_selected_search_folder_that_is_combined_is_limited_to_it_inside_the_new_one() {
    let mut state = AppState::default();
    state.search_location.selected = Some(
        SearchLocation::remembered(SourceId::from_string("s_notes".to_string()), "Notes")
            .with_scope(crate::state::SearchFolderScope::FolderOnly),
    );
    state.update(&combined("s_docs", &[("s_notes", "Notes")]));

    let location = state.search_location.selected.clone().unwrap();
    assert_eq!(location.source_id().unwrap().as_str(), "s_docs");
    assert_eq!(location.display_name(), "Notes");
    assert_eq!(location.limit_path(), Some("/docs/Notes"));
    assert_eq!(
        location.scope(),
        crate::state::SearchFolderScope::FolderOnly
    );
}

/// A location that was already limited to a subfolder of the combined folder
/// keeps its own limit and name.
#[test]
fn a_location_inside_a_combined_folder_keeps_its_own_limit() {
    let mut state = AppState::default();
    state.search_location.selected = Some(SearchLocation::within(
        SourceId::from_string("s_notes".to_string()),
        "Sub",
        "/docs/Notes/Sub",
    ));
    state.update(&combined("s_docs", &[("s_notes", "Notes")]));

    let location = state.search_location.selected.clone().unwrap();
    assert_eq!(location.source_id().unwrap().as_str(), "s_docs");
    assert_eq!(location.display_name(), "Sub");
    assert_eq!(location.limit_path(), Some("/docs/Notes/Sub"));
}

#[test]
fn a_location_elsewhere_and_the_recent_list_are_handled() {
    let mut state = AppState::default();
    state.search_location.selected = Some(SearchLocation::remembered(
        SourceId::from_string("s_other".to_string()),
        "Other",
    ));
    state.search_location.recent_locations = vec![
        SearchLocationSummary {
            source_id: SourceId::from_string("s_notes".to_string()),
            display_name: "Notes".into(),
        },
        SearchLocationSummary {
            source_id: SourceId::from_string("s_other".to_string()),
            display_name: "Other".into(),
        },
    ];
    state.update(&combined("s_docs", &[("s_notes", "Notes")]));

    let location = state.search_location.selected.clone().unwrap();
    assert_eq!(
        location.source_id().unwrap().as_str(),
        "s_other",
        "not touched"
    );
    let recent: Vec<_> = state
        .search_location
        .recent_locations
        .iter()
        .map(|s| s.source_id.as_str().to_string())
        .collect();
    assert_eq!(
        recent,
        ["s_other"],
        "a chip for a folder that is gone is dropped"
    );
}
