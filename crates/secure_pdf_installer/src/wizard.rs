//! `WinUI 3` installation wizard with a plain `Win32` fallback when the runtime is absent.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secure_pdf_installer::package::{AppKind, ArchiveEntry};
use secure_pdf_installer::paths::{ProtectedDirs, normalize_install_dir, with_app_folder};
use secure_pdf_installer::win32_install::{
    ExistingInstall, InstallEnvironment, InstallOptions, InstallSummary, default_install_dir,
    detect_existing_install, is_target_running, perform_install,
};
use windows_core::{HSTRING, Interface, PCWSTR};
use winui3::app::{enqueue_ui, run_winui_app};
use winui3::bindings::Microsoft::UI::Dispatching::DispatcherQueue;
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    Border, Button, CheckBox, InfoBar, InfoBarSeverity, ProgressBar, StackPanel, TextBlock, TextBox,
};
use winui3::bindings::Microsoft::UI::Xaml::FrameworkElement;
use winui3::bindings::Windows::Win32 as w32;
use winui3::bootstrap::{MIN_RUNTIME_VERSION, bootstrap_runtime, exit_ui_thread};
use winui3::controls::{
    ACCENT_TEXT, BG_CANVAS, TEXT_PRIMARY, TEXT_SECONDARY, add_grid_cols, brush_base, button,
    card_border, check_box, gl_auto, gl_px, gl_star, grid_rows, info_bar, pad, place_in_grid,
    progress_bar, show_info, stack_v, text_block, text_box, uniform,
};
use winui3::dialog::show_confirm_dialog;
use winui3::interop::{
    confirm_message_box, message_box, pick_folder, shell_open, to_wide_null, window_hwnd,
};
use winui3::window::{FluentWindow, WindowChromeOptions};

use crate::{EXIT_OK, EXIT_RUNTIME_MISSING, RUNTIME_DOWNLOAD_URL, protected_dirs};

#[derive(Default)]
struct WizardState {
    installing: bool,
    installed_exe: Option<PathBuf>,
}

#[derive(Clone)]
struct WizardUi {
    app_kind: AppKind,
    entries: Arc<Vec<ArchiveEntry>>,
    env: InstallEnvironment,
    protected: ProtectedDirs,
    win: FluentWindow,
    dq: DispatcherQueue,
    alert: InfoBar,
    dir_box: TextBox,
    browse_btn: Button,
    chk_desktop: CheckBox,
    chk_start_menu: CheckBox,
    chk_assoc: Option<CheckBox>,
    status_lbl: TextBlock,
    progress: ProgressBar,
    action_btn: Button,
    close_btn: Button,
    state: Arc<Mutex<WizardState>>,
}

thread_local! {
    /// The wizard's UI handles stay on the UI thread; background work reaches them only through
    /// closures enqueued on the dispatcher queue.
    static WIZARD: RefCell<Option<WizardUi>> = const { RefCell::new(None) };
}

/// Runs `f` with the wizard UI. Must be called on the UI thread; a no-op elsewhere.
fn with_ui(f: impl FnOnce(&WizardUi)) {
    let ui = WIZARD.with(|slot| slot.borrow().clone());
    if let Some(ui) = ui {
        f(&ui);
    }
}

/// Launches the `WinUI 3` installation wizard, or the `Win32` fallback if the runtime is missing.
///
/// Returns the process exit code.
///
/// # Errors
///
/// Returns a descriptive error string if initialization or fallback installation fails.
pub fn run_gui_installer(app_kind: AppKind, entries: Vec<ArchiveEntry>) -> Result<u8, String> {
    let entries_arc = Arc::new(entries);
    if bootstrap_runtime().is_err() {
        return run_win32_fallback(app_kind, &entries_arc);
    }

    let title = window_title(app_kind);
    let launch_kind = app_kind;
    let launch_entries = Arc::clone(&entries_arc);
    run_winui_app(&title, move || {
        if let Err(ui_err) = build_wizard_window(launch_kind, &launch_entries) {
            message_box(
                w32::HWND(std::ptr::null_mut()),
                &window_title(launch_kind),
                &format!(
                    "无法显示安装向导窗口（错误代码 0x{:08X}）。",
                    ui_err.code().0
                ),
                true,
            );
            exit_ui_thread();
        }
    })
    .map(|()| EXIT_OK)
    .map_err(|e| format!("安装向导运行失败（错误代码 0x{:08X}）。", e.code().0))
}

