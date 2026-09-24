use super::*;

/// RFC-055 §4.5/§9.1: the compatibility claim between `for_app("orbok")`
/// and `new()` must be measured, not reasoned -- so this calls both real
/// constructors rather than asserting a path we derived by reading the
/// upstream source. `new()` yields `<platform base>/<exe-stem>`, so
/// `via_new.folder_path().parent()` **is** the platform base -- the same
/// private algorithm's output, obtained through the public API, without
/// needing the *running* process to itself be named "orbok" (a `cargo
/// test` harness binary never is). Joining `"orbok"` onto that measured
/// base reconstructs exactly what `new()` would produce in a process
/// named `orbok`, proving full path equality (Review 151 §4). Named for
/// what it proves now, not `..._share_the_platform_config_parent` (the
/// weaker claim an earlier revision made) -- Review 152 §3.
///
/// Every CI leg has a resolvable platform configuration directory, so a
/// failure here is treated as a real failure, not tiptoed around with a
/// silent skip (Review 151 §4, "related, at your discretion").
#[test]
fn for_app_orbok_and_new_resolve_to_the_same_path() {
    let via_new = ConfigManager::<OrbokSettings>::new();
    let via_for_app = ConfigManager::<OrbokSettings>::for_app("orbok")
        .expect("platform configuration directory must resolve in this environment");

    let base = via_new
        .folder_path()
        .parent()
        .expect("new()'s folder_path always has a parent -- it is base.join(name)");

    assert_eq!(
        via_for_app.folder_path(),
        base.join("orbok"),
        "for_app(\"orbok\") must resolve to the same path new() would in a \
         process named \"orbok\" (RFC-055 §4.5)"
    );
}

/// Review 152 §3: the test above proves the crate's behavior for the
/// literal `"orbok"` -- it constructs `for_app("orbok")` directly and
/// never calls `standard_settings_dir()`, so the link from "the crate
/// resolves this way for `\"orbok\"`" to "production actually passes
/// `\"orbok\"`" was carried entirely by `runtime_isolation_tests.rs`'s
/// `include_str!` source-text scan. That scan does catch a changed
/// literal (verified: breaking the production literal produces a real
/// CI failure there), but it is a textual check, not a call through the
/// production path. This closes the loop directly, asserting the actual
/// production function's result against the same measured base.
///
/// Task 019: this asserts a **directory**, not `.../settings.json` -- the
/// function itself was renamed from `standard_settings_file()` because its
/// only production caller immediately discarded the filename with
/// `.parent()`. Kept calling the production function and comparing
/// against `base.join("orbok")` rather than weakening this while renaming
/// it (Review 152 §3's whole point): verified by temporarily changing the
/// production literal to something other than `"orbok"` and confirming
/// this test fails, then restoring it.
#[test]
fn standard_settings_dir_resolves_through_the_production_call_site() {
    let via_new = ConfigManager::<OrbokSettings>::new();
    let base = via_new
        .folder_path()
        .parent()
        .expect("new()'s folder_path always has a parent -- it is base.join(name)");

    assert_eq!(
        super::standard_settings_dir()
            .expect("platform configuration directory must resolve in this environment"),
        base.join("orbok"),
        "standard_settings_dir() must resolve under the same platform \
         configuration directory new() would use for a process named \"orbok\""
    );
}

/// RFC-057 §4.4 / §6.2 item 5, HANDOFF-057 §5: `pause_on_battery` renamed
/// to `pause_embedding_on_battery` -- a profile's existing `settings.json`
/// must keep honoring its saved preference. Written as literal JSON text
/// naming the *old* field, not a constructed `OrbokSettings` round-tripped
/// through serde: a struct that already knows about `#[serde(alias)]`
/// only proves serde understands its own attribute, not that a file an
/// old orbok version actually wrote still parses under the new field.
#[test]
fn legacy_pause_on_battery_field_name_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{
            "embedding_model_dir": null,
            "reranker_model_dir": null,
            "index_mode": "balanced",
            "locale": "en",
            "theme": "system",
            "text_scale": "default",
            "reduced_motion": false,
            "rerank_enabled": false,
            "background_indexing": true,
            "pause_on_battery": false,
            "privacy_mode": "standard",
            "remember_recent_searches": true,
            "persist_snippets": true,
            "clear_temporary_previews_on_exit": false
        }"#,
    )
    .unwrap();

    let loaded = load_settings(&path);
    assert!(
        !loaded.pause_embedding_on_battery,
        "a legacy settings.json's `pause_on_battery` value must still be \
         honored under the new field name -- a broken alias would silently \
         fall back to OrbokSettings::default() (true), not this file's `false`"
    );
}

