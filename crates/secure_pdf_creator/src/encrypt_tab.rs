//! Encryption and batch `.spdf` distribution tab for `SecurePdfCreator`.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use secure_pdf_core::{LocalDateTime, MoireLevel};
use windows_core::{HSTRING, Interface};
use winui3::app::enqueue_ui;
use winui3::bindings::Microsoft::UI::Dispatching::DispatcherQueue;
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    Border, Button, CheckBox, ComboBox, Grid, InfoBar, InfoBarSeverity, ListView,
    ListViewSelectionMode, ProgressBar, TextBlock, TextBox,
};
use winui3::bindings::Microsoft::UI::Xaml::{HorizontalAlignment, TextWrapping, UIElement};
use winui3::bindings::Windows::UI::Color;
use winui3::controls::{
    ACCENT_GREEN, ACCENT_RED, BG_SURFACE, TEXT_PRIMARY, TEXT_SECONDARY, add_grid_cols, brush_base,
    button, card_border, check_box, combo_box, gl_auto, gl_px, gl_star, grid_rows, hv, info_bar,
    pad, place_in_grid, progress_bar, scroll_viewer_vertical, show_info, stack_h, stack_v,
    text_block, text_box, uniform, wire_file_drop,
};
use winui3::dialog::show_confirm_dialog;
use winui3::interop::{
    copy_unicode_text, pick_folder, pick_open_files, pick_save_file, shell_open, window_hwnd,
};
use winui3::window::FluentWindow;

use crate::logic::{
    BatchPolicyInput, BatchResultRow, BatchSummary, BusyGuard, CreatorQueue, EXPIRY_CUSTOM_INDEX,
    EXPIRY_PRESETS, PolicyOption, clipboard_listing, encrypt_batch, expand_to_pdf_files,
    expiry_hours_for_choice, export_batch_csv, normalize_output_dir, now_policy_ticks,
    parse_max_prints, validate_policy_input,
};
use crate::shell::desktop_dir;

/// Process-wide flags shared between the encryption tab and the window close guard.
#[derive(Clone, Default)]
pub struct SessionFlags {
    /// The result table holds verification codes that were neither exported nor copied.
    pub unsaved_codes: Arc<AtomicBool>,
    /// A batch is being encrypted right now.
    pub encrypting: Arc<AtomicBool>,
}

#[derive(Default)]
struct EncryptState {
    queue: CreatorQueue,
    results: Vec<BatchResultRow>,
    last_output_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct EncryptUi {
    win: FluentWindow,
    dq: DispatcherQueue,
    state: Arc<Mutex<EncryptState>>,
    flags: SessionFlags,
    queue_list: ListView,
    queue_count_tb: TextBlock,
    prints_box: TextBox,
    expiry_combo: ComboBox,
    custom_hours_box: TextBox,
    moire_combo: ComboBox,
    chk_visible_wm: CheckBox,
    chk_hidden_wm: CheckBox,
    chk_blur_unfocus: CheckBox,
    chk_adjust_moire: CheckBox,
    wm_note_box: TextBox,
    out_dir_box: TextBox,
    start_btn: Button,
    results_list: ListView,
    results_count_tb: TextBlock,
    copy_selected_btn: Button,
    copy_all_btn: Button,
    export_csv_btn: Button,
    clear_results_btn: Button,
    open_out_btn: Button,
    progress: ProgressBar,
    status_tb: TextBlock,
    alert: InfoBar,
}

impl EncryptUi {
    /// Locks the tab state; a poisoned lock is recovered so one failed handler cannot freeze
    /// the whole tab.
    fn state(&self) -> MutexGuard<'_, EncryptState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_status(&self, text: &str) {
        let _ = self.status_tb.SetText(&HSTRING::from(text));
    }
}

pub fn build_encrypt_tab(
    win: FluentWindow,
    dq: DispatcherQueue,
    flags: SessionFlags,
) -> windows_core::Result<Grid> {
    let root = grid_rows(&[gl_star(1.0)])?;
    add_grid_cols(&root, &[gl_star(1.45), gl_px(14.0), gl_star(0.9)])?;
    root.SetPadding(uniform(14.0))?;

    let state = Arc::new(Mutex::new(EncryptState::default()));
    let ui = create_widgets(win, dq, state, flags)?;

    let left_panel = build_left_column(&ui)?;
    let right_panel = build_right_column(&ui)?;

    place_in_grid(&left_panel, 0, 0, 1, 1)?;
    place_in_grid(&right_panel, 0, 2, 1, 1)?;
    root.Children()?.Append(&left_panel)?;
    root.Children()?.Append(&right_panel)?;

    wire_drag_drop(&ui, &left_panel)?;
    wire_policy_actions(&ui)?;
    wire_result_actions(&ui)?;
    refresh_queue_count(&ui);
    refresh_result_controls(&ui);

    Ok(root)
}

