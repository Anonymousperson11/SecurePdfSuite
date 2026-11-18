//! LZMS self-extracting installer GUI and packer CLI for `SecurePdfSuite`.

#![cfg_attr(
    all(target_os = "windows", not(test), not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod wizard;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use secure_pdf_installer::logging::write_failure_log;
use secure_pdf_installer::package::{
    AppKind, ArchiveEntry, collect_directory_entries, pack_installer_executable,
    read_embedded_package,
};
use secure_pdf_installer::paths::{ProtectedDirs, normalize_install_dir};
use secure_pdf_installer::selftest::run_isolated_selftest;
use secure_pdf_installer::win32_install::{
    InstallEnvironment, InstallOptions, default_install_dir, perform_install,
};
use winui3::bindings::Windows::Win32::HWND;
use winui3::bootstrap::{MIN_RUNTIME_VERSION, find_installed_runtime, harden_dll_search_path};
use winui3::interop::message_box;

/// Installation (or the requested command) completed successfully.
pub const EXIT_OK: u8 = 0;
/// Installation, packing or another command failed; see `install_error.log`.
pub const EXIT_FAILED: u8 = 1;
/// Invalid command line or unacceptable install directory; nothing was changed.
pub const EXIT_USAGE: u8 = 2;
/// Program files were installed, but Windows App Runtime is missing so the app cannot start yet.
pub const EXIT_RUNTIME_MISSING: u8 = 3;
/// The installer file is incomplete or damaged; nothing was changed.
pub const EXIT_PACKAGE_INVALID: u8 = 4;
/// This executable carries no install payload (a bare installer stub); nothing was changed.
pub const EXIT_NO_PAYLOAD: u8 = 5;
/// The isolated self-test found a discrepancy.
pub const EXIT_SELFTEST_FAILED: u8 = 6;

/// Official download page of the Windows App Runtime.
pub const RUNTIME_DOWNLOAD_URL: &str =
    "https://learn.microsoft.com/zh-cn/windows/apps/windows-app-sdk/downloads";

const USAGE_TEXT: &str = "\
SecurePdfSuite 安装程序

用法:
  1) 交互安装（双击安装包）:
       SecurePdfReaderSetup.exe

  2) 静默安装:
       SecurePdfReaderSetup.exe --silent [--dir <目录>] [--desktop] [--start-menu] [--assoc]

       --dir <目录>     安装目录，必须是完整路径（例如 D:\\Apps\\SecurePdfReader）。
                        省略时：已安装过则装到原目录，否则装到
                        %LOCALAPPDATA%\\Programs\\<程序名>。
       --desktop        创建桌面快捷方式。            默认：不创建
       --start-menu     创建“开始”菜单快捷方式。      默认：不创建
       --assoc          关联 .spdf 文件（仅阅读端）。 默认：不关联
       --no-desktop / --no-start-menu / --no-assoc
                        显式关闭对应选项（与默认值相同，可省略）。
       选项不区分大小写，默认值与 1.x 版本一致。

       退出码: 0 成功；1 安装失败；2 参数或安装目录无效；
               3 文件已安装但系统缺少 Windows App Runtime；
               4 安装包不完整或已损坏；5 本程序不含安装内容。
       失败原因写入安装包所在目录的 install_error.log（该目录不可写时写入 %TEMP%）。

  3) 制作安装包:
       SecurePdfInstaller.exe pack --app <creator|reader> --source-dir <目录> --output <输出.exe> [--stub <底包.exe>]

  4) 隔离自检（在临时目录和临时注册表项中完整执行“安装 → 卸载”，不影响本机已有安装）:
       SecurePdfInstaller.exe INSTALLER_SELFTEST [--report <报告文件>]
       退出码: 0 通过；6 发现问题。

  5) 显示本说明:
       SecurePdfInstaller.exe --help";

