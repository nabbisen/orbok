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
