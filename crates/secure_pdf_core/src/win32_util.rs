//! Shared Win32 UTF-16 and RAII guard utilities for `secure_pdf_core`.

use crate::bindings;
use crate::error::{CoreError, CoreResult};

/// Converts a UTF-8 string into a NUL-terminated UTF-16 buffer for Win32 `*W` APIs.
#[must_use]
pub fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Converts a UTF-8 string into NUL-terminated UTF-16 little-endian bytes for CNG string properties.
#[must_use]
pub fn to_wide_le_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len().saturating_mul(2).saturating_add(2));
    for unit in s.encode_utf16().chain(std::iter::once(0)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// Decodes a NUL-terminated wide string pointer into a trimmed Rust `String`.
///
/// # Safety
///
/// `ptr` must either be null or point to a valid NUL-terminated UTF-16 string readable until the
/// first zero `u16` code unit.
pub unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    loop {
        // SAFETY: Caller guarantees `ptr` is valid up to and including the NUL terminator.
        let ch_ptr = unsafe { ptr.add(len) };
        // SAFETY: `ch_ptr` points within the readable NUL-terminated UTF-16 string.
        let ch = unsafe { *ch_ptr };
        if ch == 0 {
            break;
        }
        len = len.saturating_add(1);
        if len >= 65_536 {
            break;
        }
    }
    // SAFETY: `len` code units starting at `ptr` are non-NUL valid `u16` values within bounds.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

/// Converts a fixed-capacity UTF-16 buffer (up to first NUL) to a `String`.
#[must_use]
pub fn wide_slice_to_string(slice: &[u16]) -> String {
    let end = slice.iter().position(|&ch| ch == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

/// RAII guard for a Win32 kernel `HANDLE` closed via `CloseHandle`.
pub struct HandleGuard(pub bindings::HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Non-null handle owned by this guard; closed exactly once on drop.
            unsafe {
                let _ = bindings::CloseHandle(self.0);
            }
        }
    }
}

/// RAII guard for a Win32 registry key closed via `RegCloseKey`.
pub struct HKeyGuard(pub bindings::HKEY);

impl Drop for HKeyGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Open HKEY owned by this guard; closed exactly once on drop.
            unsafe {
                let _ = bindings::RegCloseKey(self.0);
            }
        }
    }
}

/// RAII guard for a `LocalAlloc` pointer freed via `LocalFree` (e.g. DPAPI output).
pub struct LocalAllocGuard(pub *mut u8);

impl Drop for LocalAllocGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Non-null pointer returned by Win32 DPAPI (`LocalAlloc`) owned by this guard.
            unsafe {
                let _ = bindings::LocalFree(bindings::HANDLE(self.0.cast()));
            }
        }
    }
}

/// RAII guard for a loaded DLL handle freed via `FreeLibrary`.
pub struct LibraryGuard(usize);

impl LibraryGuard {
    /// Wraps a non-null loaded Win32 `HMODULE`.
    #[must_use]
    pub fn new(module: bindings::HMODULE) -> Self {
        Self(module.0 as usize)
    }

    /// Returns the underlying `HMODULE` handle.
    #[must_use]
    pub const fn as_hmodule(&self) -> bindings::HMODULE {
        bindings::HINSTANCE(self.0 as *mut std::ffi::c_void)
    }
}

impl Drop for LibraryGuard {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: Valid loaded module handle owned by this guard.
            unsafe {
                let _ = bindings::FreeLibrary(self.as_hmodule());
            }
        }
    }
}

/// RAII guard for CNG algorithm handles closed via `BCryptCloseAlgorithmProvider`.
pub struct BcryptAlgGuard(pub bindings::BCRYPT_ALG_HANDLE);

impl Drop for BcryptAlgGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid CNG algorithm handle owned by this guard.
            unsafe {
                let _ = bindings::BCryptCloseAlgorithmProvider(self.0, 0);
            }
        }
    }
}

/// RAII guard for CNG symmetric key handles destroyed via `BCryptDestroyKey`.
pub struct BcryptKeyGuard(pub bindings::BCRYPT_KEY_HANDLE);