/// A command failure with the process exit code it maps to.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Options of a silent (unattended) installation.
#[derive(Debug, PartialEq, Eq, Default)]
struct SilentOptions {
    dir: Option<String>,
    desktop: bool,
    start_menu: bool,
    assoc: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Help,
    Selftest {
        report: Option<PathBuf>,
    },
    Pack {
        app: AppKind,
        source_dir: PathBuf,
        output: PathBuf,
        stub: Option<PathBuf>,
    },
    SilentEmbedded(SilentOptions),
    AutoEmbedded,
}

fn has_silent_flag(args: &[String]) -> bool {
    args.iter().any(|a| a.eq_ignore_ascii_case("--silent"))
}

/// `true` for invocations that must never show UI (unattended install, packing, self-test).
fn is_unattended(args: &[String]) -> bool {
    has_silent_flag(args)
        || args
            .first()
            .is_some_and(|first| first.eq_ignore_ascii_case("pack") || is_selftest_keyword(first))
}

fn is_selftest_keyword(arg: &str) -> bool {
    ["INSTALLER_SELFTEST", "--selftest", "selftest"]
        .iter()
        .any(|kw| arg.eq_ignore_ascii_case(kw))
}

fn main() -> ExitCode {
    harden_dll_search_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let unattended = is_unattended(&args);

    let outcome = parse_cli_args(&args)
        .map_err(|err| Failure::new(EXIT_USAGE, format!("参数错误：{err}")))
        .and_then(run_action);

    match outcome {
        Ok(code) => ExitCode::from(code),
        Err(failure) => {
            eprintln!("{}", failure.message);
            if unattended {
                let _ = write_failure_log(&failure.message);
            } else {
                let text = if failure.code == EXIT_USAGE {
                    format!("{}\n\n{USAGE_TEXT}", failure.message)
                } else {
                    failure.message
                };
                message_box(
                    HWND(std::ptr::null_mut()),
                    "SecurePdfSuite 安装程序",
                    &text,
                    true,
                );
            }
            ExitCode::from(failure.code)
        }
    }
}

fn run_action(action: CliAction) -> Result<u8, Failure> {
    match action {
        CliAction::Help => {
            println!("{USAGE_TEXT}");
            message_box(
                HWND(std::ptr::null_mut()),
                "SecurePdfSuite 安装程序",
                USAGE_TEXT,
                false,
            );
            Ok(EXIT_OK)
        }
        CliAction::Selftest { report } => run_selftest(report.as_deref()),
        CliAction::Pack {
            app,
            source_dir,
            output,
            stub,
        } => run_pack(app, &source_dir, &output, stub.as_deref())
            .map(|()| EXIT_OK)
            .map_err(|err| Failure::new(EXIT_FAILED, format!("制作安装包失败：{err}"))),
        CliAction::SilentEmbedded(options) => run_silent_embedded(&options),
        CliAction::AutoEmbedded => run_auto_embedded(),
    }
}

fn run_selftest(report_path: Option<&Path>) -> Result<u8, Failure> {
    let embedded =
        load_current_embedded_package().map_err(|err| Failure::new(EXIT_PACKAGE_INVALID, err))?;
    let report = run_isolated_selftest(embedded);
    let text = report.render();
    println!("{text}");
    if let Some(path) = report_path {
        let mut bytes = "\u{feff}".as_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        fs::write(path, bytes).map_err(|e| {
            Failure::new(
                EXIT_SELFTEST_FAILED,
                format!("无法写入自检报告 {}：{e}", path.display()),
            )
        })?;
    }
    if report.passed() {
        Ok(EXIT_OK)
    } else {
        Err(Failure::new(EXIT_SELFTEST_FAILED, text))
    }
}

