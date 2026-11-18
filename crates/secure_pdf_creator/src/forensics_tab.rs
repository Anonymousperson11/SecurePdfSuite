//! Watermark forensics enhancement workbench tab for `SecurePdfCreator`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use secure_pdf_core::LocalDateTime;
use secure_pdf_core::forensics::{
    EnhanceMode, ForensicsImage, enhance_image, load_image_upright, save_png,
};
use secure_pdf_core::localtime::format_unix_seconds_local;
use secure_pdf_core::watermark::{MicrodotDecode, decode_microdots};
use windows_core::HSTRING;
use winui3::app::enqueue_ui;
use winui3::bindings::Microsoft::UI::Dispatching::DispatcherQueue;
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    Button, ComboBox, Grid, Image, InfoBar, InfoBarSeverity, ScrollViewer, TextBlock,
};
use winui3::bindings::Microsoft::UI::Xaml::Media::{ImageSource, Stretch};
use winui3::bindings::Microsoft::UI::Xaml::TextWrapping;
use winui3::controls::{
    BG_SURFACE, TEXT_PRIMARY, TEXT_SECONDARY, add_grid_cols, brush_base, button, card_border,
    combo_box, gl_auto, gl_px, gl_star, grid_rows, info_bar, place_in_grid, scroll_viewer,
    show_info, stack_h, stack_v, text_block, uniform, writeable_bitmap_from_bgra,
};
use winui3::interop::{pick_open_files, pick_save_file, window_hwnd};
use winui3::window::FluentWindow;

use crate::logic::{BusyGuard, forensics_preview_bgra8};

const IDLE_HINT: &str = "请选择疑似泄密的截图或翻拍照片。超大照片会自动缩小到 5000 万像素以内再处理；\
                         界面预览最长边为 3000 像素，保存的取证图为完整分辨率。";
const NO_MATRIX_TEXT: &str = "微点矩阵：未在图片中找到。微点只能从原始比例的截图中解码；\
                              翻拍照片或缩放过的图片请查看右侧增强结果中的隐形水印文字（IP、时间、设备码）。";

#[derive(Default)]
struct ForensicsState {
    source: Option<Arc<ForensicsImage>>,
    enhanced: Option<Arc<ForensicsImage>>,
}

#[derive(Clone)]
struct ForensicsUi {
    win: FluentWindow,
    dq: DispatcherQueue,
    state: Arc<Mutex<ForensicsState>>,
    busy: Arc<AtomicBool>,
    mode_combo: ComboBox,
    load_btn: Button,
    save_btn: Button,
    meta_tb: TextBlock,
    trace_tb: TextBlock,
    orig_img: Image,
    enh_img: Image,
    alert: InfoBar,
}

impl ForensicsUi {
    fn state(&self) -> MutexGuard<'_, ForensicsState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_meta(&self, text: &str) {
        let _ = self.meta_tb.SetText(&HSTRING::from(text));
    }

    fn selected_mode(&self) -> EnhanceMode {
        match self.mode_combo.SelectedIndex().unwrap_or(0) {
            1 => EnhanceMode::AdaptiveBackgroundSubtract,
            2 => EnhanceMode::GlobalEqualizeAndInvert,
            _ => EnhanceMode::ChromaStegoExtract,
        }
    }

    /// Puts every control back into the state that matches what is currently loaded.
    /// Called at the end of every job, successful or not.
    fn restore_idle_controls(&self) {
        let (has_source, has_enhanced) = {
            let st = self.state();
            (st.source.is_some(), st.enhanced.is_some())
        };
        let _ = self.load_btn.SetIsEnabled(true);
        let _ = self.mode_combo.SetIsEnabled(true);
        let _ = self.save_btn.SetIsEnabled(has_enhanced);
        if !has_source {
            self.set_meta(IDLE_HINT);
        }
    }

    fn enter_busy_controls(&self, status: &str) {
        let _ = self.load_btn.SetIsEnabled(false);
        let _ = self.mode_combo.SetIsEnabled(false);
        let _ = self.save_btn.SetIsEnabled(false);
        self.set_meta(status);
    }
}

