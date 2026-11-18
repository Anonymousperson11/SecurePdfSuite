//! Secure physical-printer selection dialog and background raster GDI print execution.
//!
//! Printer enumeration, authorisation re-checks and the print job itself run on background
//! threads that carry only the session and the dispatcher queue; every control stays on the UI
//! thread and is reached through [`with_reader`].

use std::cell::RefCell;
use std::sync::Arc;

use secure_pdf_core::localtime::{LocalDateTime, format_dotnet_ticks_local};
use secure_pdf_core::printing::{
    PrintJobContext, PrinterPolicyEvaluation, list_evaluated_printers, print_document_guarded,
};
use secure_pdf_core::storage::LocalQuotaStore;
use secure_pdf_core::{CoreError, QuotaStatus};
use windows_core::{HSTRING, Interface};
use winui3::app::enqueue_ui;
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    ComboBox, InfoBarSeverity, StackPanel, TextBox,
};
use winui3::bindings::Microsoft::UI::Xaml::FrameworkElement;
use winui3::controls::{
    ACCENT_GREEN, ACCENT_RED, TEXT_PRIMARY, TEXT_SECONDARY, combo_box, show_info, stack_v,
    text_block, text_box,
};
use winui3::dialog::{show_alert_dialog, show_custom_dialog};

use crate::logic::{PageSelectionMode, resolve_print_pages};
use crate::reader_window::{
    PrintPhase, ReaderUi, lock_and_close, update_quota_display, with_reader,
};
use crate::session::ReaderSession;

/// Controls and data of the print dialog that is currently open.
struct PrintDialog {
    printer_combo: ComboBox,
    mode_combo: ComboBox,
    range_box: TextBox,
    allowed_names: Vec<String>,
    evaluated: Vec<PrinterPolicyEvaluation>,
    current_page: i32,
}

/// Selections restored when the dialog has to be shown again after an invalid page range.
struct Prefill {
    printer_name: Option<String>,
    mode: PageSelectionMode,
    custom_range: String,
    error: Option<String>,
}

thread_local! {
    static PRINT_DIALOG: RefCell<Option<PrintDialog>> = const { RefCell::new(None) };
}

/// Why the pre-print authorisation check did not pass.
enum PreflightFailure {
    /// The authorisation record is invalid or the document expired: the document must lock.
    Fatal(String),
    /// A transient problem (for example the record is busy); printing is simply not started.
    Transient(String),
}

/// What a finished print job reports back to the window.
struct PrintOutcome {
    /// `Ok(pages sent)` or the failure description.
    result: Result<usize, PrintFailure>,
    /// Set when the authorisation record turned out to be invalid afterwards.
    lock_reason: Option<String>,
}

/// User-facing description of a failed or refused print job.
#[derive(Debug, PartialEq, Eq)]
struct PrintFailure {
    title: &'static str,
    message: String,
}

/// `true` for errors after which the document may no longer be shown at all.
const fn is_fatal_authorisation_error(err: &CoreError) -> bool {
    matches!(
        err,
        CoreError::QuotaTampered(_) | CoreError::DocumentExpired
    )
}

/// Turns a print error into a title and a message that always says whether the print count
/// was deducted.
fn describe_print_error(err: &CoreError) -> PrintFailure {
    match err {
        // The core message already states how many pages were sent and whether the count was
        // deducted.
        CoreError::PrintFailed { message, .. } => PrintFailure {
            title: "打印失败",
            message: message.clone(),
        },
        // Everything else is raised before a print slot is reserved.
        other => PrintFailure {
            title: "打印被拒绝",
            message: format!("{other}\n尚未送出任何页面，本次打印次数未扣减。"),
        },
    }
}

