//! Low-level Win32/COM/WinRT interop utilities for native `WinUI` 3 windows, affinity, clipboard,
//! display scaling, and modal Vista+ file/folder dialogs.

use std::path::{Path, PathBuf};
use std::ptr::copy_nonoverlapping;

use windows_core::{Interface, PCWSTR};

use crate::bindings::Microsoft::UI::Xaml::Window;
use crate::bindings::Windows::Win32 as w32;

/// Native COM interop definitions omitted from WinUI 3 metadata projections (`IWindowNative` and
/// `IBufferByteAccess`).
#[allow(
    missing_docs,
    non_camel_case_types,
    non_snake_case,
    unsafe_op_in_unsafe_fn,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::restriction
)]
pub mod raw_com {
    use core::ffi::c_void;

    windows_core::imp::define_interface!(
        IWindowNative,
        IWindowNative_Vtbl,
        0xeecdbf0e_bae9_4cb6_a68e_9598e1cb57bb
    );
    windows_core::imp::interface_hierarchy!(IWindowNative, windows_core::IUnknown);

    impl IWindowNative {
        pub unsafe fn WindowHandle(&self) -> windows_core::Result<*mut c_void> {
            let mut hwnd = core::ptr::null_mut();
            (windows_core::Interface::vtable(self).WindowHandle)(
                windows_core::Interface::as_raw(self),
                &mut hwnd,
            )
            .map(|| hwnd)
        }
    }

    #[repr(C)]
    pub struct IWindowNative_Vtbl {
        pub base__: windows_core::IUnknown_Vtbl,
        pub WindowHandle:
            unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> windows_core::HRESULT,
    }

    windows_core::imp::define_interface!(
        IBufferByteAccess,
        IBufferByteAccess_Vtbl,
        0x905a0fef_bc53_11df_8c49_001e4fc686da
    );
    windows_core::imp::interface_hierarchy!(IBufferByteAccess, windows_core::IUnknown);

    impl IBufferByteAccess {
        pub unsafe fn Buffer(&self) -> windows_core::Result<*mut u8> {
            let mut ptr = core::ptr::null_mut();
            (windows_core::Interface::vtable(self).Buffer)(
                windows_core::Interface::as_raw(self),
                &mut ptr,
            )
            .map(|| ptr)
        }
    }

    #[repr(C)]
    pub struct IBufferByteAccess_Vtbl {
        pub base__: windows_core::IUnknown_Vtbl,
        pub Buffer: unsafe extern "system" fn(*mut c_void, *mut *mut u8) -> windows_core::HRESULT,
    }
}

/// Converts UTF-8 string into NUL-terminated UTF-16 buffer.
#[must_use]
pub fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Extracts the underlying Win32 `HWND` from a native `WinUI` 3 [`Window`].
///
/// # Errors
///
/// Returns an `HRESULT` error if the window does not support `IWindowNative` or handle query fails.
pub fn window_hwnd(window: &Window) -> windows_core::Result<w32::HWND> {
    let native: raw_com::IWindowNative = window.cast()?;
    // SAFETY: `native` is a valid `IWindowNative` proxy backed by a live WinUI 3 `Window`.
    let raw = unsafe { native.WindowHandle()? };
    Ok(w32::HWND(raw))
}

/// Display capture protection level successfully applied to a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProtectionLevel {
    /// Window disappears entirely from screen captures (`WDA_EXCLUDEFROMCAPTURE`).
    ExcludeFromCapture,
    /// Window turns opaque black in screen captures (`WDA_MONITOR`).
    MonitorBlackout,
}

/// Applies Win32 `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE` first, falling back to
/// `WDA_MONITOR`) and verifies that protection succeeded.
///
/// # Errors
///
/// Fails closed with a `windows_core::Error` if neither capture protection mode could be set.
pub fn enforce_window_capture_protection(
    window: &Window,
) -> windows_core::Result<CaptureProtectionLevel> {
    let hwnd = window_hwnd(window)?;
    let exclude = u32::try_from(w32::WDA_EXCLUDEFROMCAPTURE).unwrap_or(0x11);
    // SAFETY: Valid top-level WinUI 3 `hwnd`; enables capture exclusion.
    if unsafe { w32::SetWindowDisplayAffinity(hwnd, exclude) }.as_bool() {
        return Ok(CaptureProtectionLevel::ExcludeFromCapture);
    }

    let monitor = u32::try_from(w32::WDA_MONITOR).unwrap_or(0x01);
    // SAFETY: Valid top-level WinUI 3 `hwnd`; falls back to monitor-only blackout affinity.
    if !unsafe { w32::SetWindowDisplayAffinity(hwnd, monitor) }.as_bool() {
        return Err(windows_core::Error::from_thread());
    }
    Ok(CaptureProtectionLevel::MonitorBlackout)
}