fn window_title(app_kind: AppKind) -> String {
    format!("{} 安装程序", app_kind.uninstall_display_name())
}

fn running_warning(app_kind: AppKind) -> String {
    format!(
        "{} 正在运行。继续安装会关闭它，尚未保存的内容（例如创建端表格里还没导出的验证码）将丢失。\n\n是否继续？",
        app_kind.exe_name()
    )
}

/// `MB_YESNO` question whose default button is "No", for confirmations that destroy data.
fn confirm_default_no(title: &str, text: &str) -> bool {
    const MB_YESNO: u32 = 0x0000_0004;
    const MB_ICONWARNING: u32 = 0x0000_0030;
    const MB_DEFBUTTON2: u32 = 0x0000_0100;
    let w_title = to_wide_null(title);
    let w_text = to_wide_null(text);
    // SAFETY: Modal message box with NUL-terminated strings that outlive the call.
    let answer = unsafe {
        w32::MessageBoxW(
            None,
            PCWSTR(w_text.as_ptr()),
            PCWSTR(w_title.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        )
    };
    answer == w32::IDYES
}

/// Plain message-box installation used when Windows App Runtime is not installed, so the
/// wizard window itself cannot be shown.
fn run_win32_fallback(app_kind: AppKind, entries: &[ArchiveEntry]) -> Result<u8, String> {
    let title = window_title(app_kind);
    let env = InstallEnvironment::real();
    let target_dir = normalize_install_dir(
        &default_install_dir(app_kind, &env).to_string_lossy(),
        app_kind,
        &protected_dirs(&env),
    )?;
    let (major, minor) = MIN_RUNTIME_VERSION;
    let assoc_line = if app_kind == AppKind::Reader {
        "、关联 .spdf 文件"
    } else {
        ""
    };
    let prompt = format!(
        "这台电脑尚未安装 Microsoft Windows App Runtime {major}.{minor} 或更高版本（x64），\
         安装向导界面无法显示，{} 也要在装好运行库之后才能启动。\n\n\
         可以先按默认选项完成安装：\n  安装目录：{}\n  创建桌面与“开始”菜单快捷方式{assoc_line}\n\n\
         是否现在安装？",
        app_kind.uninstall_display_name(),
        target_dir.display()
    );
    let null_hwnd = w32::HWND(std::ptr::null_mut());
    if !confirm_message_box(null_hwnd, &title, &prompt) {
        offer_runtime_download(&title, "尚未安装任何文件。");
        return Ok(EXIT_RUNTIME_MISSING);
    }
    if is_target_running(&target_dir.join(app_kind.exe_name()))
        && !confirm_default_no(&title, &running_warning(app_kind))
    {
        return Ok(EXIT_OK);
    }

    let opts = InstallOptions {
        install_dir: target_dir,
        create_desktop_shortcut: true,
        create_start_menu_shortcut: true,
        associate_spdf: app_kind == AppKind::Reader,
    };
    let summary = perform_install(app_kind, entries, &opts, &env, &mut |_, _| {})
        .map_err(|err| format!("安装失败：{err}"))?;
    offer_runtime_download(
        &title,
        &format!(
            "{}\n\n程序文件已安装，但还需要安装 Windows App Runtime {major}.{minor} 或更高版本才能启动。",
            format_summary_text(&summary)
        ),
    );
    Ok(EXIT_RUNTIME_MISSING)
}

fn offer_runtime_download(title: &str, lead: &str) {
    let text = format!("{lead}\n\n是否现在打开微软官方的运行库下载页面？");
    if confirm_message_box(w32::HWND(std::ptr::null_mut()), title, &text) {
        shell_open(RUNTIME_DOWNLOAD_URL);
    }
}

fn build_wizard_window(
    app_kind: AppKind,
    entries: &Arc<Vec<ArchiveEntry>>,
) -> windows_core::Result<()> {
    let win = FluentWindow::new(&WindowChromeOptions {
        title: window_title(app_kind),
        width_dip: 720,
        height_dip: 580,
        resizable: true,
        maximizable: false,
        minimizable: true,
        always_on_top: false,
        capture_protected: false,
        use_mica: true,
    })?;
    let dq = win.dispatcher_queue()?;

    let root = grid_rows(&[gl_auto(), gl_auto(), gl_star(1.0), gl_auto()])?;
    root.SetBackground(&brush_base(BG_CANVAS)?)?;
    root.SetPadding(uniform(20.0))?;
    root.SetRowSpacing(12.0)?;

    let env = InstallEnvironment::real();
    let existing = detect_existing_install(app_kind, &env);
    let ui = create_wizard_widgets(app_kind, Arc::clone(entries), env, win.clone(), dq)?;
    let header = build_header(app_kind, entries)?;
    let options_card = build_options_card(&ui)?;
    let footer = build_footer(&ui)?;

    place_in_grid(&header, 0, 0, 1, 1)?;
    place_in_grid(&ui.alert, 1, 0, 1, 1)?;
    place_in_grid(&options_card, 2, 0, 1, 1)?;
    place_in_grid(&footer, 3, 0, 1, 1)?;

    root.Children()?.Append(&header)?;
    root.Children()?.Append(&ui.alert)?;
    root.Children()?.Append(&options_card)?;
    root.Children()?.Append(&footer)?;

    wire_events(&ui)?;
    if let Some(prev) = &existing {
        show_info(
            &ui.alert,
            InfoBarSeverity::Informational,
            "检测到已安装的版本",
            &existing_install_note(prev),
        );
    }
    WIZARD.with(|slot| *slot.borrow_mut() = Some(ui.clone()));
    win.set_content(&root.cast::<FrameworkElement>()?)?;
    win.activate()
}

fn existing_install_note(prev: &ExistingInstall) -> String {
    format!(
        "已安装版本 {}，位于 {}。默认在原目录升级，旧版本的文件、快捷方式和卸载项会被替换，不会出现两套程序。",
        prev.version.as_deref().unwrap_or("1.x"),
        prev.dir.display()
    )
}

fn create_wizard_widgets(
    app_kind: AppKind,
    entries: Arc<Vec<ArchiveEntry>>,
    env: InstallEnvironment,
    win: FluentWindow,
    dq: DispatcherQueue,
) -> windows_core::Result<WizardUi> {
    let dir_box = text_box("", false)?;
    dir_box.SetText(&HSTRING::from(
        default_install_dir(app_kind, &env)
            .to_string_lossy()
            .as_ref(),
    ))?;
    let chk_assoc = if app_kind == AppKind::Reader {
        Some(check_box(
            "关联 .spdf 文件（双击加密文件直接打开阅读）",
            true,
        )?)
    } else {
        None
    };
    let protected = protected_dirs(&env);

    Ok(WizardUi {
        app_kind,
        entries,
        env,
        protected,
        win,
        dq,
        alert: info_bar()?,
        dir_box,
        browse_btn: button("浏览...", false)?,
        chk_desktop: check_box(
            &format!("在桌面创建快捷方式：{}", app_kind.shortcut_name()),
            true,
        )?,
        chk_start_menu: check_box("在“开始”菜单中创建快捷方式", true)?,
        chk_assoc,
        status_lbl: text_block(
            "就绪，点击“立即安装”开始安装。",
            13.0,
            false,
            TEXT_SECONDARY,
        )?,
        progress: progress_bar()?,
        action_btn: button("立即安装", true)?,
        close_btn: button("取消", false)?,
        state: Arc::new(Mutex::new(WizardState::default())),
    })
}

fn build_header(app_kind: AppKind, entries: &[ArchiveEntry]) -> windows_core::Result<StackPanel> {
    let total_bytes: u64 = entries
        .iter()
        .map(|entry| u64::try_from(entry.data.len()).unwrap_or(u64::MAX))
        .fold(0_u64, u64::saturating_add);
    let mb_tenths = total_bytes.saturating_mul(10) / (1024 * 1024);

    let panel = stack_v(4.0)?;
    panel.Children()?.Append(&text_block(
        &format!("{} 安装向导", app_kind.uninstall_display_name()),
        20.0,
        true,
        TEXT_PRIMARY,
    )?)?;
    panel.Children()?.Append(&text_block(
        &format!(
            "共 {} 个文件，安装后约占 {}.{} MB · 仅为当前用户安装，不需要管理员权限",
            entries.len(),
            mb_tenths / 10,
            mb_tenths % 10
        ),
        12.5,
        false,
        TEXT_SECONDARY,
    )?)?;
    Ok(panel)
}

fn build_options_card(ui: &WizardUi) -> windows_core::Result<Border> {
    let card = card_border(uniform(18.0))?;
    let col = stack_v(12.0)?;

    col.Children()?
        .Append(&text_block("安装位置", 13.5, true, TEXT_PRIMARY)?)?;

    let dir_row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&dir_row, &[gl_star(1.0), gl_px(10.0), gl_auto()])?;
    place_in_grid(&ui.dir_box, 0, 0, 1, 1)?;
    place_in_grid(&ui.browse_btn, 0, 2, 1, 1)?;
    dir_row.Children()?.Append(&ui.dir_box)?;
    dir_row.Children()?.Append(&ui.browse_btn)?;
    col.Children()?.Append(&dir_row)?;

    col.Children()?.Append(&{
        let sep = text_block("快捷方式与文件关联", 13.5, true, TEXT_PRIMARY)?;
        sep.SetMargin(pad(0.0, 6.0, 0.0, 0.0))?;
        sep
    })?;
    col.Children()?.Append(&ui.chk_desktop)?;
    col.Children()?.Append(&ui.chk_start_menu)?;
    if let Some(assoc) = &ui.chk_assoc {
        col.Children()?.Append(assoc)?;
    }

    ui.status_lbl.SetMargin(pad(0.0, 8.0, 0.0, 0.0))?;
    col.Children()?.Append(&ui.status_lbl)?;
    col.Children()?.Append(&ui.progress)?;

    card.SetChild(&col)?;
    Ok(card)
}