fn run_pack(
    app: AppKind,
    source_dir: &Path,
    output: &Path,
    stub: Option<&Path>,
) -> Result<(), String> {
    let stub_path = match stub {
        Some(p) => p.to_path_buf(),
        None => std::env::current_exe().map_err(|e| format!("无法获取当前可执行文件路径：{e}"))?,
    };
    let base_bytes = fs::read(&stub_path)
        .map_err(|e| format!("读取安装器底包 {} 失败：{e}", stub_path.display()))?;
    let entries = collect_directory_entries(source_dir)?;
    let packed = pack_installer_executable(&base_bytes, app, &entries)?;

    // Read the result back before publishing it: a package that cannot be unpacked is useless.
    match read_embedded_package(&packed)? {
        Some((kind, unpacked)) if kind == app && unpacked == entries => {}
        _ => return Err("安装包回读校验失败。".to_owned()),
    }

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建输出目录 {} 失败：{e}", parent.display()))?;
    }
    fs::write(output, &packed)
        .map_err(|e| format!("写入自解压安装包 {} 失败：{e}", output.display()))?;
    println!(
        "Packed {} files ({} bytes) for {} -> {}",
        entries.len(),
        packed.len(),
        app.display_name(),
        output.display()
    );
    Ok(())
}

fn load_current_embedded_package() -> Result<Option<(AppKind, Vec<ArchiveEntry>)>, String> {
    let exe_path = std::env::current_exe().map_err(|e| format!("无法定位当前程序：{e}"))?;
    let exe_bytes = fs::read(&exe_path)
        .map_err(|e| format!("读取当前可执行文件 {} 失败：{e}", exe_path.display()))?;
    read_embedded_package(&exe_bytes)
}

/// Protected locations of this machine, including the resolved Desktop / Start Menu folders.
fn protected_dirs(env: &InstallEnvironment) -> ProtectedDirs {
    let mut protected = ProtectedDirs::from_environment();
    for dir in [&env.desktop_dir, &env.programs_dir].into_iter().flatten() {
        protected.add_container(dir);
    }
    protected
}

/// Text explaining that the runtime is missing after files were installed to `install_dir`.
fn runtime_missing_message(app_kind: AppKind, install_dir: &Path) -> String {
    let (major, minor) = MIN_RUNTIME_VERSION;
    format!(
        "{} 的程序文件已安装到 {}，但这台电脑尚未安装 Microsoft Windows App Runtime {major}.{minor} \
         或更高版本（x64），程序暂时无法启动。\n请安装运行库后再运行：{RUNTIME_DOWNLOAD_URL}",
        app_kind.uninstall_display_name(),
        install_dir.display()
    )
}

fn run_silent_embedded(options: &SilentOptions) -> Result<u8, Failure> {
    let (app_kind, entries) = load_current_embedded_package()
        .map_err(|err| Failure::new(EXIT_PACKAGE_INVALID, err))?
        .ok_or_else(|| {
            Failure::new(
                EXIT_NO_PAYLOAD,
                "本程序不含安装内容，无法执行静默安装（请使用 pack 制作的安装包）。",
            )
        })?;

    let env = InstallEnvironment::real();
    let raw_dir = options.dir.clone().unwrap_or_else(|| {
        default_install_dir(app_kind, &env)
            .to_string_lossy()
            .into_owned()
    });
    let install_dir = normalize_install_dir(&raw_dir, app_kind, &protected_dirs(&env))
        .map_err(|err| Failure::new(EXIT_USAGE, format!("安装目录无效：{err}")))?;

    let opts = InstallOptions {
        install_dir,
        create_desktop_shortcut: options.desktop,
        create_start_menu_shortcut: options.start_menu,
        associate_spdf: app_kind == AppKind::Reader && options.assoc,
    };
    let summary = perform_install(app_kind, &entries, &opts, &env, &mut |_, _| {})
        .map_err(|err| Failure::new(EXIT_FAILED, format!("安装失败：{err}")))?;
    println!(
        "Installed {} ({} files, {} shortcuts) -> {}",
        app_kind.display_name(),
        summary.files_installed,
        summary.shortcuts_created.len(),
        summary.main_exe.display()
    );
    for warning in &summary.warnings {
        println!("warning: {warning}");
    }

    if find_installed_runtime().is_none() {
        return Err(Failure::new(
            EXIT_RUNTIME_MISSING,
            runtime_missing_message(app_kind, &summary.install_dir),
        ));
    }
    Ok(EXIT_OK)
}

