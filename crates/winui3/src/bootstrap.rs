//! Unpackaged Windows App Runtime (`WinAppSDK`) dynamic dependency bootstrapper and STA init.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use windows_core::{HRESULT, PCSTR, PCWSTR, PWSTR};

use crate::bindings::Windows::Win32 as w32;
use crate::interop::{confirm_message_box, shell_open, to_wide_null};

/// `PackageDependencyProcessorArchitectures_X64` from the Windows SDK header `appmodel.h`.
pub const PACKAGE_DEPENDENCY_ARCH_X64: u32 = 0x0000_0004;
/// `PackageDependencyProcessorArchitectures_Arm64` from the Windows SDK header `appmodel.h`.
pub const PACKAGE_DEPENDENCY_ARCH_ARM64: u32 = 0x0000_0010;

/// Oldest Windows App Runtime release (major, minor) this application can run on.
pub const MIN_RUNTIME_VERSION: (u16, u16) = (1, 6);

/// File name of the optional redistributable bootstrapper shipped next to the executable.
pub const BOOTSTRAP_DLL_NAME: &str = "Microsoft.WindowsAppRuntime.Bootstrap.dll";

const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;
const E_NOT_FOUND: HRESULT = HRESULT(0x8007_0490_u32.cast_signed());

type RawProc = unsafe extern "system" fn() -> isize;

type TryCreatePackageDependencyFn = unsafe extern "system" fn(
    user: w32::PSID,
    package_family_name: PCWSTR,
    min_version: w32::PACKAGE_VERSION,
    architectures: u32,
    lifetime_kind: u32,
    lifetime_artifact: PCWSTR,
    options: u32,
    package_dependency_id: *mut PWSTR,
) -> HRESULT;

type AddPackageDependencyFn = unsafe extern "system" fn(
    package_dependency_id: PCWSTR,
    rank: i32,
    options: u32,
    context: *mut w32::PACKAGEDEPENDENCY_CONTEXT,
    package_full_name: *mut PWSTR,
) -> HRESULT;

type MddBootstrapInitialize2Fn = unsafe extern "system" fn(
    major_minor_version: u32,
    version_tag: PCWSTR,
    min_version: w32::PACKAGE_VERSION,
    options: u32,
) -> HRESULT;

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod ffi {
    use core::ffi::c_void;
    use windows_core::{BOOL, PCSTR, PCWSTR};

    windows_core::link!("kernel32.dll" "system" fn GetProcessHeap() -> *mut c_void);
    windows_core::link!("kernel32.dll" "system" fn HeapFree(
        hheap: *mut c_void,
        dwflags: u32,
        lpmem: *const c_void,
    ) -> BOOL);
    windows_core::link!("kernel32.dll" "system" fn GetModuleHandleW(
        lpmodulename: PCWSTR,
    ) -> *mut c_void);
    windows_core::link!("kernel32.dll" "system" fn LoadLibraryExW(
        lplibfilename: PCWSTR,
        hfile: *mut c_void,
        dwflags: u32,
    ) -> *mut c_void);
    windows_core::link!("kernel32.dll" "system" fn GetProcAddress(
        hmodule: *mut c_void,
        lpprocname: PCSTR,
    ) -> Option<super::RawProc>);
    windows_core::link!("kernel32.dll" "system" fn SetDefaultDllDirectories(
        directoryflags: u32,
    ) -> BOOL);
    windows_core::link!("kernel32.dll" "system" fn GetPackagesByPackageFamily(
        packagefamilyname: PCWSTR,
        count: *mut u32,
        packagefullnames: *mut *mut u16,
        bufferlength: *mut u32,
        buffer: *mut u16,
    ) -> i32);
    windows_core::link!("kernel32.dll" "system" fn GetModuleFileNameW(
        hmodule: *mut c_void,
        lpfilename: *mut u16,
        nsize: u32,
    ) -> u32);
}

