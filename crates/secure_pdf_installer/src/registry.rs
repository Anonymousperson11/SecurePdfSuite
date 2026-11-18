//! Minimal `HKEY_CURRENT_USER` registry helpers (create / write / read back / delete).

use core::ptr::null_mut;

use windows_core::PCWSTR;

use crate::bindings;

const KEY_READ: u32 = 0x0002_0019;
const REG_DWORD: u32 = 4;
const REG_EXPAND_SZ: u32 = 2;
const ERROR_FILE_NOT_FOUND: i32 = 2;

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod ffi {
    use windows_core::PCWSTR;

    use crate::bindings::{HKEY, LSTATUS};

    windows_core::link!("advapi32.dll" "system" fn RegOpenKeyExW(
        hkey: HKEY,
        lpsubkey: PCWSTR,
        uloptions: u32,
        samdesired: u32,
        phkresult: *mut HKEY,
    ) -> LSTATUS);
    windows_core::link!("advapi32.dll" "system" fn RegQueryValueExW(
        hkey: HKEY,
        lpvaluename: PCWSTR,
        lpreserved: *const u32,
        lptype: *mut u32,
        lpdata: *mut u8,
        lpcbdata: *mut u32,
    ) -> LSTATUS);
    windows_core::link!("advapi32.dll" "system" fn RegDeleteKeyW(
        hkey: HKEY,
        lpsubkey: PCWSTR,
    ) -> LSTATUS);
}

struct RegKeyGuard(bindings::HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid registry key handle opened by this module.
            let _ = unsafe { bindings::RegCloseKey(self.0) };
            self.0 = bindings::HKEY(null_mut());
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

fn create_key(subkey: &str) -> Result<RegKeyGuard, String> {
    let w_subkey = to_wide_null(subkey);
    let mut hkey = bindings::HKEY(null_mut());
    let sam = bindings::ACCESS_MASK(u32::try_from(bindings::KEY_WRITE).unwrap_or(0x0002_0006));
    let opts = u32::try_from(bindings::REG_OPTION_NON_VOLATILE).unwrap_or(0);
    // SAFETY: Opens/creates an HKCU subkey for the current user without elevation.
    let rc = unsafe {
        bindings::RegCreateKeyExW(
            bindings::HKEY_CURRENT_USER,
            PCWSTR(w_subkey.as_ptr()),
            None,
            PCWSTR::null(),
            opts,
            sam,
            None,
            &raw mut hkey,
            None,
        )
    };
    if rc.0 != 0 || hkey.0.is_null() {
        return Err(format!("创建注册表项 {subkey} 失败（错误码 {}）。", rc.0));
    }
    Ok(RegKeyGuard(hkey))
}

fn open_key_read(subkey: &str) -> Option<RegKeyGuard> {
    let w_subkey = to_wide_null(subkey);
    let mut hkey = bindings::HKEY(null_mut());
    // SAFETY: Read-only open of an HKCU subkey; `hkey` receives the handle on success.
    let rc = unsafe {
        ffi::RegOpenKeyExW(
            bindings::HKEY_CURRENT_USER,
            PCWSTR(w_subkey.as_ptr()),
            0,
            KEY_READ,
            &raw mut hkey,
        )
    };
    (rc.0 == 0 && !hkey.0.is_null()).then(|| RegKeyGuard(hkey))
}

fn set_value(subkey: &str, name: &str, value_type: u32, data: &[u8]) -> Result<(), String> {
    let key = create_key(subkey)?;
    let w_name = to_wide_null(name);
    let name_ptr = if name.is_empty() {
        PCWSTR::null()
    } else {
        PCWSTR(w_name.as_ptr())
    };
    // SAFETY: Writes `data` as a value of `value_type` under the freshly opened key.
    let rc = unsafe { bindings::RegSetValueExW(key.0, name_ptr, None, value_type, Some(data)) };
    if rc.0 != 0 {
        return Err(format!(
            "写入注册表值 {subkey}\\{name} 失败（错误码 {}）。",
            rc.0
        ));
    }
    Ok(())
}

/// Writes a `REG_SZ` value under `HKCU\<subkey>` (an empty `name` is the default value).
///
/// # Errors
///
/// Returns a descriptive error if the key cannot be created or the value cannot be written.
pub fn set_hkcu_sz(subkey: &str, name: &str, value: &str) -> Result<(), String> {
    let data: Vec<u8> = to_wide_null(value)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    set_value(subkey, name, bindings::REG_SZ, &data)
}

/// Writes a `REG_DWORD` value under `HKCU\<subkey>`.
///
/// # Errors
///
/// Returns a descriptive error if the key cannot be created or the value cannot be written.
pub fn set_hkcu_dword(subkey: &str, name: &str, value: u32) -> Result<(), String> {
    set_value(subkey, name, REG_DWORD, &value.to_le_bytes())
}

fn query_value(subkey: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    let key = open_key_read(subkey)?;
    let w_name = to_wide_null(name);
    let name_ptr = if name.is_empty() {
        PCWSTR::null()
    } else {
        PCWSTR(w_name.as_ptr())
    };
    let mut value_type = 0_u32;
    let mut byte_len = 0_u32;
    // SAFETY: Size probe; only the type and required byte length are written.
    let rc = unsafe {
        ffi::RegQueryValueExW(
            key.0,
            name_ptr,
            core::ptr::null(),
            &raw mut value_type,
            null_mut(),
            &raw mut byte_len,
        )
    };
    if rc.0 != 0 {
        return None;
    }
    let mut data = vec![0_u8; usize::try_from(byte_len).ok()?];
    // SAFETY: `data` holds `byte_len` writable bytes as reported by the probe above.
    let rc = unsafe {
        ffi::RegQueryValueExW(
            key.0,
            name_ptr,
            core::ptr::null(),
            &raw mut value_type,
            data.as_mut_ptr(),
            &raw mut byte_len,
        )
    };
    if rc.0 != 0 {
        return None;
    }
    data.truncate(usize::try_from(byte_len).ok()?);
    Some((value_type, data))
}

/// Reads a string value under `HKCU\<subkey>` (an empty `name` is the default value).
#[must_use]
pub fn get_hkcu_sz(subkey: &str, name: &str) -> Option<String> {
    let (value_type, data) = query_value(subkey, name)?;
    if value_type != bindings::REG_SZ && value_type != REG_EXPAND_SZ {
        return None;
    }
    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]))
}