/// Starts the print workflow; ignored while the dialog is open or a job is running, no matter
/// whether it was requested with the button or with `Ctrl+P`.
pub fn open_print_flow(ui: &ReaderUi) {
    {
        let mut state = ui.state.borrow_mut();
        if state.locked || state.print_phase != PrintPhase::Idle {
            return;
        }
        state.print_phase = PrintPhase::Preparing;
    }
    if ui.session.policy.max_prints <= 0 {
        end_print_flow(ui);
        notify(
            ui,
            InfoBarSeverity::Warning,
            "禁止打印",
            "发送方已禁止打印该文档。",
        );
        return;
    }
    set_status(ui, "正在核对打印授权并检测打印机…");

    let session = Arc::clone(&ui.session);
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let verified = LocalQuotaStore::default()
            .initialize_or_verify(&session.policy, &session.device.machine_hash)
            .map(|status| session.set_quota_status(status))
            .map_err(|err| {
                if is_fatal_authorisation_error(&err) {
                    PreflightFailure::Fatal(err.to_string())
                } else {
                    PreflightFailure::Transient(err.to_string())
                }
            });
        // Spooler and WMI queries can take seconds; they must not run on the UI thread.
        let evaluated = if verified.is_ok() {
            list_evaluated_printers()
        } else {
            Vec::new()
        };
        let _ = enqueue_ui(&dq, move || {
            with_reader(|ui| on_preflight_done(ui, verified, evaluated));
        });
    });
}

fn on_preflight_done(
    ui: &ReaderUi,
    verified: Result<(), PreflightFailure>,
    evaluated: Vec<PrinterPolicyEvaluation>,
) {
    if ui.state.borrow().locked {
        end_print_flow(ui);
        return;
    }
    set_status(ui, "");
    match verified {
        Err(PreflightFailure::Fatal(reason)) => {
            end_print_flow(ui);
            lock_and_close(ui, &reason);
            return;
        }
        Err(PreflightFailure::Transient(reason)) => {
            end_print_flow(ui);
            notify(ui, InfoBarSeverity::Error, "安全校验失败", &reason);
            return;
        }
        Ok(()) => update_quota_display(ui),
    }
    if ui.session.quota_status().remaining_prints <= 0 {
        end_print_flow(ui);
        notify(
            ui,
            InfoBarSeverity::Warning,
            "禁止打印",
            "当前文档已无剩余打印次数。",
        );
        return;
    }
    if !evaluated.iter().any(|p| p.allowed) {
        end_print_flow(ui);
        notify(
            ui,
            InfoBarSeverity::Warning,
            "未检测到可用的物理打印机",
            &format!(
                "受保护文档只能输出到本地或网络物理打印机，虚拟打印机和输出到文件的端口已被拦截。\n\n{}",
                build_blocked_summary(&evaluated)
            ),
        );
        return;
    }

    let prefill = Prefill {
        printer_name: ui.state.borrow().last_printer.clone(),
        mode: PageSelectionMode::All,
        custom_range: String::new(),
        error: None,
    };
    if show_print_dialog(ui, evaluated, &prefill).is_err() {
        PRINT_DIALOG.with(|slot| slot.borrow_mut().take());
        end_print_flow(ui);
        show_info(
            &ui.alert,
            InfoBarSeverity::Error,
            "无法打开打印对话框",
            "请关闭其他提示框后重试。",
        );
    }
}

/// Printer label shown in the list: offline printers are marked, as is the Windows default.
fn printer_display_name(printer: &PrinterPolicyEvaluation) -> String {
    let mut label = printer.printer_name.clone();
    if printer.is_offline {
        label.push_str("（脱机）");
    }
    if printer.is_default {
        label.push_str("（默认）");
    }
    label
}

/// Index preselected in the printer list: last used printer, else the default, else the first.
fn preferred_printer_index(allowed: &[&PrinterPolicyEvaluation], preferred: Option<&str>) -> usize {
    preferred
        .and_then(|name| {
            allowed
                .iter()
                .position(|p| p.printer_name.eq_ignore_ascii_case(name))
        })
        .or_else(|| allowed.iter().position(|p| p.is_default))
        .unwrap_or(0)
}

