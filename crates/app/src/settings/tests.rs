use super::*;

/// RFC-055 §4.5/§9.1: the compatibility claim between `for_app("orbok")`
/// and `new()` must be measured, not reasoned -- so this calls both real
/// constructors rather than asserting a path we derived by reading the
/// upstream source. Both resolve the platform configuration directory
/// through the same private algorithm, so their parent directories must
/// agree unconditionally; that much is measurable in any process. Full
/// path equality additionally requires the *running* executable's own
/// derived name (`new()`'s `std::env::current_exe()` file-stem
/// derivation) to be "orbok" -- true for the shipped `orbok` binary, but
/// not for a `cargo test` harness binary, whose name carries a build
/// hash. That stronger comparison runs whenever it actually applies,
/// rather than being asserted unconditionally.
#[test]
fn for_app_orbok_and_new_share_the_platform_config_parent() {
    let via_new = ConfigManager::<OrbokSettings>::new();
    let Ok(via_for_app) = ConfigManager::<OrbokSettings>::for_app("orbok") else {
        eprintln!("skip: platform configuration directory unavailable in this environment");
        return;
    };

    assert_eq!(
        via_for_app.folder_path().parent(),
        via_new.folder_path().parent(),
        "for_app(\"orbok\") and new() must resolve under the same platform \
         configuration directory"
    );

    let derived_name = via_new
        .folder_path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    if derived_name.as_deref() == Some("orbok") {
        assert_eq!(
            via_for_app.folder_path(),
            via_new.folder_path(),
            "when the running executable's derived name is \"orbok\", \
             for_app(\"orbok\") must match new() exactly (RFC-055 §4.5)"
        );
    }
}
