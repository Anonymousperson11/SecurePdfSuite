//! Native `WinUI 3` Creator GUI (`SecurePdfCreator.exe`) for batch `.spdf` encryption and forensics.

#![cfg_attr(
    all(target_os = "windows", not(test), not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod encrypt_tab;
mod forensics_tab;
mod logic;
mod shell;

use std::sync::atomic::Ordering;

use windows_core::Interface;
use winui3::app::run_winui_app;
use winui3::bindings::Microsoft::UI::Xaml::Controls::{Grid, TabView, TabViewItem};
use winui3::bindings::Microsoft::UI::Xaml::FrameworkElement;
use winui3::bootstrap::exit_ui_thread;
use winui3::controls::{
    BG_CANVAS, TEXT_PRIMARY, TEXT_SECONDARY, brush_base, gl_auto, gl_star, grid_rows, hv,
    place_in_grid, stack_v, text_block,
};
use winui3::dialog::{show_alert_dialog, show_confirm_dialog};
use winui3::interop::{message_box, window_hwnd};
use winui3::window::{FluentWindow, WindowChromeOptions};

use crate::encrypt_tab::SessionFlags;

const APP_TITLE: &str = "SecurePdf 创建端 — PDF 加密与水印取证";

fn main() {
    // A missing Windows App Runtime is reported to the user inside `run_winui_app`; any other
    // start-up failure is shown here, because a release build has no console to print to.
    if let Err(err) = run_winui_app(APP_TITLE, || {
        if let Err(ui_err) = build_creator_window() {
            message_box(
                winui3::bindings::Windows::Win32::HWND(std::ptr::null_mut()),
                APP_TITLE,
                &format!(
                    "无法打开主窗口，程序即将退出。\n错误代码：0x{:08X}",
                    ui_err.code().0
                ),
                true,
            );
            exit_ui_thread();
        }
    }) {
        eprintln!("SecurePdfCreator 启动失败：{err}");
    }
}

fn build_creator_window() -> windows_core::Result<()> {
    let win = FluentWindow::new(&WindowChromeOptions {
        title: APP_TITLE.to_owned(),
        width_dip: 1260,
        height_dip: 860,
        resizable: true,
        maximizable: true,
        minimizable: true,
        always_on_top: false,
        capture_protected: false,
        use_mica: true,
    })?;
    let dq = win.dispatcher_queue()?;
    let flags = SessionFlags::default();

    let root = grid_rows(&[gl_auto(), gl_star(1.0)])?;
    root.SetBackground(&brush_base(BG_CANVAS)?)?;

    let hdr = stack_v(2.0)?;
    hdr.SetPadding(hv(20.0, 8.0))?;
    hdr.Children()?
        .Append(&text_block("SecurePdf 创建端", 20.0, true, TEXT_PRIMARY)?)?;
    hdr.Children()?.Append(&text_block(
        "把 PDF 加密为受控文档（.spdf），并从截图或翻拍照片中还原溯源水印",
        12.5,
        false,
        TEXT_SECONDARY,
    )?)?;

    let tabs = TabView::new()?;
    tabs.SetIsAddTabButtonVisible(false)?;
    let tab_encrypt = TabViewItem::new()?;
    tab_encrypt.SetIsClosable(false)?;
    tab_encrypt.SetHeader(&text_block("创建加密 PDF", 13.5, true, TEXT_PRIMARY)?)?;
    let encrypt_body: Grid =
        encrypt_tab::build_encrypt_tab(win.clone(), dq.clone(), flags.clone())?;
    tab_encrypt.SetContent(&encrypt_body)?;

    let tab_forensics = TabViewItem::new()?;
    tab_forensics.SetIsClosable(false)?;
    tab_forensics.SetHeader(&text_block("水印取证", 13.5, true, TEXT_PRIMARY)?)?;
    let forensics_body: Grid = forensics_tab::build_forensics_tab(win.clone(), dq)?;
    tab_forensics.SetContent(&forensics_body)?;

    tabs.TabItems()?.Append(&tab_encrypt)?;
    tabs.TabItems()?.Append(&tab_forensics)?;

    place_in_grid(&hdr, 0, 0, 1, 1)?;
    place_in_grid(&tabs, 1, 0, 1, 1)?;
    root.Children()?.Append(&hdr)?;
    root.Children()?.Append(&tabs)?;

    wire_closing_guard(&win, &flags)?;
    win.set_content(&root.cast::<FrameworkElement>()?)?;
    win.activate()
}

/// Why a close request cannot simply go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDecision {
    /// Nothing at stake: let the window close.
    Allow,
    /// A batch is running: refuse and tell the user.
    RefuseWhileEncrypting,
    /// Codes would be lost: ask for confirmation first.
    ConfirmUnsavedCodes,
}