fn quota_banner_text(session: &ReaderSession, quota: QuotaStatus) -> String {
    let expiry = session
        .policy
        .expires_at_utc_ticks
        .map_or_else(String::new, |t| {
            format!("（有效期至 {}）", format_dotnet_ticks_local(t))
        });
    format!(
        "当前剩余打印次数: {} / {}{expiry}",
        quota.remaining_prints, quota.max_prints
    )
}

fn show_print_dialog(
    ui: &ReaderUi,
    evaluated: Vec<PrinterPolicyEvaluation>,
    prefill: &Prefill,
) -> windows_core::Result<()> {
    let xaml_root = ui.win.xaml_root()?;
    let current_page = ui.state.borrow().page_idx;
    let page_count = ui.session.page_count;

    let allowed: Vec<&PrinterPolicyEvaluation> = evaluated.iter().filter(|p| p.allowed).collect();
    let labels: Vec<String> = allowed.iter().map(|p| printer_display_name(p)).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let selected = preferred_printer_index(&allowed, prefill.printer_name.as_deref());
    let printer_combo = combo_box(&label_refs, i32::try_from(selected).unwrap_or(0))?;
    let allowed_names: Vec<String> = allowed.iter().map(|p| p.printer_name.clone()).collect();

    let mode_labels = [
        format!("全部页面（共 {page_count} 页）"),
        format!("仅当前页（第 {} 页）", current_page + 1),
        "自定义页码范围".to_owned(),
    ];
    let mode_refs: Vec<&str> = mode_labels.iter().map(String::as_str).collect();
    let mode_combo = combo_box(&mode_refs, prefill.mode.index())?;
    let range_box = text_box("自定义页码范围，例如 1-3,5", false)?;
    range_box.SetText(&HSTRING::from(&prefill.custom_range))?;
    // Typing a range implies the custom mode.
    range_box
        .GotFocus(|_, _| {
            PRINT_DIALOG.with(|slot| {
                if let Some(dialog) = slot.borrow().as_ref() {
                    let _ = dialog
                        .mode_combo
                        .SetSelectedIndex(PageSelectionMode::Custom.index());
                }
            });
        })?
        .forget();

    let body = stack_v(10.0)?;
    populate_dialog_body(
        &body,
        ui,
        &evaluated,
        prefill.error.as_deref(),
        &printer_combo,
        &mode_combo,
        &range_box,
    )?;
    let body_fe = body.cast::<FrameworkElement>()?;

    PRINT_DIALOG.with(|slot| {
        *slot.borrow_mut() = Some(PrintDialog {
            printer_combo,
            mode_combo,
            range_box,
            allowed_names,
            evaluated,
            current_page,
        });
    });
    let shown = show_custom_dialog(
        &xaml_root,
        "安全打印",
        &body_fe,
        Some("确认打印（消耗 1 次）"),
        "取消",
        |confirmed| {
            with_reader(|ui| on_dialog_closed(ui, confirmed));
        },
    );
    if shown {
        Ok(())
    } else {
        Err(windows_core::Error::from_hresult(windows_core::HRESULT(
            0x8000_4005_u32.cast_signed(),
        )))
    }
}

