//! Native Win32 installation primitives: process handling, resilient file placement, shortcuts,
//! `.spdf` association, "Apps & features" registration, v1.x upgrade handling and the uninstall
//! script.

use core::mem::size_of;
use core::ptr::null_mut;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_core::{Interface, PCWSTR, PWSTR};

use crate::bindings;
use crate::logging::{local_date_compact, local_timestamp};
use crate::package::{
    AppKind, ArchiveEntry, ensure_main_executable, ensure_no_case_insensitive_duplicates,
    sanitize_relative_path,
};
use crate::registry::{get_hkcu_sz, hkcu_key_exists, set_hkcu_dword, set_hkcu_sz};
use crate::uninstall::{AssociationRemoval, UninstallPlan, build_uninstall_script};

const MAX_PATH_BUF: usize = 512;

/// Start Menu program group shared by both products (same as v1.x).
pub const START_MENU_FOLDER: &str = "SecurePdfSuite";
/// Protected document file extension.
pub const SPDF_EXTENSION: &str = ".spdf";
/// `ProgID` of protected documents (same as v1.x).
pub const SPDF_PROG_ID: &str = "SecurePdfSuite.SpdfFile";
/// `ProgID` written by the v1.0.x association helper script; removed on uninstall.
pub const LEGACY_SPDF_PROG_ID: &str = "SecurePdfSuite.spdf";
/// Friendly type name shown by Explorer for `.spdf` files.
pub const SPDF_TYPE_DESCRIPTION: &str = "SecurePdf 加密保护文档";
/// Publisher shown in "Apps & features".
pub const PUBLISHER: &str = "SecurePdfSuite";
/// Product version written to the uninstall entry.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Install log written into the installation directory.
pub const INSTALL_LOG_NAME: &str = "install.log";

const STAGING_SUFFIX: &str = ".spdf-install-tmp";

/// Files shipped by v1.x that the Rust rewrite no longer uses; removed when upgrading in place.
#[must_use]
pub fn obsolete_file_names(app_kind: AppKind) -> Vec<String> {
    vec![
        "BouncyCastle.Cryptography.dll".to_owned(),
        "SecurePdf.Core.dll".to_owned(),
        format!("{}.config", app_kind.exe_name()),
    ]
}

/// Where an installation writes its side effects; the self-test swaps in a private sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallEnvironment {
    /// HKCU subkey used as `Software\Classes`.
    pub classes_root: String,
    /// HKCU subkey used as `Software\Microsoft\Windows\CurrentVersion\Uninstall`.
    pub uninstall_root: String,
    /// Desktop folder for shortcuts (`None` if it cannot be resolved).
    pub desktop_dir: Option<PathBuf>,
    /// Start Menu "Programs" folder for shortcuts (`None` if it cannot be resolved).
    pub programs_dir: Option<PathBuf>,
    /// `%LOCALAPPDATA%`, base of the default and the v1.x install locations.
    pub local_app_data: Option<PathBuf>,
    /// `true` inside the self-test sandbox: no shell notifications and no kill-by-image-name.
    pub sandboxed: bool,
}

impl InstallEnvironment {
    /// The real per-user environment of the current account.
    #[must_use]
    pub fn real() -> Self {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|profile| PathBuf::from(profile).join("AppData").join("Local"))
            });
        Self {
            classes_root: r"Software\Classes".to_owned(),
            uninstall_root: r"Software\Microsoft\Windows\CurrentVersion\Uninstall".to_owned(),
            desktop_dir: query_shell_folder(bindings::CSIDL_DESKTOPDIRECTORY),
            programs_dir: query_shell_folder(bindings::CSIDL_PROGRAMS),
            local_app_data,
            sandboxed: false,
        }
    }

    /// A fully redirected environment: folders below `root_dir`, registry below the throw-away
    /// HKCU subkey `registry_root`. Nothing outside those two locations is touched.
    #[must_use]
    pub fn sandbox(root_dir: &Path, registry_root: &str) -> Self {
        Self {
            classes_root: format!(r"{registry_root}\Classes"),
            uninstall_root: format!(r"{registry_root}\Uninstall"),
            desktop_dir: Some(root_dir.join("Desktop")),
            programs_dir: Some(root_dir.join("StartMenu").join("Programs")),
            local_app_data: Some(root_dir.join("LocalAppData")),
            sandboxed: true,
        }
    }

    /// HKCU subkey of the "Apps & features" entry for `app_kind`.
    #[must_use]
    pub fn uninstall_key(&self, app_kind: AppKind) -> String {
        format!(r"{}\{}", self.uninstall_root, app_kind.uninstall_key_name())
    }

    /// Desktop shortcut path for `app_kind`, if the Desktop folder is known.
    #[must_use]
    pub fn desktop_shortcut(&self, app_kind: AppKind) -> Option<PathBuf> {
        self.desktop_dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.lnk", app_kind.shortcut_name())))
    }

    /// Start Menu program group folder, if the Programs folder is known.
    #[must_use]
    pub fn start_menu_dir(&self) -> Option<PathBuf> {
        self.programs_dir
            .as_ref()
            .map(|dir| dir.join(START_MENU_FOLDER))
    }

    /// Start Menu shortcut path for `app_kind`, if the Programs folder is known.
    #[must_use]
    pub fn start_menu_shortcut(&self, app_kind: AppKind) -> Option<PathBuf> {
        self.start_menu_dir()
            .map(|dir| dir.join(format!("{}.lnk", app_kind.shortcut_name())))
    }
}

/// Options controlling silent or interactive installation.
#[derive(Clone, Debug)]
pub struct InstallOptions {
    /// Target directory (already validated by [`crate::paths::normalize_install_dir`]).
    pub install_dir: PathBuf,
    /// Create a desktop `.lnk` shortcut.
    pub create_desktop_shortcut: bool,
    /// Create a Start Menu Programs shortcut.
    pub create_start_menu_shortcut: bool,
    /// Associate `.spdf` files with `SecurePdfReader.exe` (ignored for the Creator).
    pub associate_spdf: bool,
}

/// Result report returned after completing an installation.
#[derive(Clone, Debug)]
pub struct InstallSummary {
    /// Target installation directory.
    pub install_dir: PathBuf,
    /// Main installed executable path.
    pub main_exe: PathBuf,
    /// Number of extracted payload files.
    pub files_installed: usize,
    /// Paths of `.lnk` shortcuts created.
    pub shortcuts_created: Vec<PathBuf>,
    /// Generated per-file uninstall batch script.
    pub uninstall_script: PathBuf,
    /// Non-fatal problems (shortcut / registry steps that could not be completed).
    pub warnings: Vec<String>,
    /// Version of the installation that was replaced, if one was detected.
    pub replaced_version: Option<String>,
}

/// A previously installed copy of the product found on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExistingInstall {
    /// Directory containing the installed main executable.
    pub dir: PathBuf,
    /// `DisplayVersion` from its uninstall entry, when registered.
    pub version: Option<String>,
}

/// How persistently file operations are retried while a file is briefly locked.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    /// Total attempts per file.
    pub attempts: u32,
    /// Pause between attempts.
    pub delay: Duration,
}

impl RetryPolicy {
    /// v1.x behaviour: 15 attempts, 200 ms apart (a just-terminated process or an antivirus
    /// scan may hold files for a moment).
    pub const DEFAULT: Self = Self {
        attempts: 15,
        delay: Duration::from_millis(200),
    };
}

