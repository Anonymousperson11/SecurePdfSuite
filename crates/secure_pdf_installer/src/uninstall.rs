//! Generation of the per-install uninstall batch script (pure text building, no I/O).

use std::path::{Path, PathBuf};

/// `.spdf` association ownership data needed to remove it safely at uninstall time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociationRemoval {
    /// HKCU subkey that plays the role of `Software\Classes`.
    pub classes_root: String,
    /// File extension key name, including the dot (`.spdf`).
    pub extension: String,
    /// `ProgID` registered by this product.
    pub prog_id: String,
    /// `ProgID`s written by earlier releases, removed together with ours.
    pub legacy_prog_ids: Vec<String>,
}

/// Everything the uninstall script has to undo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UninstallPlan {
    /// Absolute installation directory.
    pub install_dir: PathBuf,
    /// Main executable file name (for `taskkill` and the association ownership check).
    pub exe_name: String,
    /// File name of the script itself inside `install_dir`.
    pub script_name: String,
    /// Terminate running instances by image name first (disabled inside the self-test sandbox).
    pub kill_running: bool,
    /// Shortcut files to delete (resolved real paths, e.g. a redirected Desktop).
    pub shortcut_files: Vec<PathBuf>,
    /// Shortcut folders to remove if they end up empty (never recursive).
    pub shortcut_dirs: Vec<PathBuf>,
    /// Association to remove, only if it still belongs to this installation.
    pub association: Option<AssociationRemoval>,
    /// Full HKCU subkey of the "Apps & features" entry.
    pub uninstall_key: Option<String>,
    /// Installed files relative to `install_dir` (either slash style).
    pub files: Vec<String>,
    /// Files left behind by earlier releases that should also disappear.
    pub obsolete_files: Vec<String>,
}

/// Escapes text for use inside a batch file: `%` must be doubled. Every path is emitted inside
/// double quotes and delayed expansion is disabled, so no other character needs escaping.
#[must_use]
pub fn bat_escape(text: &str) -> String {
    text.replace('%', "%%")
}

fn bat_path(path: &Path) -> String {
    bat_escape(&path.to_string_lossy().replace('/', "\\"))
}

/// Directories (relative, backslash style) that contain installed files, deepest first.
fn nested_directories(files: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for file in files {
        let unified = file.replace('/', "\\");
        let mut parts: Vec<&str> = unified.split('\\').filter(|p| !p.is_empty()).collect();
        parts.pop();
        while !parts.is_empty() {
            let dir = parts.join("\\");
            if !dirs.iter().any(|d| d.eq_ignore_ascii_case(&dir)) {
                dirs.push(dir);
            }
            parts.pop();
        }
    }
    dirs.sort_by(|a, b| {
        let depth = |d: &String| d.matches('\\').count();
        depth(b).cmp(&depth(a)).then_with(|| a.cmp(b))
    });
    dirs
}