fn create_widgets(
    win: FluentWindow,
    dq: DispatcherQueue,
    state: Arc<Mutex<EncryptState>>,
    flags: SessionFlags,
) -> windows_core::Result<EncryptUi> {
    let queue_list = ListView::new()?;
    queue_list.SetSelectionMode(ListViewSelectionMode::Single)?;
    queue_list.SetHeight(98.0)?;

    let results_list = ListView::new()?;
    results_list.SetSelectionMode(ListViewSelectionMode::Single)?;
    results_list.SetMinHeight(96.0)?;

    let prints_box = text_box("0 表示禁止打印，最多 1000", false)?;
    prints_box.SetText(&HSTRING::from("1"))?;

    let preset_labels: Vec<&str> = EXPIRY_PRESETS.iter().map(|(label, _)| *label).collect();
    let expiry_combo = combo_box(&preset_labels, 0)?;
    let custom_hours_box = text_box("小时数，如 0.5", false)?;
    custom_hours_box.SetText(&HSTRING::from("12"))?;
    custom_hours_box.SetIsEnabled(false)?;

    let moire_combo = combo_box(&["关闭", "低（推荐，最清晰）", "中", "高（强干扰）"], 1)?;

    let out_dir_box = text_box("留空则输出到每个原文件所在的文件夹", false)?;

    let start_btn = button("开始加密并生成验证码", true)?;

    Ok(EncryptUi {
        win,
        dq,
        state,
        flags,
        queue_list,
        queue_count_tb: text_block("", 12.5, false, TEXT_SECONDARY)?,
        prints_box,
        expiry_combo,
        custom_hours_box,
        moire_combo,
        chk_visible_wm: check_box("在页面上叠加半透明可见水印（可选明水印）", false)?,
        chk_hidden_wm: check_box("启用隐形溯源水印（时间、IP、设备码）", true)?,
        chk_blur_unfocus: check_box("阅读窗口失去焦点时自动遮挡内容", true)?,
        chk_adjust_moire: check_box("允许接收方调节摩尔纹强度", false)?,
        wm_note_box: text_box(
            "可选，例如：内部资料 严禁外传（打印时 80% 透明度置于底层）",
            false,
        )?,
        out_dir_box,
        start_btn,
        results_list,
        results_count_tb: text_block("", 12.5, false, TEXT_SECONDARY)?,
        copy_selected_btn: button("复制选中验证码", false)?,
        copy_all_btn: button("复制全部清单", false)?,
        export_csv_btn: button("导出清单 (.csv)", false)?,
        clear_results_btn: button("清空结果", false)?,
        open_out_btn: button("打开输出文件夹", false)?,
        progress: progress_bar()?,
        status_tb: text_block("就绪", 12.5, false, TEXT_SECONDARY)?,
        alert: info_bar()?,
    })
}

/// Column layout shared by the result table header and its rows.
fn result_columns(grid: &Grid) -> windows_core::Result<()> {
    add_grid_cols(
        grid,
        &[
            gl_star(1.3),
            gl_px(178.0),
            gl_px(70.0),
            gl_px(214.0),
            gl_px(44.0),
            gl_star(2.0),
        ],
    )?;
    grid.SetColumnSpacing(10.0)
}

fn result_cell(
    grid: &Grid,
    col: i32,
    text: &str,
    bold: bool,
    color: Color,
) -> windows_core::Result<()> {
    let tb = text_block(text, 12.5, bold, color)?;
    tb.SetTextWrapping(TextWrapping::Wrap)?;
    place_in_grid(&tb, 0, col, 1, 1)?;
    grid.Children()?.Append(&tb)?;
    Ok(())
}

fn result_header() -> windows_core::Result<Grid> {
    let grid = grid_rows(&[gl_auto()])?;
    result_columns(&grid)?;
    grid.SetPadding(pad(16.0, 0.0, 12.0, 0.0))?;
    let titles = [
        "原 PDF 文件",
        "验证码",
        "允许打印",
        "有效期至",
        "状态",
        "输出路径 / 失败原因",
    ];
    for (col, title) in (0_i32..).zip(titles) {
        result_cell(&grid, col, title, true, TEXT_SECONDARY)?;
    }
    Ok(grid)
}

