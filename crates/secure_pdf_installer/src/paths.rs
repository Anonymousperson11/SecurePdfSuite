//! Install-directory validation (v1.x `NormalizeInstallDir`) as pure, testable functions.

use std::path::{Path, PathBuf};

use crate::package::{AppKind, is_safe_file_component};

/// Longest accepted install directory, leaving room under `MAX_PATH` for payload file names.
pub const MAX_INSTALL_DIR_CHARS: usize = 200;

/// Directories an installation must never be placed in (or directly on top of).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtectedDirs {
    /// Directory trees that are off limits entirely (the Windows directory and everything below).
    pub system_trees: Vec<String>,
    /// Shared container folders that may hold an install *sub*folder but not the files themselves
    /// (Program Files, the user profile, the Desktop ...).
    pub containers: Vec<String>,
}

impl ProtectedDirs {
    /// Collects the protected locations of the running system from its environment.
    #[must_use]
    pub fn from_environment() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let mut system_trees = Vec::new();
        for name in ["SystemRoot", "windir"] {
            if let Some(dir) = var(name) {
                system_trees.push(dir);
            }
        }
        let mut containers = Vec::new();
        for name in [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "ProgramData",
            "PUBLIC",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            if let Some(dir) = var(name) {
                containers.push(dir);
            }
        }
        if let Some(local) = var("LOCALAPPDATA") {
            containers.push(format!(r"{}\Programs", local.trim_end_matches('\\')));
        }
        if let Some(profile) = var("USERPROFILE") {
            let profile = profile.trim_end_matches('\\').to_owned();
            for leaf in ["Desktop", "Documents", "Downloads"] {
                containers.push(format!(r"{profile}\{leaf}"));
            }
        }
        Self {
            system_trees,
            containers,
        }
    }

    /// Adds another container folder (for example the resolved Desktop directory).
    pub fn add_container(&mut self, dir: &Path) {
        self.containers.push(dir.to_string_lossy().into_owned());
    }
}

fn fold(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Splits `text` into its absolute root (`X:` or `\\server\share`) and the remaining path.
fn split_root(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        let drive = char::from(bytes[0].to_ascii_uppercase());
        return Some((format!("{drive}:"), &text[3..]));
    }
    let unc = text.strip_prefix(r"\\")?;
    let mut parts = unc.splitn(3, '\\');
    let server = parts.next().filter(|s| !s.is_empty())?;
    let share = parts.next().filter(|s| !s.is_empty())?;
    if matches!(server, "." | "?")
        || !is_safe_file_component(server)
        || !is_safe_file_component(share)
    {
        return None;
    }
    Some((format!(r"\\{server}\{share}"), parts.next().unwrap_or("")))
}