struct HandleGuard(bindings::HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        let invalid = (-1_isize) as *mut core::ffi::c_void;
        if !self.0.0.is_null() && self.0.0 != invalid {
            // SAFETY: Valid Win32 kernel handle opened in this scope.
            let _ = unsafe { bindings::CloseHandle(self.0) };
            self.0 = bindings::HANDLE(null_mut());
        }
    }
}

struct ComApartmentGuard(bool);
impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Matches successful `CoInitializeEx` on this thread.
            unsafe { bindings::CoUninitialize() };
            self.0 = false;
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------------------------
// Existing installation detection
// ---------------------------------------------------------------------------------------------

/// Finds an installed copy of `app_kind`: the registered uninstall location first, then the
/// v1.x default (`%LOCALAPPDATA%\<App>`), then the current default
/// (`%LOCALAPPDATA%\Programs\<App>`).
#[must_use]
pub fn detect_existing_install(
    app_kind: AppKind,
    env: &InstallEnvironment,
) -> Option<ExistingInstall> {
    let key = env.uninstall_key(app_kind);
    let registered_dir = get_hkcu_sz(&key, "InstallLocation")
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let version = get_hkcu_sz(&key, "DisplayVersion").filter(|v| !v.trim().is_empty());

    let mut candidates: Vec<(PathBuf, bool)> = Vec::new();
    if let Some(dir) = registered_dir {
        candidates.push((dir, true));
    }
    if let Some(local) = &env.local_app_data {
        candidates.push((local.join(app_kind.install_folder_name()), false));
        candidates.push((
            local.join("Programs").join(app_kind.install_folder_name()),
            false,
        ));
    }
    candidates
        .into_iter()
        .find(|(dir, _)| dir.join(app_kind.exe_name()).is_file())
        .map(|(dir, registered)| ExistingInstall {
            dir,
            version: if registered { version.clone() } else { None },
        })
}

/// Default installation directory: the directory of an existing installation (so upgrades
/// replace it in place), otherwise `%LOCALAPPDATA%\Programs\<App>`.
#[must_use]
pub fn default_install_dir(app_kind: AppKind, env: &InstallEnvironment) -> PathBuf {
    if let Some(existing) = detect_existing_install(app_kind, env) {
        return existing.dir;
    }
    env.local_app_data.as_ref().map_or_else(
        || PathBuf::from(r"C:\SecurePdfSuite").join(app_kind.install_folder_name()),
        |local| local.join("Programs").join(app_kind.install_folder_name()),
    )
}

// ---------------------------------------------------------------------------------------------
// Running processes
// ---------------------------------------------------------------------------------------------

fn for_each_matching_process(
    target_exe: &Path,
    extra_access: u32,
    mut on_match: impl FnMut(bindings::HANDLE),
) -> Result<(), String> {
    let flags = u32::try_from(bindings::TH32CS_SNAPPROCESS).unwrap_or(2);
    // SAFETY: Creates a system-wide process snapshot handle.
    let snap = unsafe { bindings::CreateToolhelp32Snapshot(flags, 0) };
    let invalid = (-1_isize) as *mut core::ffi::c_void;
    if snap.0.is_null() || snap.0 == invalid {
        return Err("创建进程快照失败。".to_owned());
    }
    let snap_guard = HandleGuard(snap);

    let target_norm = normalize_win_path(target_exe);
    let self_pid = std::process::id();
    let mut entry = bindings::PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<bindings::PROCESSENTRY32W>()).unwrap_or(0),
        ..Default::default()
    };
    let access =
        u32::try_from(bindings::PROCESS_QUERY_LIMITED_INFORMATION).unwrap_or(0x1000) | extra_access;

    // SAFETY: Valid snapshot handle and sized `PROCESSENTRY32W` output buffer.
    let mut has_next = unsafe { bindings::Process32FirstW(snap_guard.0, &raw mut entry) }.as_bool();
    while has_next {
        let pid = entry.th32ProcessID;
        if pid != 0 && pid != self_pid {
            // SAFETY: Opens the candidate PID with the minimum rights needed to inspect it.
            let proc_handle = unsafe { bindings::OpenProcess(access, false, pid) };
            if !proc_handle.0.is_null() {
                let proc_guard = HandleGuard(proc_handle);
                if process_image_matches(proc_guard.0, &target_norm) {
                    on_match(proc_guard.0);
                }
            }
        }
        // SAFETY: Advances snapshot iteration using initialized `PROCESSENTRY32W`.
        has_next = unsafe { bindings::Process32NextW(snap_guard.0, &raw mut entry) }.as_bool();
    }
    Ok(())
}

/// Returns `true` when a process whose image is exactly `target_exe` is running.
#[must_use]
pub fn is_target_running(target_exe: &Path) -> bool {
    let mut found = false;
    let _ = for_each_matching_process(target_exe, 0, |_| found = true);
    found
}

/// Terminates any running processes whose full executable path matches `target_exe`.
///
/// Returns how many matching processes were terminated.
///
/// # Errors
///
/// Returns an error if process enumeration snapshot creation fails.
pub fn terminate_running_instances(target_exe: &Path) -> Result<usize, String> {
    let terminate_access =
        u32::try_from(bindings::PROCESS_TERMINATE | bindings::SYNCHRONIZE).unwrap_or(0x0010_0001);
    let mut terminated = 0_usize;
    for_each_matching_process(target_exe, terminate_access, |handle| {
        // SAFETY: Terminates the exactly matched executable before its files are replaced.
        let ok = unsafe { bindings::TerminateProcess(handle, 1) }.as_bool();
        if ok {
            // SAFETY: Waits briefly for file handle release on the terminated process.
            let _ = unsafe { bindings::WaitForSingleObject(handle, 5000) };
            terminated += 1;
        }
    })?;
    Ok(terminated)
}

fn process_image_matches(proc_handle: bindings::HANDLE, target_norm: &str) -> bool {
    let mut buf = vec![0_u16; MAX_PATH_BUF];
    let mut len = u32::try_from(buf.len()).unwrap_or(512);
    // SAFETY: Queries Win32 image path into `buf` with updated `len`.
    let ok = unsafe {
        bindings::QueryFullProcessImageNameW(proc_handle, 0, PWSTR(buf.as_mut_ptr()), &raw mut len)
    }
    .as_bool();
    if !ok || len == 0 {
        return false;
    }
    let slice = &buf[..(len as usize).min(buf.len())];
    let image = String::from_utf16_lossy(slice);
    normalize_win_path(Path::new(&image)) == target_norm
}

fn normalize_win_path(p: &Path) -> String {
    let canonical = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = canonical.to_string_lossy();
    let trimmed = s.strip_prefix(r"\\?\").unwrap_or(&s);
    trimmed.replace('/', "\\").to_lowercase()
}

// ---------------------------------------------------------------------------------------------
// File placement
// ---------------------------------------------------------------------------------------------

fn clear_read_only(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
}

/// Runs `op` until it succeeds or the policy is exhausted, clearing a read-only attribute on
/// `blocking` between attempts.
fn with_retries<T>(
    policy: RetryPolicy,
    blocking: &Path,
    mut op: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let attempts = policy.attempts.max(1);
    let mut attempt = 0_u32;
    loop {
        attempt += 1;
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if attempt >= attempts => return Err(err),
            Err(_) => {
                clear_read_only(blocking);
                std::thread::sleep(policy.delay);
            }
        }
    }
}