/// Reads a `REG_DWORD` value under `HKCU\<subkey>`.
#[must_use]
pub fn get_hkcu_dword(subkey: &str, name: &str) -> Option<u32> {
    let (value_type, data) = query_value(subkey, name)?;
    if value_type != REG_DWORD {
        return None;
    }
    Some(u32::from_le_bytes(
        <[u8; 4]>::try_from(data.as_slice()).ok()?,
    ))
}

/// Returns `true` when `HKCU\<subkey>` exists.
#[must_use]
pub fn hkcu_key_exists(subkey: &str) -> bool {
    open_key_read(subkey).is_some()
}

/// Deletes `HKCU\<subkey>` and everything below it; a missing key is not an error.
///
/// # Errors
///
/// Returns a descriptive error if the key exists but cannot be removed.
pub fn delete_hkcu_tree(subkey: &str) -> Result<(), String> {
    if subkey.trim_matches('\\').is_empty() {
        return Err("拒绝删除空的注册表路径。".to_owned());
    }
    let w_subkey = to_wide_null(subkey);
    // SAFETY: Removes all values and subkeys below the named HKCU subkey.
    let rc_tree =
        unsafe { bindings::RegDeleteTreeW(bindings::HKEY_CURRENT_USER, PCWSTR(w_subkey.as_ptr())) };
    if rc_tree.0 == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    // SAFETY: Removes the now-empty key itself.
    let rc_key =
        unsafe { ffi::RegDeleteKeyW(bindings::HKEY_CURRENT_USER, PCWSTR(w_subkey.as_ptr())) };
    if rc_tree.0 != 0 || (rc_key.0 != 0 && rc_key.0 != ERROR_FILE_NOT_FOUND) {
        return Err(format!(
            "删除注册表项 {subkey} 失败（错误码 {} / {}）。",
            rc_tree.0, rc_key.0
        ));
    }
    Ok(())
}

/// Removes `HKCU\<subkey>` only when it has no subkeys left (values are discarded).
///
/// Used to tidy the shared parent of throw-away sandbox keys without touching live siblings.
pub fn delete_hkcu_key_if_empty(subkey: &str) {
    if subkey.trim_matches('\\').is_empty() {
        return;
    }
    let w_subkey = to_wide_null(subkey);
    // SAFETY: `RegDeleteKeyW` is non-recursive and fails harmlessly when subkeys remain.
    let _ = unsafe { ffi::RegDeleteKeyW(bindings::HKEY_CURRENT_USER, PCWSTR(w_subkey.as_ptr())) };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ThrowAwayKey(String);

    impl Drop for ThrowAwayKey {
        fn drop(&mut self) {
            let _ = delete_hkcu_tree(&self.0);
            delete_hkcu_key_if_empty(r"Software\SecurePdfSuite_SelfTest");
        }
    }

    #[test]
    fn registry_values_read_back_exactly_and_tree_delete_leaves_nothing() {
        let root = ThrowAwayKey(format!(
            r"Software\SecurePdfSuite_SelfTest\unit-registry-{}",
            std::process::id()
        ));
        let sub = format!(r"{}\Uninstall\SecurePdfReader", root.0);

        assert!(!hkcu_key_exists(&sub));
        assert_eq!(get_hkcu_sz(&sub, "DisplayName"), None);

        set_hkcu_sz(&sub, "DisplayName", "SecurePdf 阅读端 (接收方)").unwrap();
        set_hkcu_sz(&sub, "", r#""D:\应用 100%\SecurePdfReader.exe" "%1""#).unwrap();
        set_hkcu_dword(&sub, "EstimatedSize", 8_421).unwrap();
        set_hkcu_dword(&sub, "NoModify", 1).unwrap();

        assert!(hkcu_key_exists(&sub));
        assert_eq!(
            get_hkcu_sz(&sub, "DisplayName").as_deref(),
            Some("SecurePdf 阅读端 (接收方)")
        );
        assert_eq!(
            get_hkcu_sz(&sub, "").as_deref(),
            Some(r#""D:\应用 100%\SecurePdfReader.exe" "%1""#)
        );
        assert_eq!(get_hkcu_dword(&sub, "EstimatedSize"), Some(8_421));
        assert_eq!(get_hkcu_dword(&sub, "NoModify"), Some(1));
        // Type confusion must not be silently accepted.
        assert_eq!(get_hkcu_dword(&sub, "DisplayName"), None);
        assert_eq!(get_hkcu_sz(&sub, "EstimatedSize"), None);

        delete_hkcu_tree(&root.0).unwrap();
        assert!(!hkcu_key_exists(&sub));
        assert!(!hkcu_key_exists(&root.0));
        // Deleting again is a no-op, and an empty path is refused outright.
        delete_hkcu_tree(&root.0).unwrap();
        assert!(delete_hkcu_tree("").is_err());
        assert!(delete_hkcu_tree(r"\").is_err());
    }
}