enum RuntimeAttachment {
    // Keeps the OS dynamic dependency alive for the process lifetime.
    OsDynamicDependency(#[allow(dead_code)] w32::PACKAGEDEPENDENCY_CONTEXT),
    // The redistributable bootstrapper DLL stays loaded for the process lifetime.
    BootstrapDll,
}
// SAFETY: The dependency context is an opaque process-wide handle that is stored once and never
// dereferenced by this crate.
unsafe impl Send for RuntimeAttachment {}

static RUNTIME_ATTACHMENT: OnceLock<Mutex<Option<RuntimeAttachment>>> = OnceLock::new();

struct HeapPwstrGuard(*mut u16);

impl Drop for HeapPwstrGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Process heap handle retrieval is always safe for the current process.
            let heap = unsafe { ffi::GetProcessHeap() };
            if !heap.is_null() {
                // SAFETY: Frees PWSTR returned by Windows AppModel runtime APIs via `HeapFree`.
                unsafe {
                    let _ = ffi::HeapFree(heap, 0, self.0.cast::<c_void>());
                }
            }
        }
    }
}

/// Restricts run-time DLL lookups to System32 and explicitly registered directories.
///
/// Removes the application directory and the current directory from the default search path so
/// a DLL planted next to the executable (for example in the Downloads folder) cannot be picked
/// up by a later `LoadLibrary` call. Libraries shipped with the application must therefore be
/// loaded by absolute path. Statically imported DLLs are covered separately by the
/// `/DEPENDENTLOADFLAG` linker option. Call this first thing in `main`; it is idempotent.
pub fn harden_dll_search_path() {
    const LOAD_LIBRARY_SEARCH_USER_DIRS: u32 = 0x0000_0400;
    // SAFETY: Process-wide loader policy change with documented flag values; no pointers.
    unsafe {
        let _ = ffi::SetDefaultDllDirectories(
            LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_USER_DIRS,
        );
    }
}

/// Checks if the current process is already running inside an MSIX package identity.
#[must_use]
pub fn is_packaged_process() -> bool {
    let mut len = 0_u32;
    // SAFETY: Probes current package full name length (`APPMODEL_ERROR_NO_PACKAGE` for unpackaged).
    let rc = unsafe { w32::GetCurrentPackageFullName(&raw mut len, None) };
    rc != w32::APPMODEL_ERROR_NO_PACKAGE
}

/// Returns the `PackageDependencyProcessorArchitectures` flag matching this build's target.
#[must_use]
pub const fn current_package_dependency_architecture() -> u32 {
    if cfg!(target_arch = "aarch64") {
        PACKAGE_DEPENDENCY_ARCH_ARM64
    } else {
        PACKAGE_DEPENDENCY_ARCH_X64
    }
}

/// Windows App Runtime releases probed in order of preference: (package family, major, minor).
const RUNTIME_CANDIDATES: &[(&str, u16, u16)] = &[
    ("Microsoft.WindowsAppRuntime.2_8wekyb3d8bbwe", 2, 0),
    ("Microsoft.WindowsAppRuntime.1.8_8wekyb3d8bbwe", 1, 8),
    ("Microsoft.WindowsAppRuntime.1.7_8wekyb3d8bbwe", 1, 7),
    ("Microsoft.WindowsAppRuntime.1.6_8wekyb3d8bbwe", 1, 6),
];

