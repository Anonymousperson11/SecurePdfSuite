//! Capture-protected `.spdf` unlock window with header preview, machine code copy, and backoff.
//!
//! The window's controls live in a UI-thread-local [`UnlockUi`]; background threads (device
//! probe, decryption) carry only plain data and the dispatcher queue.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use secure_pdf_core::format::{open_spdf_file, read_spdf_info, utc_now_dotnet_ticks};
use secure_pdf_core::licensing::{DeviceSnapshot, collect_device_snapshot};
use secure_pdf_core::pdf::MemoryPdfDocument;
use secure_pdf_core::storage::LocalQuotaStore;
use secure_pdf_core::{CoreError, QuotaStoreHealth};
use windows_collections::IVectorView;
use windows_core::{HSTRING, Interface};
use windows_future::AsyncOperationCompletedHandler;
use winui3::app::{enqueue_ui, schedule_once_timer};
use winui3::bindings::Microsoft::UI::Dispatching::{DispatcherQueue, DispatcherQueueTimer};
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    Border, Button, Grid, InfoBar, InfoBarSeverity, PasswordBox, TextBlock, TextBox,
};
use winui3::bindings::Microsoft::UI::Xaml::{FrameworkElement, HorizontalAlignment, UIElement};
use winui3::bindings::Windows::ApplicationModel::DataTransfer::{
    DataPackageOperation, StandardDataFormats,
};
use winui3::bindings::Windows::Storage::IStorageItem;
use winui3::controls::{
    BG_CANVAS, TEXT_PRIMARY, TEXT_SECONDARY, add_grid_cols, brush_base, button, card_border,
    gl_auto, gl_px, gl_star, grid_rows, info_bar, pad, place_in_grid, scroll_viewer_vertical,
    show_info, stack_v, text_block, text_box, uniform,
};
use winui3::interop::{copy_unicode_text, pick_open_files, window_hwnd};
use winui3::window::{FluentWindow, WindowChromeOptions};
use zeroize::Zeroizing;

use crate::logic::{
    ProtectionState, UnlockBackoff, cooldown_text, file_summary_text, protection_status_text, vk,
};
use crate::protection::{WindowOpenError, activate_protected_window, create_protected_window};
use crate::reader_window::open_reader_window;
use crate::session::ReaderSession;

#[derive(Default)]
struct UnlockState {
    spdf_path: Option<PathBuf>,
    device: Option<DeviceSnapshot>,
    backoff: UnlockBackoff,
    busy: bool,
    cooldown_timer: Option<DispatcherQueueTimer>,
}

struct UnlockUi {
    win: FluentWindow,
    dq: DispatcherQueue,
    state: RefCell<UnlockState>,
    path_box: TextBox,
    browse_btn: Button,
    info_tb: TextBlock,
    machine_box: TextBox,
    copy_machine_btn: Button,
    code_box: PasswordBox,
    unlock_btn: Button,
    protect_tb: TextBlock,
    alert: InfoBar,
}

thread_local! {
    static UNLOCK_UI: RefCell<Option<Rc<UnlockUi>>> = const { RefCell::new(None) };
}

/// Runs `f` with the live unlock window on the UI thread; `None` once it is closed.
fn with_unlock<R>(f: impl FnOnce(&UnlockUi) -> R) -> Option<R> {
    let ui = UNLOCK_UI.with(|slot| slot.borrow().clone())?;
    Some(f(&ui))
}

/// Opens the capture-protected `.spdf` unlock window.
///
/// # Errors
///
/// [`WindowOpenError::CaptureProtection`] when Windows refuses capture protection for the
/// window, [`WindowOpenError::Ui`] for any other construction failure. A half-built window is
/// closed before returning.
pub fn open_unlock_window(initial_spdf: Option<PathBuf>) -> Result<(), WindowOpenError> {
    let win = create_protected_window(&WindowChromeOptions {
        title: "SecurePdf 安全阅读器 — 打开受保护文档".to_owned(),
        width_dip: 580,
        height_dip: 630,
        resizable: true,
        maximizable: false,
        minimizable: true,
        always_on_top: false,
        capture_protected: true,
        use_mica: true,
    })?;
    build_and_show(&win, initial_spdf).inspect_err(|_| {
        UNLOCK_UI.with(|slot| slot.borrow_mut().take());
        let _ = win.window.Close();
    })
}