fn result_row_element(row: &BatchResultRow) -> windows_core::Result<Grid> {
    let grid = grid_rows(&[gl_auto()])?;
    result_columns(&grid)?;
    grid.SetPadding(hv(0.0, 4.0))?;
    let ok = row.succeeded();
    let detail = if ok { &row.output_spdf } else { &row.note };
    result_cell(&grid, 0, row.source_file_name(), false, TEXT_PRIMARY)?;
    result_cell(&grid, 1, &row.verification_code, true, TEXT_PRIMARY)?;
    result_cell(&grid, 2, &row.max_prints, false, TEXT_SECONDARY)?;
    result_cell(&grid, 3, &row.expires_at, false, TEXT_SECONDARY)?;
    result_cell(
        &grid,
        4,
        &row.status,
        true,
        if ok { ACCENT_GREEN } else { ACCENT_RED },
    )?;
    result_cell(
        &grid,
        5,
        detail,
        false,
        if ok { TEXT_SECONDARY } else { ACCENT_RED },
    )?;
    Ok(grid)
}

fn build_left_column(ui: &EncryptUi) -> windows_core::Result<Grid> {
    let grid = grid_rows(&[gl_auto(), gl_star(1.0), gl_auto()])?;
    grid.SetRowSpacing(12.0)?;

    // --- queue
    let queue_card = card_border(uniform(12.0))?;
    let queue_box = stack_v(8.0)?;
    let queue_hdr = grid_rows(&[gl_auto()])?;
    add_grid_cols(&queue_hdr, &[gl_star(1.0), gl_auto()])?;
    let titles = stack_v(2.0)?;
    titles
        .Children()?
        .Append(&text_block("待加密的 PDF 文件", 15.0, true, TEXT_PRIMARY)?)?;
    titles.Children()?.Append(&ui.queue_count_tb)?;
    let queue_btns = stack_h(8.0)?;
    let btn_add_files = button("添加 PDF 文件...", false)?;
    let btn_add_dir = button("添加文件夹...", false)?;
    let btn_remove = button("移除选中", false)?;
    let btn_clear = button("清空列表", false)?;
    attach_queue_buttons(ui, &btn_add_files, &btn_add_dir, &btn_remove, &btn_clear)?;
    for b in [&btn_add_files, &btn_add_dir, &btn_remove, &btn_clear] {
        queue_btns.Children()?.Append(b)?;
    }
    place_in_grid(&titles, 0, 0, 1, 1)?;
    place_in_grid(&queue_btns, 0, 1, 1, 1)?;
    queue_hdr.Children()?.Append(&titles)?;
    queue_hdr.Children()?.Append(&queue_btns)?;
    queue_box.Children()?.Append(&queue_hdr)?;
    queue_box
        .Children()?
        .Append(&list_frame(&ui.queue_list.cast::<UIElement>()?)?)?;
    queue_card.SetChild(&queue_box)?;

    // --- results
    let results_card = card_border(uniform(12.0))?;
    let results_grid = grid_rows(&[gl_auto(), gl_auto(), gl_star(1.0)])?;
    results_grid.SetRowSpacing(6.0)?;
    let results_titles = stack_v(2.0)?;
    results_titles.Children()?.Append(&text_block(
        "加密结果（每个文件有独立的验证码）",
        15.0,
        true,
        TEXT_PRIMARY,
    )?)?;
    results_titles.Children()?.Append(&ui.results_count_tb)?;
    let header = result_header()?;
    let frame = list_frame(&ui.results_list.cast::<UIElement>()?)?;
    place_in_grid(&results_titles, 0, 0, 1, 1)?;
    place_in_grid(&header, 1, 0, 1, 1)?;
    place_in_grid(&frame, 2, 0, 1, 1)?;
    results_grid.Children()?.Append(&results_titles)?;
    results_grid.Children()?.Append(&header)?;
    results_grid.Children()?.Append(&frame)?;
    results_card.SetChild(&results_grid)?;

    // --- status + result actions
    let bottom = card_border(uniform(12.0))?;
    let bot_col = stack_v(8.0)?;
    bot_col.Children()?.Append(&ui.progress)?;
    bot_col.Children()?.Append(&ui.status_tb)?;
    let action_row = stack_h(8.0)?;
    for b in [
        &ui.copy_selected_btn,
        &ui.copy_all_btn,
        &ui.export_csv_btn,
        &ui.open_out_btn,
        &ui.clear_results_btn,
    ] {
        action_row.Children()?.Append(b)?;
    }
    bot_col.Children()?.Append(&action_row)?;
    bot_col.Children()?.Append(&ui.alert)?;
    bottom.SetChild(&bot_col)?;

    place_in_grid(&queue_card, 0, 0, 1, 1)?;
    place_in_grid(&results_card, 1, 0, 1, 1)?;
    place_in_grid(&bottom, 2, 0, 1, 1)?;
    grid.Children()?.Append(&queue_card)?;
    grid.Children()?.Append(&results_card)?;
    grid.Children()?.Append(&bottom)?;
    Ok(grid)
}

