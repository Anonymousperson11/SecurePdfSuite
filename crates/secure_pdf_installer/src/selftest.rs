//! Isolated end-to-end self-test: a real "install → verify → uninstall → verify" cycle on disk
//! that never touches the user's real registry keys, Desktop, Start Menu or install locations.
//!
//! All folders live under a private temporary root and all registry writes go below a
//! throw-away key `HKCU\Software\SecurePdfSuite_SelfTest\run-<pid>-<n>`; both are removed when the
//! test finishes, whatever its outcome.

use std::fs;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::package::{AppKind, ArchiveEntry, run_pack_roundtrip_check};
use crate::registry::{
    delete_hkcu_key_if_empty, delete_hkcu_tree, get_hkcu_sz, hkcu_key_exists, set_hkcu_sz,
};
use crate::win32_install::{
    INSTALL_LOG_NAME, InstallEnvironment, InstallOptions, PRODUCT_VERSION, SPDF_EXTENSION,
    SPDF_PROG_ID, association_points_to, default_install_dir, perform_install,
    register_spdf_association, uninstall_entry_exists,
};

/// Parent of every throw-away self-test registry key.
pub const SELFTEST_REGISTRY_PARENT: &str = r"Software\SecurePdfSuite_SelfTest";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Outcome of the self-test: every check that ran, and the ones that failed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SelfTestReport {
    /// Human-readable trace of the checks performed.
    pub log: Vec<String>,
    /// Descriptions of failed checks (empty when the self-test passed).
    pub failures: Vec<String>,
    /// Private temporary folder used by this run (removed again before returning).
    pub sandbox_dir: Option<PathBuf>,
    /// Throw-away HKCU subkey used by this run (removed again before returning).
    pub sandbox_registry_key: Option<String>,
}

impl SelfTestReport {
    /// `true` when no check failed.
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// Full report text (trace followed by the verdict).
    #[must_use]
    pub fn render(&self) -> String {
        let mut lines = self.log.clone();
        if self.failures.is_empty() {
            lines.push("SecurePdfInstaller selftest: OK".to_owned());
        } else {
            lines.push(format!(
                "SecurePdfInstaller selftest: FAILED ({} 项)",
                self.failures.len()
            ));
            lines.extend(self.failures.iter().map(|f| format!("  FAIL: {f}")));
        }
        lines.join("\r\n")
    }

    fn note(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
    }

    fn check(&mut self, ok: bool, what: impl Into<String>) {
        let what = what.into();
        if ok {
            self.log.push(format!("  ok   {what}"));
        } else {
            self.log.push(format!("  FAIL {what}"));
            self.failures.push(what);
        }
    }
}

/// Removes the sandbox folder and registry key when dropped.
struct Sandbox {
    root_dir: PathBuf,
    registry_root: String,
}

impl Sandbox {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let tag = format!(
            "{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let root_dir = std::env::temp_dir().join(format!("SecurePdfSuite_SelfTest_{tag}"));
        let registry_root = format!(r"{SELFTEST_REGISTRY_PARENT}\run-{tag}");
        let sandbox = Self {
            root_dir,
            registry_root,
        };
        sandbox.wipe();
        fs::create_dir_all(&sandbox.root_dir)
            .map_err(|e| format!("无法创建自检目录 {}：{e}", sandbox.root_dir.display()))?;
        Ok(sandbox)
    }

    fn wipe(&self) {
        // The temporary root is created by, and private to, this self-test run.
        let _ = fs::remove_dir_all(&self.root_dir);
        let _ = delete_hkcu_tree(&self.registry_root);
        delete_hkcu_key_if_empty(SELFTEST_REGISTRY_PARENT);
    }

    fn env(&self) -> InstallEnvironment {
        InstallEnvironment::sandbox(&self.root_dir, &self.registry_root)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.wipe();
    }
}

fn synthetic_payload(app_kind: AppKind) -> Vec<ArchiveEntry> {
    vec![
        ArchiveEntry {
            rel_path: app_kind.exe_name().to_owned(),
            data: b"MZ selftest main executable".to_vec(),
        },
        ArchiveEntry {
            rel_path: "pdfium.dll".to_owned(),
            data: vec![0x5A; 4096],
        },
        ArchiveEntry {
            rel_path: "docs/说明 100%.txt".to_owned(),
            data: "自检用说明文件".as_bytes().to_vec(),
        },
    ]
}

