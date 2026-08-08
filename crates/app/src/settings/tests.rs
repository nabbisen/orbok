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
/// named `orbok`, which is the full compatibility claim (Review 151 §4).
///
/// Every CI leg has a resolvable platform configuration directory, so a
/// failure here is treated as a real failure, not tiptoed around with a
/// silent skip (Review 151 §4, "related, at your discretion").
#[test]
fn for_app_orbok_and_new_share_the_platform_config_parent() {
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