fn list_frame(list: &UIElement) -> windows_core::Result<Border> {
    let wrap = card_border(uniform(4.0))?;
    wrap.SetBackground(&brush_base(BG_SURFACE)?)?;
    wrap.SetChild(list)?;
    Ok(wrap)
}

fn attach_queue_buttons(
    ui: &EncryptUi,
    btn_files: &Button,
    btn_dir: &Button,
    btn_remove: &Button,
    btn_clear: &Button,
) -> windows_core::Result<()> {
    let ui_f = ui.clone();
    btn_files
        .Click(move |_, _| {
            let owner = window_hwnd(&ui_f.win.window).ok();
            if let Ok(paths) = pick_open_files(
                owner,
                "选择一个或多个待加密的 PDF 文件",
                &[("PDF 文档 (*.pdf)", "*.pdf")],
                true,
            ) && !paths.is_empty()
            {
                add_paths_in_background(&ui_f, paths);
            }
        })?
        .forget();

    let ui_d = ui.clone();
    btn_dir
        .Click(move |_, _| {
            let owner = window_hwnd(&ui_d.win.window).ok();
            if let Ok(Some(dir)) = pick_folder(owner, "选择包含 PDF 文件的文件夹", None) {
                add_paths_in_background(&ui_d, vec![dir]);
            }
        })?
        .forget();

    let ui_r = ui.clone();
    btn_remove
        .Click(move |_, _| {
            let sel = ui_r.queue_list.SelectedIndex().unwrap_or(-1);
            let (Ok(idx), Ok(idx_u32)) = (usize::try_from(sel), u32::try_from(sel)) else {
                return;
            };
            if ui_r.state().queue.remove_at(idx).is_some()
                && let Ok(items) = ui_r.queue_list.Items()
            {
                // Only the removed row is touched; the rest of the list stays as it is.
                let _ = items.RemoveAt(idx_u32);
            }
            refresh_queue_count(&ui_r);
        })?
        .forget();

    let ui_c = ui.clone();
    btn_clear
        .Click(move |_, _| {
            ui_c.state().queue.clear();
            if let Ok(items) = ui_c.queue_list.Items() {
                let _ = items.Clear();
            }
            refresh_queue_count(&ui_c);
        })?
        .forget();

    Ok(())
}

/// Expands folders and filters PDFs on a worker thread, then appends only the newly added
/// entries to the on-screen list (no full rebuild, no disk access on the UI thread).
fn add_paths_in_background(ui: &EncryptUi, paths: Vec<PathBuf>) {
    ui.set_status("正在扫描所选文件和文件夹...");
    let ui_bg = ui.clone();
    std::thread::spawn(move || {
        let found = catch_unwind(AssertUnwindSafe(|| expand_to_pdf_files(&paths)));
        let ui_fin = ui_bg.clone();
        let _ = enqueue_ui(&ui_bg.dq, move || {
            ui_fin.set_status("就绪");
            let Ok(found) = found else {
                show_info(
                    &ui_fin.alert,
                    InfoBarSeverity::Error,
                    "扫描失败",
                    "扫描所选文件夹时发生内部错误，请重试。",
                );
                return;
            };
            let found_count = found.len();
            let added = ui_fin.state().queue.push_many(found);
            append_queue_rows(&ui_fin, &added);
            refresh_queue_count(&ui_fin);
            let skipped = found_count - added.len();
            let message = if skipped == 0 {
                format!("已加入 {} 个 PDF 文件。", added.len())
            } else {
                format!(
                    "已加入 {} 个 PDF 文件，另有 {skipped} 个已在列表中，未重复添加。",
                    added.len()
                )
            };
            show_info(
                &ui_fin.alert,
                if added.is_empty() {
                    InfoBarSeverity::Warning
                } else {
                    InfoBarSeverity::Informational
                },
                "文件列表已更新",
                &message,
            );
        });
    });
}

fn append_queue_rows(ui: &EncryptUi, added: &[PathBuf]) {
    let Ok(items) = ui.queue_list.Items() else {
        return;
    };
    for path in added {
        if let Ok(tb) = text_block(&path.to_string_lossy(), 13.0, false, TEXT_PRIMARY) {
            let _ = tb.SetPadding(hv(6.0, 2.0));
            let _ = items.Append(&tb);
        }
    }
}

fn refresh_queue_count(ui: &EncryptUi) {
    let count = ui.state().queue.items().len();
    let _ = ui.queue_count_tb.SetText(&HSTRING::from(format!(
        "共 {count} 个文件（可将文件或文件夹拖到此处）"
    )));
}

fn labeled(label: &str, control: &UIElement) -> windows_core::Result<UIElement> {
    let panel = stack_v(3.0)?;
    panel
        .Children()?
        .Append(&text_block(label, 12.5, false, TEXT_SECONDARY)?)?;
    panel.Children()?.Append(control)?;
    panel.cast()
}

