//! Small Win32 shell helpers used by the Creator.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null_mut;

use windows_core::{GUID, HRESULT, PWSTR};

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod ffi {
    use core::ffi::c_void;
    use windows_core::{GUID, HRESULT, PWSTR};

    windows_core::link!("shell32.dll" "system" fn SHGetKnownFolderPath(
        rfid: *const GUID,
        dwflags: u32,
        htoken: *mut c_void,
        ppszpath: *mut PWSTR,
    ) -> HRESULT);
    windows_core::link!("ole32.dll" "system" fn CoTaskMemFree(pv: *const c_void));
}

/// `FOLDERID_Desktop` (`{B4BFCC3A-DB2C-424C-B029-7FE99A87C641}`) from `KnownFolders.h`.
static FOLDERID_DESKTOP: GUID = GUID::from_u128(0xB4BF_CC3A_DB2C_424C_B029_7FE9_9A87_C641);

struct CoTaskMemString(PWSTR);

impl Drop for CoTaskMemString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Frees the buffer allocated by `SHGetKnownFolderPath` exactly once.
            unsafe { ffi::CoTaskMemFree(self.0.0.cast::<c_void>().cast_const()) };
        }
    }
}

/// The current user's real Desktop folder as reported by the shell.
///
/// Unlike `%USERPROFILE%\Desktop`, this follows redirected desktops (`OneDrive`, group policy,
/// a relocated profile). Returns `None` when the shell cannot resolve it.
#[must_use]
pub fn desktop_dir() -> Option<PathBuf> {
    let mut raw = PWSTR::null();
    // SAFETY: `FOLDERID_DESKTOP` and `raw` are live for the call; a null token selects the
    // current user. The returned buffer is owned by `CoTaskMemString` below.
    let hr: HRESULT = unsafe {
        ffi::SHGetKnownFolderPath(&raw const FOLDERID_DESKTOP, 0, null_mut(), &raw mut raw)
    };
    let owned = CoTaskMemString(raw);
    if hr.is_err() || owned.0.is_null() {
        return None;
    }
    // SAFETY: On success the shell returns a valid NUL-terminated UTF-16 path.
    let text = unsafe { owned.0.to_string() }.ok()?;
    if text.is_empty() {
        None
    } else {
        Some(PathBuf::from(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_known_folder_resolves_to_an_existing_absolute_directory() {
        let desktop = desktop_dir().expect("the shell must know the Desktop folder");
        assert!(desktop.is_absolute(), "{}", desktop.display());
        assert!(desktop.is_dir(), "{}", desktop.display());
    }

    #[test]
    fn desktop_folder_id_matches_knownfolders_h() {
        assert_eq!(
            format!("{FOLDERID_DESKTOP:?}").to_ascii_uppercase(),
            "B4BFCC3A-DB2C-424C-B029-7FE99A87C641"
        );
    }
}