// ── Task 115 (RFC-039 Amendment): the privacy mode leaves settings.json ──

/// A complete settings file, as an older orbok wrote it, with `privacy_mode`
/// and `remember_recent_searches` as given (`None` omits the field).
fn old_settings_json(privacy_mode: Option<&str>, remember: bool) -> String {
    let mode = privacy_mode
        .map(|m| format!("\"privacy_mode\": \"{m}\","))
        .unwrap_or_default();
    format!(
        r#"{{
            "embedding_model_dir": null,
            "reranker_model_dir": null,
            "index_mode": "balanced",
            "locale": "en",
            "theme": "system",
            "text_scale": "default",
            "reduced_motion": false,
            "rerank_enabled": false,
            "background_indexing": true,
            "pause_embedding_on_battery": true,
            {mode}
            "remember_recent_searches": {remember},
            "persist_snippets": true,
            "clear_temporary_previews_on_exit": false
        }}"#
    )
}

fn load_from(json: &str) -> OrbokSettings {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, json).unwrap();
    load_settings(&path)
}

/// §5 test 1: a hand-edited `"strict"` is honoured once, as **Remember recent
/// searches: Off** -- a user's effective choice is never silently reversed --
/// and the next save no longer carries the field.
#[test]
fn a_strict_privacy_mode_loads_as_recent_searches_off_and_is_not_saved_again() {
    let loaded = load_from(&old_settings_json(Some("strict"), true));
    assert!(
        !loaded.remember_recent_searches,
        "\"strict\" loads with recent searches off"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    save_settings(&path, &loaded).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        !saved.contains("privacy_mode"),
        "the saved file drops the field: {saved}"
    );
    // And it stays Off on the next load: the file now says so itself.
    assert!(!load_settings(&path).remember_recent_searches);
}

/// Any other value, or no field, changes nothing -- the toggle's own value
/// stands, on or off.
#[test]
fn any_other_privacy_mode_or_none_changes_nothing() {
    for mode in [
        None,
        Some("standard"),
        Some("portable"),
        Some("diagnostics"),
        Some("nonsense"),
    ] {
        for remember in [true, false] {
            let loaded = load_from(&old_settings_json(mode, remember));
            assert_eq!(
                loaded.remember_recent_searches, remember,
                "privacy_mode {mode:?}, toggle {remember}"
            );
            let saved = serde_json::to_string(&loaded).unwrap();
            assert!(!saved.contains("privacy_mode"), "{mode:?}: {saved}");
        }
    }
}

/// The rest of the file is read as before (the compatibility rule must not
/// cost a profile its other settings).
#[test]
fn a_strict_file_keeps_its_other_settings() {
    let loaded =
        load_from(&old_settings_json(Some("strict"), true).replace("\"system\"", "\"dark\""));
    assert_eq!(loaded.theme, "dark");
    assert!(loaded.background_indexing);
}