const fn decide_close(encrypting: bool, unsaved_codes: bool) -> CloseDecision {
    if encrypting {
        CloseDecision::RefuseWhileEncrypting
    } else if unsaved_codes {
        CloseDecision::ConfirmUnsavedCodes
    } else {
        CloseDecision::Allow
    }
}

const ENCRYPTING_NOTICE: &str = "正在加密文档，请等待完成后再关闭。";
const UNSAVED_NOTICE: &str = "结果表中的验证码还没有导出或复制。\n验证码只显示在这里，关闭后无法找回，对应的 .spdf 文件将无法打开。\n\n确定要关闭吗？";
const DIALOG_OPEN_NOTICE: &str = "窗口中还有一个未关闭的对话框。请先处理该对话框，然后再关闭程序。";

/// Falls back to a native message box so a refused close is never silent, even when an in-app
/// dialog is already open and a second one cannot be shown.
fn notify_close_refused(win: &FluentWindow, text: &str) {
    let hwnd = window_hwnd(&win.window)
        .unwrap_or(winui3::bindings::Windows::Win32::HWND(std::ptr::null_mut()));
    message_box(hwnd, APP_TITLE, text, false);
}

fn wire_closing_guard(win: &FluentWindow, flags: &SessionFlags) -> windows_core::Result<()> {
    let flags = flags.clone();
    let win_dlg = win.clone();
    win.app_window
        .Closing(move |_, args| {
            let decision = decide_close(
                flags.encrypting.load(Ordering::Acquire),
                flags.unsaved_codes.load(Ordering::Acquire),
            );
            if decision == CloseDecision::Allow {
                return;
            }
            if let Some(ev) = args.as_ref() {
                let _ = ev.SetCancel(true);
            }
            let root = win_dlg.xaml_root().ok();
            match decision {
                CloseDecision::Allow => {}
                CloseDecision::RefuseWhileEncrypting => {
                    let shown = root.as_ref().is_some_and(|r| {
                        show_alert_dialog(r, "正在加密", ENCRYPTING_NOTICE, "知道了")
                    });
                    if !shown {
                        notify_close_refused(&win_dlg, ENCRYPTING_NOTICE);
                    }
                }
                CloseDecision::ConfirmUnsavedCodes => {
                    let flag_yes = flags.unsaved_codes.clone();
                    let win_yes = win_dlg.clone();
                    // "返回" is the default button, so Enter never discards the codes.
                    let shown = root.as_ref().is_some_and(|r| {
                        show_confirm_dialog(
                            r,
                            "验证码尚未保存",
                            UNSAVED_NOTICE,
                            "仍然关闭",
                            "返回",
                            move |confirmed| {
                                if confirmed {
                                    flag_yes.store(false, Ordering::Release);
                                    let _ = win_yes.window.Close();
                                }
                            },
                        )
                    });
                    if !shown {
                        notify_close_refused(&win_dlg, DIALOG_OPEN_NOTICE);
                    }
                }
            }
        })?
        .forget();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_is_refused_while_encrypting_and_confirmed_when_codes_are_unsaved() {
        assert_eq!(decide_close(false, false), CloseDecision::Allow);
        assert_eq!(
            decide_close(false, true),
            CloseDecision::ConfirmUnsavedCodes
        );
        // A running batch always wins: closing is refused outright, not merely confirmed.
        assert_eq!(
            decide_close(true, false),
            CloseDecision::RefuseWhileEncrypting
        );
        assert_eq!(
            decide_close(true, true),
            CloseDecision::RefuseWhileEncrypting
        );
    }

    #[test]
    fn user_visible_window_text_has_no_developer_jargon() {
        for text in [
            APP_TITLE,
            ENCRYPTING_NOTICE,
            UNSAVED_NOTICE,
            DIALOG_OPEN_NOTICE,
        ] {
            for jargon in ["Rust", "WinUI", "O(W)", "scrypt", "AES", "Fluent"] {
                assert!(!text.contains(jargon), "{text:?} contains {jargon}");
            }
        }
    }
}