pub fn build_forensics_tab(win: FluentWindow, dq: DispatcherQueue) -> windows_core::Result<Grid> {
    let root = grid_rows(&[gl_auto(), gl_auto(), gl_star(1.0)])?;
    root.SetPadding(uniform(16.0))?;
    root.SetRowSpacing(12.0)?;

    let ui = create_widgets(win, dq)?;

    let toolbar = card_border(uniform(12.0))?;
    let row = stack_h(10.0)?;
    row.Children()?.Append(&ui.load_btn)?;
    row.Children()?.Append(&ui.mode_combo)?;
    row.Children()?.Append(&ui.save_btn)?;
    toolbar.SetChild(&row)?;

    let status_strip = stack_v(6.0)?;
    status_strip.Children()?.Append(&ui.meta_tb)?;
    status_strip.Children()?.Append(&ui.trace_tb)?;
    status_strip.Children()?.Append(&ui.alert)?;

    let viewer_grid = grid_rows(&[gl_star(1.0)])?;
    add_grid_cols(&viewer_grid, &[gl_star(1.0), gl_px(12.0), gl_star(1.0)])?;

    let left_card = build_image_pane("原始图片（已按拍摄方向转正）", &ui.orig_img)?;
    let right_card = build_image_pane("增强结果（可辨认隐藏的 IP、时间、设备码）", &ui.enh_img)?;
    place_in_grid(&left_card, 0, 0, 1, 1)?;
    place_in_grid(&right_card, 0, 2, 1, 1)?;
    viewer_grid.Children()?.Append(&left_card)?;
    viewer_grid.Children()?.Append(&right_card)?;

    place_in_grid(&toolbar, 0, 0, 1, 1)?;
    place_in_grid(&status_strip, 1, 0, 1, 1)?;
    place_in_grid(&viewer_grid, 2, 0, 1, 1)?;
    root.Children()?.Append(&toolbar)?;
    root.Children()?.Append(&status_strip)?;
    root.Children()?.Append(&viewer_grid)?;

    wire_actions(&ui)?;
    Ok(root)
}

fn create_widgets(win: FluentWindow, dq: DispatcherQueue) -> windows_core::Result<ForensicsUi> {
    let mode_combo = combo_box(
        &[
            "色度隐形水印提取（适合手机翻拍）",
            "自适应背景消除与对比增强",
            "全局直方图均衡与反相",
        ],
        0,
    )?;
    mode_combo.SetMinWidth(300.0)?;

    let save_btn = button("保存增强取证图...", false)?;
    save_btn.SetIsEnabled(false)?;

    let orig_img = Image::new()?;
    orig_img.SetStretch(Stretch::Uniform)?;
    let enh_img = Image::new()?;
    enh_img.SetStretch(Stretch::Uniform)?;

    let meta_tb = text_block(IDLE_HINT, 12.5, false, TEXT_SECONDARY)?;
    meta_tb.SetTextWrapping(TextWrapping::Wrap)?;
    let trace_tb = text_block("", 13.0, true, TEXT_PRIMARY)?;
    trace_tb.SetTextWrapping(TextWrapping::Wrap)?;
    trace_tb.SetIsTextSelectionEnabled(true)?;

    Ok(ForensicsUi {
        win,
        dq,
        state: Arc::new(Mutex::new(ForensicsState::default())),
        busy: Arc::new(AtomicBool::new(false)),
        mode_combo,
        load_btn: button("选择疑似泄密的截图或照片...", true)?,
        save_btn,
        meta_tb,
        trace_tb,
        orig_img,
        enh_img,
        alert: info_bar()?,
    })
}

fn build_image_pane(
    heading: &str,
    img_ctrl: &Image,
) -> windows_core::Result<winui3::bindings::Microsoft::UI::Xaml::Controls::Border> {
    let card = card_border(uniform(12.0))?;
    let grid = grid_rows(&[gl_auto(), gl_star(1.0)])?;
    grid.SetRowSpacing(8.0)?;
    let hdr = text_block(heading, 14.0, true, TEXT_PRIMARY)?;
    let sv: ScrollViewer = scroll_viewer(true)?;
    sv.SetBackground(&brush_base(BG_SURFACE)?)?;
    sv.SetContent(img_ctrl)?;
    place_in_grid(&hdr, 0, 0, 1, 1)?;
    place_in_grid(&sv, 1, 0, 1, 1)?;
    grid.Children()?.Append(&hdr)?;
    grid.Children()?.Append(&sv)?;
    card.SetChild(&grid)?;
    Ok(card)
}

fn wire_actions(ui: &ForensicsUi) -> windows_core::Result<()> {
    let ui_l = ui.clone();
    ui.load_btn
        .Click(move |_, _| handle_load_image(&ui_l))?
        .forget();

    // Switching the algorithm re-runs the enhancement on the loaded image automatically.
    let ui_m = ui.clone();
    ui.mode_combo
        .SelectionChanged(move |_, _| {
            let has_source = ui_m.state().source.is_some();
            if has_source {
                start_job(&ui_m, None);
            }
        })?
        .forget();

    let ui_s = ui.clone();
    ui.save_btn
        .Click(move |_, _| handle_save_png(&ui_s))?
        .forget();

    Ok(())
}