/// §5 test 2: the toggle is the whole truth. With it Off nothing is recorded;
/// with it On one search is -- whatever any other setting (including a
/// leftover privacy mode) says.
#[test]
fn recent_searches_follow_the_toggle_and_nothing_else() {
    let catalog = orbok_db::Catalog::open_in_memory().unwrap();
    for mode in [
        None,
        Some("standard"),
        Some("portable"),
        Some("diagnostics"),
    ] {
        for (remember, expected) in [(false, 0), (true, 1)] {
            let settings = load_from(&old_settings_json(mode, remember));
            crate::history::record_search(
                &catalog,
                &settings.privacy_settings(),
                &settings.history_settings(),
                &format!("query {mode:?} {remember}"),
                &[],
                1,
                &settings.locale,
            );
            let recorded = crate::history::load_history(&catalog)
                .iter()
                .filter(|e| e.search_text == format!("query {mode:?} {remember}"))
                .count();
            assert_eq!(recorded, expected, "mode {mode:?}, toggle {remember}");
        }
    }
    // A leftover "strict" is the toggle Off, so it records nothing either.
    let strict = load_from(&old_settings_json(Some("strict"), true));
    crate::history::record_search(
        &catalog,
        &strict.privacy_settings(),
        &strict.history_settings(),
        "kept private",
        &[],
        1,
        &strict.locale,
    );
    assert!(
        crate::history::load_history(&catalog)
            .iter()
            .all(|e| e.search_text != "kept private")
    );
}

// ── Task 117: an upgrade keeps your settings ──────────────────────────
//
// These go through the production load and save
// (`orbok::runtime_storage::load_settings` / `save_settings`), not the
// test-only `load_settings(path)` above: the unreadable-file rule lives there.

use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};

fn context_in(dir: &Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(dir.as_os_str().to_os_string())).unwrap(),
        dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(dir),
            standard_settings_dir: Some(dir),
        },
    )
    .unwrap()
}

fn load_prod(context: &RuntimeContext) -> OrbokSettings {
    orbok::runtime_storage::load_settings::<OrbokSettings>(context).unwrap()
}

fn as_value(settings: &OrbokSettings) -> serde_json::Value {
    serde_json::to_value(settings).unwrap()
}

/// A settings file in which **every** field the app reads has a value that is
/// not its default.
fn non_default_settings() -> serde_json::Value {
    serde_json::json!({
        "embedding_model_dir": "/models/e5",
        "locale": "ja",
        "theme": "dark",
        "text_scale": "larger",
        "reduced_motion": true,
        "background_indexing": false,
        "pause_embedding_on_battery": false,
        "remember_recent_searches": false,
    })
}

/// Test 1: every field, missing one at a time. That field takes its default;
/// every other keeps its own value.
#[test]
fn a_missing_field_takes_its_default_and_nothing_else_changes() {
    let full = non_default_settings();
    let default = as_value(&OrbokSettings::default());
    // The fixture really is non-default everywhere, or this proves nothing.
    for (key, value) in full.as_object().unwrap() {
        assert_ne!(value, &default[key], "{key} must differ from its default");
    }
    assert_eq!(
        full.as_object().unwrap().len(),
        default.as_object().unwrap().len(),
        "the fixture names every field"
    );
    for missing in full.as_object().unwrap().keys() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = full.clone();
        file.as_object_mut().unwrap().remove(missing);
        std::fs::write(dir.path().join("settings.json"), file.to_string()).unwrap();

        let loaded = as_value(&load_prod(&context_in(dir.path())));

        for (key, expected) in full.as_object().unwrap() {
            let want = if key == missing {
                &default[key]
            } else {
                expected
            };
            assert_eq!(
                &loaded[key], want,
                "without `{missing}`, `{key}` must be {want}"
            );
        }
    }
}

/// Test 2: a file from before 0.14.0 (no `text_scale`, no `reduced_motion`)
/// keeps what it had. These are the fields orbok then wrote.
#[test]
fn a_file_from_before_text_scale_and_reduced_motion_keeps_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{
            "embedding_model_dir": "/models/e5",
            "reranker_model_dir": null,
            "index_mode": "balanced",
            "locale": "ja",
            "theme": "dark",
            "rerank_enabled": false,
            "background_indexing": true,
            "pause_on_battery": false
        }"#,
    )
    .unwrap();

    let loaded = load_prod(&context_in(dir.path()));

    assert_eq!(loaded.embedding_model_dir.as_deref(), Some("/models/e5"));
    assert_eq!(loaded.locale, "ja");
    assert_eq!(loaded.theme, "dark");
    assert!(
        !loaded.pause_embedding_on_battery,
        "the old name still loads"
    );
    assert_eq!(loaded.text_scale, "default", "the new fields take defaults");
    assert!(!loaded.reduced_motion);
}