fn build_footer(
    ui: &WizardUi,
) -> windows_core::Result<winui3::bindings::Microsoft::UI::Xaml::Controls::Grid> {
    let row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&row, &[gl_star(1.0), gl_auto(), gl_px(10.0), gl_auto()])?;

    let hint = text_block(
        "静默安装：--silent [--dir <目录>] [--desktop] [--start-menu] [--assoc]",
        12.0,
        false,
        TEXT_SECONDARY,
    )?;
    place_in_grid(&hint, 0, 0, 1, 1)?;
    place_in_grid(&ui.close_btn, 0, 1, 1, 1)?;
    place_in_grid(&ui.action_btn, 0, 3, 1, 1)?;

    row.Children()?.Append(&hint)?;
    row.Children()?.Append(&ui.close_btn)?;
    row.Children()?.Append(&ui.action_btn)?;
    Ok(row)
}

fn is_installing(state: &Mutex<WizardState>) -> bool {
    state.lock().is_ok_and(|guard| guard.installing)
}

fn wire_events(ui: &WizardUi) -> windows_core::Result<()> {
    let ui_browse = ui.clone();
    ui.browse_btn
        .Click(move |_, _| on_browse_clicked(&ui_browse))?
        .forget();

    let ui_close = ui.clone();
    ui.close_btn
        .Click(move |_, _| {
            let _ = ui_close.win.window.Close();
        })?
        .forget();

    let ui_action = ui.clone();
    ui.action_btn
        .Click(move |_, _| on_action_clicked(&ui_action))?
        .forget();

    // Closing the window while files are being placed would leave a half-installed directory.
    let state = Arc::clone(&ui.state);
    let alert = ui.alert.clone();
    ui.win
        .app_window
        .Closing(move |_, args| {
            if !is_installing(&state) {
                return;
            }
            if let Some(ev) = args.as_ref() {
                let _ = ev.SetCancel(true);
            }
            show_info(
                &alert,
                InfoBarSeverity::Warning,
                "正在安装",
                "安装尚未完成，暂时不能关闭窗口，请稍候。",
            );
        })?
        .forget();

    Ok(())
}