/// Returns the DPI for `window` (defaulting to 96 if unavailable).
#[must_use]
pub fn window_dpi(window: &Window) -> u32 {
    window_hwnd(window).map_or(96, |hwnd| {
        // SAFETY: Valid top-level `hwnd`.
        let dpi = unsafe { w32::GetDpiForWindow(hwnd) };
        if dpi == 0 { 96 } else { dpi }
    })
}

/// Restores (if minimized) and activates/foregrounds `window`.
pub fn activate_and_foreground(window: &Window) {
    let _ = window.Activate();
    if let Ok(hwnd) = window_hwnd(window) {
        // SAFETY: Standard Win32 restore call if minimized.
        unsafe {
            let _ = w32::ShowWindow(hwnd, w32::SW_RESTORE);
        }
        // SAFETY: Brings top-level `hwnd` to foreground.
        unsafe {
            let _ = w32::SetForegroundWindow(hwnd);
        }
    }
}

/// Shows a modal Win32 message box (`MB_OK` + icon).
pub fn message_box(hwnd: w32::HWND, title: &str, text: &str, is_error: bool) {
    let t = to_wide_null(title);
    let m = to_wide_null(text);
    let flags = u32::try_from(
        w32::MB_OK
            | if is_error {
                w32::MB_ICONERROR
            } else {
                w32::MB_ICONWARNING
            },
    )
    .unwrap_or(0);
    // SAFETY: Both UTF-16 strings are NUL-terminated for the duration of the modal dialog.
    unsafe {
        let _ = w32::MessageBoxW(Some(hwnd), PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), flags);
    }
}

/// Prompts user with `MB_YESNO`, returning `true` only when Yes (`IDYES`) is clicked.
#[must_use]
pub fn confirm_message_box(hwnd: w32::HWND, title: &str, text: &str) -> bool {
    let t = to_wide_null(title);
    let m = to_wide_null(text);
    let flags = u32::try_from(w32::MB_YESNO | w32::MB_ICONWARNING).unwrap_or(0x34);
    // SAFETY: Modal confirmation dialog with valid NUL-terminated strings.
    let rc = unsafe { w32::MessageBoxW(Some(hwnd), PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), flags) };
    rc == w32::IDYES
}

/// Opens a file, folder, or HTTPS URL via Windows shell (`ShellExecuteW`).
pub fn shell_open(target: &str) {
    let op = to_wide_null("open");
    let tgt = to_wide_null(target);
    // SAFETY: Standard Win32 shell verb invocation on valid NUL-terminated UTF-16 buffers.
    unsafe {
        let _ = w32::ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(tgt.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            w32::SW_RESTORE,
        );
    }
}

struct ClipboardCloser;

impl Drop for ClipboardCloser {
    fn drop(&mut self) {
        // SAFETY: Always closes opened Win32 clipboard handle.
        unsafe {
            let _ = w32::CloseClipboard();
        }
    }
}