fn staging_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(STAGING_SUFFIX);
    target.with_file_name(name)
}

struct PlacedFile {
    staged: PathBuf,
    target: PathBuf,
    existed_before: bool,
    committed: bool,
}

/// Undoes a partially performed extraction: staged files always go, committed files only when
/// they did not exist before (an overwritten file cannot be restored).
fn roll_back(placed: &[PlacedFile], created_dirs: &[PathBuf]) {
    for file in placed {
        let _ = fs::remove_file(&file.staged);
        if file.committed && !file.existed_before {
            let _ = fs::remove_file(&file.target);
        }
    }
    // Deepest first; `remove_dir` only removes empty directories.
    for dir in created_dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }
}

fn create_dir_tracked(dir: &Path, created: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut cursor = Some(dir);
    while let Some(current) = cursor {
        if current.as_os_str().is_empty() || current.is_dir() {
            break;
        }
        missing.push(current.to_path_buf());
        cursor = current.parent();
    }
    fs::create_dir_all(dir)?;
    for path in missing.into_iter().rev() {
        if !created.contains(&path) {
            created.push(path);
        }
    }
    Ok(())
}

/// Places all entries under `install_dir`.
///
/// Every file is first written next to its destination under a temporary name and only then
/// moved into place, so a failure while writing leaves an existing installation untouched.
/// Locked or read-only destinations are retried per `policy`. If placement still fails, files
/// and directories that did not exist before are removed again.
///
/// # Errors
///
/// Returns an error if a path is unsafe, a directory cannot be created, or a file cannot be
/// written after all retries.
pub fn extract_entries_to_dir(
    install_dir: &Path,
    entries: &[ArchiveEntry],
    policy: RetryPolicy,
) -> Result<Vec<PathBuf>, String> {
    ensure_no_case_insensitive_duplicates(entries)?;
    let mut created_dirs: Vec<PathBuf> = Vec::new();
    create_dir_tracked(install_dir, &mut created_dirs)
        .map_err(|e| format!("无法创建安装目录 {}：{e}", install_dir.display()))?;

    let mut placed: Vec<PlacedFile> = Vec::with_capacity(entries.len());

    // Phase 1: stage everything under temporary names.
    for entry in entries {
        let staged_result = (|| -> Result<PlacedFile, String> {
            let clean = sanitize_relative_path(&entry.rel_path)?;
            let mut target = install_dir.to_path_buf();
            for part in clean.split('/') {
                target.push(part);
            }
            if let Some(parent) = target.parent() {
                create_dir_tracked(parent, &mut created_dirs)
                    .map_err(|e| format!("无法创建子目录 {}：{e}", parent.display()))?;
            }
            let staged = staging_path(&target);
            with_retries(policy, &staged, || fs::write(&staged, &entry.data))
                .map_err(|e| format!("写入文件 {} 失败：{e}", target.display()))?;
            Ok(PlacedFile {
                existed_before: target.exists(),
                staged,
                target,
                committed: false,
            })
        })();
        match staged_result {
            Ok(file) => placed.push(file),
            Err(err) => {
                roll_back(&placed, &created_dirs);
                return Err(err);
            }
        }
    }

    // Phase 2: move the staged files over their destinations.
    for index in 0..placed.len() {
        let (staged, target) = (placed[index].staged.clone(), placed[index].target.clone());
        let moved = with_retries(policy, &target, || fs::rename(&staged, &target));
        if let Err(err) = moved {
            roll_back(&placed, &created_dirs);
            return Err(format!(
                "写入文件 {} 失败：{err}（文件可能正被其他程序占用，请关闭后重试）。",
                target.display()
            ));
        }
        placed[index].committed = true;
    }

    Ok(placed.into_iter().map(|file| file.target).collect())
}

// ---------------------------------------------------------------------------------------------
// Shell folders, shortcuts, association, uninstall entry
// ---------------------------------------------------------------------------------------------

/// Queries special folder path for Desktop or Start Menu Programs via `SHGetFolderPathW`.
#[must_use]
pub fn query_shell_folder(csidl: i32) -> Option<PathBuf> {
    let mut buf = vec![0_u16; MAX_PATH_BUF];
    // SAFETY: `buf` holds >= `MAX_PATH` UTF-16 code units for `SHGetFolderPathW`.
    let hr = unsafe { bindings::SHGetFolderPathW(None, csidl, None, 0, PWSTR(buf.as_mut_ptr())) };
    if hr.is_err() {
        return None;
    }
    let nul_pos = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    if nul_pos == 0 {
        None
    } else {
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..nul_pos])))
    }
}

/// Creates a standard Windows `.lnk` shortcut using `IShellLinkW` and `IPersistFile`.
///
/// # Errors
///
/// Returns an error if COM instantiation or shortcut persistence fails.
pub fn create_shortcut(
    lnk_path: &Path,
    target_exe: &Path,
    working_dir: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = lnk_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建快捷方式目录 {}：{e}", parent.display()))?;
    }

    let coinit = u32::try_from(bindings::COINIT_APARTMENTTHREADED).unwrap_or(2);
    // SAFETY: Ensures thread-local COM STA initialization for ShellLink.
    let hr_com = unsafe { bindings::CoInitializeEx(None, coinit) };
    let _com_guard = ComApartmentGuard(hr_com.is_ok());

    let clsctx = bindings::CLSCTX_INPROC_SERVER;
    // SAFETY: Creates in-process `CLSID_ShellLink` COM instance.
    let link: bindings::IShellLinkW =
        unsafe { bindings::CoCreateInstance(&bindings::ShellLink, None, clsctx) }
            .map_err(|e| format!("CoCreateInstance(ShellLink) 失败：{e}"))?;

    let w_target = to_wide_null(&target_exe.to_string_lossy());
    let w_workdir = to_wide_null(&working_dir.to_string_lossy());
    let w_desc = to_wide_null(description);
    let w_lnk = to_wide_null(&lnk_path.to_string_lossy());

    // SAFETY: Sets valid NUL-terminated shortcut target path.
    unsafe { link.SetPath(PCWSTR(w_target.as_ptr())) }
        .ok()
        .map_err(|e| format!("SetPath 失败：{e}"))?;
    // SAFETY: Sets working directory on initialized `IShellLinkW`.
    unsafe { link.SetWorkingDirectory(PCWSTR(w_workdir.as_ptr())) }
        .ok()
        .map_err(|e| format!("SetWorkingDirectory 失败：{e}"))?;
    // SAFETY: Sets shortcut tooltip description.
    unsafe { link.SetDescription(PCWSTR(w_desc.as_ptr())) }
        .ok()
        .map_err(|e| format!("SetDescription 失败：{e}"))?;
    // SAFETY: Sets shortcut icon location cleanly from executable index 0.
    unsafe { link.SetIconLocation(PCWSTR(w_target.as_ptr()), 0) }
        .ok()
        .map_err(|e| format!("SetIconLocation 失败：{e}"))?;

    let persist: bindings::IPersistFile = link
        .cast()
        .map_err(|e| format!("查询 IPersistFile 接口失败：{e}"))?;
    // SAFETY: Saves `.lnk` to disk cleanly via `IPersistFile::Save`.
    unsafe { persist.Save(PCWSTR(w_lnk.as_ptr()), true) }
        .ok()
        .map_err(|e| format!("保存快捷方式 {} 失败：{e}", lnk_path.display()))?;

    Ok(())
}

