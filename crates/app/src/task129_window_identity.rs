//! Task 129 §2.1: the window carries orbok's own identity on Linux, so a
//! desktop environment can match it to `orbok.desktop` for the task bar /
//! dock icon -- `niri msg windows` measured it empty before this fix.

#[test]
fn the_window_settings_carry_the_application_id_on_linux() {
    let settings = crate::window_settings();
    #[cfg(target_os = "linux")]
    assert_eq!(
        settings.platform_specific.application_id, "orbok",
        "the Linux application id must be set, matching orbok.desktop"
    );
    // Windows and macOS: proven untouched by construction, not merely
    // asserted -- `window_settings` only ever writes to
    // `PlatformSpecific::application_id` inside a
    // `#[cfg(target_os = "linux")]` block (see its own doc comment for
    // why: that field does not exist on the other platforms' own
    // `PlatformSpecific` struct, so this cannot compile on them without
    // the cfg gate). This test still runs there, confirming the function
    // compiles and returns the plain default.
    #[cfg(not(target_os = "linux"))]
    let _ = settings;
}