fn build_right_column(ui: &EncryptUi) -> windows_core::Result<Border> {
    let card = card_border(uniform(14.0))?;
    let card_grid = grid_rows(&[gl_star(1.0), gl_auto()])?;
    card_grid.SetRowSpacing(10.0)?;

    let scroller = scroll_viewer_vertical()?;
    let col = stack_v(8.0)?;
    col.SetPadding(pad(0.0, 0.0, 6.0, 2.0))?;

    col.Children()?
        .Append(&text_block("安全策略", 16.0, true, TEXT_PRIMARY)?)?;
    col.Children()?.Append(&text_block(
        "验证码由程序为每个文件自动生成，加密完成后显示在结果表中。",
        12.5,
        false,
        TEXT_SECONDARY,
    )?)?;

    col.Children()?.Append(&labeled(
        "允许打印次数（0 = 完全禁止打印）",
        &ui.prints_box.cast()?,
    )?)?;

    let expiry_row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&expiry_row, &[gl_star(1.0), gl_px(8.0), gl_px(120.0)])?;
    place_in_grid(&ui.expiry_combo, 0, 0, 1, 1)?;
    place_in_grid(&ui.custom_hours_box, 0, 2, 1, 1)?;
    expiry_row.Children()?.Append(&ui.expiry_combo)?;
    expiry_row.Children()?.Append(&ui.custom_hours_box)?;
    col.Children()?.Append(&labeled(
        "有效期（按小时，自定义时支持小数，如 0.5 = 30 分钟）",
        &expiry_row.cast()?,
    )?)?;

    col.Children()?
        .Append(&labeled("防翻拍摩尔纹强度", &ui.moire_combo.cast()?)?)?;

    col.Children()?.Append(&ui.chk_hidden_wm)?;
    col.Children()?.Append(&ui.chk_visible_wm)?;
    col.Children()?.Append(&ui.chk_blur_unfocus)?;
    col.Children()?.Append(&ui.chk_adjust_moire)?;

    col.Children()?.Append(&labeled(
        "自定义水印文字（可选，用于打印底层水印及页面备注）",
        &ui.wm_note_box.cast()?,
    )?)?;

    let out_row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&out_row, &[gl_star(1.0), gl_px(8.0), gl_auto()])?;
    let btn_pick_out = button("选择...", false)?;
    let ui_o = ui.clone();
    btn_pick_out
        .Click(move |_, _| pick_output_dir(&ui_o))?
        .forget();
    place_in_grid(&ui.out_dir_box, 0, 0, 1, 1)?;
    place_in_grid(&btn_pick_out, 0, 2, 1, 1)?;
    out_row.Children()?.Append(&ui.out_dir_box)?;
    out_row.Children()?.Append(&btn_pick_out)?;
    col.Children()?.Append(&labeled(
        "输出文件夹（留空则输出到每个原文件所在的文件夹）",
        &out_row.cast()?,
    )?)?;

    scroller.SetContent(&col)?;

    ui.start_btn
        .SetHorizontalAlignment(HorizontalAlignment::Stretch)?;
    ui.start_btn.SetPadding(hv(14.0, 9.0))?;

    place_in_grid(&scroller, 0, 0, 1, 1)?;
    place_in_grid(&ui.start_btn, 1, 0, 1, 1)?;
    card_grid.Children()?.Append(&scroller)?;
    card_grid.Children()?.Append(&ui.start_btn)?;

    card.SetChild(&card_grid)?;
    Ok(card)
}

/// Opens the folder picker, starting at the typed folder or, failing that, the real Desktop.
fn pick_output_dir(ui: &EncryptUi) {
    let typed = ui.out_dir_box.Text().unwrap_or_default().to_string_lossy();
    let start = normalize_output_dir(&typed)
        .ok()
        .flatten()
        .filter(|dir| dir.is_dir())
        .or_else(desktop_dir);
    let owner = window_hwnd(&ui.win.window).ok();
    match pick_folder(owner, "选择 .spdf 文件的输出文件夹", start.as_deref()) {
        Ok(Some(dir)) => {
            let _ = ui
                .out_dir_box
                .SetText(&HSTRING::from(dir.to_string_lossy().as_ref()));
        }
        Ok(None) => {}
        Err(err) => show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "无法打开文件夹选择框",
            &format!(
                "请直接在输入框中填写完整路径。（错误代码 0x{:08X}）",
                err.code().0
            ),
        ),
    }
}

fn wire_drag_drop(ui: &EncryptUi, target: &Grid) -> windows_core::Result<()> {
    let root_ui: UIElement = target.cast()?;
    let ui_drop = ui.clone();
    wire_file_drop(&root_ui, ui.dq.clone(), move |dropped_paths| {
        add_paths_in_background(&ui_drop, dropped_paths);
    })
}