fn populate_dialog_body(
    body: &StackPanel,
    ui: &ReaderUi,
    evaluated: &[PrinterPolicyEvaluation],
    error: Option<&str>,
    printer_combo: &ComboBox,
    mode_combo: &ComboBox,
    range_box: &TextBox,
) -> windows_core::Result<()> {
    let children = body.Children()?;
    if let Some(error) = error {
        children.Append(&text_block(error, 13.0, true, ACCENT_RED)?)?;
    }
    children.Append(&text_block(
        &quota_banner_text(&ui.session, ui.session.quota_status()),
        13.0,
        true,
        TEXT_PRIMARY,
    )?)?;
    children.Append(&text_block(
        "每一页都会印上追溯水印（打印时间、本机 IP、设备码）。文档只能输出到物理打印机，确认后消耗 1 次打印次数。",
        12.5,
        false,
        TEXT_SECONDARY,
    )?)?;
    children.Append(&text_block("选择物理打印机：", 13.5, true, TEXT_PRIMARY)?)?;
    children.Append(printer_combo)?;
    children.Append(&text_block("打印范围：", 13.5, true, TEXT_PRIMARY)?)?;
    children.Append(mode_combo)?;
    children.Append(range_box)?;

    let blocked: Vec<&PrinterPolicyEvaluation> = evaluated.iter().filter(|p| !p.allowed).collect();
    if blocked.is_empty() {
        children.Append(&text_block(
            "已安装的打印机均通过物理打印机校验。",
            12.0,
            false,
            ACCENT_GREEN,
        )?)?;
    } else {
        children.Append(&text_block(
            &format!("已屏蔽 {} 台不符合要求的打印机：", blocked.len()),
            12.5,
            true,
            ACCENT_RED,
        )?)?;
        for printer in blocked.iter().take(4) {
            children.Append(&text_block(
                &format!("• {}：{}", printer.printer_name, printer.reason),
                11.5,
                false,
                TEXT_SECONDARY,
            )?)?;
        }
    }
    Ok(())
}

fn build_blocked_summary(evaluated: &[PrinterPolicyEvaluation]) -> String {
    if evaluated.is_empty() {
        return "系统中未安装任何打印机（或打印服务未运行）。".to_owned();
    }
    let mut lines = vec!["已拦截的打印机：".to_owned()];
    for item in evaluated.iter().filter(|p| !p.allowed).take(8) {
        lines.push(format!("• {}：{}", item.printer_name, item.reason));
    }
    lines.join("\n")
}

fn on_dialog_closed(ui: &ReaderUi, confirmed: bool) {
    let Some(dialog) = PRINT_DIALOG.with(|slot| slot.borrow_mut().take()) else {
        end_print_flow(ui);
        return;
    };
    if !confirmed || ui.state.borrow().locked {
        end_print_flow(ui);
        return;
    }

    let printer_index = usize::try_from(dialog.printer_combo.SelectedIndex().unwrap_or(0)).ok();
    let Some(printer_name) = printer_index.and_then(|i| dialog.allowed_names.get(i).cloned())
    else {
        end_print_flow(ui);
        notify(
            ui,
            InfoBarSeverity::Warning,
            "未选择打印机",
            "请先选择一台物理打印机。",
        );
        return;
    };
    let mode = PageSelectionMode::from_index(dialog.mode_combo.SelectedIndex().unwrap_or(0));
    let custom_range = dialog
        .range_box
        .Text()
        .unwrap_or_default()
        .to_string_lossy();

    match resolve_print_pages(
        mode,
        &custom_range,
        dialog.current_page,
        ui.session.page_count,
    ) {
        Ok(pages) => launch_print_job(ui, printer_name, pages),
        Err(err) => {
            // Nothing was printed and nothing deducted: show the dialog again with the input
            // preserved and the problem spelled out.
            let prefill = Prefill {
                printer_name: Some(printer_name),
                mode,
                custom_range,
                error: Some(format!("页码格式错误：{err}")),
            };
            if show_print_dialog(ui, dialog.evaluated, &prefill).is_err() {
                PRINT_DIALOG.with(|slot| slot.borrow_mut().take());
                end_print_flow(ui);
                show_info(
                    &ui.alert,
                    InfoBarSeverity::Error,
                    "页码格式错误",
                    &err.to_string(),
                );
            }
        }
    }
}