fn run_auto_embedded() -> Result<u8, Failure> {
    match load_current_embedded_package() {
        Err(err) => Err(Failure::new(EXIT_PACKAGE_INVALID, err)),
        Ok(Some((app_kind, entries))) => wizard::run_gui_installer(app_kind, entries)
            .map_err(|err| Failure::new(EXIT_FAILED, err)),
        Ok(None) => {
            println!("{USAGE_TEXT}");
            message_box(
                HWND(std::ptr::null_mut()),
                "SecurePdfSuite 安装程序",
                &format!("本程序不含安装内容。\n\n{USAGE_TEXT}"),
                false,
            );
            Ok(EXIT_NO_PAYLOAD)
        }
    }
}

fn parse_cli_args(args: &[String]) -> Result<CliAction, String> {
    let Some(first) = args.first() else {
        return Ok(CliAction::AutoEmbedded);
    };
    // `help` is only a command in first position; elsewhere it may be a legitimate value.
    if ["--help", "-h", "/?", "help"]
        .iter()
        .any(|kw| first.eq_ignore_ascii_case(kw))
    {
        return Ok(CliAction::Help);
    }
    if is_selftest_keyword(first) {
        return parse_selftest_args(&args[1..]);
    }
    if first.eq_ignore_ascii_case("pack") {
        return parse_pack_args(&args[1..]);
    }
    parse_embedded_args(args)
}

/// Returns the value following the flag at `*idx`; a value may not look like another flag.
fn next_flag_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    *idx += 1;
    match args.get(*idx) {
        Some(value) if !value.starts_with("--") && !value.trim().is_empty() => Ok(value.clone()),
        Some(value) => Err(format!("参数 {flag} 缺少取值（后面是 {value}）。")),
        None => Err(format!("参数 {flag} 缺少取值。")),
    }
}

fn parse_selftest_args(args: &[String]) -> Result<CliAction, String> {
    let mut report = None;
    let mut idx = 0_usize;
    while idx < args.len() {
        if args[idx].eq_ignore_ascii_case("--report") {
            report = Some(PathBuf::from(next_flag_value(args, &mut idx, "--report")?));
        } else {
            return Err(format!("自检命令不支持的参数：{}", args[idx]));
        }
        idx += 1;
    }
    Ok(CliAction::Selftest { report })
}

fn parse_pack_args(args: &[String]) -> Result<CliAction, String> {
    let mut app: Option<AppKind> = None;
    let mut source_dir: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut stub: Option<PathBuf> = None;

    let mut idx = 0_usize;
    while idx < args.len() {
        match args[idx].to_ascii_lowercase().as_str() {
            "--app" => {
                let raw = next_flag_value(args, &mut idx, "--app")?;
                app = Some(parse_app_kind(&raw)?);
            }
            "--source-dir" => {
                let raw = next_flag_value(args, &mut idx, "--source-dir")?;
                source_dir = Some(PathBuf::from(raw));
            }
            "--output" => {
                let raw = next_flag_value(args, &mut idx, "--output")?;
                output = Some(PathBuf::from(raw));
            }
            "--stub" => {
                let raw = next_flag_value(args, &mut idx, "--stub")?;
                stub = Some(PathBuf::from(raw));
            }
            _ => return Err(format!("`pack` 不支持的参数：{}", args[idx])),
        }
        idx += 1;
    }

    Ok(CliAction::Pack {
        app: app.ok_or_else(|| "`pack` 缺少必填参数 `--app <creator|reader>`。".to_owned())?,
        source_dir: source_dir
            .ok_or_else(|| "`pack` 缺少必填参数 `--source-dir <目录>`。".to_owned())?,
        output: output.ok_or_else(|| "`pack` 缺少必填参数 `--output <输出.exe>`。".to_owned())?,
        stub,
    })
}