fn wire_policy_actions(ui: &EncryptUi) -> windows_core::Result<()> {
    let ui_go = ui.clone();
    ui.start_btn
        .Click(move |_, _| start_batch_encryption(&ui_go))?
        .forget();

    // The custom-hours box is only editable while "自定义小时数" is selected.
    let ui_exp = ui.clone();
    ui.expiry_combo
        .SelectionChanged(move |_, _| {
            let custom = ui_exp.expiry_combo.SelectedIndex().unwrap_or(0) == EXPIRY_CUSTOM_INDEX;
            let _ = ui_exp.custom_hours_box.SetIsEnabled(custom);
        })?
        .forget();

    Ok(())
}

fn wire_result_actions(ui: &EncryptUi) -> windows_core::Result<()> {
    let ui_sel = ui.clone();
    ui.copy_selected_btn
        .Click(move |_, _| copy_selected_code(&ui_sel))?
        .forget();

    let ui_all = ui.clone();
    ui.copy_all_btn
        .Click(move |_, _| copy_all_codes(&ui_all))?
        .forget();

    let ui_csv = ui.clone();
    ui.export_csv_btn
        .Click(move |_, _| export_csv_handler(&ui_csv))?
        .forget();

    let ui_clear = ui.clone();
    ui.clear_results_btn
        .Click(move |_, _| clear_results_handler(&ui_clear))?
        .forget();

    let ui_open = ui.clone();
    ui.open_out_btn
        .Click(move |_, _| {
            let dir = ui_open.state().last_output_dir.clone();
            if let Some(dir) = dir {
                shell_open(&dir.to_string_lossy());
            }
        })?
        .forget();

    Ok(())
}

fn read_policy_input(ui: &EncryptUi) -> Result<BatchPolicyInput, String> {
    let max_prints = parse_max_prints(&ui.prints_box.Text().unwrap_or_default().to_string_lossy())?;
    let expiry_hours = expiry_hours_for_choice(
        ui.expiry_combo.SelectedIndex().unwrap_or(0),
        &ui.custom_hours_box
            .Text()
            .unwrap_or_default()
            .to_string_lossy(),
    )?;
    let moire_level = match ui.moire_combo.SelectedIndex().unwrap_or(1) {
        0 => MoireLevel::Off,
        2 => MoireLevel::Medium,
        3 => MoireLevel::High,
        _ => MoireLevel::Low,
    };
    // A box that cannot be read counts as its safe state: protections on, moire adjustment off.
    let toggles = [
        (PolicyOption::HiddenWatermark, &ui.chk_hidden_wm, true),
        (PolicyOption::BlurOnUnfocus, &ui.chk_blur_unfocus, true),
        (PolicyOption::AllowAdjustMoire, &ui.chk_adjust_moire, false),
        (PolicyOption::VisibleWatermark, &ui.chk_visible_wm, false),
    ];
    let options = toggles
        .into_iter()
        .filter(|(_, check_box, fallback)| check_box.IsChecked().unwrap_or(*fallback))
        .map(|(option, _, _)| option)
        .collect();

    Ok(BatchPolicyInput {
        max_prints,
        expiry_hours,
        moire_level,
        options,
        watermark_label: ui.wm_note_box.Text().unwrap_or_default().to_string_lossy(),
    })
}

fn start_batch_encryption(ui: &EncryptUi) {
    let pdfs = ui.state().queue.items().to_vec();
    if pdfs.is_empty() {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "列表为空",
            "请先添加至少一个 PDF 文件。",
        );
        return;
    }

    let policy = match read_policy_input(ui)
        .and_then(|input| validate_policy_input(&input, now_policy_ticks()))
    {
        Ok(p) => p,
        Err(msg) => {
            show_info(&ui.alert, InfoBarSeverity::Error, "参数错误", &msg);
            return;
        }
    };

    let typed_dir = ui.out_dir_box.Text().unwrap_or_default().to_string_lossy();
    let custom_out_dir = match normalize_output_dir(&typed_dir).and_then(|dir| {
        if let Some(dir) = &dir {
            fs::create_dir_all(dir).map_err(|e| format!("输出文件夹无效或无法创建：{e}"))?;
        }
        Ok(dir)
    }) {
        Ok(dir) => dir,
        Err(msg) => {
            show_info(&ui.alert, InfoBarSeverity::Error, "输出文件夹不可用", &msg);
            return;
        }
    };

    // Claimed last, so every early return above leaves the tab idle.
    let Some(busy) = BusyGuard::try_acquire(&ui.flags.encrypting) else {
        return;
    };
    let _ = ui.start_btn.SetIsEnabled(false);
    let _ = ui.clear_results_btn.SetIsEnabled(false);
    let _ = ui.progress.SetValue2(0.0);
    ui.set_status(&format!("正在加密（0 / {}）...", pdfs.len()));

    let ui_bg = ui.clone();
    std::thread::spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            encrypt_batch(
                &pdfs,
                custom_out_dir.as_deref(),
                &policy,
                |row, done, total| post_result_row(&ui_bg, row, done, total),
            )
        }));
        let ui_done = ui_bg.clone();
        // The busy flag is released on the UI thread, after every queued result row has been
        // shown, so the close guard never sees "idle" while codes are still on their way to
        // the table. If the closure never runs (window gone) or this thread unwinds, dropping
        // the guard still clears the flag.
        let _ = enqueue_ui(&ui_bg.dq, move || {
            drop(busy);
            finish_batch(&ui_done, outcome.ok());
        });
    });
}