impl Drop for BcryptKeyGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid CNG key handle owned by this guard.
            unsafe {
                let _ = bindings::BCryptDestroyKey(self.0);
            }
        }
    }
}

/// RAII guard for CNG hash handles destroyed via `BCryptDestroyHash`.
pub struct BcryptHashGuard(pub bindings::BCRYPT_HASH_HANDLE);

impl Drop for BcryptHashGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid CNG hash handle owned by this guard.
            unsafe {
                let _ = bindings::BCryptDestroyHash(self.0);
            }
        }
    }
}

/// RAII guard for COM apartment initialization on the current thread.
pub struct ComApartmentGuard {
    should_uninit: bool,
}

impl ComApartmentGuard {
    /// Initializes COM with the specified apartment flags, tolerating already-initialized
    /// different-mode apartments (`RPC_E_CHANGED_MODE`).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Context`] if `CoInitializeEx` fails with an unexpected `HRESULT`.
    pub fn init(coinit: u32) -> CoreResult<Self> {
        // SAFETY: `CoInitializeEx` takes optional null reserved pointer and valid COINIT flags.
        let hr = unsafe { bindings::CoInitializeEx(None, coinit) };
        if hr.is_ok() {
            Ok(Self {
                should_uninit: true,
            })
        } else if hr == bindings::RPC_E_CHANGED_MODE {
            Ok(Self {
                should_uninit: false,
            })
        } else {
            Err(CoreError::Context(format!(
                "COM 初始化失败：0x{:08X}",
                hr.0
            )))
        }
    }
}

impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            // SAFETY: Balances a successful `CoInitializeEx` call on the same thread.
            unsafe { bindings::CoUninitialize() };
        }
    }
}

/// Acquires a named Win32 mutex with timeout, properly recovering `WAIT_ABANDONED`.
pub struct NamedMutexLock {
    handle: HandleGuard,
}

impl NamedMutexLock {
    /// Acquires `name` waiting up to `timeout_ms` milliseconds.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::QuotaLockTimeout`] if the mutex cannot be acquired within `timeout_ms`.
    pub fn acquire(name: &str, timeout_ms: u32) -> CoreResult<Self> {
        let wide = to_wide_null(name);
        // SAFETY: `wide` is NUL-terminated; attributes are omitted and initial owner is false.
        let raw =
            unsafe { bindings::CreateMutexW(None, false, windows_core::PCWSTR(wide.as_ptr())) };
        if raw.0.is_null() {
            return Err(CoreError::Io(std::io::Error::last_os_error()));
        }
        let guard = HandleGuard(raw);
        // SAFETY: `guard.0` is an open mutex handle.
        let wait_rc = unsafe { bindings::WaitForSingleObject(guard.0, timeout_ms) };
        let wait_obj = u32::try_from(bindings::WAIT_OBJECT_0).unwrap_or(0);
        let wait_abandoned = u32::try_from(bindings::WAIT_ABANDONED).unwrap_or(0x80);
        let wait_timeout = u32::try_from(bindings::WAIT_TIMEOUT).unwrap_or(0x102);
        if wait_rc == wait_obj || wait_rc == wait_abandoned {
            Ok(Self { handle: guard })
        } else if wait_rc == wait_timeout {
            Err(CoreError::QuotaLockTimeout(
                "配额状态被其他进程占用超时，请稍后重试。".to_owned(),
            ))
        } else {
            Err(CoreError::Io(std::io::Error::last_os_error()))
        }
    }
}

impl Drop for NamedMutexLock {
    fn drop(&mut self) {
        if !self.handle.0.0.is_null() {
            // SAFETY: Current thread holds ownership of `self.handle.0` and releases before close.
            unsafe {
                let _ = bindings::ReleaseMutex(self.handle.0);
            }
        }
    }
}

/// Checks an `NTSTATUS` value and returns `CoreError::Crypto` on negative status codes.
///
/// # Errors
///
/// Returns `CoreError::Crypto` with hex status details when `status.is_err()`.
pub fn check_ntstatus(op: &str, status: windows_core::NTSTATUS) -> CoreResult<()> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(CoreError::Crypto(format!(
            "Windows CNG 操作 {op} 失败：NTSTATUS 0x{:08X}",
            status.0
        )))
    }
}