/// The `shell\open\command` value registered for a reader executable.
#[must_use]
pub fn association_open_command(reader_exe: &Path) -> String {
    format!("\"{}\" \"%1\"", reader_exe.to_string_lossy())
}

/// Registers the `.spdf` association for `reader_exe` under the environment's classes root.
///
/// # Errors
///
/// Returns an error if creating or writing registry keys fails.
pub fn register_spdf_association(
    env: &InstallEnvironment,
    reader_exe: &Path,
) -> Result<(), String> {
    let classes = &env.classes_root;
    let exe_str = reader_exe.to_string_lossy();
    set_hkcu_sz(&format!(r"{classes}\{SPDF_EXTENSION}"), "", SPDF_PROG_ID)?;
    set_hkcu_sz(
        &format!(r"{classes}\{SPDF_PROG_ID}"),
        "",
        SPDF_TYPE_DESCRIPTION,
    )?;
    set_hkcu_sz(
        &format!(r"{classes}\{SPDF_PROG_ID}\DefaultIcon"),
        "",
        &format!("\"{exe_str}\",0"),
    )?;
    set_hkcu_sz(
        &format!(r"{classes}\{SPDF_PROG_ID}\shell\open\command"),
        "",
        &association_open_command(reader_exe),
    )?;
    if !env.sandboxed {
        // SAFETY: Notifies Windows Explorer shell of the updated file association.
        unsafe {
            bindings::SHChangeNotify(
                bindings::SHCNE_ASSOCCHANGED,
                u32::try_from(bindings::SHCNF_IDLIST).unwrap_or(0),
                None,
                None,
            );
        };
    }
    Ok(())
}

/// Returns `true` when `.spdf` currently opens with exactly `reader_exe`.
#[must_use]
pub fn association_points_to(env: &InstallEnvironment, reader_exe: &Path) -> bool {
    let classes = &env.classes_root;
    let ext_ok = get_hkcu_sz(&format!(r"{classes}\{SPDF_EXTENSION}"), "")
        .is_some_and(|v| v.eq_ignore_ascii_case(SPDF_PROG_ID));
    let cmd_ok = get_hkcu_sz(&format!(r"{classes}\{SPDF_PROG_ID}\shell\open\command"), "")
        .is_some_and(|v| v.eq_ignore_ascii_case(&association_open_command(reader_exe)));
    ext_ok && cmd_ok
}

/// Writes the "Apps & features" entry (same key and value names as v1.x, so an upgrade
/// replaces the old entry instead of adding a second one).
///
/// # Errors
///
/// Returns an error if any registry value cannot be written.
pub fn write_uninstall_entry(
    env: &InstallEnvironment,
    app_kind: AppKind,
    install_dir: &Path,
    uninstall_script: &Path,
    installed_bytes: u64,
) -> Result<(), String> {
    let key = env.uninstall_key(app_kind);
    let main_exe = install_dir.join(app_kind.exe_name());
    let size_kb = u32::try_from(installed_bytes.div_ceil(1024).max(1)).unwrap_or(u32::MAX);
    set_hkcu_sz(&key, "DisplayName", app_kind.uninstall_display_name())?;
    set_hkcu_sz(&key, "DisplayVersion", PRODUCT_VERSION)?;
    set_hkcu_sz(&key, "Publisher", PUBLISHER)?;
    set_hkcu_sz(&key, "InstallLocation", &install_dir.to_string_lossy())?;
    set_hkcu_sz(
        &key,
        "DisplayIcon",
        &format!("{},0", main_exe.to_string_lossy()),
    )?;
    set_hkcu_sz(&key, "InstallDate", &local_date_compact())?;
    set_hkcu_dword(&key, "EstimatedSize", size_kb)?;
    set_hkcu_sz(
        &key,
        "UninstallString",
        &format!("\"{}\"", uninstall_script.to_string_lossy()),
    )?;
    set_hkcu_dword(&key, "NoModify", 1)?;
    set_hkcu_dword(&key, "NoRepair", 1)?;
    Ok(())
}

/// Builds the uninstall plan for an installation of `app_kind` into `install_dir`.
#[must_use]
pub fn uninstall_plan(
    env: &InstallEnvironment,
    app_kind: AppKind,
    install_dir: &Path,
    installed_files: &[String],
) -> UninstallPlan {
    let shortcut_files = [
        env.desktop_shortcut(app_kind),
        env.start_menu_shortcut(app_kind),
    ]
    .into_iter()
    .flatten()
    .collect();
    UninstallPlan {
        install_dir: install_dir.to_path_buf(),
        exe_name: app_kind.exe_name().to_owned(),
        script_name: app_kind.uninstall_script_name().to_owned(),
        kill_running: !env.sandboxed,
        shortcut_files,
        shortcut_dirs: env.start_menu_dir().into_iter().collect(),
        association: (app_kind == AppKind::Reader).then(|| AssociationRemoval {
            classes_root: env.classes_root.clone(),
            extension: SPDF_EXTENSION.to_owned(),
            prog_id: SPDF_PROG_ID.to_owned(),
            legacy_prog_ids: vec![LEGACY_SPDF_PROG_ID.to_owned()],
        }),
        uninstall_key: Some(env.uninstall_key(app_kind)),
        files: installed_files.to_vec(),
        obsolete_files: obsolete_file_names(app_kind),
    }
}

/// Writes the uninstall script (UTF-8 without BOM) into `install_dir`.
///
/// # Errors
///
/// Returns an error if the script file cannot be written.
pub fn write_uninstall_script(plan: &UninstallPlan) -> Result<PathBuf, String> {
    let bat_path = plan.install_dir.join(&plan.script_name);
    clear_read_only(&bat_path);
    fs::write(&bat_path, build_uninstall_script(plan).as_bytes())
        .map_err(|e| format!("写入卸载脚本 {} 失败：{e}", bat_path.display()))?;
    Ok(bat_path)
}

// ---------------------------------------------------------------------------------------------
// Upgrade handling
// ---------------------------------------------------------------------------------------------

fn remove_obsolete_files(app_kind: AppKind, install_dir: &Path, payload_names: &[String]) {
    for name in obsolete_file_names(app_kind) {
        if payload_names.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let path = install_dir.join(&name);
        if path.is_file() {
            clear_read_only(&path);
            let _ = fs::remove_file(&path);
        }
    }
}

/// Removes the program files of an earlier installation that lives in a *different* directory
/// than the new one, so that only one copy of the product remains. Only known file names are
/// deleted; the directory itself is removed only if it ends up empty.
fn remove_relocated_install(app_kind: AppKind, old_dir: &Path, payload_names: &[String]) {
    let mut names: Vec<String> = payload_names.to_vec();
    names.extend(obsolete_file_names(app_kind));
    names.push(app_kind.uninstall_script_name().to_owned());
    names.push(INSTALL_LOG_NAME.to_owned());
    for well_known in [
        "pdfium.dll",
        "THIRD-PARTY-NOTICES.txt",
        "关联spdf文件格式.bat",
    ] {
        names.push(well_known.to_owned());
    }
    names.push(app_kind.exe_name().to_owned());

    let mut dirs: Vec<PathBuf> = Vec::new();
    for name in &names {
        let rel = name.replace('/', "\\");
        let path = old_dir.join(&rel);
        if path.is_file() {
            clear_read_only(&path);
            let _ = fs::remove_file(&path);
        }
        let mut parent = Path::new(&rel).parent();
        while let Some(dir) = parent.filter(|d| !d.as_os_str().is_empty()) {
            let full = old_dir.join(dir);
            if !dirs.contains(&full) {
                dirs.push(full);
            }
            parent = dir.parent();
        }
    }
    dirs.sort_by_key(|d| core::cmp::Reverse(d.components().count()));
    for dir in dirs {
        let _ = fs::remove_dir(dir);
    }
    let _ = fs::remove_dir(old_dir);
}