/// Hands one finished row to the UI thread: it is stored and shown immediately, so nothing is
/// lost if a later file fails or the batch stops early.
fn post_result_row(ui: &EncryptUi, row: BatchResultRow, done: usize, total: usize) {
    #[allow(clippy::cast_precision_loss)]
    let pct = (done as f64) * 100.0 / (total.max(1) as f64);
    let ui_step = ui.clone();
    let _ = enqueue_ui(&ui.dq, move || {
        let file_name = row.source_file_name().to_owned();
        if row.succeeded() {
            ui_step.flags.unsaved_codes.store(true, Ordering::Release);
        }
        let element = result_row_element(&row).ok();
        {
            let mut st = ui_step.state();
            if row.succeeded() {
                st.last_output_dir = Path::new(&row.output_spdf).parent().map(Path::to_path_buf);
            }
            st.results.push(row);
        }
        if let Some(element) = element
            && let Ok(items) = ui_step.results_list.Items()
            && items.Append(&element).is_ok()
        {
            let _ = ui_step.results_list.ScrollIntoView(&element);
        }
        let _ = ui_step.progress.SetValue2(pct);
        ui_step.set_status(&format!("正在加密（{done} / {total}）：{file_name}"));
        refresh_result_controls(&ui_step);
    });
}

fn finish_batch(ui: &EncryptUi, summary: Option<BatchSummary>) {
    let _ = ui.start_btn.SetIsEnabled(true);
    let _ = ui.progress.SetValue2(100.0);
    refresh_result_controls(ui);
    let Some(summary) = summary else {
        let text = "加密过程中发生内部错误，本批次已停止。已完成的文件保留在结果表中，请核对后重试其余文件。";
        ui.set_status(text);
        show_info(&ui.alert, InfoBarSeverity::Error, "加密中断", text);
        return;
    };
    let text = summary.describe();
    ui.set_status(&text);
    let severity = if summary.aborted.is_some() {
        InfoBarSeverity::Error
    } else if summary.failed > 0 {
        InfoBarSeverity::Warning
    } else {
        InfoBarSeverity::Success
    };
    show_info(&ui.alert, severity, "加密完成", &text);
}

fn refresh_result_controls(ui: &EncryptUi) {
    let (total, succeeded, has_dir) = {
        let st = ui.state();
        (
            st.results.len(),
            st.results.iter().filter(|r| r.succeeded()).count(),
            st.last_output_dir.is_some(),
        )
    };
    let running = ui.flags.encrypting.load(Ordering::Acquire);
    let _ = ui.results_count_tb.SetText(&HSTRING::from(if total == 0 {
        "尚无记录。验证码只显示在此表中，请在关闭程序前导出或复制。".to_owned()
    } else {
        format!(
            "共 {total} 条记录：成功 {succeeded} 条，失败 {} 条。",
            total - succeeded
        )
    }));
    let _ = ui.copy_selected_btn.SetIsEnabled(succeeded > 0);
    let _ = ui.copy_all_btn.SetIsEnabled(succeeded > 0);
    // Failed rows are worth exporting too: the list records which files still need attention.
    let _ = ui.export_csv_btn.SetIsEnabled(total > 0);
    let _ = ui.clear_results_btn.SetIsEnabled(total > 0 && !running);
    let _ = ui.open_out_btn.SetIsEnabled(has_dir);
}

/// Copies text to the clipboard, retrying briefly while another program holds it.
fn copy_with_retry(ui: &EncryptUi, text: &str) -> bool {
    let Ok(owner) = window_hwnd(&ui.win.window) else {
        return false;
    };
    for attempt in 0..5 {
        if copy_unicode_text(owner, text).is_ok() {
            return true;
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_millis(60));
        }
    }
    show_info(
        &ui.alert,
        InfoBarSeverity::Warning,
        "复制失败",
        "剪贴板正被其它程序占用，请稍后重试。",
    );
    false
}