fn parse_app_kind(raw: &str) -> Result<AppKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "creator" | "securepdfcreator" => Ok(AppKind::Creator),
        "reader" | "securepdfreader" => Ok(AppKind::Reader),
        _ => Err(format!(
            "无效应用类型 `{raw}`，仅支持 `creator` 或 `reader`。"
        )),
    }
}

/// Records an opt-in (`true`) or opt-out (`false`) request, rejecting contradictory pairs.
fn set_choice(slot: &mut Option<bool>, value: bool, flag: &str) -> Result<(), String> {
    match *slot {
        Some(previous) if previous != value => Err(format!(
            "参数 {flag} 与之前给出的相反选项冲突，请只保留其中一个。"
        )),
        _ => {
            *slot = Some(value);
            Ok(())
        }
    }
}

/// Parses the options of an embedded (Setup.exe) invocation.
///
/// Accepts the v1.x opt-in flags (`--desktop`, `--start-menu`, `--assoc`) and the opt-out
/// spellings (`--no-desktop`, `--no-start-menu`, `--no-assoc`), all case-insensitive. As in
/// v1.x, nothing optional is created unless asked for.
fn parse_embedded_args(args: &[String]) -> Result<CliAction, String> {
    let mut silent = false;
    let mut dir: Option<String> = None;
    let mut desktop: Option<bool> = None;
    let mut start_menu: Option<bool> = None;
    let mut assoc: Option<bool> = None;

    let mut idx = 0_usize;
    while idx < args.len() {
        let flag = args[idx].as_str();
        match flag.to_ascii_lowercase().as_str() {
            "--silent" => silent = true,
            "--dir" => {
                if dir.is_some() {
                    return Err("参数 --dir 只能出现一次。".to_owned());
                }
                dir = Some(next_flag_value(args, &mut idx, "--dir")?);
            }
            "--desktop" => set_choice(&mut desktop, true, flag)?,
            "--no-desktop" => set_choice(&mut desktop, false, flag)?,
            "--start-menu" => set_choice(&mut start_menu, true, flag)?,
            "--no-start-menu" => set_choice(&mut start_menu, false, flag)?,
            "--assoc" => set_choice(&mut assoc, true, flag)?,
            "--no-assoc" => set_choice(&mut assoc, false, flag)?,
            _ => return Err(format!("未知参数：{flag}")),
        }
        idx += 1;
    }

    if !silent {
        return Err("安装选项需要与 --silent 一起使用；不带参数运行则进入安装向导。".to_owned());
    }
    Ok(CliAction::SilentEmbedded(SilentOptions {
        dir,
        desktop: desktop.unwrap_or(false),
        start_menu: start_menu.unwrap_or(false),
        assoc: assoc.unwrap_or(false),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn silent(items: &[&str]) -> Result<SilentOptions, String> {
        match parse_cli_args(&vec_s(items))? {
            CliAction::SilentEmbedded(options) => Ok(options),
            other => Err(format!("unexpected action {other:?}")),
        }
    }

    #[test]
    fn cli_parses_selftest_and_empty() {
        assert_eq!(parse_cli_args(&[]), Ok(CliAction::AutoEmbedded));
        for keyword in ["INSTALLER_SELFTEST", "installer_selftest", "--selftest"] {
            assert_eq!(
                parse_cli_args(&vec_s(&[keyword])),
                Ok(CliAction::Selftest { report: None })
            );
        }
        assert_eq!(
            parse_cli_args(&vec_s(&["INSTALLER_SELFTEST", "--report", r"C:\t\r.log"])),
            Ok(CliAction::Selftest {
                report: Some(PathBuf::from(r"C:\t\r.log"))
            })
        );
        assert!(parse_cli_args(&vec_s(&["INSTALLER_SELFTEST", "--report"])).is_err());
        assert!(parse_cli_args(&vec_s(&["INSTALLER_SELFTEST", "extra"])).is_err());
    }

    #[test]
    fn cli_parses_pack_command() {
        let act = parse_cli_args(&vec_s(&[
            "pack",
            "--app",
            "reader",
            "--source-dir",
            "dist/reader",
            "--output",
            "dist/Setup.exe",
            "--stub",
            "dist/stub.exe",
        ]))
        .expect("pack args must parse");
        assert_eq!(
            act,
            CliAction::Pack {
                app: AppKind::Reader,
                source_dir: PathBuf::from("dist/reader"),
                output: PathBuf::from("dist/Setup.exe"),
                stub: Some(PathBuf::from("dist/stub.exe")),
            }
        );
        assert!(parse_cli_args(&vec_s(&["pack", "--app", "reader"])).is_err());
        assert!(parse_cli_args(&vec_s(&["pack", "--app", "writer"])).is_err());
        assert!(parse_cli_args(&vec_s(&["pack", "--app", "--source-dir", "x"])).is_err());
    }

    #[test]
    fn silent_defaults_match_v1_nothing_optional_is_created() {
        assert_eq!(silent(&["--silent"]), Ok(SilentOptions::default()));
        assert_eq!(
            silent(&["--silent", "--dir", r"C:\Apps\SecurePdf"]),
            Ok(SilentOptions {
                dir: Some(r"C:\Apps\SecurePdf".to_owned()),
                ..SilentOptions::default()
            })
        );
    }

    #[test]
    fn v1_opt_in_flags_are_accepted_in_any_order_and_case() {
        // The exact command line documented for v1.x deployment scripts.
        assert_eq!(
            silent(&[
                "--silent",
                "--dir",
                r"D:\SecurePdfSuite\SecurePdfReader",
                "--desktop",
                "--start-menu",
                "--assoc"
            ]),
            Ok(SilentOptions {
                dir: Some(r"D:\SecurePdfSuite\SecurePdfReader".to_owned()),
                desktop: true,
                start_menu: true,
                assoc: true,
            })
        );
        assert_eq!(
            silent(&["--ASSOC", "--Desktop", "--SILENT", "--DIR", r"D:\x"]),
            Ok(SilentOptions {
                dir: Some(r"D:\x".to_owned()),
                desktop: true,
                start_menu: false,
                assoc: true,
            })
        );
    }

    #[test]
    fn v2_opt_out_flags_are_accepted_and_contradictions_are_rejected() {
        assert_eq!(
            silent(&["--silent", "--no-desktop", "--No-Start-Menu", "--no-assoc"]),
            Ok(SilentOptions::default())
        );
        assert_eq!(
            silent(&["--silent", "--desktop", "--no-assoc"]),
            Ok(SilentOptions {
                desktop: true,
                ..SilentOptions::default()
            })
        );
        // Repeating the same choice is harmless; opposite choices are an error.
        assert!(silent(&["--silent", "--desktop", "--DESKTOP"]).is_ok());
        assert!(silent(&["--silent", "--desktop", "--no-desktop"]).is_err());
        assert!(silent(&["--silent", "--no-assoc", "--assoc"]).is_err());
        assert!(silent(&["--silent", "--start-menu", "--no-start-menu"]).is_err());
    }

    #[test]
    fn dir_requires_a_value_that_is_not_another_flag() {
        // The flag after --dir must not be swallowed as the directory.
        let err = silent(&["--silent", "--dir", "--no-desktop"]).unwrap_err();
        assert!(err.contains("--dir") && err.contains("缺少取值"), "{err}");
        assert!(
            silent(&["--silent", "--dir"])
                .unwrap_err()
                .contains("缺少取值")
        );
        assert!(silent(&["--dir", "--silent"]).is_err());
        assert!(silent(&["--silent", "--dir", "   "]).is_err());
        assert!(silent(&["--silent", "--dir", r"D:\a", "--dir", r"D:\b"]).is_err());
        // A value containing spaces or a single leading dash is still a value.
        assert_eq!(
            silent(&["--silent", "--dir", r"D:\My Apps\-reader"]).map(|o| o.dir),
            Ok(Some(r"D:\My Apps\-reader".to_owned()))
        );
    }

    #[test]
    fn options_without_silent_and_unknown_flags_are_rejected() {
        assert!(parse_cli_args(&vec_s(&["--no-desktop"])).is_err());
        assert!(parse_cli_args(&vec_s(&["--desktop"])).is_err());
        assert!(parse_cli_args(&vec_s(&["--dir", r"D:\x"])).is_err());
        assert!(silent(&["--silent", "--unknown"]).is_err());
        assert!(silent(&["--silent", "stray"]).is_err());
    }

    #[test]
    fn help_is_only_recognised_as_the_first_argument() {
        for keyword in ["--help", "-h", "/?", "help", "HELP", "--Help"] {
            assert_eq!(parse_cli_args(&vec_s(&[keyword])), Ok(CliAction::Help));
        }
        // A directory literally named "help" is a directory, not a request for help.
        assert_eq!(
            silent(&["--silent", "--dir", "help"]).map(|o| o.dir),
            Ok(Some("help".to_owned()))
        );
        assert_eq!(
            silent(&["--silent", "--dir", r"D:\help"]).map(|o| o.dir),
            Ok(Some(r"D:\help".to_owned()))
        );
        // `help` in a later position of an install command is just an unknown argument.
        assert!(silent(&["--silent", "help"]).is_err());
        assert!(silent(&["--silent", "--help"]).is_err());
        let pack = parse_cli_args(&vec_s(&[
            "pack",
            "--app",
            "creator",
            "--source-dir",
            "help",
            "--output",
            "help.exe",
        ]));
        assert!(matches!(pack, Ok(CliAction::Pack { .. })), "{pack:?}");
    }

    #[test]
    fn unattended_invocations_are_detected_for_ui_suppression() {
        assert!(is_unattended(&vec_s(&["--SILENT"])));
        assert!(is_unattended(&vec_s(&["--dir", "x", "--silent"])));
        assert!(is_unattended(&vec_s(&["pack"])));
        assert!(is_unattended(&vec_s(&["INSTALLER_SELFTEST"])));
        assert!(!is_unattended(&[]));
        assert!(!is_unattended(&vec_s(&["--help"])));
        assert!(!is_unattended(&vec_s(&["--desktop"])));
    }

    #[test]
    fn exit_codes_are_distinct_and_usage_text_documents_them_and_the_defaults() {
        let codes = [
            EXIT_OK,
            EXIT_FAILED,
            EXIT_USAGE,
            EXIT_RUNTIME_MISSING,
            EXIT_PACKAGE_INVALID,
            EXIT_NO_PAYLOAD,
            EXIT_SELFTEST_FAILED,
        ];
        for (i, a) in codes.iter().enumerate() {
            for b in &codes[i + 1..] {
                assert_ne!(a, b);
            }
        }
        for needle in [
            "--desktop",
            "--start-menu",
            "--assoc",
            "--no-desktop",
            "--no-start-menu",
            "--no-assoc",
            "默认：不创建",
            "默认：不关联",
            "不区分大小写",
            "install_error.log",
            "INSTALLER_SELFTEST",
        ] {
            assert!(USAGE_TEXT.contains(needle), "usage lacks {needle}");
        }
    }

    #[test]
    fn silent_install_rejects_root_and_relative_directories_as_usage_errors() {
        let env = InstallEnvironment::real();
        let protected = protected_dirs(&env);
        for bad in [r"C:\", ".", "relative", r"C:\Windows\Temp\x"] {
            assert!(
                normalize_install_dir(bad, AppKind::Reader, &protected).is_err(),
                "{bad:?} must be rejected"
            );
        }
        // The real Desktop of this machine is a protected container too.
        if let Some(desktop) = &env.desktop_dir {
            assert!(
                normalize_install_dir(&desktop.to_string_lossy(), AppKind::Reader, &protected)
                    .is_err()
            );
        }
        let message = runtime_missing_message(AppKind::Reader, Path::new(r"D:\Apps\R"));
        assert!(message.contains("Windows App Runtime 1.6") && message.contains(r"D:\Apps\R"));
    }
}