fn build_and_show(
    win: &FluentWindow,
    initial_spdf: Option<PathBuf>,
) -> Result<(), WindowOpenError> {
    let dq = win.dispatcher_queue()?;
    let root = grid_rows(&[gl_star(1.0)])?;
    root.SetBackground(&brush_base(BG_CANVAS)?)?;

    let ui = Rc::new(create_widgets(win.clone(), dq)?);
    let card = layout_unlock_card(&ui)?;
    card.SetMargin(uniform(20.0))?;
    let scroller = scroll_viewer_vertical()?;
    scroller.SetContent(&card)?;
    root.Children()?.Append(&scroller)?;
    UNLOCK_UI.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&ui)));

    wire_events(&ui, &root)?;
    spawn_device_probe(&ui);
    if let Some(path) = initial_spdf {
        select_spdf_file(&ui, path);
    }

    win.set_content(&root.cast::<FrameworkElement>()?)?;
    let protection = activate_protected_window(win)?;
    let _ = ui
        .protect_tb
        .SetText(&HSTRING::from(unlock_subtitle(protection)));
    Ok(())
}

/// Subtitle of the unlock card, stating the protection that is really in effect.
fn unlock_subtitle(protection: ProtectionState) -> String {
    format!(
        "{}。文档只在内存中解密，不会在磁盘上留下明文。",
        protection_status_text(protection)
    )
}

fn create_widgets(win: FluentWindow, dq: DispatcherQueue) -> windows_core::Result<UnlockUi> {
    let path_box = text_box("请选择 .spdf 文件，或把文件拖到此窗口", true)?;
    let machine_box = text_box("正在读取本机设备码…", true)?;
    let code_box = winui3::controls::password_box("输入发送方提供的验证码（不区分大小写）")?;
    let unlock_btn = button("验证并打开", true)?;
    unlock_btn.SetHorizontalAlignment(HorizontalAlignment::Stretch)?;

    Ok(UnlockUi {
        win,
        dq,
        state: RefCell::new(UnlockState::default()),
        path_box,
        browse_btn: button("浏览…", false)?,
        info_tb: text_block("尚未选择文件", 12.5, false, TEXT_SECONDARY)?,
        machine_box,
        copy_machine_btn: button("复制设备码", false)?,
        code_box,
        unlock_btn,
        protect_tb: text_block("", 12.0, false, TEXT_SECONDARY)?,
        alert: info_bar()?,
    })
}

fn layout_unlock_card(ui: &UnlockUi) -> windows_core::Result<Border> {
    let card = card_border(uniform(18.0))?;
    let col = stack_v(10.0)?;

    col.Children()?
        .Append(&text_block("打开受保护文档", 20.0, true, TEXT_PRIMARY)?)?;
    col.Children()?.Append(&ui.protect_tb)?;

    col.Children()?.Append(&text_block(
        "受保护文件（.spdf）",
        13.0,
        true,
        TEXT_PRIMARY,
    )?)?;
    let file_row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&file_row, &[gl_star(1.0), gl_px(8.0), gl_auto()])?;
    place_in_grid(&ui.path_box, 0, 0, 1, 1)?;
    place_in_grid(&ui.browse_btn, 0, 2, 1, 1)?;
    file_row.Children()?.Append(&ui.path_box)?;
    file_row.Children()?.Append(&ui.browse_btn)?;
    col.Children()?.Append(&file_row)?;
    col.Children()?.Append(&ui.info_tb)?;

    col.Children()?
        .Append(&text_block("本机设备码", 13.0, true, TEXT_PRIMARY)?)?;
    let dev_row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&dev_row, &[gl_star(1.0), gl_px(8.0), gl_auto()])?;
    place_in_grid(&ui.machine_box, 0, 0, 1, 1)?;
    place_in_grid(&ui.copy_machine_btn, 0, 2, 1, 1)?;
    dev_row.Children()?.Append(&ui.machine_box)?;
    dev_row.Children()?.Append(&ui.copy_machine_btn)?;
    col.Children()?.Append(&dev_row)?;

    col.Children()?
        .Append(&text_block("验证码", 13.0, true, TEXT_PRIMARY)?)?;
    col.Children()?.Append(&ui.code_box)?;
    ui.unlock_btn.SetMargin(pad(0.0, 6.0, 0.0, 0.0))?;
    col.Children()?.Append(&ui.unlock_btn)?;
    col.Children()?.Append(&ui.alert)?;

    card.SetChild(&col)?;
    Ok(card)
}