/// Test 3: one unreadable value does not cost the others.
#[test]
fn a_wrong_typed_value_takes_its_default_and_the_rest_load() {
    let dir = tempfile::tempdir().unwrap();
    let mut file = non_default_settings();
    file["theme"] = serde_json::json!(5);
    file["background_indexing"] = serde_json::json!("yes");
    std::fs::write(dir.path().join("settings.json"), file.to_string()).unwrap();

    let loaded = load_prod(&context_in(dir.path()));

    assert_eq!(loaded.theme, "system", "the bad value takes its default");
    assert!(loaded.background_indexing, "so does the second one");
    assert_eq!(loaded.locale, "ja");
    assert_eq!(loaded.embedding_model_dir.as_deref(), Some("/models/e5"));
    assert_eq!(loaded.text_scale, "larger");
    assert!(loaded.reduced_motion);
    assert!(!loaded.remember_recent_searches);
}

/// Test 4: a file that cannot be parsed at all is kept, not overwritten.
#[test]
fn an_unparseable_file_is_kept_beside_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let context = context_in(dir.path());
    let settings_path = dir.path().join("settings.json");
    let unreadable = dir.path().join("settings.json.unreadable");
    let truncated = br#"{"locale": "ja", "embedding_model_dir": "/models/e5", "theme":"#;
    std::fs::write(&settings_path, truncated).unwrap();

    let loaded = load_prod(&context);

    assert_eq!(as_value(&loaded), as_value(&OrbokSettings::default()));
    assert_eq!(
        std::fs::read(&unreadable).unwrap(),
        truncated,
        "the user's bytes are kept, intact"
    );
    assert!(!settings_path.exists(), "and moved, not copied");

    // The next save writes a fresh file and leaves the copy alone.
    let mut changed = loaded;
    changed.theme = "dark".into();
    orbok::runtime_storage::save_settings(&context, &changed).unwrap();
    assert_eq!(std::fs::read(&unreadable).unwrap(), truncated);
    assert_eq!(load_prod(&context).theme, "dark");

    // A second unreadable file replaces the older copy.
    std::fs::write(&settings_path, b"not json at all").unwrap();
    let _ = load_prod(&context);
    assert_eq!(std::fs::read(&unreadable).unwrap(), b"not json at all");
}

/// A file that is JSON but not an object is unreadable too.
#[test]
fn a_file_that_is_not_an_object_is_kept_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), b"[1, 2, 3]").unwrap();
    let loaded = load_prod(&context_in(dir.path()));
    assert_eq!(as_value(&loaded), as_value(&OrbokSettings::default()));
    assert_eq!(
        std::fs::read(dir.path().join("settings.json.unreadable")).unwrap(),
        b"[1, 2, 3]"
    );
}

/// Test 5: fields nothing reads load without complaint, and the next save
/// omits them.
#[test]
fn removed_fields_load_and_are_not_saved_again() {
    let dir = tempfile::tempdir().unwrap();
    let context = context_in(dir.path());
    let mut file = non_default_settings();
    for (key, value) in [
        ("index_mode", serde_json::json!("space_saving")),
        ("clear_temporary_previews_on_exit", serde_json::json!(true)),
        ("reranker_model_dir", serde_json::json!("/models/rerank")),
        ("rerank_enabled", serde_json::json!(true)),
    ] {
        file[key] = value;
    }
    std::fs::write(dir.path().join("settings.json"), file.to_string()).unwrap();

    let loaded = load_prod(&context);
    assert_eq!(loaded.locale, "ja", "the rest of the file loads");
    orbok::runtime_storage::save_settings(&context, &loaded).unwrap();

    let saved = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
    for gone in [
        "index_mode",
        "clear_temporary_previews_on_exit",
        "reranker_model_dir",
        "rerank_enabled",
        "privacy_mode",
        "persist_snippets",
    ] {
        assert!(
            !saved.contains(gone),
            "the saved file still has `{gone}`: {saved}"
        );
    }
}