/// Copies UTF-8 text into the Windows clipboard as `CF_UNICODETEXT`.
///
/// `owner` must be a window of the calling thread: opening the clipboard without an owner makes
/// `EmptyClipboard` clear the ownership and the following `SetClipboardData` unreliable.
///
/// # Errors
///
/// Returns `windows_core::Error` if clipboard acquisition or global memory allocation fails.
pub fn copy_unicode_text(owner: w32::HWND, text: &str) -> windows_core::Result<()> {
    if owner.0.is_null() {
        return Err(windows_core::Error::new(
            windows_core::HRESULT(0x8007_0057_u32.cast_signed()),
            "clipboard owner window is required",
        ));
    }
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = utf16
        .len()
        .checked_mul(2)
        .ok_or_else(windows_core::Error::from_thread)?;

    let gmem_moveable = u32::try_from(w32::GMEM_MOVEABLE).unwrap_or(2);
    // SAFETY: Allocates moveable global memory block owned by clipboard upon `SetClipboardData`.
    let hmem = unsafe { w32::GlobalAlloc(gmem_moveable, byte_len) };
    if hmem.0.is_null() {
        return Err(windows_core::Error::from_thread());
    }
    // SAFETY: Locks `hmem` to copy UTF-16 NUL-terminated text bytes.
    let dst = unsafe { w32::GlobalLock(hmem) };
    if dst.is_null() {
        // SAFETY: Allocation failed to lock; release `hmem` cleanly before returning error.
        unsafe {
            let _ = w32::GlobalFree(hmem);
        }
        return Err(windows_core::Error::from_thread());
    }

    // SAFETY: `dst` is valid writable memory of size `byte_len == utf16.len() * 2`.
    unsafe {
        copy_nonoverlapping(utf16.as_ptr().cast::<u8>(), dst.cast::<u8>(), byte_len);
    }
    // SAFETY: Unlocks locked `hmem` handle before handing ownership to clipboard.
    unsafe {
        let _ = w32::GlobalUnlock(hmem);
    }

    // SAFETY: Opens the clipboard on behalf of the caller's window.
    if !unsafe { w32::OpenClipboard(Some(owner)) }.as_bool() {
        let err = windows_core::Error::from_thread();
        // SAFETY: `OpenClipboard` failed so clipboard does not own `hmem`; free `hmem`.
        unsafe {
            let _ = w32::GlobalFree(hmem);
        }
        return Err(err);
    }

    let _closer = ClipboardCloser;

    // SAFETY: Clears previous clipboard contents while clipboard is held open.
    if !unsafe { w32::EmptyClipboard() }.as_bool() {
        let err = windows_core::Error::from_thread();
        // SAFETY: Clipboard did not take ownership of `hmem`; release it before returning.
        unsafe {
            let _ = w32::GlobalFree(hmem);
        }
        return Err(err);
    }
    let cf_unicode = u32::try_from(w32::CF_UNICODETEXT).unwrap_or(13);
    // SAFETY: Transfers ownership of `hmem` to Windows clipboard on success.
    let set_handle = unsafe { w32::SetClipboardData(cf_unicode, Some(w32::HANDLE(hmem.0))) };
    if set_handle.0.is_null() {
        let err = windows_core::Error::from_thread();
        // SAFETY: Ownership did not transfer on error; free `hmem`.
        unsafe {
            let _ = w32::GlobalFree(hmem);
        }
        return Err(err);
    }
    Ok(())
}

/// Returns `true` if either `Control` key (`VK_CONTROL = 0x11`) is currently depressed.
#[must_use]
pub fn is_ctrl_pressed() -> bool {
    // SAFETY: Queries asynchronous virtual key state for VK_CONTROL.
    let state = unsafe { w32::GetKeyState(0x11) };
    state < 0
}

/// Returns `true` if either `Shift` key (`VK_SHIFT = 0x10`) is currently depressed.
#[must_use]
pub fn is_shift_pressed() -> bool {
    // SAFETY: Queries virtual key state for VK_SHIFT.
    let state = unsafe { w32::GetKeyState(0x10) };
    state < 0
}

/// Picks one or more files using the native modal COM `IFileOpenDialog`.
///
/// # Errors
///
/// Returns `Ok(Vec::new())` if cancelled (`ERROR_CANCELLED`), or an `HRESULT` error otherwise.
pub fn pick_open_files(
    owner: Option<w32::HWND>,
    title: &str,
    filters: &[(&str, &str)],
    multi_select: bool,
) -> windows_core::Result<Vec<PathBuf>> {
    let clsctx = u32::try_from(w32::CLSCTX_ALL).unwrap_or(23);
    // SAFETY: Creates standard Vista+ in-proc COM `FileOpenDialog` instance on STA thread.
    let dialog: w32::IFileOpenDialog =
        unsafe { w32::CoCreateInstance(&w32::FileOpenDialog, None, clsctx) }?;
    let title_w = to_wide_null(title);
    // SAFETY: Sets dialog caption using NUL-terminated UTF-16 string.
    unsafe { dialog.SetTitle(PCWSTR(title_w.as_ptr())) }.ok()?;

    let mut opts = w32::FOS_FORCEFILESYSTEM;
    if multi_select {
        opts |= w32::FOS_ALLOWMULTISELECT;
    }
    // SAFETY: Reads current options before ORing required file system / multi-select bits.
    let existing = unsafe { dialog.GetOptions() }?;
    // SAFETY: Applies updated dialog options.
    unsafe { dialog.SetOptions(existing | opts) }.ok()?;

    let _filter_pin = apply_dialog_filters(&dialog, filters)?;

    // SAFETY: Shows modal dialog owned by `owner`.
    let show_res = unsafe { dialog.Show(owner) }.ok();
    if let Err(err) = show_res {
        if err.code().0 == 0x8007_04C7_u32.cast_signed() {
            return Ok(Vec::new());
        }
        return Err(err);
    }

    // SAFETY: Reads selected item array after user confirms dialog.
    let results = unsafe { dialog.GetResults() }?;
    // SAFETY: Queries number of selected items.
    let count = unsafe { results.GetCount() }?;
    let mut paths = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for idx in 0..count {
        // SAFETY: Retrieves `IShellItem` at valid index `idx < count`.
        let item = unsafe { results.GetItemAt(idx) }?;
        if let Some(p) = shell_item_path(&item)? {
            paths.push(p);
        }
    }
    Ok(paths)
}

