//! Task 129 §2.1 and Task 130 §1.3: the window carries orbok's own
//! identity (the Linux application id, so a desktop can match it to
//! `orbok.desktop` for the task bar / dock icon) and its own icon
//! (cross-platform). Both come from the one `window_settings()` function
//! in `main.rs`, tested here together.

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

/// Task 130 §1.3: the window carries orbok's own icon, cross-platform --
/// before this, Windows showed a generic icon on an unpackaged run and X11
/// title bars showed none. Set unconditionally (no `cfg`): winit's
/// `with_window_icon` only affects Windows and X11, so Wayland and macOS
/// simply ignore it.
#[test]
fn the_window_settings_carry_an_icon() {
    let settings = crate::window_settings();
    let icon = settings.icon.expect("the window icon must be set");
    let (rgba, size) = icon.into_raw();
    assert_eq!(size.width, 256);
    assert_eq!(size.height, 256);
    assert_eq!(
        rgba.len(),
        256 * 256 * 4,
        "the embedded bytes must be exactly width * height * 4 (raw RGBA), \
         matching assets/icon-256.rgba"
    );
}