fn same_dir(a: &Path, b: &Path) -> bool {
    normalize_win_path(a) == normalize_win_path(b)
}

// ---------------------------------------------------------------------------------------------
// Installation pipeline
// ---------------------------------------------------------------------------------------------

fn write_install_log(
    install_dir: &Path,
    app_kind: AppKind,
    opts: &InstallOptions,
    file_count: usize,
    replaced: Option<&ExistingInstall>,
    warnings: &[String],
) {
    let mut lines = vec![
        format!(
            "{} 安装 {} {PRODUCT_VERSION}",
            local_timestamp(),
            app_kind.uninstall_display_name()
        ),
        format!("安装目录：{}", install_dir.display()),
        format!("写入文件：{file_count} 个"),
        format!(
            "选项：桌面快捷方式={}，开始菜单={}，关联 .spdf={}",
            opts.create_desktop_shortcut, opts.create_start_menu_shortcut, opts.associate_spdf
        ),
    ];
    if let Some(prev) = replaced {
        lines.push(format!(
            "替换了已有安装：{}（版本 {}）",
            prev.dir.display(),
            prev.version.as_deref().unwrap_or("未登记")
        ));
    }
    if warnings.is_empty() {
        lines.push("结果：成功".to_owned());
    } else {
        lines.push("结果：成功，但有以下警告".to_owned());
        lines.extend(warnings.iter().map(|w| format!("  - {w}")));
    }
    let mut text = String::from('\u{feff}');
    text.push_str(&lines.join("\r\n"));
    text.push_str("\r\n");
    let _ = fs::write(install_dir.join(INSTALL_LOG_NAME), text.as_bytes());
}

/// Best-effort shell integration after the files and the uninstall script are in place:
/// shortcuts, the `.spdf` association and the "Apps & features" entry. Failures become warnings.
fn integrate_with_shell(
    app_kind: AppKind,
    opts: &InstallOptions,
    env: &InstallEnvironment,
    uninstall_script: &Path,
    installed_bytes: u64,
    progress: &mut dyn FnMut(u8, &str),
) -> (Vec<PathBuf>, Vec<String>) {
    let install_dir = opts.install_dir.as_path();
    let main_exe = install_dir.join(app_kind.exe_name());
    let mut warnings: Vec<String> = Vec::new();
    let mut shortcuts_created = Vec::new();

    progress(86, "正在配置快捷方式...");
    let wanted = [
        (
            opts.create_desktop_shortcut,
            env.desktop_shortcut(app_kind),
            "桌面",
        ),
        (
            opts.create_start_menu_shortcut,
            env.start_menu_shortcut(app_kind),
            "开始菜单",
        ),
    ];
    for (enabled, lnk, label) in wanted {
        if !enabled {
            continue;
        }
        let Some(lnk) = lnk else {
            warnings.push(format!(
                "无法确定{label}文件夹的位置，未创建{label}快捷方式。"
            ));
            continue;
        };
        match create_shortcut(
            &lnk,
            &main_exe,
            install_dir,
            app_kind.shortcut_description(),
        ) {
            Ok(()) => shortcuts_created.push(lnk),
            Err(err) => warnings.push(format!("未能创建{label}快捷方式：{err}")),
        }
    }

    if app_kind == AppKind::Reader && opts.associate_spdf {
        progress(92, "正在关联 .spdf 文件格式...");
        if let Err(err) = register_spdf_association(env, &main_exe) {
            warnings.push(format!("未能关联 .spdf 文件格式：{err}"));
        }
    }

    progress(96, "正在写入卸载信息...");
    if let Err(err) = write_uninstall_entry(
        env,
        app_kind,
        install_dir,
        uninstall_script,
        installed_bytes,
    ) {
        warnings.push(format!(
            "未能登记到“应用和功能”：{err}（仍可运行安装目录中的 {} 卸载）",
            app_kind.uninstall_script_name()
        ));
    }
    (shortcuts_created, warnings)
}

/// Removes the previous copy of the product after it was installed to a different directory,
/// together with its shortcuts that were not recreated (they would point at nothing).
fn retire_relocated_install(
    app_kind: AppKind,
    env: &InstallEnvironment,
    previous: &ExistingInstall,
    installed_names: &[String],
    shortcuts_created: &[PathBuf],
    warnings: &mut Vec<String>,
) {
    remove_relocated_install(app_kind, &previous.dir, installed_names);
    for stale in [
        env.desktop_shortcut(app_kind),
        env.start_menu_shortcut(app_kind),
    ]
    .into_iter()
    .flatten()
    .filter(|lnk| !shortcuts_created.contains(lnk))
    {
        let _ = fs::remove_file(&stale);
    }
    if let Some(menu_dir) = env.start_menu_dir() {
        // Non-recursive: stays if the other product's shortcut is still inside.
        let _ = fs::remove_dir(menu_dir);
    }
    if previous.dir.join(app_kind.exe_name()).exists() {
        warnings.push(format!(
            "旧安装目录 {} 中的程序文件未能全部移除，请手动删除。",
            previous.dir.display()
        ));
    }
}