/// Full names of the packages of `family_name` registered for the current user.
fn installed_package_full_names(family_name: &str) -> Vec<String> {
    const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
    let family_w = to_wide_null(family_name);
    let mut count = 0_u32;
    let mut buffer_len = 0_u32;
    // SAFETY: Size probe with null output buffers; only `count` and `buffer_len` are written.
    let rc = unsafe {
        ffi::GetPackagesByPackageFamily(
            PCWSTR(family_w.as_ptr()),
            &raw mut count,
            null_mut(),
            &raw mut buffer_len,
            null_mut(),
        )
    };
    if rc != ERROR_INSUFFICIENT_BUFFER || count == 0 || buffer_len == 0 {
        return Vec::new();
    }
    let (Ok(name_slots), Ok(buffer_units)) = (usize::try_from(count), usize::try_from(buffer_len))
    else {
        return Vec::new();
    };
    let mut names: Vec<*mut u16> = vec![null_mut(); name_slots];
    let mut buffer = vec![0_u16; buffer_units];
    // SAFETY: `names` has `count` slots and `buffer` has `buffer_len` UTF-16 units, exactly as
    // reported by the probe; the API fills `names` with pointers into `buffer`.
    let rc = unsafe {
        ffi::GetPackagesByPackageFamily(
            PCWSTR(family_w.as_ptr()),
            &raw mut count,
            names.as_mut_ptr(),
            &raw mut buffer_len,
            buffer.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Vec::new();
    }
    // The buffer holds consecutive NUL-terminated full names.
    buffer
        .split(|&unit| unit == 0)
        .filter(|name| !name.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Newest installed Windows App Runtime release `(major, minor)` this application can use.
///
/// Only inspects the packages registered for the current user (matching this build's
/// architecture); nothing is attached to the process. Installers use it to tell the user up
/// front that the runtime is missing.
#[must_use]
pub fn find_installed_runtime() -> Option<(u16, u16)> {
    let arch_tag = if cfg!(target_arch = "aarch64") {
        "_arm64__"
    } else {
        "_x64__"
    };
    RUNTIME_CANDIDATES
        .iter()
        .find(|&&(family, _, _)| {
            installed_package_full_names(family)
                .iter()
                .any(|full_name| full_name.to_ascii_lowercase().contains(arch_tag))
        })
        .map(|&(_, major, minor)| (major, minor))
}

/// Dynamically attaches Windows App Runtime package dependency for unpackaged desktop EXEs.
///
/// Windows 11 exposes the dynamic dependency API in the OS; it is resolved at run time so that the
/// executable still loads on Windows 10, where the redistributable
/// `Microsoft.WindowsAppRuntime.Bootstrap.dll` next to the executable is used instead.
///
/// # Errors
///
/// Returns an `HRESULT` error if no compatible Windows App Runtime framework package is installed
/// or no bootstrap mechanism is available on this system.
pub fn bootstrap_runtime() -> windows_core::Result<()> {
    if is_packaged_process() {
        return Ok(());
    }

    let slot = RUNTIME_ATTACHMENT.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = slot.lock()
        && guard.is_some()
    {
        return Ok(());
    }

    let attachment = attach_runtime()?;
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(attachment);
    }
    Ok(())
}

fn attach_runtime() -> windows_core::Result<RuntimeAttachment> {
    let mut last_err = None;

    let force_bootstrap_dll =
        cfg!(debug_assertions) && std::env::var_os("SECUREPDF_DEBUG_FORCE_BOOTSTRAP_DLL").is_some();
    if !force_bootstrap_dll && let Some(api) = OsDynamicDependencyApi::resolve() {
        let arch = current_package_dependency_architecture();
        for &(family_name, major, minor) in RUNTIME_CANDIDATES {
            match api.attach(family_name, major, minor, arch) {
                Ok(ctx) => return Ok(RuntimeAttachment::OsDynamicDependency(ctx)),
                Err(err) => last_err = Some(err),
            }
        }
    }

    if let Some(initialize) = resolve_bootstrap_dll_initialize() {
        for &(_, major, minor) in RUNTIME_CANDIDATES {
            match bootstrap_dll_attach(initialize, major, minor) {
                Ok(()) => return Ok(RuntimeAttachment::BootstrapDll),
                Err(err) => last_err = Some(err),
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        windows_core::Error::new(E_NOT_FOUND, "Windows App Runtime package was not found")
    }))
}

const fn package_version(major: u16, minor: u16) -> w32::PACKAGE_VERSION {
    w32::PACKAGE_VERSION {
        Anonymous: w32::PACKAGE_VERSION_0 {
            Anonymous: w32::PACKAGE_VERSION_0_0 {
                Major: major,
                Minor: minor,
                Build: 0,
                Revision: 0,
            },
        },
    }
}

fn resolve_proc(module: *mut c_void, name: &'static [u8]) -> Option<RawProc> {
    if module.is_null() || name.last() != Some(&0) {
        return None;
    }
    // SAFETY: `module` is a loaded module handle and `name` is a NUL-terminated ANSI export name.
    unsafe { ffi::GetProcAddress(module, PCSTR(name.as_ptr())) }
}

/// Windows 11 (build 22000+) dynamic dependency entry points, resolved without a static import so
/// the process image still loads on Windows 10.
#[derive(Clone, Copy)]
struct OsDynamicDependencyApi {
    try_create: TryCreatePackageDependencyFn,
    add: AddPackageDependencyFn,
}

impl OsDynamicDependencyApi {
    fn resolve() -> Option<Self> {
        let kernelbase = to_wide_null("kernelbase.dll");
        // SAFETY: `kernelbase.dll` is mapped into every Win32 process; the name is NUL-terminated.
        let mut module = unsafe { ffi::GetModuleHandleW(PCWSTR(kernelbase.as_ptr())) };
        if resolve_proc(module, b"TryCreatePackageDependency\0").is_none() {
            let api_set = to_wide_null("api-ms-win-appmodel-runtime-l1-1-5.dll");
            // SAFETY: Resolves the API set strictly from the system directory; a missing API set
            // (Windows 10) yields NULL.
            module = unsafe {
                ffi::LoadLibraryExW(
                    PCWSTR(api_set.as_ptr()),
                    null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
        }
        let try_create = resolve_proc(module, b"TryCreatePackageDependency\0")?;
        let add = resolve_proc(module, b"AddPackageDependency\0")?;
        Some(Self {
            // SAFETY: The export has exactly the `TryCreatePackageDependency` prototype declared
            // in `appmodel.h`; only the function pointer type is reinterpreted.
            try_create: unsafe {
                std::mem::transmute::<RawProc, TryCreatePackageDependencyFn>(try_create)
            },
            // SAFETY: The export has exactly the `AddPackageDependency` prototype declared in
            // `appmodel.h`; only the function pointer type is reinterpreted.
            add: unsafe { std::mem::transmute::<RawProc, AddPackageDependencyFn>(add) },
        })
    }

    fn attach(
        self,
        family_name: &str,
        major: u16,
        minor: u16,
        arch: u32,
    ) -> windows_core::Result<w32::PACKAGEDEPENDENCY_CONTEXT> {
        let family_w = to_wide_null(family_name);
        let mut dep_id = PWSTR::null();
        // SAFETY: Creates a process-lifetime dynamic dependency on installed Windows App Runtime;
        // all pointers reference live locals.
        let hr = unsafe {
            (self.try_create)(
                w32::PSID(null_mut()),
                PCWSTR(family_w.as_ptr()),
                package_version(major, minor),
                arch,
                0, // PackageDependencyLifetimeKind_Process
                PCWSTR::null(),
                0, // CreatePackageDependencyOptions_None
                &raw mut dep_id,
            )
        };
        let _dep_id_guard = HeapPwstrGuard(dep_id.0);
        hr.ok()?;

        let mut ctx = w32::PACKAGEDEPENDENCY_CONTEXT(null_mut());
        let mut full_name = PWSTR::null();
        // SAFETY: Adds the created dependency into this process package graph.
        let hr = unsafe {
            (self.add)(
                PCWSTR(dep_id.0.cast_const()),
                0,
                0, // AddPackageDependencyOptions_None
                &raw mut ctx,
                &raw mut full_name,
            )
        };
        let _full_name_guard = HeapPwstrGuard(full_name.0);
        hr.ok()?;
        Ok(ctx)
    }
}

/// Full path of the redistributable bootstrapper expected next to the running executable.
fn bootstrap_dll_path() -> Option<Vec<u16>> {
    let mut buf = vec![0_u16; 1024];
    let cap = u32::try_from(buf.len()).ok()?;
    // SAFETY: Writes at most `cap` UTF-16 units of the current executable path into `buf`.
    let len = unsafe { ffi::GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), cap) };
    let len = usize::try_from(len).ok()?;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let exe = String::from_utf16(&buf[..len]).ok()?;
    let dir_end = exe.rfind('\\')?;
    let full = format!("{}\\{BOOTSTRAP_DLL_NAME}", &exe[..dir_end]);
    std::path::Path::new(&full)
        .is_file()
        .then(|| to_wide_null(&full))
}

fn resolve_bootstrap_dll_initialize() -> Option<MddBootstrapInitialize2Fn> {
    let path = bootstrap_dll_path()?;
    // SAFETY: Loads the bootstrapper by absolute path from the application directory only.
    let module = unsafe {
        ffi::LoadLibraryExW(
            PCWSTR(path.as_ptr()),
            null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    let raw = resolve_proc(module, b"MddBootstrapInitialize2\0")?;
    // SAFETY: The export has exactly the `MddBootstrapInitialize2` prototype declared in
    // `MddBootstrap.h`; only the function pointer type is reinterpreted.
    Some(unsafe { std::mem::transmute::<RawProc, MddBootstrapInitialize2Fn>(raw) })
}

fn bootstrap_dll_attach(
    initialize: MddBootstrapInitialize2Fn,
    major: u16,
    minor: u16,
) -> windows_core::Result<()> {
    let major_minor = (u32::from(major) << 16) | u32::from(minor);
    let version_tag = [0_u16; 1];
    // SAFETY: Calls `MddBootstrapInitialize2` with an empty (stable channel) version tag, no
    // minimum build constraint and `MddBootstrapInitializeOptions_None`.
    let hr = unsafe {
        initialize(
            major_minor,
            PCWSTR(version_tag.as_ptr()),
            package_version(0, 0),
            0,
        )
    };
    hr.ok()
}

/// Configures Per-Monitor V2 DPI awareness and initializes STA COM on the current UI thread.
///
/// # Errors
///
/// Returns `RPC_E_CHANGED_MODE` or `HRESULT` failure if STA COM initialization fails.
pub fn initialize_ui_thread() -> windows_core::Result<()> {
    // SAFETY: Enables sharp Per-Monitor V2 DPI scaling prior to creating any WinUI window.
    unsafe {
        let _ = w32::SetProcessDpiAwarenessContext(w32::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // SAFETY: WinUI 3 requires single-threaded apartment (`COINIT_APARTMENTTHREADED`) on UI thread.
    let hr = unsafe {
        w32::CoInitializeEx(
            None,
            u32::try_from(w32::COINIT_APARTMENTTHREADED).unwrap_or(2),
        )
    };
    if hr == w32::RPC_E_CHANGED_MODE {
        return Err(windows_core::Error::new(
            w32::RPC_E_CHANGED_MODE,
            "WinUI 3 requires an STA UI thread",
        ));
    }
    hr.ok()
}

/// Posts `WM_QUIT` cleanly to terminate the active `WinUI` 3 message loop.
pub fn exit_ui_thread() {
    // SAFETY: Posts standard `WM_QUIT` message to current UI thread queue.
    unsafe { w32::PostQuitMessage(0) };
}

/// Builds the user-facing explanation shown when Windows App Runtime cannot be attached.
#[must_use]
pub fn missing_runtime_message(app_title: &str, err: &windows_core::Error) -> String {
    let (major, minor) = MIN_RUNTIME_VERSION;
    format!(
        "无法启动 {app_title}：这台电脑尚未安装所需的 Microsoft Windows App Runtime 运行库。\n\n\
         需要的版本：Windows App Runtime {major}.{minor} 或更高版本（x64）。\n\
         错误代码：0x{:08X}\n\n\
         是否现在打开微软官方下载页面？安装运行库后重新启动本程序即可。",
        err.code().0.cast_unsigned()
    )
}

/// Displays an actionable native dialog when Windows App Runtime is not installed on the system.
pub fn show_missing_runtime_prompt(app_title: &str, err: &windows_core::Error) {
    let message = missing_runtime_message(app_title, err);
    if confirm_message_box(w32::HWND(null_mut()), app_title, &message) {
        shell_open("https://learn.microsoft.com/zh-cn/windows/apps/windows-app-sdk/downloads");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_dependency_architecture_constants_match_windows_sdk_appmodel_h() {
        // appmodel.h: None=0, Neutral=1, X86=2, X64=4, Arm=8, Arm64=0x10, X86A64=0x20.
        assert_eq!(PACKAGE_DEPENDENCY_ARCH_X64, 0x4);
        assert_eq!(PACKAGE_DEPENDENCY_ARCH_ARM64, 0x10);
        let expected = if cfg!(target_arch = "aarch64") {
            0x10
        } else {
            0x4
        };
        assert_eq!(current_package_dependency_architecture(), expected);
    }

    #[test]
    fn runtime_candidates_respect_minimum_supported_version_and_family_naming() {
        for &(family, major, minor) in RUNTIME_CANDIDATES {
            assert!((major, minor) >= MIN_RUNTIME_VERSION);
            assert!(family.starts_with("Microsoft.WindowsAppRuntime."));
            assert!(family.ends_with("_8wekyb3d8bbwe"));
        }
    }

    /// Exercises the Windows 10 code path on any machine: attaches the runtime through the
    /// redistributable bootstrapper instead of the Windows 11 OS API.
    ///
    /// Requires the full Windows App Runtime (official installer, which adds the Main and DDLM
    /// packages); framework-only installs pulled in by Store apps fail with `0x80670016`.
    #[test]
    #[ignore = "diagnostic; needs Microsoft.WindowsAppRuntime.Bootstrap.dll next to the test exe"]
    fn bootstrap_dll_fallback_attaches_an_installed_runtime() {
        harden_dll_search_path();
        let initialize = resolve_bootstrap_dll_initialize().unwrap();
        let mut attached = None;
        for &(_, major, minor) in RUNTIME_CANDIDATES {
            let res = bootstrap_dll_attach(initialize, major, minor);
            println!("MDD_BOOTSTRAP {major}.{minor}: {res:?}");
            if res.is_ok() {
                attached = Some((major, minor));
                break;
            }
        }
        assert!(attached.is_some());
    }

    #[test]
    fn runtime_probe_ignores_unknown_families_and_agrees_with_real_bootstrap() {
        assert_eq!(
            installed_package_full_names("SecurePdfSuite.NoSuchFramework_8wekyb3d8bbwe"),
            Vec::<String>::new()
        );
        assert_eq!(installed_package_full_names(""), Vec::<String>::new());
        // If the dynamic dependency can really be attached, the probe must also see a runtime.
        if !is_packaged_process() && bootstrap_runtime().is_ok() {
            let found = find_installed_runtime();
            assert!(found.is_some_and(|version| version >= MIN_RUNTIME_VERSION));
        }
        if let Some((major, minor)) = find_installed_runtime() {
            assert!(
                RUNTIME_CANDIDATES
                    .iter()
                    .any(|&(_, m, n)| (m, n) == (major, minor))
            );
        }
    }

    #[test]
    fn missing_runtime_message_names_required_version_without_raw_english_error() {
        let err = windows_core::Error::new(E_NOT_FOUND, "Element not found.");
        let text = missing_runtime_message("SecurePdf 阅读端", &err);
        assert!(text.contains("Windows App Runtime 1.6"));
        assert!(text.contains("0x80070490"));
        assert!(!text.contains("Element not found"));
    }
}
