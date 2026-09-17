//! Task 061: how the process was started on Windows -- from a console or
//! not, and from a Microsoft Store (MSIX) package or not.
//!
//! `orbok.exe` is built for the Windows GUI subsystem (`main.rs`), so it
//! never opens a console window of its own. Its command-line output still
//! reaches a terminal by attaching to the parent's console, and reaches a
//! pipe or file untouched. On every other platform both functions are
//! no-ops.

/// Attach to the parent process's console, if there is one, so the text a
/// command prints reaches the terminal it was typed in.
///
/// Standard handles the parent already supplied -- a pipe or a file, as a
/// test harness or `orbok --version > out.txt` gives -- are left alone:
/// attaching is only attempted when neither stdout nor stderr is set. With
/// no parent console (launched from the Start menu), this does nothing;
/// the output is dropped and the exit code is unchanged.
#[cfg(windows)]
pub(crate) fn attach_parent_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    let unset = |which| {
        // SAFETY: `GetStdHandle` takes no pointers and only reads the
        // process's standard-handle table.
        let handle = unsafe { GetStdHandle(which) };
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    };
    if unset(STD_OUTPUT_HANDLE) && unset(STD_ERROR_HANDLE) {
        // SAFETY: no pointers are passed. Failure (no parent console) is
        // the expected Start-menu case and needs no handling.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn attach_parent_console() {}

/// Whether this process runs from an MSIX package (the Microsoft Store
/// build). A packaged process has a package identity, so
/// `GetCurrentPackageFullName` reports a name too long for an empty buffer;
/// an unpackaged one reports `APPMODEL_ERROR_NO_PACKAGE`.
#[cfg(windows)]
pub(crate) fn is_packaged() -> bool {
    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

    let mut length = 0u32;
    // SAFETY: `length` is a valid writable `u32`, and a zero length with a
    // null buffer is the documented way to ask only for the required size.
    let result = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    // Unpackaged gives `APPMODEL_ERROR_NO_PACKAGE`. Anything other than
    // "buffer too small" is treated as unpackaged, keeping today's behaviour.
    result == ERROR_INSUFFICIENT_BUFFER
}

#[cfg(not(windows))]
pub(crate) fn is_packaged() -> bool {
    false
}