/// Picks a directory using the native modal COM `IFileOpenDialog` (`FOS_PICKFOLDERS`).
///
/// # Errors
///
/// Returns `Ok(None)` if user cancels cleanly.
pub fn pick_folder(
    owner: Option<w32::HWND>,
    title: &str,
    initial_dir: Option<&Path>,
) -> windows_core::Result<Option<PathBuf>> {
    let clsctx = u32::try_from(w32::CLSCTX_ALL).unwrap_or(23);
    // SAFETY: Creates standard Vista+ in-proc COM `FileOpenDialog` in folder-pick mode.
    let dialog: w32::IFileOpenDialog =
        unsafe { w32::CoCreateInstance(&w32::FileOpenDialog, None, clsctx) }?;
    let title_w = to_wide_null(title);
    // SAFETY: Sets folder picker caption.
    unsafe { dialog.SetTitle(PCWSTR(title_w.as_ptr())) }.ok()?;
    // SAFETY: Reads current options.
    let existing = unsafe { dialog.GetOptions() }?;
    // SAFETY: Restricts picker to filesystem folders.
    unsafe { dialog.SetOptions(existing | w32::FOS_PICKFOLDERS | w32::FOS_FORCEFILESYSTEM) }
        .ok()?;

    if let Some(dir) = initial_dir
        && dir.is_dir()
    {
        let wide = to_wide_null(&dir.to_string_lossy());
        // SAFETY: Valid UI thread, NUL-terminated path, and null bind context.
        let item_res = unsafe {
            w32::SHCreateItemFromParsingName::<_, Option<&w32::IBindCtx>, w32::IShellItem>(
                PCWSTR(wide.as_ptr()),
                None,
            )
        };
        if let Ok(folder) = item_res {
            // SAFETY: Valid COM `IFileOpenDialog` and `IShellItem` folder instance.
            let _ = unsafe { dialog.SetFolder(&folder) };
        }
    }

    // SAFETY: Displays modal folder picker owned by `owner`.
    let show_res = unsafe { dialog.Show(owner) }.ok();
    if let Err(err) = show_res {
        if err.code().0 == 0x8007_04C7_u32.cast_signed() {
            return Ok(None);
        }
        return Err(err);
    }

    // SAFETY: Retrieves selected folder item.
    let item = unsafe { dialog.GetResult() }?;
    shell_item_path(&item)
}