fn on_browse_clicked(ui: &WizardUi) {
    let current = PathBuf::from(ui.dir_box.Text().unwrap_or_default().to_string_lossy());
    let initial = if current.is_dir() {
        Some(current)
    } else {
        current
            .parent()
            .filter(|p| p.is_dir())
            .map(Path::to_path_buf)
    };
    let owner = window_hwnd(&ui.win.window).ok();
    let title = format!(
        "选择安装位置（程序将安装到其中的 {} 文件夹）",
        ui.app_kind.install_folder_name()
    );
    if let Ok(Some(picked)) = pick_folder(owner, &title, initial.as_deref()) {
        let target = with_app_folder(&picked, ui.app_kind);
        let _ = ui
            .dir_box
            .SetText(&HSTRING::from(target.to_string_lossy().as_ref()));
    }
}

fn on_action_clicked(ui: &WizardUi) {
    let (installing, existing_exe) = {
        let Ok(guard) = ui.state.lock() else {
            return;
        };
        (guard.installing, guard.installed_exe.clone())
    };
    if installing {
        return;
    }
    if let Some(exe_path) = existing_exe {
        shell_open(&exe_path.to_string_lossy());
        let _ = ui.win.window.Close();
        return;
    }

    let raw_dir = ui.dir_box.Text().unwrap_or_default().to_string_lossy();
    let install_dir = match normalize_install_dir(&raw_dir, ui.app_kind, &ui.protected) {
        Ok(dir) => dir,
        Err(err) => {
            show_info(&ui.alert, InfoBarSeverity::Error, "安装路径无效", &err);
            return;
        }
    };
    let _ = ui
        .dir_box
        .SetText(&HSTRING::from(install_dir.to_string_lossy().as_ref()));

    let opts = InstallOptions {
        install_dir,
        create_desktop_shortcut: ui.chk_desktop.IsChecked().unwrap_or(true),
        create_start_menu_shortcut: ui.chk_start_menu.IsChecked().unwrap_or(true),
        associate_spdf: ui
            .chk_assoc
            .as_ref()
            .and_then(|cb| cb.IsChecked().ok())
            .unwrap_or(false),
    };

    if !is_target_running(&opts.install_dir.join(ui.app_kind.exe_name())) {
        start_background_install(ui, opts);
        return;
    }
    // The running program may hold unsaved work: never terminate it without asking.
    let shown = ui.win.xaml_root().is_ok_and(|root| {
        show_confirm_dialog(
            &root,
            "程序正在运行",
            &running_warning(ui.app_kind),
            "继续安装",
            "取消",
            move |confirmed| {
                if confirmed {
                    with_ui(|ui| start_background_install(ui, opts));
                }
            },
        )
    });
    if !shown {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "程序正在运行",
            &format!(
                "{} 正在运行，请先关闭它（并保存好未导出的内容），再点击“立即安装”。",
                ui.app_kind.exe_name()
            ),
        );
    }
}