/// Executes the full installation pipeline for `app_kind` inside `env`.
///
/// Order of operations: validate payload → stop running instances → place files (staged, with
/// retries) → remove obsolete v1.x files → write the uninstall script → shortcuts, association
/// and the "Apps & features" entry. Everything after the uninstall script is best effort: a
/// failure there is reported in [`InstallSummary::warnings`] and never leaves files on disk
/// without a way to remove them.
///
/// # Errors
///
/// Returns an error if the payload is unusable, the directory is not absolute, files cannot be
/// placed, or the uninstall script cannot be written.
pub fn perform_install(
    app_kind: AppKind,
    entries: &[ArchiveEntry],
    opts: &InstallOptions,
    env: &InstallEnvironment,
    progress: &mut dyn FnMut(u8, &str),
) -> Result<InstallSummary, String> {
    progress(5, "正在校验安装内容...");
    ensure_main_executable(app_kind, entries)?;
    let install_dir = opts.install_dir.as_path();
    if !install_dir.is_absolute() {
        return Err("安装目录必须是完整路径。".to_owned());
    }
    let main_exe = install_dir.join(app_kind.exe_name());
    let previous = detect_existing_install(app_kind, env);

    progress(20, "正在准备安装目录...");
    if main_exe.exists() {
        terminate_running_instances(&main_exe)?;
    }
    if let Some(prev) = previous.as_ref().filter(|p| !same_dir(&p.dir, install_dir)) {
        let _ = terminate_running_instances(&prev.dir.join(app_kind.exe_name()));
    }

    progress(35, "正在释放程序文件...");
    let installed_paths = extract_entries_to_dir(install_dir, entries, RetryPolicy::DEFAULT)?;
    let mut installed_names: Vec<String> = installed_paths
        .iter()
        .filter_map(|p| p.strip_prefix(install_dir).ok())
        .map(|rel| rel.to_string_lossy().replace('/', "\\"))
        .collect();
    remove_obsolete_files(app_kind, install_dir, &installed_names);
    if !main_exe.is_file() {
        return Err(format!("安装后未找到主程序 {}。", main_exe.display()));
    }

    progress(80, "正在生成卸载程序...");
    installed_names.push(INSTALL_LOG_NAME.to_owned());
    let plan = uninstall_plan(env, app_kind, install_dir, &installed_names);
    let uninstall_script = write_uninstall_script(&plan)?;

    let script_bytes = fs::metadata(&uninstall_script).map_or(0, |m| m.len());
    let installed_bytes = entries
        .iter()
        .map(|e| u64::try_from(e.data.len()).unwrap_or(u64::MAX))
        .fold(script_bytes, u64::saturating_add);
    let (shortcuts_created, mut warnings) = integrate_with_shell(
        app_kind,
        opts,
        env,
        &uninstall_script,
        installed_bytes,
        progress,
    );

    if let Some(prev) = previous.as_ref().filter(|p| !same_dir(&p.dir, install_dir)) {
        retire_relocated_install(
            app_kind,
            env,
            prev,
            &installed_names,
            &shortcuts_created,
            &mut warnings,
        );
    }

    write_install_log(
        install_dir,
        app_kind,
        opts,
        installed_paths.len(),
        previous.as_ref(),
        &warnings,
    );
    progress(100, "安装完成。");

    Ok(InstallSummary {
        install_dir: install_dir.to_path_buf(),
        main_exe,
        files_installed: installed_paths.len(),
        shortcuts_created,
        uninstall_script,
        warnings,
        replaced_version: previous.map(|p| p.version.unwrap_or_else(|| "1.x".to_owned())),
    })
}