/// Picks a destination path for saving a file using native modal `IFileSaveDialog`.
///
/// # Errors
///
/// Returns `Ok(None)` if user cancels cleanly.
pub fn pick_save_file(
    owner: Option<w32::HWND>,
    title: &str,
    default_name: &str,
    default_ext: &str,
    filters: &[(&str, &str)],
) -> windows_core::Result<Option<PathBuf>> {
    let clsctx = u32::try_from(w32::CLSCTX_ALL).unwrap_or(23);
    // SAFETY: Creates standard Vista+ in-proc COM `FileSaveDialog`.
    let dialog: w32::IFileSaveDialog =
        unsafe { w32::CoCreateInstance(&w32::FileSaveDialog, None, clsctx) }?;
    let title_w = to_wide_null(title);
    let name_w = to_wide_null(default_name);
    let ext_w = to_wide_null(default_ext);
    // SAFETY: Sets modal save dialog caption.
    unsafe { dialog.SetTitle(PCWSTR(title_w.as_ptr())) }.ok()?;
    // SAFETY: Sets initial filename suggestion.
    unsafe { dialog.SetFileName(PCWSTR(name_w.as_ptr())) }.ok()?;
    // SAFETY: Sets default extension appended automatically.
    unsafe { dialog.SetDefaultExtension(PCWSTR(ext_w.as_ptr())) }.ok()?;
    // SAFETY: Reads existing dialog options.
    let existing = unsafe { dialog.GetOptions() }?;
    // SAFETY: Prompts on overwrite and requires filesystem path.
    unsafe { dialog.SetOptions(existing | w32::FOS_FORCEFILESYSTEM | w32::FOS_OVERWRITEPROMPT) }
        .ok()?;

    let _pin = apply_save_filters(&dialog, filters)?;

    // SAFETY: Displays modal save dialog owned by `owner`.
    let show_res = unsafe { dialog.Show(owner) }.ok();
    if let Err(err) = show_res {
        if err.code().0 == 0x8007_04C7_u32.cast_signed() {
            return Ok(None);
        }
        return Err(err);
    }
    // SAFETY: Retrieves chosen destination shell item.
    let item = unsafe { dialog.GetResult() }?;
    shell_item_path(&item)
}

struct PinnedFilterStorage {
    // Owns wide filter label/spec string allocations while `specs` raw pointers remain in use.
    #[allow(dead_code)]
    strings: Vec<Vec<u16>>,
    specs: Vec<w32::COMDLG_FILTERSPEC>,
}

fn build_filter_specs(filters: &[(&str, &str)]) -> PinnedFilterStorage {
    let mut strings = Vec::with_capacity(filters.len() * 2);
    for (name, spec) in filters {
        strings.push(to_wide_null(name));
        strings.push(to_wide_null(spec));
    }
    let mut specs = Vec::with_capacity(filters.len());
    for pair in strings.as_chunks::<2>().0 {
        specs.push(w32::COMDLG_FILTERSPEC {
            pszName: PCWSTR(pair[0].as_ptr()),
            pszSpec: PCWSTR(pair[1].as_ptr()),
        });
    }
    PinnedFilterStorage { strings, specs }
}

fn apply_dialog_filters(
    dialog: &w32::IFileOpenDialog,
    filters: &[(&str, &str)],
) -> windows_core::Result<PinnedFilterStorage> {
    let pin = build_filter_specs(filters);
    if !pin.specs.is_empty() {
        // SAFETY: `pin.specs` and underlying wide buffers outlive the COM dialog call.
        unsafe {
            dialog.SetFileTypes(
                u32::try_from(pin.specs.len()).unwrap_or(0),
                pin.specs.as_ptr(),
            )
        }
        .ok()?;
    }
    Ok(pin)
}

fn apply_save_filters(
    dialog: &w32::IFileSaveDialog,
    filters: &[(&str, &str)],
) -> windows_core::Result<PinnedFilterStorage> {
    let pin = build_filter_specs(filters);
    if !pin.specs.is_empty() {
        // SAFETY: `pin.specs` and underlying wide buffers outlive the COM dialog call.
        unsafe {
            dialog.SetFileTypes(
                u32::try_from(pin.specs.len()).unwrap_or(0),
                pin.specs.as_ptr(),
            )
        }
        .ok()?;
    }
    Ok(pin)
}

struct CoTaskMemGuard(*mut core::ffi::c_void);

impl Drop for CoTaskMemGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Frees memory allocated by the shell with `CoTaskMemAlloc`.
            unsafe { w32::CoTaskMemFree(self.0) };
        }
    }
}

fn shell_item_path(item: &w32::IShellItem) -> windows_core::Result<Option<PathBuf>> {
    // SAFETY: Queries filesystem path allocated with `CoTaskMemAlloc`.
    let pwstr = unsafe { item.GetDisplayName(w32::SIGDN_FILESYSPATH) }?;
    if pwstr.is_null() {
        return Ok(None);
    }
    // Released on every path, including a failed UTF-16 conversion below.
    let _guard = CoTaskMemGuard(pwstr.0.cast());
    // SAFETY: Valid NUL-terminated UTF-16 path returned by `GetDisplayName`.
    let path_str = unsafe { pwstr.to_string() }?;
    Ok(Some(PathBuf::from(path_str)))
}