fn target_of(install_dir: &Path, entry: &ArchiveEntry) -> PathBuf {
    let mut path = install_dir.to_path_buf();
    for part in entry.rel_path.replace('\\', "/").split('/') {
        path.push(part);
    }
    path
}

fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(read) = fs::read_dir(dir) else {
        return out;
    };
    for item in read.flatten() {
        let path = item.path();
        if path.is_dir() {
            out.extend(list_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Names of whatever is still directly inside `dir` (empty when it is gone or empty).
fn remaining_entries(dir: &Path) -> Vec<String> {
    fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |read| {
            read.flatten()
                .map(|item| item.file_name().to_string_lossy().into_owned())
                .collect()
        },
    )
}

/// Runs the generated uninstall script the way Explorer would: from inside the install folder.
fn run_uninstall_script(install_dir: &Path, script_name: &str) -> Result<i32, String> {
    let system_root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .ok_or_else(|| "环境变量 SystemRoot 不存在。".to_owned())?;
    let cmd_exe = PathBuf::from(system_root).join("System32").join("cmd.exe");
    let mut child = Command::new(cmd_exe)
        .args(["/d", "/c", script_name])
        .current_dir(install_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("无法启动卸载脚本：{e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
            Ok(None) if started.elapsed() > UNINSTALL_TIMEOUT => {
                let _ = child.kill();
                return Err("卸载脚本超时未结束。".to_owned());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("等待卸载脚本失败：{e}")),
        }
    }
}

fn verify_installed(
    report: &mut SelfTestReport,
    env: &InstallEnvironment,
    app_kind: AppKind,
    install_dir: &Path,
    entries: &[ArchiveEntry],
    expect_association: bool,
) {
    for entry in entries {
        let target = target_of(install_dir, entry);
        let same = fs::read(&target).is_ok_and(|bytes| bytes == entry.data);
        report.check(same, format!("文件内容一致：{}", entry.rel_path));
    }
    let expected_count = entries.len() + 2; // payload + uninstall script + install log
    let on_disk = list_files(install_dir);
    report.check(
        on_disk.len() == expected_count,
        format!(
            "安装目录文件清单（期望 {expected_count} 个，实际 {} 个）",
            on_disk.len()
        ),
    );
    let script = install_dir.join(app_kind.uninstall_script_name());
    let script_text = fs::read_to_string(&script).unwrap_or_default();
    report.check(
        script_text.starts_with("@echo off\r\nchcp 65001 >nul\r\ncd /d \"%SystemRoot%\"\r\nsetlocal DisableDelayedExpansion"),
        "卸载脚本已生成且文件头正确",
    );
    report.check(
        install_dir.join(INSTALL_LOG_NAME).is_file(),
        "安装日志已写入",
    );
    report.check(
        env.desktop_shortcut(app_kind).is_some_and(|p| p.is_file()),
        "桌面快捷方式已创建",
    );
    report.check(
        env.start_menu_shortcut(app_kind)
            .is_some_and(|p| p.is_file()),
        "开始菜单快捷方式已创建",
    );
    let key = env.uninstall_key(app_kind);
    report.check(
        get_hkcu_sz(&key, "InstallLocation").is_some_and(|v| Path::new(&v) == install_dir),
        "卸载项 InstallLocation 指向安装目录",
    );
    report.check(
        get_hkcu_sz(&key, "DisplayVersion").as_deref() == Some(PRODUCT_VERSION),
        "卸载项 DisplayVersion 为当前版本",
    );
    report.check(
        get_hkcu_sz(&key, "UninstallString")
            .is_some_and(|v| Path::new(v.trim_matches('"')) == script),
        "卸载项 UninstallString 指向卸载脚本",
    );
    if app_kind == AppKind::Reader {
        let main_exe = install_dir.join(app_kind.exe_name());
        report.check(
            association_points_to(env, &main_exe) == expect_association,
            if expect_association {
                ".spdf 关联指向本次安装"
            } else {
                "未请求关联时不写入 .spdf 关联"
            },
        );
    }
}

fn verify_removed(
    report: &mut SelfTestReport,
    env: &InstallEnvironment,
    app_kind: AppKind,
    install_dir: &Path,
    expect_dir_removed: bool,
) {
    report.check(
        !install_dir.join(app_kind.exe_name()).exists(),
        "卸载后主程序已删除",
    );
    report.check(
        !install_dir.join(app_kind.uninstall_script_name()).exists(),
        "卸载脚本已自删",
    );
    if expect_dir_removed {
        let leftovers = remaining_entries(install_dir);
        report.check(
            !install_dir.exists(),
            format!("卸载后安装目录已移除（残留：{leftovers:?}）"),
        );
    }
    report.check(
        env.desktop_shortcut(app_kind).is_some_and(|p| !p.exists()),
        "卸载后桌面快捷方式已删除",
    );
    report.check(
        env.start_menu_dir().is_some_and(|p| !p.exists()),
        "卸载后开始菜单文件夹已删除",
    );
    report.check(
        !uninstall_entry_exists(env, app_kind),
        "卸载后“应用和功能”卸载项已删除",
    );
}

fn install_all(
    report: &mut SelfTestReport,
    env: &InstallEnvironment,
    app_kind: AppKind,
    install_dir: &Path,
    entries: &[ArchiveEntry],
    associate: bool,
) -> bool {
    let opts = InstallOptions {
        install_dir: install_dir.to_path_buf(),
        create_desktop_shortcut: true,
        create_start_menu_shortcut: true,
        associate_spdf: associate,
    };
    match perform_install(app_kind, entries, &opts, env, &mut |_, _| {}) {
        Ok(summary) => {
            report.check(
                summary.warnings.is_empty(),
                format!("安装无警告 {:?}", summary.warnings),
            );
            true
        }
        Err(err) => {
            report.check(false, format!("安装失败：{err}"));
            false
        }
    }
}

fn uninstall(report: &mut SelfTestReport, app_kind: AppKind, install_dir: &Path) {
    match run_uninstall_script(install_dir, app_kind.uninstall_script_name()) {
        Ok(code) => report.check(code == 0, format!("卸载脚本退出码为 0（实际 {code}）")),
        Err(err) => report.check(false, err),
    }
}

/// Round 1: fresh install into a path with spaces, CJK characters and `%`, then uninstall.
fn round_fresh(
    report: &mut SelfTestReport,
    sandbox: &Sandbox,
    app_kind: AppKind,
    entries: &[ArchiveEntry],
) {
    report.note("[1/3] 全新安装 → 校验 → 卸载 → 校验无残留");
    let env = sandbox.env();
    let install_dir = sandbox
        .root_dir
        .join("安装 目录 100%")
        .join(app_kind.install_folder_name());
    if !install_all(report, &env, app_kind, &install_dir, entries, true) {
        return;
    }
    verify_installed(report, &env, app_kind, &install_dir, entries, true);
    uninstall(report, app_kind, &install_dir);
    verify_removed(report, &env, app_kind, &install_dir, true);
    if app_kind == AppKind::Reader {
        let classes = &env.classes_root;
        report.check(
            !hkcu_key_exists(&format!(r"{classes}\{SPDF_EXTENSION}"))
                && !hkcu_key_exists(&format!(r"{classes}\{SPDF_PROG_ID}")),
            "卸载后本次安装创建的 .spdf 关联已删除",
        );
    }
}

/// Round 2: upgrade over a simulated v1.x installation; the association is then re-pointed to
/// another program and a user file is added before uninstalling.
fn round_upgrade(
    report: &mut SelfTestReport,
    sandbox: &Sandbox,
    app_kind: AppKind,
    entries: &[ArchiveEntry],
) {
    report.note("[2/3] 覆盖 v1.x → 校验只剩一套 → 关联改给其他程序 → 卸载");
    let env = sandbox.env();
    let Some(local) = env.local_app_data.clone() else {
        report.check(false, "自检环境缺少 LocalAppData");
        return;
    };
    let v1_dir = local.join(app_kind.install_folder_name());
    let seeded = (|| -> std::io::Result<()> {
        fs::create_dir_all(&v1_dir)?;
        fs::write(v1_dir.join(app_kind.exe_name()), b"MZ v1 main")?;
        fs::write(
            v1_dir.join(format!("{}.config", app_kind.exe_name())),
            b"<configuration/>",
        )?;
        fs::write(v1_dir.join("SecurePdf.Core.dll"), b"MZ v1 core")?;
        fs::write(v1_dir.join("BouncyCastle.Cryptography.dll"), b"MZ v1 bc")?;
        Ok(())
    })();
    if let Err(err) = seeded {
        report.check(false, format!("无法布置 v1.x 模拟安装：{err}"));
        return;
    }
    let key = env.uninstall_key(app_kind);
    let _ = set_hkcu_sz(&key, "DisplayVersion", "1.1.0");
    let _ = set_hkcu_sz(&key, "InstallLocation", &v1_dir.to_string_lossy());

    let install_dir = default_install_dir(app_kind, &env);
    report.check(install_dir == v1_dir, "检测到 v1.x 并默认安装到同一目录");
    if !install_all(report, &env, app_kind, &install_dir, entries, true) {
        return;
    }
    verify_installed(report, &env, app_kind, &install_dir, entries, true);
    for obsolete in [
        format!("{}.config", app_kind.exe_name()),
        "SecurePdf.Core.dll".to_owned(),
        "BouncyCastle.Cryptography.dll".to_owned(),
    ] {
        report.check(
            !install_dir.join(&obsolete).exists(),
            format!("v1.x 遗留文件已清理：{obsolete}"),
        );
    }
    report.check(
        !local
            .join("Programs")
            .join(app_kind.install_folder_name())
            .exists(),
        "没有在新默认目录另装一套",
    );

    let user_file = install_dir.join("用户自己的文件.txt");
    let _ = fs::write(&user_file, b"keep");
    let classes = env.classes_root.clone();
    let ext_key = format!(r"{classes}\{SPDF_EXTENSION}");
    if app_kind == AppKind::Reader {
        let _ = set_hkcu_sz(&ext_key, "", "OtherVendor.Document");
    }

    uninstall(report, app_kind, &install_dir);
    verify_removed(report, &env, app_kind, &install_dir, false);
    report.check(
        user_file.is_file() && list_files(&install_dir).len() == 1,
        "卸载保留用户自己放入的文件，且只剩该文件",
    );
    if app_kind == AppKind::Reader {
        report.check(
            get_hkcu_sz(&ext_key, "").as_deref() == Some("OtherVendor.Document"),
            ".spdf 已改给其他程序时卸载不动它",
        );
        report.check(
            !hkcu_key_exists(&format!(r"{classes}\{SPDF_PROG_ID}")),
            "卸载后本产品的 ProgID 已删除",
        );
    }
}

/// Round 3 (Reader only): install without association while `.spdf` belongs to a different
/// installation; uninstalling must leave that association alone.
fn round_foreign_association(
    report: &mut SelfTestReport,
    sandbox: &Sandbox,
    entries: &[ArchiveEntry],
) {
    report.note("[3/3] 不关联安装 → 卸载不得删除属于另一处安装的 .spdf 关联");
    let env = sandbox.env();
    let other_exe = sandbox
        .root_dir
        .join("另一处安装")
        .join("SecurePdfReader.exe");
    let _ = delete_hkcu_tree(&env.classes_root);
    if let Err(err) = register_spdf_association(&env, &other_exe) {
        report.check(false, format!("无法布置外部关联：{err}"));
        return;
    }
    let install_dir = sandbox.root_dir.join("第三轮").join("SecurePdfReader");
    if !install_all(report, &env, AppKind::Reader, &install_dir, entries, false) {
        return;
    }
    verify_installed(report, &env, AppKind::Reader, &install_dir, entries, false);
    report.check(
        association_points_to(&env, &other_exe),
        "不关联安装没有改动已有关联",
    );
    uninstall(report, AppKind::Reader, &install_dir);
    verify_removed(report, &env, AppKind::Reader, &install_dir, true);
    report.check(
        association_points_to(&env, &other_exe),
        "卸载后属于另一处安装的 .spdf 关联原样保留",
    );
}

/// Runs the complete isolated self-test.
///
/// `embedded` is the payload carried by the running installer, if any; a bare installer stub
/// uses a small synthetic Reader payload instead so the on-disk cycle is still exercised.
#[must_use]
pub fn run_isolated_selftest(embedded: Option<(AppKind, Vec<ArchiveEntry>)>) -> SelfTestReport {
    let mut report = SelfTestReport::default();

    report.note("[0/3] LZMS 打包 → 解包 → 损坏拦截（内存）");
    match run_pack_roundtrip_check() {
        Ok(()) => report.check(true, "打包解包往返与损坏拦截"),
        Err(err) => report.check(false, format!("打包解包往返失败：{err}")),
    }

    let (app_kind, entries) = if let Some((kind, entries)) = embedded {
        report.note(format!(
            "（使用内嵌载荷：{}，{} 个文件）",
            kind.exe_name(),
            entries.len()
        ));
        (kind, entries)
    } else {
        report.note("（当前程序不含安装载荷，使用内置的模拟阅读端载荷）");
        (AppKind::Reader, synthetic_payload(AppKind::Reader))
    };

    let sandbox = match Sandbox::create() {
        Ok(sandbox) => sandbox,
        Err(err) => {
            report.check(false, err);
            return report;
        }
    };
    report.note(format!(
        "隔离目录：{}；隔离注册表：HKCU\\{}",
        sandbox.root_dir.display(),
        sandbox.registry_root
    ));
    report.sandbox_dir = Some(sandbox.root_dir.clone());
    report.sandbox_registry_key = Some(sandbox.registry_root.clone());

    round_fresh(&mut report, &sandbox, app_kind, &entries);
    round_upgrade(&mut report, &sandbox, app_kind, &entries);
    if app_kind == AppKind::Reader {
        round_foreign_association(&mut report, &sandbox, &entries);
    }

    let (root_dir, registry_root) = (sandbox.root_dir.clone(), sandbox.registry_root.clone());
    drop(sandbox);
    report.check(!root_dir.exists(), "自检结束后隔离目录已清除");
    report.check(
        !hkcu_key_exists(&registry_root),
        "自检结束后隔离注册表项已清除",
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_install_uninstall_cycle_passes_and_cleans_up_after_itself() {
        let report = run_isolated_selftest(None);
        assert!(report.passed(), "{}", report.render());
        // The run must have exercised all three on-disk rounds, not just the codec check.
        let text = report.render();
        for marker in ["[1/3]", "[2/3]", "[3/3]", "卸载脚本退出码为 0"] {
            assert!(text.contains(marker), "missing {marker} in:\n{text}");
        }
        assert!(text.ends_with("SecurePdfInstaller selftest: OK"));
        let key = report.sandbox_registry_key.clone().unwrap();
        assert!(key.starts_with(r"Software\SecurePdfSuite_SelfTest\run-"));
        assert!(!hkcu_key_exists(&key));
        let dir = report.sandbox_dir.unwrap();
        assert!(dir.starts_with(std::env::temp_dir()));
        assert!(!dir.exists());
    }

    #[test]
    fn creator_payload_cycle_never_creates_association_keys() {
        let entries = synthetic_payload(AppKind::Creator);
        let report = run_isolated_selftest(Some((AppKind::Creator, entries)));
        assert!(report.passed(), "{}", report.render());
        assert!(!report.render().contains("[3/3]"));
    }

    #[test]
    fn report_renders_failures_and_reports_not_passed() {
        let mut report = SelfTestReport::default();
        report.check(true, "第一项");
        report.check(false, "第二项");
        assert!(!report.passed());
        let text = report.render();
        assert!(text.contains("  ok   第一项"));
        assert!(text.contains("  FAIL 第二项"));
        assert!(text.contains("selftest: FAILED (1 项)"));
    }
}