fn wire_events(ui: &UnlockUi, root: &Grid) -> windows_core::Result<()> {
    ui.browse_btn
        .Click(|_, _| {
            with_unlock(|ui| {
                let owner = window_hwnd(&ui.win.window).ok();
                if let Ok(paths) = pick_open_files(
                    owner,
                    "打开受保护文档",
                    &[("受保护文档 (*.spdf)", "*.spdf")],
                    false,
                ) && let Some(path) = paths.into_iter().next()
                {
                    select_spdf_file(ui, path);
                }
            });
        })?
        .forget();

    ui.copy_machine_btn
        .Click(|_, _| {
            with_unlock(copy_machine_code);
        })?
        .forget();

    ui.unlock_btn
        .Click(|_, _| {
            with_unlock(trigger_unlock);
        })?
        .forget();

    let code_ui: UIElement = ui.code_box.cast()?;
    code_ui
        .KeyDown(|_, args| {
            if let Some(ev) = args.as_ref()
                && ev.Key().is_ok_and(|k| k.0 == vk::ENTER)
            {
                let _ = ev.SetHandled(true);
                with_unlock(trigger_unlock);
            }
        })?
        .forget();

    ui.win
        .window
        .Closed(|_, _| {
            if let Some(ui) = UNLOCK_UI.with(|slot| slot.borrow_mut().take())
                && let Some(timer) = ui.state.borrow_mut().cooldown_timer.take()
            {
                let _ = timer.Stop();
            }
        })?
        .forget();

    wire_drop_target(root)
}

/// Only the machine code (never document content) is ever placed on the clipboard.
fn copy_machine_code(ui: &UnlockUi) {
    let Some(code) = ui
        .state
        .borrow()
        .device
        .as_ref()
        .map(|d| d.machine_code.clone())
    else {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "设备码尚未就绪",
            "正在读取本机设备码，请稍后再试。",
        );
        return;
    };
    let copied = window_hwnd(&ui.win.window).and_then(|hwnd| copy_unicode_text(hwnd, &code));
    match copied {
        Ok(()) => show_info(&ui.alert, InfoBarSeverity::Success, "已复制设备码", &code),
        Err(err) => show_info(
            &ui.alert,
            InfoBarSeverity::Error,
            "复制失败",
            &format!(
                "剪贴板暂时无法使用，请稍后重试。（错误代码 0x{:08X}）",
                err.code().0.cast_unsigned()
            ),
        ),
    }
}