fn handle_load_image(ui: &ForensicsUi) {
    // The button is disabled while a job runs, so the picker only opens when idle; a selection
    // can therefore never be silently dropped because of a running job.
    if ui.busy.load(Ordering::Acquire) {
        return;
    }
    let owner = window_hwnd(&ui.win.window).ok();
    let filters = [(
        "图片文件 (*.png;*.jpg;*.jpeg;*.heic;*.heif;*.bmp;*.tif;*.tiff)",
        "*.png;*.jpg;*.jpeg;*.heic;*.heif;*.bmp;*.tif;*.tiff",
    )];
    let paths = match pick_open_files(owner, "选择待取证分析的截图或照片", &filters, false)
    {
        Ok(paths) => paths,
        Err(err) => {
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "无法打开文件选择框",
                &format!("错误代码 0x{:08X}", err.code().0),
            );
            return;
        }
    };
    if let Some(path) = paths.into_iter().next() {
        start_job(ui, Some(path));
    }
}

/// Bitmap data prepared on the worker thread for one image pane.
struct Preview {
    image: Arc<ForensicsImage>,
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

impl Preview {
    fn of(image: Arc<ForensicsImage>) -> Self {
        let (width, height, bgra) = forensics_preview_bgra8(&image);
        Self {
            image,
            width,
            height,
            bgra,
        }
    }

    fn show_in(&self, target: &Image) {
        let wi = i32::try_from(self.width).unwrap_or(1);
        let hi = i32::try_from(self.height).unwrap_or(1);
        if let Ok(bmp) = writeable_bitmap_from_bgra(wi, hi, &self.bgra) {
            let _ = target.SetSource(&bmp);
        }
    }
}

/// Result of one background job.
struct JobOutput {
    /// Newly loaded source with its microdot decode (`None` when only the mode changed).
    loaded: Option<(Preview, Option<MicrodotDecode>)>,
    /// Enhancement result, or why it failed.
    enhanced: Result<Preview, String>,
    mode: EnhanceMode,
}

fn run_job(
    new_path: Option<&PathBuf>,
    current: Option<Arc<ForensicsImage>>,
    mode: EnhanceMode,
) -> Result<JobOutput, String> {
    let (source, loaded) = match new_path {
        Some(path) => {
            let image = Arc::new(load_image_upright(path).map_err(|e| e.to_string())?);
            let decode = decode_microdots(&image);
            (image.clone(), Some((Preview::of(image), decode)))
        }
        None => (current.ok_or_else(|| "尚未加载图片。".to_owned())?, None),
    };
    let enhanced = enhance_image(&source, mode)
        .map(|img| Preview::of(Arc::new(img)))
        .map_err(|e| e.to_string());
    Ok(JobOutput {
        loaded,
        enhanced,
        mode,
    })
}

/// Loads `new_path` (if given) and enhances the current image on a worker thread.
///
/// A failed load keeps the previous image and result; a failed enhancement clears the stale
/// result. Controls are restored on every outcome, including a worker panic.
fn start_job(ui: &ForensicsUi, new_path: Option<PathBuf>) {
    let Some(busy) = BusyGuard::try_acquire(&ui.busy) else {
        return;
    };
    let mode = ui.selected_mode();
    let current = ui.state().source.clone();
    ui.enter_busy_controls(if new_path.is_some() {
        "正在读取并增强图片..."
    } else {
        "正在重新增强图片..."
    });

    let ui_bg = ui.clone();
    std::thread::spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            run_job(new_path.as_ref(), current, mode)
        }));
        drop(busy);
        let ui_done = ui_bg.clone();
        let _ = enqueue_ui(&ui_bg.dq, move || {
            let result =
                outcome.unwrap_or_else(|_| Err("处理图片时发生内部错误，请重试。".to_owned()));
            apply_job_result(&ui_done, result);
        });
    });
}

fn describe_trace(decode: Option<&MicrodotDecode>) -> String {
    decode.map_or_else(
        || NO_MATRIX_TEXT.to_owned(),
        |found| {
            format!(
                "微点矩阵解码成功：显示设备 IP {}，画面时间 {}。",
                found.payload.ipv4,
                format_unix_seconds_local(found.payload.unix_seconds)
            )
        },
    )
}