/// Builds the uninstall script text (CRLF line endings, UTF-8 without BOM).
///
/// The script only deletes what the installer put there: each installed file by name, then the
/// directories if they are empty. The `.spdf` association is removed only while it still points
/// at this installation's executable. Print-quota records are deliberately kept so that an
/// uninstall / reinstall cycle cannot reset used print counts.
#[must_use]
pub fn build_uninstall_script(plan: &UninstallPlan) -> String {
    const FIND: &str = r#""%SystemRoot%\System32\find.exe""#;
    let mut lines: Vec<String> = vec![
        "@echo off".to_owned(),
        // The script is UTF-8: without this, non-ASCII paths are misread on localized systems.
        "chcp 65001 >nul".to_owned(),
        // Leave the install directory (the working directory when the script is double-clicked
        // in Explorer) BEFORE `setlocal`: the implicit `endlocal` at the end of the script
        // restores the directory that was current at `setlocal` time, and a directory that is
        // some process's working directory cannot be removed.
        "cd /d \"%SystemRoot%\"".to_owned(),
        "setlocal DisableDelayedExpansion".to_owned(),
        format!("set \"APPDIR={}\"", bat_path(&plan.install_dir)),
    ];

    if plan.kill_running {
        lines.push(format!("taskkill /f /im \"{}\" >nul 2>&1", plan.exe_name));
        // Give the terminated process a moment to release its files.
        lines.push("ping -n 2 127.0.0.1 >nul".to_owned());
    }

    for shortcut in &plan.shortcut_files {
        lines.push(format!("del /f /q \"{}\" >nul 2>&1", bat_path(shortcut)));
    }
    for dir in &plan.shortcut_dirs {
        lines.push(format!("rmdir \"{}\" >nul 2>&1", bat_path(dir)));
    }

    if let Some(assoc) = &plan.association {
        let classes = bat_escape(&assoc.classes_root);
        let prog_key = format!("HKCU\\{classes}\\{}", bat_escape(&assoc.prog_id));
        let ext_key = format!("HKCU\\{classes}\\{}", bat_escape(&assoc.extension));
        lines.push("set \"OWNS_ASSOC=\"".to_owned());
        // Ours only while the registered open command still launches this installation.
        lines.push(format!(
            "reg query \"{prog_key}\\shell\\open\\command\" /ve 2>nul | {FIND} /i \"%APPDIR%\\{}\" >nul && set \"OWNS_ASSOC=1\"",
            bat_escape(&plan.exe_name)
        ));
        // Drop the extension mapping only if it still names our ProgID.
        lines.push(format!(
            "if defined OWNS_ASSOC reg query \"{ext_key}\" /ve 2>nul | {FIND} /i \"{}\" >nul && reg delete \"{ext_key}\" /f >nul 2>&1",
            bat_escape(&assoc.prog_id)
        ));
        lines.push(format!(
            "if defined OWNS_ASSOC reg delete \"{prog_key}\" /f >nul 2>&1"
        ));
        for legacy in &assoc.legacy_prog_ids {
            lines.push(format!(
                "if defined OWNS_ASSOC reg delete \"HKCU\\{classes}\\{}\" /f >nul 2>&1",
                bat_escape(legacy)
            ));
        }
    }

    if let Some(key) = &plan.uninstall_key {
        lines.push(format!(
            "reg delete \"HKCU\\{}\" /f >nul 2>&1",
            bat_escape(key)
        ));
    }

    for file in plan.files.iter().chain(&plan.obsolete_files) {
        let rel = bat_escape(&file.replace('/', "\\"));
        if rel.eq_ignore_ascii_case(&plan.script_name) {
            continue;
        }
        lines.push(format!("del /f /q \"%APPDIR%\\{rel}\" >nul 2>&1"));
    }
    for dir in nested_directories(&plan.files) {
        lines.push(format!(
            "rmdir \"%APPDIR%\\{}\" >nul 2>&1",
            bat_escape(&dir)
        ));
    }

    lines.push("echo 已完成卸载。".to_owned());
    // `(goto)` makes cmd finish this line without reading the script file again, so the script
    // can delete itself and then remove the (now empty) directory. The `cd` is repeated here
    // because ending the batch context restores the `setlocal`-time directory. If the user
    // keeps own files there, rmdir fails and the directory stays; `ver` keeps the exit code 0.
    lines.push(format!(
        "(goto) 2>nul & cd /d \"%SystemRoot%\" & del /f /q \"%APPDIR%\\{}\" & rmdir \"%APPDIR%\" 2>nul & ver >nul",
        bat_escape(&plan.script_name)
    ));

    let mut script = lines.join("\r\n");
    script.push_str("\r\n");
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader_plan(install_dir: &str) -> UninstallPlan {
        UninstallPlan {
            install_dir: PathBuf::from(install_dir),
            exe_name: "SecurePdfReader.exe".to_owned(),
            script_name: "uninstall_reader.bat".to_owned(),
            kill_running: true,
            shortcut_files: vec![
                PathBuf::from(r"D:\桌面 50%\SecurePdf 阅读端.lnk"),
                PathBuf::from(
                    r"C:\Users\张三\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\SecurePdfSuite\SecurePdf 阅读端.lnk",
                ),
            ],
            shortcut_dirs: vec![PathBuf::from(
                r"C:\Users\张三\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\SecurePdfSuite",
            )],
            association: Some(AssociationRemoval {
                classes_root: r"Software\Classes".to_owned(),
                extension: ".spdf".to_owned(),
                prog_id: "SecurePdfSuite.SpdfFile".to_owned(),
                legacy_prog_ids: vec!["SecurePdfSuite.spdf".to_owned()],
            }),
            uninstall_key: Some(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall\SecurePdfReader".to_owned(),
            ),
            files: vec![
                "SecurePdfReader.exe".to_owned(),
                "pdfium.dll".to_owned(),
                "关联spdf文件格式.bat".to_owned(),
                "docs/zh/100% 说明.txt".to_owned(),
                "docs/licenses.txt".to_owned(),
                "uninstall_reader.bat".to_owned(),
            ],
            obsolete_files: vec!["SecurePdf.Core.dll".to_owned()],
        }
    }

    fn index_of(lines: &[&str], needle: &str) -> usize {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("script lacks a line containing {needle:?}"))
    }

    #[test]
    fn script_header_sets_utf8_codepage_and_disables_delayed_expansion_first() {
        let script =
            build_uninstall_script(&reader_plan(r"C:\Users\张三\应用 100%\SecurePdfReader"));
        let lines: Vec<&str> = script.split("\r\n").collect();
        assert_eq!(lines[0], "@echo off");
        assert_eq!(lines[1], "chcp 65001 >nul");
        // The directory change must precede `setlocal` (see `build_uninstall_script`).
        assert_eq!(lines[2], "cd /d \"%SystemRoot%\"");
        assert_eq!(lines[3], "setlocal DisableDelayedExpansion");
        assert_eq!(
            lines[4],
            "set \"APPDIR=C:\\Users\\张三\\应用 100%%\\SecurePdfReader\""
        );
        assert!(script.ends_with("\r\n"));
        // No bare LF line endings anywhere.
        assert!(!script.replace("\r\n", "").contains('\n'));
        // UTF-8 without BOM: the first byte is '@'.
        assert_eq!(script.as_bytes()[0], b'@');
    }

    #[test]
    fn every_percent_sign_in_paths_is_doubled_and_non_ascii_is_kept_verbatim() {
        let script =
            build_uninstall_script(&reader_plan(r"C:\Users\张三\应用 100%\SecurePdfReader"));
        assert!(script.contains("del /f /q \"D:\\桌面 50%%\\SecurePdf 阅读端.lnk\" >nul 2>&1"));
        assert!(script.contains("del /f /q \"%APPDIR%\\docs\\zh\\100%% 说明.txt\" >nul 2>&1"));
        assert!(script.contains("del /f /q \"%APPDIR%\\关联spdf文件格式.bat\" >nul 2>&1"));
        // A literal single '%' may only appear as part of a %VARIABLE% reference or a doubled %%.
        for line in script.split("\r\n") {
            let without_vars = line
                .replace("%APPDIR%", "")
                .replace("%SystemRoot%", "")
                .replace("%%", "");
            assert!(!without_vars.contains('%'), "unescaped % in: {line}");
        }
    }

    #[test]
    fn processes_are_killed_then_waited_for_before_any_file_is_deleted() {
        let script = build_uninstall_script(&reader_plan(r"D:\Apps\SecurePdfReader"));
        let lines: Vec<&str> = script.split("\r\n").collect();
        let kill = index_of(&lines, "taskkill /f /im \"SecurePdfReader.exe\"");
        let wait = index_of(&lines, "ping -n 2 127.0.0.1 >nul");
        let first_del = index_of(&lines, "del /f /q");
        assert_eq!(wait, kill + 1);
        assert!(wait < first_del);

        let mut sandboxed = reader_plan(r"D:\Apps\SecurePdfReader");
        sandboxed.kill_running = false;
        let quiet = build_uninstall_script(&sandboxed);
        assert!(!quiet.contains("taskkill"));
        assert!(!quiet.contains("ping -n"));
    }

    #[test]
    fn association_is_removed_only_while_it_still_points_at_this_installation() {
        let script = build_uninstall_script(&reader_plan(r"D:\Apps\SecurePdfReader"));
        let lines: Vec<&str> = script.split("\r\n").collect();
        let owns = index_of(&lines, "&& set \"OWNS_ASSOC=1\"");
        assert!(lines[owns].starts_with(
            "reg query \"HKCU\\Software\\Classes\\SecurePdfSuite.SpdfFile\\shell\\open\\command\" /ve"
        ));
        assert!(lines[owns].contains("/i \"%APPDIR%\\SecurePdfReader.exe\""));
        assert_eq!(lines[owns - 1], "set \"OWNS_ASSOC=\"");

        // Every association delete is guarded; none is unconditional.
        let deletes: Vec<&&str> = lines
            .iter()
            .filter(|l| l.contains("reg delete \"HKCU\\Software\\Classes\\"))
            .collect();
        assert_eq!(deletes.len(), 3);
        for line in &deletes {
            assert!(line.starts_with("if defined OWNS_ASSOC "), "{line}");
        }
        let ext = index_of(&lines, "reg delete \"HKCU\\Software\\Classes\\.spdf\"");
        assert!(lines[ext].contains(
            "reg query \"HKCU\\Software\\Classes\\.spdf\" /ve 2>nul | \"%SystemRoot%\\System32\\find.exe\" /i \"SecurePdfSuite.SpdfFile\" >nul &&"
        ));
        assert!(script.contains(
            "if defined OWNS_ASSOC reg delete \"HKCU\\Software\\Classes\\SecurePdfSuite.spdf\" /f"
        ));

        // The Creator never touches the association.
        let mut creator = reader_plan(r"D:\Apps\SecurePdfCreator");
        creator.association = None;
        let creator_script = build_uninstall_script(&creator);
        assert!(!creator_script.contains("Classes"));
        assert!(!creator_script.contains("OWNS_ASSOC"));
    }

    #[test]
    fn shortcuts_start_menu_folder_and_apps_entry_are_removed() {
        let script = build_uninstall_script(&reader_plan(r"D:\Apps\SecurePdfReader"));
        assert!(script.contains(
            "rmdir \"C:\\Users\\张三\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\SecurePdfSuite\" >nul 2>&1"
        ));
        assert!(script.contains(
            "reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\SecurePdfReader\" /f >nul 2>&1"
        ));
        assert!(script.contains("del /f /q \"%APPDIR%\\SecurePdf.Core.dll\" >nul 2>&1"));
    }

    #[test]
    fn directories_are_removed_deepest_first_and_never_recursively() {
        let script = build_uninstall_script(&reader_plan(r"D:\Apps\SecurePdfReader"));
        let lines: Vec<&str> = script.split("\r\n").collect();
        let deep = index_of(&lines, "rmdir \"%APPDIR%\\docs\\zh\"");
        let shallow = index_of(&lines, "rmdir \"%APPDIR%\\docs\" ");
        let last_file = lines
            .iter()
            .rposition(|l| l.starts_with("del /f /q \"%APPDIR%\\"))
            .unwrap();
        assert!(last_file < deep && deep < shallow);
        let lower = script.to_lowercase();
        assert!(!lower.contains("rmdir /s"));
        assert!(!lower.contains("rd /s"));
        assert!(!lower.contains("del /s"));
        assert!(!lower.contains('*'));
    }

    #[test]
    fn script_leaves_the_directory_then_deletes_itself_on_one_line() {
        let script = build_uninstall_script(&reader_plan(r"D:\Apps\SecurePdfReader"));
        let lines: Vec<&str> = script.split("\r\n").filter(|l| !l.is_empty()).collect();
        let last = lines[lines.len() - 1];
        assert_eq!(
            last,
            "(goto) 2>nul & cd /d \"%SystemRoot%\" & del /f /q \"%APPDIR%\\uninstall_reader.bat\" & rmdir \"%APPDIR%\" 2>nul & ver >nul"
        );
        assert_eq!(lines[lines.len() - 2], "echo 已完成卸载。");
        // The script must not delete itself anywhere before that final line.
        let self_deletes = lines
            .iter()
            .filter(|l| l.contains("uninstall_reader.bat"))
            .count();
        assert_eq!(self_deletes, 1);
    }
}