fn wire_drop_target(root: &Grid) -> windows_core::Result<()> {
    root.SetAllowDrop(true)?;
    let root_ui: UIElement = root.cast()?;
    root_ui
        .DragOver(|_, args| {
            if let Some(drag) = args.as_ref()
                && let Ok(view) = drag.DataView()
                && let Ok(fmt) = StandardDataFormats::StorageItems()
                && view.Contains(&fmt).unwrap_or(false)
            {
                let _ = drag.SetAcceptedOperation(DataPackageOperation::Copy);
            }
        })?
        .forget();

    root_ui
        .Drop(|_, args| {
            let Some(drag) = args.as_ref() else {
                return;
            };
            let Some(dq) = with_unlock(|ui| ui.dq.clone()) else {
                return;
            };
            let Ok(deferral) = drag.GetDeferral() else {
                return;
            };
            let Ok(op) = drag.DataView().and_then(|v| v.GetStorageItemsAsync()) else {
                let _ = deferral.Complete();
                return;
            };
            let deferral_cb = deferral.clone();
            let handler =
                AsyncOperationCompletedHandler::<IVectorView<IStorageItem>>::new(move |info, _| {
                    if let Some(res) = info.as_ref().and_then(|a| a.GetResults().ok())
                        && res.Size().unwrap_or(0) > 0
                        && let Ok(raw) = res.GetAt(0)
                        && let Ok(item) = raw.cast::<IStorageItem>()
                        && let Ok(p) = item.Path()
                    {
                        let dropped = PathBuf::from(p.to_string_lossy());
                        let _ = enqueue_ui(&dq, move || {
                            with_unlock(|ui| select_spdf_file(ui, dropped));
                        });
                    }
                    let _ = deferral_cb.Complete();
                    Ok(())
                });
            if op.SetCompleted(&handler).is_err() {
                let _ = deferral.Complete();
            }
        })?
        .forget();
    Ok(())
}

fn spawn_device_probe(ui: &UnlockUi) {
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let probed = collect_device_snapshot().map_err(|e| e.to_string());
        let _ = enqueue_ui(&dq, move || {
            with_unlock(|ui| match probed {
                Ok(snap) => {
                    let _ = ui.machine_box.SetText(&HSTRING::from(&snap.machine_code));
                    ui.state.borrow_mut().device = Some(snap);
                }
                Err(reason) => {
                    let _ = ui
                        .machine_box
                        .SetText(&HSTRING::from(format!("读取失败：{reason}")));
                }
            });
        });
    });
}

fn select_spdf_file(ui: &UnlockUi, path: PathBuf) {
    let _ = ui
        .path_box
        .SetText(&HSTRING::from(path.to_string_lossy().as_ref()));
    match read_spdf_info(&path) {
        Ok(info) => {
            let size = std::fs::metadata(&path).map_or(0, |m| m.len());
            let _ = ui.info_tb.SetText(&HSTRING::from(file_summary_text(
                &info.doc_id.to_hex_n(),
                size,
            )));
            ui.state.borrow_mut().spdf_path = Some(path);
            let _ = ui.alert.SetIsOpen(false);
        }
        Err(err) => {
            let _ = ui.info_tb.SetText(&HSTRING::from("无法读取该文件"));
            ui.state.borrow_mut().spdf_path = None;
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "无法读取该文件",
                &err.to_string(),
            );
        }
    }
}

fn trigger_unlock(ui: &UnlockUi) {
    // Enter bypasses the disabled button, so the busy flag is checked here as well.
    if ui.state.borrow().busy {
        return;
    }
    let Some(spdf_path) = ui.state.borrow().spdf_path.clone() else {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "未选择文件",
            "请先选择有效的 .spdf 文档。",
        );
        return;
    };
    let code = Zeroizing::new(ui.code_box.Password().unwrap_or_default().to_string_lossy());
    if code.trim().is_empty() {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "缺少验证码",
            "请输入该文档对应的验证码。",
        );
        return;
    }

    let device = {
        let mut state = ui.state.borrow_mut();
        state.busy = true;
        state.device.clone()
    };
    let _ = ui.unlock_btn.SetIsEnabled(false);
    show_info(
        &ui.alert,
        InfoBarSeverity::Informational,
        "正在打开",
        "正在校验验证码并解密文档…",
    );

    // Only plain data and the dispatcher queue travel to the worker thread; the code is wiped
    // when the worker drops it.
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let unlocked = unlock_worker(&spdf_path, &code, device);
        drop(code);
        let _ = enqueue_ui(&dq, move || {
            with_unlock(|ui| handle_unlock_result(ui, unlocked));
        });
    });
}