fn set_controls_busy(ui: &WizardUi, busy: bool) {
    let enabled = !busy;
    let _ = ui.dir_box.SetIsEnabled(enabled);
    let _ = ui.browse_btn.SetIsEnabled(enabled);
    let _ = ui.chk_desktop.SetIsEnabled(enabled);
    let _ = ui.chk_start_menu.SetIsEnabled(enabled);
    if let Some(cb) = &ui.chk_assoc {
        let _ = cb.SetIsEnabled(enabled);
    }
    let _ = ui.action_btn.SetIsEnabled(enabled);
    let _ = ui.close_btn.SetIsEnabled(enabled);
}

fn start_background_install(ui: &WizardUi, opts: InstallOptions) {
    if let Ok(mut guard) = ui.state.lock() {
        if guard.installing {
            return;
        }
        guard.installing = true;
    }
    set_controls_busy(ui, true);
    let _ = ui.progress.SetValue2(0.0);
    let _ = ui.status_lbl.SetText(&HSTRING::from("正在安装..."));
    show_info(
        &ui.alert,
        InfoBarSeverity::Informational,
        "正在安装",
        &opts.install_dir.display().to_string(),
    );

    // Only plain data and the (agile) dispatcher queue cross to the worker thread; every UI
    // object stays on the UI thread and is reached through `with_ui`.
    let app_kind = ui.app_kind;
    let entries = Arc::clone(&ui.entries);
    let env = ui.env.clone();
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let progress_dq = dq.clone();
        let mut report = move |percent: u8, message: &str| {
            let message = message.to_owned();
            let _ = enqueue_ui(&progress_dq, move || {
                with_ui(|ui| {
                    let _ = ui.progress.SetValue2(f64::from(percent.min(100)));
                    let _ = ui.status_lbl.SetText(&HSTRING::from(message.as_str()));
                });
            });
        };
        let result = perform_install(app_kind, &entries, &opts, &env, &mut report);
        let _ = enqueue_ui(&dq, move || with_ui(|ui| finish_install(ui, result)));
    });
}