/// Turns user input into a clean absolute install directory, or explains why it is unacceptable.
///
/// Mirrors v1.x `NormalizeInstallDir`: surrounding quotes are stripped; relative, drive-relative
/// and root-relative paths are refused; `.` / `..` are resolved lexically; a bare drive or share
/// root is refused. In addition the Windows directory tree and shared container folders are
/// refused so that files are never scattered into a folder the uninstaller cannot own.
///
/// # Errors
///
/// Returns a user-facing message describing the first problem found.
pub fn normalize_install_dir(
    raw: &str,
    app_kind: AppKind,
    protected: &ProtectedDirs,
) -> Result<PathBuf, String> {
    let trimmed = raw.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return Err("安装目录不能为空。".to_owned());
    }
    let unified = trimmed.replace('/', "\\");
    let example = format!(
        "请填写完整的安装路径，例如 D:\\Apps\\{}。",
        app_kind.install_folder_name()
    );
    let Some((root, rest)) = split_root(&unified) else {
        return Err(example);
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(example);
                }
            }
            _ => {
                if !is_safe_file_component(part) {
                    return Err(format!("安装路径中包含无效的文件夹名称：{part}"));
                }
                parts.push(part);
            }
        }
    }
    if parts.is_empty() {
        return Err("不能直接安装到磁盘根目录，请指定一个文件夹。".to_owned());
    }

    let full = format!(r"{root}\{}", parts.join("\\"));
    if full.chars().count() > MAX_INSTALL_DIR_CHARS {
        return Err(format!(
            "安装路径过长（超过 {MAX_INSTALL_DIR_CHARS} 个字符），请选择更短的路径。"
        ));
    }

    let folded = fold(&full);
    for tree in &protected.system_trees {
        let tree = fold(tree);
        if !tree.is_empty() && (folded == tree || folded.starts_with(&format!(r"{tree}\"))) {
            return Err("不能安装到 Windows 系统目录，请选择其他位置。".to_owned());
        }
    }
    for container in &protected.containers {
        if folded == fold(container) {
            return Err(format!(
                "不能把程序文件直接放进 {full}，请指定其中的一个子文件夹，例如 {full}\\{}。",
                app_kind.install_folder_name()
            ));
        }
    }
    Ok(PathBuf::from(full))
}

/// Appends the application folder to a browsed location unless it already ends with it.
///
/// Picking `D:\Tools` in the folder dialog therefore installs into `D:\Tools\SecurePdfReader`
/// instead of scattering the program files across `D:\Tools`.
#[must_use]
pub fn with_app_folder(selected: &Path, app_kind: AppKind) -> PathBuf {
    let folder = app_kind.install_folder_name();
    let text = selected.to_string_lossy();
    let trimmed = text.trim_end_matches(['\\', '/']);
    let last = trimmed.rsplit(['\\', '/']).next().unwrap_or("");
    if last.eq_ignore_ascii_case(folder) {
        PathBuf::from(trimmed)
    } else {
        PathBuf::from(format!(r"{trimmed}\{folder}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected() -> ProtectedDirs {
        ProtectedDirs {
            system_trees: vec![r"C:\Windows".to_owned()],
            containers: vec![
                r"C:\Program Files".to_owned(),
                r"C:\Users\张三".to_owned(),
                r"C:\Users\张三\AppData\Local\Programs\".to_owned(),
            ],
        }
    }

    fn norm(raw: &str) -> Result<PathBuf, String> {
        normalize_install_dir(raw, AppKind::Reader, &protected())
    }

    #[test]
    fn accepts_absolute_paths_and_cleans_quotes_slashes_and_dot_segments() {
        assert_eq!(
            norm(r"D:\Apps\SecurePdfReader").unwrap(),
            PathBuf::from(r"D:\Apps\SecurePdfReader")
        );
        assert_eq!(
            norm(r#"  "d:/Apps//工具 100%\.\SecurePdfReader\"  "#).unwrap(),
            PathBuf::from(r"D:\Apps\工具 100%\SecurePdfReader")
        );
        assert_eq!(
            norm(r"D:\Apps\Old\..\SecurePdfReader\").unwrap(),
            PathBuf::from(r"D:\Apps\SecurePdfReader")
        );
        assert_eq!(
            norm(r"\\fileserver\tools\SecurePdfReader").unwrap(),
            PathBuf::from(r"\\fileserver\tools\SecurePdfReader")
        );
        // A subfolder of a container is fine; only the container itself is refused.
        assert!(norm(r"C:\Program Files\SecurePdfReader").is_ok());
        assert!(norm(r"C:\Users\张三\AppData\Local\Programs\SecurePdfReader").is_ok());
    }

    #[test]
    fn rejects_relative_drive_relative_and_root_relative_paths() {
        for raw in [
            "",
            "   ",
            "\"\"",
            ".",
            "..",
            "SecurePdfReader",
            r"Apps\SecurePdfReader",
            r".\Apps",
            r"C:Apps",
            "C:",
            r"\Apps\SecurePdfReader",
            r"\\?\C:\Apps\SecurePdfReader",
            r"\\.\C:\Apps",
            r"\\server",
            r"\\server\",
        ] {
            assert!(norm(raw).is_err(), "{raw:?} must be rejected");
        }
    }

    #[test]
    fn rejects_drive_and_share_roots_including_after_dot_dot_resolution() {
        for raw in [
            r"C:\",
            "C:/",
            r"c:\.",
            r"D:\Apps\..",
            r"D:\Apps\..\..",
            r"\\server\share",
            r"\\server\share\",
            r"\\server\share\x\..",
        ] {
            let err = norm(raw).unwrap_err();
            assert!(!err.is_empty(), "{raw:?}");
        }
        assert!(norm(r"C:\").unwrap_err().contains("根目录"));
    }

    #[test]
    fn rejects_system_tree_and_bare_container_folders_case_insensitively() {
        for raw in [
            r"C:\Windows",
            r"c:\windows\System32",
            r"C:\WINDOWS\Temp\SecurePdfReader",
        ] {
            assert!(norm(raw).unwrap_err().contains("系统目录"), "{raw:?}");
        }
        for raw in [
            r"C:\Program Files",
            r"c:\program files\",
            r"C:\Users\张三",
            r"C:\Users\张三\AppData\Local\Programs",
        ] {
            assert!(norm(raw).unwrap_err().contains("子文件夹"), "{raw:?}");
        }
        // A sibling whose name merely starts with "Windows" is not the system tree.
        assert!(norm(r"C:\WindowsApps-Mine\SecurePdfReader").is_ok());
    }

    #[test]
    fn rejects_invalid_folder_names_and_overlong_paths() {
        for raw in [
            r"D:\Apps\NUL",
            r"D:\Apps\con.txt\Reader",
            r"D:\Apps\a<b",
            r"D:\Apps\trailing. \x",
            r"D:\Apps\dot.",
            "D:\\Apps\\tab\there",
            r"D:\Apps\what?",
            r"D:\Apps\C:\nested",
        ] {
            assert!(norm(raw).is_err(), "{raw:?} must be rejected");
        }
        let long = format!(r"D:\{}", "a".repeat(MAX_INSTALL_DIR_CHARS));
        assert!(norm(&long).unwrap_err().contains("过长"));
    }

    #[test]
    fn browsing_appends_the_application_folder_exactly_once() {
        assert_eq!(
            with_app_folder(Path::new(r"D:\Tools"), AppKind::Reader),
            PathBuf::from(r"D:\Tools\SecurePdfReader")
        );
        assert_eq!(
            with_app_folder(Path::new(r"D:\Tools\"), AppKind::Creator),
            PathBuf::from(r"D:\Tools\SecurePdfCreator")
        );
        assert_eq!(
            with_app_folder(Path::new(r"D:\Tools\securepdfreader\"), AppKind::Reader),
            PathBuf::from(r"D:\Tools\securepdfreader")
        );
        // The other product's folder is not "ours": still append.
        assert_eq!(
            with_app_folder(Path::new(r"D:\SecurePdfCreator"), AppKind::Reader),
            PathBuf::from(r"D:\SecurePdfCreator\SecurePdfReader")
        );
        // A drive root gets the folder appended and then passes normalisation.
        let picked = with_app_folder(Path::new(r"E:\"), AppKind::Reader);
        assert_eq!(picked, PathBuf::from(r"E:\SecurePdfReader"));
        assert!(norm(&picked.to_string_lossy()).is_ok());
    }

    #[test]
    fn environment_protection_covers_this_machines_windows_directory() {
        let live = ProtectedDirs::from_environment();
        assert_ne!(live.system_trees, Vec::<String>::new());
        let windows = live.system_trees[0].clone();
        let inside = format!(r"{windows}\Temp\SecurePdfReader");
        assert!(normalize_install_dir(&inside, AppKind::Reader, &live).is_err());
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            assert!(normalize_install_dir(&local, AppKind::Reader, &live).is_err());
            let default_dir = format!(r"{local}\Programs\SecurePdfReader");
            assert!(normalize_install_dir(&default_dir, AppKind::Reader, &live).is_ok());
            let v1_dir = format!(r"{local}\SecurePdfReader");
            assert!(normalize_install_dir(&v1_dir, AppKind::Reader, &live).is_ok());
        }
    }
}