fn unlock_worker(
    spdf_path: &Path,
    code: &str,
    device: Option<DeviceSnapshot>,
) -> Result<Arc<ReaderSession>, CoreError> {
    let device = match device {
        Some(d) => d,
        None => collect_device_snapshot()?,
    };
    let decrypted = open_spdf_file(spdf_path, code)?;
    if decrypted.policy.is_expired_at(utc_now_dotnet_ticks()) {
        return Err(CoreError::DocumentExpired);
    }
    let store = LocalQuotaStore::default();
    // Looked at before verification repairs the record, so the reader can tell the user.
    let store_repaired = store.inspect_store_health(&decrypted.policy, &device.machine_hash)?
        == QuotaStoreHealth::Inconsistent;
    let quota_status = store.initialize_or_verify(&decrypted.policy, &device.machine_hash)?;
    // The PDF is parsed before any reader window exists, so a parse failure cannot leave a
    // window behind that was never shown.
    let pdf = MemoryPdfDocument::open_bytes(decrypted.pdf_bytes)?;
    Ok(ReaderSession::new(
        pdf,
        decrypted.policy,
        device,
        quota_status,
        store_repaired,
    ))
}

fn end_busy(ui: &UnlockUi) {
    ui.state.borrow_mut().busy = false;
    let _ = ui.unlock_btn.SetIsEnabled(true);
}

fn handle_unlock_result(ui: &UnlockUi, unlocked: Result<Arc<ReaderSession>, CoreError>) {
    match unlocked {
        Ok(session) => {
            ui.state.borrow_mut().backoff.reset();
            // The code has done its job; do not keep it in the input box.
            let _ = ui.code_box.SetPassword(&HSTRING::new());
            match open_reader_window(&session) {
                // From here on the reader window owns (and releases) the document.
                Ok(()) => {
                    let _ = ui.win.window.Close();
                }
                Err(err) => {
                    end_busy(ui);
                    show_info(
                        &ui.alert,
                        InfoBarSeverity::Error,
                        "无法打开阅读窗口",
                        &err.user_message(),
                    );
                }
            }
        }
        Err(CoreError::WrongVerificationCode) => {
            // Only a wrong code counts as a failed attempt; each one lengthens the wait.
            let wait_ms = ui.state.borrow_mut().backoff.record_failure();
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "验证码错误",
                &format!(
                    "验证码不正确，或文件内容已被改动。请在 {} 后重试。",
                    cooldown_text(wait_ms)
                ),
            );
            start_unlock_cooldown(ui, wait_ms);
        }
        Err(err) => {
            end_busy(ui);
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "无法打开该文档",
                &err.to_string(),
            );
        }
    }
}

fn start_unlock_cooldown(ui: &UnlockUi, delay_ms: u64) {
    let timer = schedule_once_timer(&ui.dq, delay_ms, || {
        with_unlock(end_busy);
    });
    match timer {
        Ok(timer) => ui.state.borrow_mut().cooldown_timer = Some(timer),
        Err(_) => end_busy(ui),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_reflects_the_protection_actually_in_effect() {
        let excluded = unlock_subtitle(ProtectionState::ExcludedFromCapture);
        let black = unlock_subtitle(ProtectionState::BlackInCapture);
        let debug = unlock_subtitle(ProtectionState::DebugDisabled);
        assert!(excluded.contains("不会出现在截图和录屏中"));
        assert!(black.contains("显示为黑色"));
        assert!(debug.contains("防截屏保护已关闭"));
        assert_ne!(excluded, black);
        for text in [&excluded, &black] {
            assert!(text.contains("只在内存中解密"));
            assert!(!text.contains("硬件"));
        }
    }
}