fn launch_print_job(ui: &ReaderUi, printer_name: String, pages: Vec<i32>) {
    {
        let mut state = ui.state.borrow_mut();
        state.print_phase = PrintPhase::Printing;
        state.last_printer = Some(printer_name.clone());
    }
    let _ = ui.print_btn.SetIsEnabled(false);
    set_status(ui, &format!("正在向打印机“{printer_name}”提交打印任务…"));

    // Time, IP and machine code are stamped onto every page regardless of the on-screen
    // watermark settings.
    let watermark = ui
        .session
        .print_watermark_text(&LocalDateTime::now().format_date_time());
    let session = Arc::clone(&ui.session);
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let outcome = run_print_job(&session, &dq, &printer_name, &pages, &watermark);
        let _ = enqueue_ui(&dq, move || {
            with_reader(|ui| finish_print(ui, outcome));
        });
    });
}

fn run_print_job(
    session: &ReaderSession,
    dq: &winui3::bindings::Microsoft::UI::Dispatching::DispatcherQueue,
    printer_name: &str,
    pages: &[i32],
    watermark: &str,
) -> PrintOutcome {
    let store = LocalQuotaStore::default();
    let ctx = PrintJobContext {
        policy: &session.policy,
        printer_name,
        machine_hash: &session.device.machine_hash,
        page_indices: pages,
        watermark_text: watermark,
    };
    let printed = session.with_pdf(|pdf| {
        print_document_guarded(pdf, &store, &ctx, |current, total| {
            let _ = enqueue_ui(dq, move || {
                with_reader(|ui| set_status(ui, &format!("正在打印：第 {current} / {total} 页…")));
            });
        })
    });
    match printed {
        None => PrintOutcome {
            result: Err(PrintFailure {
                title: "打印被拒绝",
                message: "文档已关闭，打印没有开始，本次打印次数未扣减。".to_owned(),
            }),
            lock_reason: None,
        },
        Some(Ok(status)) => {
            session.set_quota_status(status);
            PrintOutcome {
                result: Ok(pages.len()),
                lock_reason: None,
            }
        }
        Some(Err(err)) => {
            // Whatever went wrong, read the allowance back so the window shows the real
            // remaining count (a partially sent job keeps its deduction).
            let lock_reason =
                match store.initialize_or_verify(&session.policy, &session.device.machine_hash) {
                    Ok(status) => {
                        session.set_quota_status(status);
                        None
                    }
                    Err(verify_err) if is_fatal_authorisation_error(&verify_err) => {
                        Some(verify_err.to_string())
                    }
                    Err(_) => None,
                };
            PrintOutcome {
                result: Err(describe_print_error(&err)),
                lock_reason,
            }
        }
    }
}

fn finish_print(ui: &ReaderUi, outcome: PrintOutcome) {
    end_print_flow(ui);
    update_quota_display(ui);
    let quota = ui.session.quota_status();
    match outcome.result {
        Ok(page_total) => {
            set_status(ui, "打印任务已提交");
            notify(
                ui,
                InfoBarSeverity::Success,
                "打印完成",
                &format!(
                    "打印任务已提交至物理打印机（共 {page_total} 页）。\n剩余打印次数：{} / {}",
                    quota.remaining_prints, quota.max_prints
                ),
            );
        }
        Err(failure) => {
            set_status(ui, "打印未完成");
            notify(
                ui,
                InfoBarSeverity::Error,
                failure.title,
                &format!(
                    "{}\n当前剩余打印次数：{} / {}",
                    failure.message, quota.remaining_prints, quota.max_prints
                ),
            );
        }
    }
    if let Some(reason) = outcome.lock_reason {
        lock_and_close(ui, &reason);
    }
}

/// Returns the shared print workflow state to idle.
fn end_print_flow(ui: &ReaderUi) {
    ui.state.borrow_mut().print_phase = PrintPhase::Idle;
}

fn set_status(ui: &ReaderUi, text: &str) {
    let _ = ui.status_tb.SetText(&HSTRING::from(text));
}