fn copy_selected_code(ui: &EncryptUi) {
    let selected = usize::try_from(ui.results_list.SelectedIndex().unwrap_or(-1)).ok();
    let (row, success_count) = {
        let st = ui.state();
        (
            selected.and_then(|idx| st.results.get(idx).cloned()),
            st.results.iter().filter(|r| r.succeeded()).count(),
        )
    };
    let Some(row) = row.filter(BatchResultRow::succeeded) else {
        show_info(
            &ui.alert,
            InfoBarSeverity::Informational,
            "未选中可复制的记录",
            "请先在结果表中选中一条加密成功的记录。",
        );
        return;
    };
    if copy_with_retry(ui, &row.verification_code) {
        // With a single code in the table, copying it means nothing is left unsaved.
        if success_count == 1 {
            ui.flags.unsaved_codes.store(false, Ordering::Release);
        }
        show_info(
            &ui.alert,
            InfoBarSeverity::Success,
            "已复制验证码",
            &format!(
                "{} 的验证码：{}",
                row.source_file_name(),
                row.verification_code
            ),
        );
    }
}

fn copy_all_codes(ui: &EncryptUi) {
    let rows = ui.state().results.clone();
    let count = rows.iter().filter(|r| r.succeeded()).count();
    if count == 0 {
        show_info(
            &ui.alert,
            InfoBarSeverity::Informational,
            "没有可复制的记录",
            "当前没有加密成功的记录。",
        );
        return;
    }
    if copy_with_retry(ui, &clipboard_listing(&rows)) {
        ui.flags.unsaved_codes.store(false, Ordering::Release);
        show_info(
            &ui.alert,
            InfoBarSeverity::Success,
            "已复制清单",
            &format!("已将 {count} 条加密记录复制到剪贴板。"),
        );
    }
}

fn export_csv_handler(ui: &EncryptUi) {
    let rows = ui.state().results.clone();
    if rows.is_empty() {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "暂无记录",
            "请先加密至少一个文件。",
        );
        return;
    }

    let now = LocalDateTime::now();
    let default_name = format!(
        "验证码清单_{:04}{:02}{:02}_{:02}{:02}{:02}.csv",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );
    let owner = window_hwnd(&ui.win.window).ok();
    match pick_save_file(
        owner,
        "导出验证码清单",
        &default_name,
        "csv",
        &[("CSV 清单 (*.csv)", "*.csv")],
    ) {
        Ok(Some(path)) => match export_batch_csv(&path, &rows) {
            Ok(()) => {
                ui.flags.unsaved_codes.store(false, Ordering::Release);
                show_info(
                    &ui.alert,
                    InfoBarSeverity::Success,
                    "清单已导出",
                    &path.to_string_lossy(),
                );
            }
            Err(e) => show_info(&ui.alert, InfoBarSeverity::Error, "导出失败", &e),
        },
        Ok(None) => {}
        Err(e) => show_info(
            &ui.alert,
            InfoBarSeverity::Error,
            "无法打开保存对话框",
            &format!("错误代码 0x{:08X}", e.code().0),
        ),
    }
}

/// Clearing the table destroys the only copy of the codes, so it always asks first.
fn clear_results_handler(ui: &EncryptUi) {
    if ui.flags.encrypting.load(Ordering::Acquire) || ui.state().results.is_empty() {
        return;
    }
    let Ok(root) = ui.win.xaml_root() else {
        return;
    };
    let unsaved = ui.flags.unsaved_codes.load(Ordering::Acquire);
    let message = if unsaved {
        "结果表中有尚未导出或复制的验证码。验证码只显示在这里，清空后无法找回，对应的 .spdf 文件将无法打开。\n\n确定要清空吗？"
    } else {
        "清空后结果表中的验证码将不再显示。确定要清空吗？"
    };
    let ui_yes = ui.clone();
    let shown = show_confirm_dialog(
        &root,
        "清空加密结果",
        message,
        "确认清空",
        "返回",
        move |confirmed| {
            if !confirmed || ui_yes.flags.encrypting.load(Ordering::Acquire) {
                return;
            }
            {
                let mut st = ui_yes.state();
                st.results.clear();
                st.last_output_dir = None;
            }
            if let Ok(items) = ui_yes.results_list.Items() {
                let _ = items.Clear();
            }
            ui_yes.flags.unsaved_codes.store(false, Ordering::Release);
            refresh_result_controls(&ui_yes);
            ui_yes.set_status("结果表已清空。");
        },
    );
    if !shown {
        show_info(
            &ui.alert,
            InfoBarSeverity::Warning,
            "无法清空",
            "请先关闭当前打开的对话框，然后重试。",
        );
    }
}