fn apply_job_result(ui: &ForensicsUi, result: Result<JobOutput, String>) {
    let finished_mode = result.as_ref().ok().map(|job| job.mode);
    match result {
        Err(reason) => {
            // Nothing was replaced: the previous image and its result (if any) stay valid.
            show_info(&ui.alert, InfoBarSeverity::Error, "无法处理该图片", &reason);
        }
        Ok(job) => show_job_output(ui, job),
    }
    ui.restore_idle_controls();

    // If the selected algorithm differs from the one just computed (a change that arrived
    // while the job was running), recompute instead of leaving a mismatched result on screen.
    let has_source = ui.state().source.is_some();
    if has_source && finished_mode.is_some_and(|mode| mode != ui.selected_mode()) {
        start_job(ui, None);
    }
}

fn show_job_output(ui: &ForensicsUi, job: JobOutput) {
    if let Some((preview, decode)) = &job.loaded {
        {
            let mut st = ui.state();
            st.source = Some(preview.image.clone());
            // The previous enhancement belongs to the previous picture.
            st.enhanced = None;
        }
        preview.show_in(&ui.orig_img);
        let _ = ui.enh_img.SetSource(None::<&ImageSource>);
        let _ = ui
            .trace_tb
            .SetText(&HSTRING::from(describe_trace(decode.as_ref())));
    }
    let (src_w, src_h) = ui
        .state()
        .source
        .as_ref()
        .map_or((0, 0), |img| (img.width, img.height));
    match job.enhanced {
        Ok(preview) => {
            ui.state().enhanced = Some(preview.image.clone());
            preview.show_in(&ui.enh_img);
            ui.set_meta(&format!(
                "完成：图片 {src_w} × {src_h} 像素。切换增强算法会自动重新计算。"
            ));
            if let Some((_, decode)) = &job.loaded {
                let decoded = decode.is_some();
                show_info(
                    &ui.alert,
                    if decoded {
                        InfoBarSeverity::Success
                    } else {
                        InfoBarSeverity::Informational
                    },
                    "图片已增强",
                    if decoded {
                        "已从微点矩阵还原出 IP 与时间，详见上方。"
                    } else {
                        "未解码出微点矩阵，请在右侧增强结果中辨认隐形水印文字。"
                    },
                );
            }
        }
        Err(reason) => {
            ui.state().enhanced = None;
            let _ = ui.enh_img.SetSource(None::<&ImageSource>);
            ui.set_meta(&format!(
                "图片 {src_w} × {src_h} 像素已加载，但增强失败。可切换增强算法重试。"
            ));
            show_info(&ui.alert, InfoBarSeverity::Error, "增强失败", &reason);
        }
    }
}

fn handle_save_png(ui: &ForensicsUi) {
    let Some(img) = ui.state().enhanced.clone() else {
        show_info(
            &ui.alert,
            InfoBarSeverity::Informational,
            "尚无增强结果",
            "请先加载并增强图片。",
        );
        return;
    };
    let Some(busy) = BusyGuard::try_acquire(&ui.busy) else {
        return;
    };

    let now = LocalDateTime::now();
    let default_name = format!(
        "水印取证_{:04}{:02}{:02}_{:02}{:02}{:02}.png",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );
    let owner = window_hwnd(&ui.win.window).ok();
    let dst = match pick_save_file(
        owner,
        "保存增强取证图",
        &default_name,
        "png",
        &[("PNG 图片 (*.png)", "*.png")],
    ) {
        Ok(Some(dst)) => dst,
        Ok(None) => return,
        Err(err) => {
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "无法打开保存对话框",
                &format!("错误代码 0x{:08X}", err.code().0),
            );
            return;
        }
    };

    ui.enter_busy_controls("正在保存取证图...");
    let ui_bg = ui.clone();
    std::thread::spawn(move || {
        let res = catch_unwind(AssertUnwindSafe(|| {
            save_png(&dst, &img).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|_| Err("保存时发生内部错误。".to_owned()));
        drop(busy);
        let ui_done = ui_bg.clone();
        let _ = enqueue_ui(&ui_bg.dq, move || {
            match res {
                Ok(()) => {
                    ui_done.set_meta(&format!("已保存：{}", dst.display()));
                    show_info(
                        &ui_done.alert,
                        InfoBarSeverity::Success,
                        "保存成功",
                        &dst.to_string_lossy(),
                    );
                }
                Err(reason) => {
                    ui_done.set_meta("保存失败。");
                    show_info(
                        &ui_done.alert,
                        InfoBarSeverity::Error,
                        "保存失败",
                        &format!("目标文件可能被占用或没有写入权限。{reason}"),
                    );
                }
            }
            ui_done.restore_idle_controls();
        });
    });
}