/// Shows a message in the notification bar and, when no other dialog is open, as a dialog too.
fn notify(ui: &ReaderUi, severity: InfoBarSeverity, title: &str, message: &str) {
    show_info(&ui.alert, severity, title, message);
    if let Ok(root) = ui.win.xaml_root() {
        let _ = show_alert_dialog(&root, title, message, "确定");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(
        name: &str,
        allowed: bool,
        is_default: bool,
        is_offline: bool,
    ) -> PrinterPolicyEvaluation {
        PrinterPolicyEvaluation {
            printer_name: name.to_owned(),
            allowed,
            summary: String::new(),
            reason: if allowed {
                String::new()
            } else {
                "虚拟打印机".to_owned()
            },
            is_default,
            driver_name: String::new(),
            port_name: String::new(),
            is_offline,
        }
    }

    #[test]
    fn print_errors_always_say_whether_the_count_was_deducted() {
        let partial = describe_print_error(&CoreError::PrintFailed {
            message: "提交第 3 页时出错\n已有 2 页送往打印机，本次打印次数已扣减。".to_owned(),
            quota_consumed: true,
        });
        assert_eq!(partial.title, "打印失败");
        assert!(partial.message.contains("已扣减"));

        let rolled_back = describe_print_error(&CoreError::PrintFailed {
            message: "打印机拒绝了打印作业\n尚未送出任何页面，本次打印次数未扣减。".to_owned(),
            quota_consumed: false,
        });
        assert!(rolled_back.message.contains("未扣减"));

        for early in [
            CoreError::PrinterSecurity("拦截虚拟端口".to_owned()),
            CoreError::QuotaExhausted,
            CoreError::DocumentExpired,
            CoreError::InvalidArgument("页码超出文档范围。".to_owned()),
        ] {
            let refused = describe_print_error(&early);
            assert_eq!(refused.title, "打印被拒绝");
            assert!(refused.message.contains("未扣减"), "{}", refused.message);
            assert!(refused.message.starts_with(&early.to_string()));
        }
    }

    #[test]
    fn only_tampering_and_expiry_lock_the_document() {
        assert!(is_fatal_authorisation_error(&CoreError::DocumentExpired));
        assert!(is_fatal_authorisation_error(&CoreError::QuotaTampered(
            String::new()
        )));
        assert!(!is_fatal_authorisation_error(&CoreError::QuotaExhausted));
        assert!(!is_fatal_authorisation_error(&CoreError::QuotaLockTimeout(
            String::new()
        )));
        assert!(!is_fatal_authorisation_error(&CoreError::PrinterSecurity(
            String::new()
        )));
    }

    #[test]
    fn printer_list_marks_offline_and_default_and_prefers_last_used() {
        let office = printer("Office Laser", true, false, true);
        let desk = printer("Desk Inkjet", true, true, false);
        let hall = printer("Hall Copier", true, false, false);
        assert_eq!(printer_display_name(&office), "Office Laser（脱机）");
        assert_eq!(printer_display_name(&desk), "Desk Inkjet（默认）");
        assert_eq!(printer_display_name(&hall), "Hall Copier");

        let allowed = [&office, &desk, &hall];
        assert_eq!(preferred_printer_index(&allowed, Some("hall copier")), 2);
        assert_eq!(
            preferred_printer_index(&allowed, Some("Removed Printer")),
            1
        );
        assert_eq!(preferred_printer_index(&allowed, None), 1);
        assert_eq!(preferred_printer_index(&[&office, &hall], None), 0);
    }

    #[test]
    fn blocked_summary_lists_only_blocked_printers_with_reasons() {
        let list = [
            printer("Office Laser", true, false, false),
            printer("Microsoft Print to PDF", false, false, false),
        ];
        let summary = build_blocked_summary(&list);
        assert!(summary.contains("Microsoft Print to PDF：虚拟打印机"));
        assert!(!summary.contains("Office Laser"));
        assert!(build_blocked_summary(&[]).contains("未安装任何打印机"));
    }
}