/// Returns `true` when the uninstall entry for `app_kind` exists in `env`.
#[must_use]
pub fn uninstall_entry_exists(env: &InstallEnvironment, app_kind: AppKind) -> bool {
    hkcu_key_exists(&env.uninstall_key(app_kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{delete_hkcu_key_if_empty, delete_hkcu_tree, get_hkcu_dword};
    use tempfile::tempdir;

    const FAST: RetryPolicy = RetryPolicy {
        attempts: 3,
        delay: Duration::from_millis(5),
    };

    struct SandboxKey(String);
    impl SandboxKey {
        fn new(tag: &str) -> Self {
            Self(format!(
                r"Software\SecurePdfSuite_SelfTest\unit-{tag}-{}",
                std::process::id()
            ))
        }
    }
    impl Drop for SandboxKey {
        fn drop(&mut self) {
            let _ = delete_hkcu_tree(&self.0);
            delete_hkcu_key_if_empty(r"Software\SecurePdfSuite_SelfTest");
        }
    }

    fn entry(path: &str, data: &[u8]) -> ArchiveEntry {
        ArchiveEntry {
            rel_path: path.to_owned(),
            data: data.to_vec(),
        }
    }

    fn reader_payload() -> Vec<ArchiveEntry> {
        vec![
            entry("SecurePdfReader.exe", b"MZ-new-reader"),
            entry("pdfium.dll", b"MZ-pdfium"),
            entry("docs/说明 100%.txt", "说明".as_bytes()),
        ]
    }

    #[test]
    fn extraction_replaces_read_only_files_and_leaves_no_staging_files() {
        let dir = tempdir().unwrap();
        let install = dir.path().join("安装 100%").join("SecurePdfReader");
        fs::create_dir_all(&install).unwrap();
        let old_exe = install.join("SecurePdfReader.exe");
        fs::write(&old_exe, b"old").unwrap();
        let mut perms = fs::metadata(&old_exe).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&old_exe, perms).unwrap();

        let paths = extract_entries_to_dir(&install, &reader_payload(), FAST).unwrap();
        assert_eq!(paths.len(), 3);
        assert_eq!(fs::read(&old_exe).unwrap(), b"MZ-new-reader");
        assert_eq!(
            fs::read(install.join("docs").join("说明 100%.txt")).unwrap(),
            "说明".as_bytes()
        );
        let leftovers: Vec<_> = walk(&install)
            .into_iter()
            .filter(|p| p.to_string_lossy().ends_with(STAGING_SUFFIX))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn failed_extraction_rolls_back_a_fresh_install_completely() {
        let dir = tempdir().unwrap();
        let install = dir.path().join("Fresh").join("SecurePdfReader");
        // A directory squatting on a file name makes the final move of that entry fail.
        fs::create_dir_all(install.join("pdfium.dll").join("blocker")).unwrap();

        let err = extract_entries_to_dir(&install, &reader_payload(), FAST).unwrap_err();
        assert!(err.contains("pdfium.dll"), "{err}");
        // Nothing of the payload may remain: no exe, no docs folder, no staged files.
        assert!(!install.join("SecurePdfReader.exe").exists());
        assert!(!install.join("docs").exists());
        let remaining = walk(&install);
        assert!(remaining.is_empty(), "{remaining:?}");
        // The pre-existing obstacle is untouched.
        assert!(install.join("pdfium.dll").join("blocker").is_dir());
    }

    #[test]
    fn failed_extraction_keeps_previously_installed_files_in_place() {
        let dir = tempdir().unwrap();
        let install = dir.path().join("SecurePdfReader");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("SecurePdfReader.exe"), b"old-reader").unwrap();
        // An unsafe entry aborts during staging, before anything is replaced.
        let mut payload = reader_payload();
        payload.push(entry("NUL.txt", b"x"));
        assert!(extract_entries_to_dir(&install, &payload, FAST).is_err());
        assert_eq!(
            fs::read(install.join("SecurePdfReader.exe")).unwrap(),
            b"old-reader"
        );
        assert_eq!(walk(&install).len(), 1);
    }

    #[test]
    fn extraction_rejects_case_insensitive_duplicates_before_touching_disk() {
        let dir = tempdir().unwrap();
        let install = dir.path().join("Dup");
        let payload = vec![
            entry("SecurePdfReader.exe", b"MZ"),
            entry("Readme.txt", b"a"),
            entry("README.TXT", b"b"),
        ];
        assert!(extract_entries_to_dir(&install, &payload, FAST).is_err());
        assert!(!install.exists());
    }

    #[test]
    fn association_and_uninstall_entry_read_back_from_a_throw_away_key() {
        let key = SandboxKey::new("assoc");
        let dir = tempdir().unwrap();
        let env = InstallEnvironment::sandbox(dir.path(), &key.0);
        let install = dir.path().join("应用 100%").join("SecurePdfReader");
        fs::create_dir_all(&install).unwrap();
        let exe = install.join("SecurePdfReader.exe");
        fs::write(&exe, b"MZ").unwrap();

        assert!(!association_points_to(&env, &exe));
        register_spdf_association(&env, &exe).unwrap();
        assert!(association_points_to(&env, &exe));
        assert!(!association_points_to(&env, &dir.path().join("Other.exe")));
        assert_eq!(
            get_hkcu_sz(&format!(r"{}\.spdf", env.classes_root), "").as_deref(),
            Some(SPDF_PROG_ID)
        );
        assert_eq!(
            get_hkcu_sz(
                &format!(r"{}\{SPDF_PROG_ID}\shell\open\command", env.classes_root),
                ""
            ),
            Some(format!("\"{}\" \"%1\"", exe.display()))
        );

        let script = install.join("uninstall_reader.bat");
        assert!(!uninstall_entry_exists(&env, AppKind::Reader));
        write_uninstall_entry(&env, AppKind::Reader, &install, &script, 8_400_123).unwrap();
        assert!(uninstall_entry_exists(&env, AppKind::Reader));
        let entry_key = env.uninstall_key(AppKind::Reader);
        assert!(entry_key.ends_with(r"\Uninstall\SecurePdfReader"));
        let sz = |name: &str| get_hkcu_sz(&entry_key, name);
        assert_eq!(
            sz("DisplayName").as_deref(),
            Some("SecurePdf 阅读端 (接收方)")
        );
        assert_eq!(sz("DisplayVersion").as_deref(), Some(PRODUCT_VERSION));
        assert_eq!(sz("Publisher").as_deref(), Some("SecurePdfSuite"));
        assert_eq!(sz("InstallLocation"), Some(install.display().to_string()));
        assert_eq!(
            sz("UninstallString"),
            Some(format!("\"{}\"", script.display()))
        );
        assert_eq!(sz("DisplayIcon"), Some(format!("{},0", exe.display())));
        let date = sz("InstallDate").unwrap();
        assert!(
            date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()),
            "{date}"
        );
        assert_eq!(get_hkcu_dword(&entry_key, "EstimatedSize"), Some(8_204));
        assert_eq!(get_hkcu_dword(&entry_key, "NoModify"), Some(1));
        assert_eq!(get_hkcu_dword(&entry_key, "NoRepair"), Some(1));
    }

    #[test]
    fn existing_install_detection_prefers_registry_then_v1_default_then_new_default() {
        let key = SandboxKey::new("detect");
        let dir = tempdir().unwrap();
        let env = InstallEnvironment::sandbox(dir.path(), &key.0);
        let local = env.local_app_data.clone().unwrap();
        let new_default = local.join("Programs").join("SecurePdfCreator");

        assert_eq!(detect_existing_install(AppKind::Creator, &env), None);
        assert_eq!(default_install_dir(AppKind::Creator, &env), new_default);

        // v1.x default location with the main exe present.
        let v1_dir = local.join("SecurePdfCreator");
        fs::create_dir_all(&v1_dir).unwrap();
        fs::write(v1_dir.join("SecurePdfCreator.exe"), b"MZ").unwrap();
        assert_eq!(
            detect_existing_install(AppKind::Creator, &env),
            Some(ExistingInstall {
                dir: v1_dir.clone(),
                version: None
            })
        );
        assert_eq!(default_install_dir(AppKind::Creator, &env), v1_dir);
        // The other product is unaffected.
        assert_eq!(detect_existing_install(AppKind::Reader, &env), None);

        // A registered location wins, and carries its version.
        let custom = dir.path().join("Custom Dir").join("SecurePdfCreator");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("SecurePdfCreator.exe"), b"MZ").unwrap();
        let entry_key = env.uninstall_key(AppKind::Creator);
        set_hkcu_sz(&entry_key, "InstallLocation", &custom.to_string_lossy()).unwrap();
        set_hkcu_sz(&entry_key, "DisplayVersion", "1.1.0").unwrap();
        assert_eq!(
            detect_existing_install(AppKind::Creator, &env),
            Some(ExistingInstall {
                dir: custom.clone(),
                version: Some("1.1.0".to_owned())
            })
        );

        // A stale registration (exe gone) falls back to what is really on disk.
        fs::remove_file(custom.join("SecurePdfCreator.exe")).unwrap();
        assert_eq!(
            detect_existing_install(AppKind::Creator, &env).map(|e| e.dir),
            Some(v1_dir)
        );
    }

    #[test]
    fn upgrade_over_v1_leaves_one_program_one_shortcut_set_and_one_uninstall_entry() {
        let key = SandboxKey::new("upgrade");
        let dir = tempdir().unwrap();
        let env = InstallEnvironment::sandbox(dir.path(), &key.0);
        let local = env.local_app_data.clone().unwrap();

        // Simulated v1.1.0 Reader: files, shortcuts, uninstall entry and association.
        let v1_dir = local.join("SecurePdfReader");
        fs::create_dir_all(&v1_dir).unwrap();
        for (name, data) in [
            ("SecurePdfReader.exe", &b"MZ-v1"[..]),
            ("SecurePdfReader.exe.config", b"<configuration/>"),
            ("SecurePdf.Core.dll", b"MZ-core"),
            ("BouncyCastle.Cryptography.dll", b"MZ-bc"),
            ("pdfium.dll", b"MZ-old-pdfium"),
            ("uninstall_reader.bat", b"@echo v1 uninstaller"),
            ("我的笔记.txt", b"user file"),
        ] {
            fs::write(v1_dir.join(name), data).unwrap();
        }
        let desktop_lnk = env.desktop_shortcut(AppKind::Reader).unwrap();
        let menu_lnk = env.start_menu_shortcut(AppKind::Reader).unwrap();
        for lnk in [&desktop_lnk, &menu_lnk] {
            fs::create_dir_all(lnk.parent().unwrap()).unwrap();
            fs::write(lnk, b"v1-shortcut").unwrap();
        }
        let entry_key = env.uninstall_key(AppKind::Reader);
        set_hkcu_sz(&entry_key, "DisplayName", "SecurePdf 阅读端 (接收方)").unwrap();
        set_hkcu_sz(&entry_key, "DisplayVersion", "1.1.0").unwrap();
        set_hkcu_sz(&entry_key, "InstallLocation", &v1_dir.to_string_lossy()).unwrap();
        register_spdf_association(&env, &v1_dir.join("SecurePdfReader.exe")).unwrap();

        // The upgrade defaults to the very same directory.
        let target = default_install_dir(AppKind::Reader, &env);
        assert_eq!(target, v1_dir);
        let opts = InstallOptions {
            install_dir: target,
            create_desktop_shortcut: true,
            create_start_menu_shortcut: true,
            associate_spdf: true,
        };
        let summary = perform_install(
            AppKind::Reader,
            &reader_payload(),
            &opts,
            &env,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert_eq!(summary.replaced_version.as_deref(), Some("1.1.0"));

        // One program: new binaries in place, v1-only files gone, user file kept.
        assert_eq!(
            fs::read(v1_dir.join("SecurePdfReader.exe")).unwrap(),
            b"MZ-new-reader"
        );
        assert_eq!(fs::read(v1_dir.join("pdfium.dll")).unwrap(), b"MZ-pdfium");
        for gone in [
            "SecurePdfReader.exe.config",
            "SecurePdf.Core.dll",
            "BouncyCastle.Cryptography.dll",
        ] {
            assert!(!v1_dir.join(gone).exists(), "{gone} must be removed");
        }
        assert!(v1_dir.join("我的笔记.txt").exists());
        assert!(!local.join("Programs").join("SecurePdfReader").exists());

        // One shortcut set: the v1 Chinese-named files were replaced by real shortcuts.
        let desktop_files: Vec<_> = fs::read_dir(env.desktop_dir.as_ref().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(desktop_files, vec!["SecurePdf 阅读端.lnk".to_owned()]);
        let menu_files: Vec<_> = fs::read_dir(env.start_menu_dir().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(menu_files, vec!["SecurePdf 阅读端.lnk".to_owned()]);
        assert_ne!(fs::read(&desktop_lnk).unwrap(), b"v1-shortcut");

        // One uninstall entry, now describing the new version; the script is the new one.
        assert_eq!(
            get_hkcu_sz(&entry_key, "DisplayVersion").as_deref(),
            Some(PRODUCT_VERSION)
        );
        let script = fs::read_to_string(v1_dir.join("uninstall_reader.bat")).unwrap();
        assert!(script.starts_with("@echo off\r\nchcp 65001 >nul\r\n"));
        assert!(script.contains("SecurePdf.Core.dll"));
        // Association usable: points at the upgraded exe.
        assert!(association_points_to(
            &env,
            &v1_dir.join("SecurePdfReader.exe")
        ));
    }

    #[test]
    fn installing_elsewhere_removes_the_old_copy_but_keeps_user_files() {
        let key = SandboxKey::new("relocate");
        let dir = tempdir().unwrap();
        let env = InstallEnvironment::sandbox(dir.path(), &key.0);
        let old_dir = env.local_app_data.clone().unwrap().join("SecurePdfReader");
        fs::create_dir_all(&old_dir).unwrap();
        for name in [
            "SecurePdfReader.exe",
            "SecurePdf.Core.dll",
            "pdfium.dll",
            "uninstall_reader.bat",
            "keep-me.txt",
        ] {
            fs::write(old_dir.join(name), b"x").unwrap();
        }
        // Old shortcuts (ours and the other product's) from the previous installation.
        let old_desktop = env.desktop_shortcut(AppKind::Reader).unwrap();
        let old_menu = env.start_menu_shortcut(AppKind::Reader).unwrap();
        let creator_menu = env.start_menu_shortcut(AppKind::Creator).unwrap();
        fs::create_dir_all(old_desktop.parent().unwrap()).unwrap();
        fs::create_dir_all(old_menu.parent().unwrap()).unwrap();
        for lnk in [&old_desktop, &old_menu, &creator_menu] {
            fs::write(lnk, b"old-shortcut").unwrap();
        }
        let new_dir = dir.path().join("Elsewhere").join("SecurePdfReader");
        let opts = InstallOptions {
            install_dir: new_dir.clone(),
            create_desktop_shortcut: false,
            create_start_menu_shortcut: false,
            associate_spdf: false,
        };
        let summary = perform_install(
            AppKind::Reader,
            &reader_payload(),
            &opts,
            &env,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert_eq!(summary.shortcuts_created, Vec::<PathBuf>::new());
        assert!(new_dir.join("SecurePdfReader.exe").is_file());
        let left: Vec<_> = fs::read_dir(&old_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["keep-me.txt".to_owned()]);
        // The old copy's shortcuts were not recreated, so they must not be left dangling;
        // the other product's shortcut (and therefore the shared folder) stays.
        assert!(!old_desktop.exists());
        assert!(!old_menu.exists());
        assert!(creator_menu.is_file());
        // No association was requested and none existed: nothing may have been created.
        assert!(!association_points_to(
            &env,
            &new_dir.join("SecurePdfReader.exe")
        ));
        assert!(!hkcu_key_exists(&format!(r"{}\.spdf", env.classes_root)));
        assert_eq!(
            get_hkcu_sz(&env.uninstall_key(AppKind::Reader), "InstallLocation"),
            Some(new_dir.display().to_string())
        );
    }

    #[test]
    fn install_refuses_payload_without_main_exe_and_relative_directories() {
        let key = SandboxKey::new("refuse");
        let dir = tempdir().unwrap();
        let env = InstallEnvironment::sandbox(dir.path(), &key.0);
        let opts = InstallOptions {
            install_dir: dir.path().join("X"),
            create_desktop_shortcut: false,
            create_start_menu_shortcut: false,
            associate_spdf: false,
        };
        let creator_only = vec![entry("SecurePdfCreator.exe", b"MZ")];
        let err = perform_install(AppKind::Reader, &creator_only, &opts, &env, &mut |_, _| {})
            .unwrap_err();
        assert!(
            err.contains("SecurePdfReader.exe") && err.contains("不匹配"),
            "{err}"
        );
        assert!(!dir.path().join("X").exists());

        let relative = InstallOptions {
            install_dir: PathBuf::from("relative-dir"),
            ..opts
        };
        assert!(
            perform_install(
                AppKind::Reader,
                &reader_payload(),
                &relative,
                &env,
                &mut |_, _| {}
            )
            .is_err()
        );
        assert!(!Path::new("relative-dir").exists());
    }

    #[test]
    fn shortcut_or_registry_failure_is_a_warning_and_the_uninstall_script_still_exists() {
        let key = SandboxKey::new("warn");
        let dir = tempdir().unwrap();
        let mut env = InstallEnvironment::sandbox(dir.path(), &key.0);
        // A file where the Desktop folder should be makes shortcut creation fail.
        let fake_desktop = dir.path().join("Desktop");
        fs::write(&fake_desktop, b"not a folder").unwrap();
        // An invalid registry path makes the uninstall entry fail.
        env.uninstall_root = String::new();
        let install = dir.path().join("App").join("SecurePdfCreator");
        let opts = InstallOptions {
            install_dir: install.clone(),
            create_desktop_shortcut: true,
            create_start_menu_shortcut: true,
            associate_spdf: false,
        };
        let payload = vec![entry("SecurePdfCreator.exe", b"MZ-creator")];
        let summary =
            perform_install(AppKind::Creator, &payload, &opts, &env, &mut |_, _| {}).unwrap();
        assert!(install.join("SecurePdfCreator.exe").is_file());
        assert!(summary.uninstall_script.is_file());
        assert_eq!(summary.shortcuts_created.len(), 1, "start menu still works");
        assert!(
            summary.warnings.iter().any(|w| w.contains("桌面快捷方式")),
            "{:?}",
            summary.warnings
        );
        assert!(
            summary.warnings.iter().any(|w| w.contains("应用和功能")),
            "{:?}",
            summary.warnings
        );
        let log = fs::read_to_string(install.join(INSTALL_LOG_NAME)).unwrap();
        assert!(log.contains("警告") && log.contains("桌面快捷方式"));
    }

    #[test]
    fn real_shell_link_is_created_for_non_ascii_names() {
        let dir = tempdir().unwrap();
        let fake_exe = dir.path().join("SecurePdfReader.exe");
        fs::write(&fake_exe, b"MZ").unwrap();
        let lnk = dir.path().join("SecurePdf 阅读端.lnk");
        create_shortcut(&lnk, &fake_exe, dir.path(), "受控阅读加密 .spdf 文档").unwrap();
        let bytes = fs::read(&lnk).unwrap();
        // Shell link header: HeaderSize 0x4C followed by the ShellLink CLSID.
        assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
        assert!(bytes.len() > 200);
    }

    #[test]
    fn process_probe_finds_this_test_binary_only_by_exact_path() {
        // The probe skips the calling process itself, so the current exe reports "not running"
        // unless a second copy runs; a non-existent path is never reported.
        assert!(!is_target_running(Path::new(
            r"C:\definitely\missing\SecurePdfReader.exe"
        )));
        assert_eq!(
            terminate_running_instances(Path::new(r"C:\definitely\missing\SecurePdfReader.exe")),
            Ok(0)
        );
    }
}