fn finish_install(ui: &WizardUi, result: Result<InstallSummary, String>) {
    if let Ok(mut guard) = ui.state.lock() {
        guard.installing = false;
    }
    set_controls_busy(ui, false);

    match result {
        Ok(summary) => {
            if let Ok(mut guard) = ui.state.lock() {
                guard.installed_exe = Some(summary.main_exe.clone());
            }
            let _ = ui.progress.SetValue2(100.0);
            let _ = ui.status_lbl.SetText(&HSTRING::from(format!(
                "安装成功，已安装至：{}",
                summary.install_dir.display()
            )));
            // Options no longer apply once the files are in place.
            set_controls_busy(ui, true);
            let _ = ui.action_btn.SetIsEnabled(true);
            let _ = ui.close_btn.SetIsEnabled(true);
            if let Ok(lbl) = text_block("立即打开", 13.5, true, ACCENT_TEXT) {
                let _ = ui.action_btn.SetContent(&lbl);
            }
            if let Ok(lbl) = text_block("完成", 13.5, false, TEXT_PRIMARY) {
                let _ = ui.close_btn.SetContent(&lbl);
            }
            let (severity, title) = if summary.warnings.is_empty() {
                (InfoBarSeverity::Success, "安装已完成")
            } else {
                (InfoBarSeverity::Warning, "安装已完成，但有部分步骤未成功")
            };
            show_info(&ui.alert, severity, title, &format_summary_text(&summary));
        }
        Err(err) => {
            let _ = ui.progress.SetValue2(0.0);
            let _ = ui
                .status_lbl
                .SetText(&HSTRING::from("安装未能完成，原因见上方提示。"));
            show_info(&ui.alert, InfoBarSeverity::Error, "安装失败", &err);
        }
    }
}

fn format_summary_text(summary: &InstallSummary) -> String {
    let mut text = format!(
        "安装目录：{}\n已安装 {} 个文件，创建 {} 个快捷方式。\n卸载：在“设置 → 应用”中卸载，或运行安装目录中的 {}。",
        summary.install_dir.display(),
        summary.files_installed,
        summary.shortcuts_created.len(),
        summary
            .uninstall_script
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    if let Some(version) = &summary.replaced_version {
        let _ = write!(text, "\n已替换原有版本 {version}。");
    }
    for warning in &summary.warnings {
        let _ = write!(text, "\n注意：{warning}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_text_lists_warnings_and_the_replaced_version() {
        let summary = InstallSummary {
            install_dir: PathBuf::from(r"D:\Apps\SecurePdfReader"),
            main_exe: PathBuf::from(r"D:\Apps\SecurePdfReader\SecurePdfReader.exe"),
            files_installed: 4,
            shortcuts_created: vec![PathBuf::from(r"D:\Desktop\SecurePdf 阅读端.lnk")],
            uninstall_script: PathBuf::from(r"D:\Apps\SecurePdfReader\uninstall_reader.bat"),
            warnings: vec!["未能创建桌面快捷方式：拒绝访问".to_owned()],
            replaced_version: Some("1.1.0".to_owned()),
        };
        let text = format_summary_text(&summary);
        assert!(text.contains(r"安装目录：D:\Apps\SecurePdfReader"));
        assert!(text.contains("已安装 4 个文件，创建 1 个快捷方式"));
        assert!(text.contains("uninstall_reader.bat"));
        assert!(text.contains("已替换原有版本 1.1.0"));
        assert!(text.contains("注意：未能创建桌面快捷方式：拒绝访问"));
    }

    #[test]
    fn user_facing_texts_name_the_product_and_the_data_loss_risk() {
        assert_eq!(
            window_title(AppKind::Creator),
            "SecurePdf 创建端 (发送方) 安装程序"
        );
        let warning = running_warning(AppKind::Creator);
        assert!(warning.contains("SecurePdfCreator.exe") && warning.contains("验证码"));
        let note = existing_install_note(&ExistingInstall {
            dir: PathBuf::from(r"C:\Users\u\AppData\Local\SecurePdfReader"),
            version: Some("1.1.0".to_owned()),
        });
        assert!(note.contains("1.1.0") && note.contains("原目录"));
    }
}
