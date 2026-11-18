//! Protected high-DPI `WinUI 3` PDF viewport with capture protection, curtain, overlay, and housekeeping.
//!
//! All `WinUI` objects live in a UI-thread-local [`ReaderUi`]; event handlers and background
//! results reach it through [`with_reader`], so no UI object is ever moved to another thread and
//! closing the window drops the only owner of the session.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use secure_pdf_core::MoireLevel;
use secure_pdf_core::format::{SecurityFlags, utc_now_dotnet_ticks};
use secure_pdf_core::licensing::enumerate_ipv4_addresses;
use secure_pdf_core::localtime::{LocalDateTime, format_dotnet_ticks_local, unix_seconds_now};
use secure_pdf_core::storage::LocalQuotaStore;
use windows_core::{HSTRING, Interface};
use winui3::app::{enqueue_ui, schedule_once_timer, start_repeating_timer};
use winui3::bindings::Microsoft::UI::Dispatching::{DispatcherQueue, DispatcherQueueTimer};
use winui3::bindings::Microsoft::UI::Xaml::Controls::{
    Border, Button, ComboBox, Grid, Image, InfoBar, InfoBarSeverity, PasswordBox, ScrollViewer,
    TextBlock, TextBox,
};
use winui3::bindings::Microsoft::UI::Xaml::Input::{
    FocusManager, KeyRoutedEventArgs, PointerRoutedEventArgs,
};
use winui3::bindings::Microsoft::UI::Xaml::Media::Imaging::WriteableBitmap;
use winui3::bindings::Microsoft::UI::Xaml::Media::{ImageSource, Stretch};
use winui3::bindings::Microsoft::UI::Xaml::{
    FrameworkElement, HorizontalAlignment, UIElement, VerticalAlignment, Visibility,
    WindowActivationState,
};
use winui3::bootstrap::exit_ui_thread;
use winui3::controls::{
    ACCENT_AMBER, ACCENT_GREEN, ACCENT_RED, BG_CANVAS, BG_ELEVATED, BG_SURFACE, TEXT_PRIMARY,
    TEXT_SECONDARY, add_grid_cols, argb, brush_base, button, card_border, combo_box, gl_auto,
    gl_star, grid_rows, hv, info_bar, pad, place_in_grid, scroll_viewer, show_info, stack_h,
    stack_v, text_block, text_box, uniform, write_bgra_into_bitmap, writeable_bitmap_from_bgra,
};
use winui3::interop::{is_ctrl_pressed, message_box, window_hwnd};
use winui3::window::{FluentWindow, WindowChromeOptions};

use crate::logic::{
    HousekeepingVerdict, MOIRE_PHASE_STEPS, OverlaySpec, ProtectionState, ShortcutAction,
    WHEEL_PAGING_PAUSE_MS, WheelAction, ZOOM_STEP, centered_offset, clamp_zoom, classify_shortcut,
    evaluate_housekeeping, millis_until_expiry, print_button_text, protection_status_text,
    quota_status_text, resolve_page_input, trace_status_text, viewport_center_ratio, vk,
    wheel_paging_action,
};
use crate::print_dialog::open_print_flow;
use crate::protection::{
    WindowOpenError, activate_protected_window, create_protected_window, reverify_protection,
};
use crate::render_worker::{OverlayJob, OverlayResult, PageJob, PageResult, RenderWorker};
use crate::session::{ReaderSession, register_active_session, release_active_session};

/// Shown when the trace watermark cannot be drawn: the document is not displayed without it.
const WATERMARK_FAILURE_REASON: &str =
    "追溯水印无法显示，为避免文档在没有水印的情况下展示，已锁定文档。";

/// Longest delay the precise expiry timer is armed for; later expiries rely on housekeeping.
const MAX_EXPIRY_TIMER_MS: u64 = 7 * 24 * 3600 * 1000;

thread_local! {
    static READER_UI: RefCell<Option<Rc<ReaderUi>>> = const { RefCell::new(None) };
}

/// Runs `f` with the live reader window state on the UI thread; `None` once it is closed.
pub fn with_reader<R>(f: impl FnOnce(&ReaderUi) -> R) -> Option<R> {
    let ui = READER_UI.with(|slot| slot.borrow().clone())?;
    Some(f(&ui))
}

/// Where the viewport should end up after the next page render.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollIntent {
    /// Start of the page.
    Top,
    /// End of the page (after paging backwards with the wheel).
    Bottom,
    /// Keep the same relative spot in the middle of the viewport (zoom / DPI change).
    KeepCenter {
        /// Horizontal position of the viewport centre as a fraction of the extent.
        ratio_x: f64,
        /// Vertical position of the viewport centre as a fraction of the extent.
        ratio_y: f64,
    },
}

/// Page bitmap currently on screen.
#[derive(Clone, Copy, Debug)]
struct ShownFrame {
    px_width: i32,
    px_height: i32,
    /// Bitmap pixels per device-independent pixel.
    effective_scale: f64,
}

/// Where the print workflow currently is; anything but `Idle` refuses a second start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrintPhase {
    /// No print dialog open and no job running.
    Idle,
    /// Authorisation is being checked or the print dialog is open.
    Preparing,
    /// A print job is running; the window must not be closed until it reports back.
    Printing,
}

/// Mutable view state; only ever touched on the UI thread.
pub struct ViewState {
    pub(crate) page_idx: i32,
    zoom: f64,
    moire: MoireLevel,
    moire_phase: u32,
    generation: u64,
    scroll_intent: ScrollIntent,
    shown: Option<ShownFrame>,
    page_bitmap: Option<(WriteableBitmap, i32, i32)>,
    overlay_bitmap: Option<(WriteableBitmap, i32, i32)>,
    dpi_scale: f64,
    pub(crate) locked: bool,
    lock_reason: String,
    opened: bool,
    curtain_shown: bool,
    housekeeping_ticks: u64,
    last_wheel: Option<Instant>,
    /// Print workflow phase shared by `Ctrl+P` and the print button.
    pub(crate) print_phase: PrintPhase,
    /// Printer chosen last time, preselected when the dialog opens again.
    pub(crate) last_printer: Option<String>,
}

/// Reader window, its controls and per-window services.
pub struct ReaderUi {
    pub(crate) win: FluentWindow,
    pub(crate) dq: DispatcherQueue,
    pub(crate) session: Arc<ReaderSession>,
    pub(crate) state: RefCell<ViewState>,
    worker: RenderWorker,
    timers: RefCell<Vec<DispatcherQueueTimer>>,
    prev_btn: Button,
    next_btn: Button,
    page_box: TextBox,
    zoom_out_btn: Button,
    zoom_tb: TextBlock,
    zoom_in_btn: Button,
    zoom_reset_btn: Button,
    moire_combo: Option<ComboBox>,
    pub(crate) print_btn: Button,
    print_label: TextBlock,
    protect_tb: TextBlock,
    quota_tb: TextBlock,
    trace_tb: TextBlock,
    pub(crate) status_tb: TextBlock,
    pub(crate) alert: InfoBar,
    scroller: ScrollViewer,
    stage_host: Grid,
    page_img: Image,
    overlay_img: Image,
    curtain: Border,
    curtain_title: TextBlock,
    curtain_msg: TextBlock,
}

/// Opens and activates the capture-protected `.spdf` Reader window.
///
/// On any failure the half-built window is closed and the decrypted document is released.
///
/// # Errors
///
/// [`WindowOpenError::CaptureProtection`] when Windows refuses capture protection,
/// [`WindowOpenError::Ui`] for any other window construction failure.
pub fn open_reader_window(session: &Arc<ReaderSession>) -> Result<(), WindowOpenError> {
    let title = if session.policy.original_file_name.trim().is_empty() {
        "受保护文档 — 安全阅读器".to_owned()
    } else {
        format!("{} — 安全阅读器", session.policy.original_file_name)
    };
    let opened = create_protected_window(&WindowChromeOptions {
        title,
        width_dip: 1180,
        height_dip: 860,
        resizable: true,
        maximizable: true,
        minimizable: true,
        always_on_top: false,
        capture_protected: true,
        use_mica: true,
    })
    .and_then(|win| match build_and_show(&win, session) {
        Ok(()) => Ok(()),
        Err(err) => {
            // The window object exists as soon as it is created; it must be closed explicitly
            // or it would linger without content.
            let _ = teardown_reader();
            let _ = win.window.Close();
            Err(err)
        }
    });
    if opened.is_err() {
        release_active_session();
        session.release_plaintext();
    }
    opened
}

fn build_and_show(win: &FluentWindow, session: &Arc<ReaderSession>) -> Result<(), WindowOpenError> {
    let dq = win.dispatcher_queue()?;
    let ui = Rc::new(build_ui_controls(win.clone(), dq, Arc::clone(session))?);
    let root = build_layout(&ui)?;

    register_active_session(session);
    READER_UI.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&ui)));

    wire_toolbar_actions(&ui)?;
    wire_window_events(&ui, &root)?;
    wire_dpi_change_events(&root)?;
    win.set_content(&root.cast::<FrameworkElement>()?)?;

    let protection = activate_protected_window(win)?;
    let _ = ui
        .protect_tb
        .SetText(&HSTRING::from(protection_status_text(protection)));
    if protection == ProtectionState::DebugDisabled {
        let _ = ui.protect_tb.SetForeground(&brush_base(ACCENT_RED)?);
    }

    start_timers(&ui)?;
    update_quota_display(&ui);
    update_trace_status(&ui);
    ui.state.borrow_mut().opened = true;
    trigger_render(&ui, ScrollIntent::Top);
    Ok(())
}

fn build_ui_controls(
    win: FluentWindow,
    dq: DispatcherQueue,
    session: Arc<ReaderSession>,
) -> windows_core::Result<ReaderUi> {
    let initial_moire = session.policy.moire_level;
    let allow_moire = session
        .policy
        .flags
        .contains(SecurityFlags::ALLOW_USER_ADJUST_MOIRE);
    let moire_combo = if allow_moire {
        let selected = match initial_moire {
            MoireLevel::Off => 0,
            MoireLevel::Low => 1,
            MoireLevel::Medium => 2,
            MoireLevel::High => 3,
        };
        let combo = combo_box(
            &["摩尔纹：关闭", "摩尔纹：低", "摩尔纹：中", "摩尔纹：高"],
            selected,
        )?;
        combo.SetMinWidth(116.0)?;
        Some(combo)
    } else {
        None
    };

    let print_btn = Button::new()?;
    let print_label = text_block("安全打印", 13.0, false, TEXT_PRIMARY)?;
    print_btn.SetContent(&print_label)?;
    print_btn.SetPadding(hv(12.0, 6.0))?;
    print_btn.SetBackground(&brush_base(BG_ELEVATED)?)?;

    let page_box = text_box("", false)?;
    page_box.SetText(&HSTRING::from("1"))?;
    page_box.SetWidth(64.0)?;

    let scroller = scroll_viewer(true)?;
    scroller.SetBackground(&brush_base(argb(255, 18, 18, 18))?)?;
    // Fills the whole viewport and has a background, so wheel input is received everywhere,
    // not only on top of the page itself.
    let stage_host = Grid::new()?;
    stage_host.SetBackground(&brush_base(argb(255, 18, 18, 18))?)?;

    let page_img = Image::new()?;
    page_img.SetStretch(Stretch::Fill)?;
    let overlay_img = Image::new()?;
    overlay_img.SetStretch(Stretch::Fill)?;
    overlay_img.SetIsHitTestVisible(false)?;

    // Fully opaque: nothing of the page may shine through while the curtain is up.
    let curtain = Border::new()?;
    curtain.SetBackground(&brush_base(argb(255, 16, 16, 16))?)?;
    curtain.SetVisibility(Visibility::Collapsed)?;
    let curtain_title = text_block("内容已临时遮蔽", 20.0, true, ACCENT_RED)?;
    curtain_title.SetHorizontalAlignment(HorizontalAlignment::Center)?;
    let curtain_msg = text_block(
        "阅读窗口已失去焦点，为防止旁观或后台窃取，内容已遮蔽。点击本窗口即可恢复显示。",
        14.0,
        false,
        TEXT_SECONDARY,
    )?;
    curtain_msg.SetHorizontalAlignment(HorizontalAlignment::Center)?;

    let worker = RenderWorker::spawn(Arc::clone(&session), dq.clone());
    let dpi_scale = win.dpi_scale();
    let total_pages = session.page_count;
    Ok(ReaderUi {
        win,
        dq,
        session,
        state: RefCell::new(ViewState {
            page_idx: 0,
            zoom: 1.0,
            moire: initial_moire,
            moire_phase: 0,
            generation: 0,
            scroll_intent: ScrollIntent::Top,
            shown: None,
            page_bitmap: None,
            overlay_bitmap: None,
            dpi_scale,
            locked: false,
            lock_reason: String::new(),
            opened: false,
            curtain_shown: false,
            housekeeping_ticks: 0,
            last_wheel: None,
            print_phase: PrintPhase::Idle,
            last_printer: None,
        }),
        worker,
        timers: RefCell::new(Vec::new()),
        prev_btn: button("上一页", false)?,
        next_btn: button("下一页", false)?,
        page_box,
        zoom_out_btn: button("-", false)?,
        zoom_tb: text_block("100%", 13.0, true, TEXT_PRIMARY)?,
        zoom_in_btn: button("+", false)?,
        zoom_reset_btn: button("100%", false)?,
        moire_combo,
        print_btn,
        print_label,
        protect_tb: text_block("", 12.0, false, ACCENT_GREEN)?,
        quota_tb: text_block("剩余打印次数: --", 12.0, false, TEXT_PRIMARY)?,
        trace_tb: text_block("", 11.5, false, TEXT_SECONDARY)?,
        status_tb: text_block(&format!("共 {total_pages} 页"), 12.0, false, TEXT_SECONDARY)?,
        alert: info_bar()?,
        scroller,
        stage_host,
        page_img,
        overlay_img,
        curtain,
        curtain_title,
        curtain_msg,
    })
}

fn build_layout(ui: &ReaderUi) -> windows_core::Result<Grid> {
    let root = grid_rows(&[gl_auto(), gl_auto(), gl_star(1.0), gl_auto()])?;
    root.SetBackground(&brush_base(BG_CANVAS)?)?;

    let toolbar = build_toolbar(ui)?;
    let viewport = build_viewport(ui)?;
    let status_bar = build_status_bar(ui)?;

    place_in_grid(&toolbar, 0, 0, 1, 1)?;
    place_in_grid(&ui.alert, 1, 0, 1, 1)?;
    place_in_grid(&viewport, 2, 0, 1, 1)?;
    place_in_grid(&status_bar, 3, 0, 1, 1)?;
    root.Children()?.Append(&toolbar)?;
    root.Children()?.Append(&ui.alert)?;
    root.Children()?.Append(&viewport)?;
    root.Children()?.Append(&status_bar)?;
    Ok(root)
}

fn build_toolbar(ui: &ReaderUi) -> windows_core::Result<Border> {
    let card = card_border(hv(14.0, 10.0))?;
    card.SetMargin(pad(12.0, 8.0, 12.0, 6.0))?;
    let row = stack_h(8.0)?;
    row.Children()?.Append(&ui.prev_btn)?;
    row.Children()?.Append(&ui.page_box)?;
    row.Children()?.Append(&text_block(
        &format!("/ {}", ui.session.page_count),
        13.5,
        false,
        TEXT_SECONDARY,
    )?)?;
    row.Children()?.Append(&ui.next_btn)?;
    row.Children()?.Append(&ui.zoom_out_btn)?;
    row.Children()?.Append(&ui.zoom_tb)?;
    row.Children()?.Append(&ui.zoom_in_btn)?;
    row.Children()?.Append(&ui.zoom_reset_btn)?;
    if let Some(combo) = &ui.moire_combo {
        row.Children()?.Append(combo)?;
    }
    row.Children()?.Append(&ui.print_btn)?;
    row.Children()?.Append(&ui.protect_tb)?;
    card.SetChild(&row)?;
    Ok(card)
}

fn build_viewport(ui: &ReaderUi) -> windows_core::Result<Grid> {
    let container = grid_rows(&[gl_star(1.0)])?;
    let stage_card = Border::new()?;
    stage_card.SetHorizontalAlignment(HorizontalAlignment::Center)?;
    stage_card.SetVerticalAlignment(VerticalAlignment::Center)?;
    stage_card.SetMargin(uniform(20.0))?;
    stage_card.SetBackground(&brush_base(BG_SURFACE)?)?;

    let stage = Grid::new()?;
    stage.Children()?.Append(&ui.page_img)?;
    stage.Children()?.Append(&ui.overlay_img)?;
    stage_card.SetChild(&stage)?;
    ui.stage_host.Children()?.Append(&stage_card)?;
    ui.scroller.SetContent(&ui.stage_host)?;

    let curtain_body = stack_v(10.0)?;
    curtain_body.SetHorizontalAlignment(HorizontalAlignment::Center)?;
    curtain_body.SetVerticalAlignment(VerticalAlignment::Center)?;
    curtain_body.Children()?.Append(&ui.curtain_title)?;
    curtain_body.Children()?.Append(&ui.curtain_msg)?;
    ui.curtain.SetChild(&curtain_body)?;

    place_in_grid(&ui.scroller, 0, 0, 1, 1)?;
    place_in_grid(&ui.curtain, 0, 0, 1, 1)?;
    container.Children()?.Append(&ui.scroller)?;
    container.Children()?.Append(&ui.curtain)?;
    Ok(container)
}

fn build_status_bar(ui: &ReaderUi) -> windows_core::Result<Border> {
    let bar = card_border(hv(14.0, 6.0))?;
    bar.SetMargin(pad(12.0, 6.0, 12.0, 8.0))?;
    let row = grid_rows(&[gl_auto()])?;
    add_grid_cols(&row, &[gl_auto(), gl_star(1.0), gl_auto(), gl_auto()])?;

    let middle = stack_h(14.0)?;
    middle.SetMargin(pad(16.0, 0.0, 16.0, 0.0))?;
    if ui.session.store_repaired {
        middle.Children()?.Append(&text_block(
            "提示：本机授权记录曾缺失或不一致，已按较严格的一份自动修复",
            11.5,
            false,
            ACCENT_AMBER,
        )?)?;
    }
    middle.Children()?.Append(&ui.status_tb)?;

    place_in_grid(&ui.quota_tb, 0, 0, 1, 1)?;
    place_in_grid(&middle, 0, 1, 1, 1)?;
    place_in_grid(&ui.trace_tb, 0, 3, 1, 1)?;
    row.Children()?.Append(&ui.quota_tb)?;
    row.Children()?.Append(&middle)?;
    row.Children()?.Append(&ui.trace_tb)?;
    bar.SetChild(&row)?;
    Ok(bar)
}

fn wire_toolbar_actions(ui: &ReaderUi) -> windows_core::Result<()> {
    ui.prev_btn
        .Click(|_, _| {
            with_reader(|ui| step_page(ui, -1));
        })?
        .forget();
    ui.next_btn
        .Click(|_, _| {
            with_reader(|ui| step_page(ui, 1));
        })?
        .forget();
    ui.zoom_in_btn
        .Click(|_, _| {
            with_reader(|ui| adjust_zoom(ui, ZOOM_STEP));
        })?
        .forget();
    ui.zoom_out_btn
        .Click(|_, _| {
            with_reader(|ui| adjust_zoom(ui, -ZOOM_STEP));
        })?
        .forget();
    ui.zoom_reset_btn
        .Click(|_, _| {
            with_reader(|ui| set_zoom(ui, 1.0));
        })?
        .forget();
    ui.print_btn
        .Click(|_, _| {
            with_reader(open_print_flow);
        })?
        .forget();

    if let Some(combo) = &ui.moire_combo {
        combo
            .SelectionChanged(|_, _| {
                with_reader(on_moire_selected);
            })?
            .forget();
    }

    let page_box_ui: UIElement = ui.page_box.cast()?;
    page_box_ui
        .KeyDown(|_, args| {
            if let Some(ev) = args.as_ref()
                && ev.Key().is_ok_and(|k| k.0 == vk::ENTER)
            {
                let _ = ev.SetHandled(true);
                with_reader(commit_page_input);
            }
        })?
        .forget();
    page_box_ui
        .LostFocus(|_, _| {
            with_reader(sync_page_box);
        })?
        .forget();
    Ok(())
}

fn wire_window_events(ui: &ReaderUi, root: &Grid) -> windows_core::Result<()> {
    ui.win
        .window
        .Activated(|_, args| {
            if let Some(ev) = args.as_ref() {
                let deactivated = ev
                    .WindowActivationState()
                    .is_ok_and(|s| s == WindowActivationState::Deactivated);
                with_reader(|ui| on_activation_changed(ui, deactivated));
            }
        })?
        .forget();

    // A running print job still needs the document and must hand its result (and the deducted
    // count) back to this window, so closing is refused until it reports back.
    ui.win
        .app_window
        .Closing(|_, args| {
            if let Some(ev) = args.as_ref()
                && with_reader(|ui| ui.state.borrow().print_phase == PrintPhase::Printing)
                    .unwrap_or(false)
            {
                let _ = ev.SetCancel(true);
                with_reader(|ui| {
                    show_info(
                        &ui.alert,
                        InfoBarSeverity::Warning,
                        "正在打印",
                        "打印任务尚未结束，请等待打印完成后再关闭窗口。",
                    );
                });
            }
        })?
        .forget();

    ui.win
        .window
        .Closed(|_, _| {
            if teardown_reader() == Some(true) {
                exit_ui_thread();
            }
        })?
        .forget();

    let root_ui: UIElement = root.cast()?;
    root_ui
        .KeyDown(|_, args| {
            if let Some(ev) = args.as_ref() {
                with_reader(|ui| on_key_down(ui, ev));
            }
        })?
        .forget();

    // The wheel event bubbles from the page up to the `ScrollViewer`, which scrolls when it
    // receives it unhandled. Listening on the scroller's content therefore sees every notch
    // first and can claim it (zoom, page turn) or let it through as ordinary scrolling.
    let stage_ui: UIElement = ui.stage_host.cast()?;
    stage_ui
        .PointerWheelChanged(|_, args| {
            if let Some(ev) = args.as_ref() {
                with_reader(|ui| on_wheel(ui, ev));
            }
        })?
        .forget();
    Ok(())
}

/// Re-renders at the new pixel density when the window moves to a display with another scale.
///
/// The `XamlRoot` only exists once the content is loaded, so the hook is installed from the
/// root element's `Loaded` event; the one-second overlay tick re-checks the scale as well.
fn wire_dpi_change_events(root: &Grid) -> windows_core::Result<()> {
    let root_fe: FrameworkElement = root.cast()?;
    root_fe
        .Loaded(|_, _| {
            with_reader(|ui| {
                if let Ok(xaml_root) = ui.win.xaml_root()
                    && let Ok(revoker) = xaml_root.Changed(|_, _| {
                        with_reader(rerender_if_dpi_changed);
                    })
                {
                    revoker.forget();
                }
            });
        })?
        .forget();
    Ok(())
}

fn rerender_if_dpi_changed(ui: &ReaderUi) {
    let new_scale = ui.win.dpi_scale();
    let changed = (ui.state.borrow().dpi_scale - new_scale).abs() > 0.001;
    if changed {
        let intent = keep_center_intent(ui);
        trigger_render(ui, intent);
    }
}

fn start_timers(ui: &ReaderUi) -> windows_core::Result<()> {
    let mut timers = Vec::new();
    timers.push(start_repeating_timer(&ui.dq, 30_000, || {
        with_reader(housekeeping_tick);
    })?);
    // Live clock in the watermarks, fresh microdot time stamp and moire phase, once a second.
    timers.push(start_repeating_timer(&ui.dq, 1_000, || {
        with_reader(overlay_tick);
    })?);
    if let Some(expires) = ui.session.policy.expires_at_utc_ticks {
        let wait_ms = millis_until_expiry(utc_now_dotnet_ticks(), expires);
        if wait_ms <= MAX_EXPIRY_TIMER_MS {
            // Lock right at the expiry instant instead of up to 30 seconds later.
            timers.push(schedule_once_timer(&ui.dq, wait_ms + 200, || {
                with_reader(check_expiry_now);
            })?);
        }
    }
    *ui.timers.borrow_mut() = timers;
    Ok(())
}

fn stop_timers(ui: &ReaderUi) {
    let timers = std::mem::take(&mut *ui.timers.borrow_mut());
    for timer in timers {
        let _ = timer.Stop();
    }
}

// ------------------------------------------------------------------ input handling

fn on_moire_selected(ui: &ReaderUi) {
    let Some(combo) = &ui.moire_combo else {
        return;
    };
    let level = match combo.SelectedIndex().unwrap_or(2) {
        0 => MoireLevel::Off,
        1 => MoireLevel::Low,
        3 => MoireLevel::High,
        _ => MoireLevel::Medium,
    };
    ui.state.borrow_mut().moire = level;
    request_overlay(ui);
}

fn on_key_down(ui: &ReaderUi, ev: &KeyRoutedEventArgs) {
    let vkey = ev.Key().map_or(0, |k| k.0);
    let in_text = ui
        .win
        .xaml_root()
        .ok()
        .and_then(|xr| FocusManager::GetFocusedElementWithRoot(&xr).ok())
        .is_some_and(|el| el.cast::<TextBox>().is_ok() || el.cast::<PasswordBox>().is_ok());

    let action = classify_shortcut(vkey, is_ctrl_pressed(), in_text);
    if action != ShortcutAction::Allow {
        let _ = ev.SetHandled(true);
    }
    match action {
        ShortcutAction::Block | ShortcutAction::Allow => {}
        ShortcutAction::Print => open_print_flow(ui),
        ShortcutAction::ZoomIn => adjust_zoom(ui, ZOOM_STEP),
        ShortcutAction::ZoomOut => adjust_zoom(ui, -ZOOM_STEP),
        ShortcutAction::ZoomReset => set_zoom(ui, 1.0),
        ShortcutAction::PrevPage => step_page(ui, -1),
        ShortcutAction::NextPage => step_page(ui, 1),
    }
}

fn on_wheel(ui: &ReaderUi, ev: &PointerRoutedEventArgs) {
    let Some(delta) = ev
        .GetCurrentPoint(None::<&UIElement>)
        .and_then(|pt| pt.Properties())
        .and_then(|props| props.MouseWheelDelta())
        .ok()
        .filter(|d| *d != 0)
    else {
        return;
    };
    if is_ctrl_pressed() {
        let _ = ev.SetHandled(true);
        adjust_zoom(ui, if delta > 0 { ZOOM_STEP } else { -ZOOM_STEP });
        return;
    }

    // Cooldown applies only right after a wheel-triggered page turn so normal scrolling into the
    // top/bottom edge turns seamlessly on the next notch without locking out trackpad gestures.
    let now = Instant::now();
    let (page_idx, paused) = {
        let state = ui.state.borrow();
        let paused = state
            .last_wheel
            .is_none_or(|last| now.duration_since(last).as_millis() >= WHEEL_PAGING_PAUSE_MS);
        (state.page_idx, paused)
    };
    let action = wheel_paging_action(
        delta,
        ui.scroller.VerticalOffset().unwrap_or(0.0),
        ui.scroller.ScrollableHeight().unwrap_or(0.0),
        page_idx,
        ui.session.page_count,
        paused,
    );
    match action {
        WheelAction::Scroll => {}
        WheelAction::Swallow => {
            let _ = ev.SetHandled(true);
        }
        WheelAction::NextPage => {
            let _ = ev.SetHandled(true);
            ui.state.borrow_mut().last_wheel = Some(now);
            go_to_page(ui, page_idx + 1, ScrollIntent::Top);
        }
        WheelAction::PrevPageAtBottom => {
            let _ = ev.SetHandled(true);
            ui.state.borrow_mut().last_wheel = Some(now);
            go_to_page(ui, page_idx - 1, ScrollIntent::Bottom);
        }
    }
}

fn on_activation_changed(ui: &ReaderUi, deactivated: bool) {
    let locked = ui.state.borrow().locked;
    if deactivated {
        if locked
            || ui
                .session
                .policy
                .flags
                .contains(SecurityFlags::BLUR_ON_UNFOCUS)
        {
            ui.state.borrow_mut().curtain_shown = true;
            let _ = ui.curtain.SetVisibility(Visibility::Visible);
        }
        return;
    }
    if locked {
        return;
    }
    match reverify_protection(&ui.win) {
        Ok(protection) => {
            let _ = ui
                .protect_tb
                .SetText(&HSTRING::from(protection_status_text(protection)));
            ui.state.borrow_mut().curtain_shown = false;
            let _ = ui.curtain.SetVisibility(Visibility::Collapsed);
            request_overlay(ui);
        }
        Err(_) => lock_and_close(
            ui,
            "系统已无法为本窗口保持防截屏保护，为避免文档被截屏或录屏，已停止显示并锁定文档。",
        ),
    }
}

// ------------------------------------------------------------------ navigation & zoom

fn step_page(ui: &ReaderUi, delta: i32) {
    let current = ui.state.borrow().page_idx;
    go_to_page(ui, current + delta, ScrollIntent::Top);
}

fn go_to_page(ui: &ReaderUi, page_index: i32, intent: ScrollIntent) {
    let changed = {
        let mut state = ui.state.borrow_mut();
        if page_index < 0 || page_index >= ui.session.page_count || page_index == state.page_idx {
            false
        } else {
            state.page_idx = page_index;
            true
        }
    };
    if changed {
        trigger_render(ui, intent);
    }
}

/// Applies the number typed into the page box (clamped to the document) and normalises the box.
fn commit_page_input(ui: &ReaderUi) {
    let text = ui.page_box.Text().unwrap_or_default().to_string_lossy();
    let current = ui.state.borrow().page_idx;
    let target = resolve_page_input(&text, current, ui.session.page_count);
    go_to_page(ui, target, ScrollIntent::Top);
    sync_page_box(ui);
}

/// Puts the real page number back into the page box (invalid or out-of-range input is undone).
fn sync_page_box(ui: &ReaderUi) {
    let shown = (ui.state.borrow().page_idx + 1).to_string();
    if ui.page_box.Text().unwrap_or_default().to_string_lossy() != shown {
        let _ = ui.page_box.SetText(&HSTRING::from(shown));
    }
}

fn adjust_zoom(ui: &ReaderUi, delta: f64) {
    let next = clamp_zoom(ui.state.borrow().zoom + delta);
    set_zoom(ui, next);
}

fn set_zoom(ui: &ReaderUi, zoom: f64) {
    let clamped = clamp_zoom(zoom);
    if (ui.state.borrow().zoom - clamped).abs() < 0.001 {
        return;
    }
    // Capture where the viewport is looking *before* the content changes size.
    let intent = keep_center_intent(ui);
    ui.state.borrow_mut().zoom = clamped;
    trigger_render(ui, intent);
}

fn keep_center_intent(ui: &ReaderUi) -> ScrollIntent {
    let sv = &ui.scroller;
    ScrollIntent::KeepCenter {
        ratio_x: viewport_center_ratio(
            sv.HorizontalOffset().unwrap_or(0.0),
            sv.ViewportWidth().unwrap_or(0.0),
            sv.ExtentWidth().unwrap_or(0.0),
            0.5,
        ),
        ratio_y: viewport_center_ratio(
            sv.VerticalOffset().unwrap_or(0.0),
            sv.ViewportHeight().unwrap_or(0.0),
            sv.ExtentHeight().unwrap_or(0.0),
            0.0,
        ),
    }
}

// ------------------------------------------------------------------ rendering

fn trigger_render(ui: &ReaderUi, intent: ScrollIntent) {
    let dpi_scale = ui.win.dpi_scale();
    let (generation, page_index, zoom, reuse_overlay_size) = {
        let mut state = ui.state.borrow_mut();
        if state.locked {
            return;
        }
        let same_dpi = (state.dpi_scale - dpi_scale).abs() <= 0.001;
        let reuse_overlay_size = if same_dpi {
            state.overlay_bitmap.as_ref().map(|(_, w, h)| (*w, *h))
        } else {
            None
        };
        state.generation += 1;
        state.scroll_intent = intent;
        state.dpi_scale = dpi_scale;
        (
            state.generation,
            state.page_idx,
            state.zoom,
            reuse_overlay_size,
        )
    };

    sync_page_box(ui);
    #[allow(clippy::cast_possible_truncation)]
    let zoom_pct = (zoom * 100.0).round() as i32;
    let _ = ui.zoom_tb.SetText(&HSTRING::from(format!("{zoom_pct}%")));
    let _ = ui.prev_btn.SetIsEnabled(page_index > 0);
    let _ = ui
        .next_btn
        .SetIsEnabled(page_index + 1 < ui.session.page_count);
    // The page and its overlay are rendered together (or reuse an already live identical-size
    // overlay), so a page never reaches the screen without its watermark.
    ui.worker.submit_page(PageJob {
        generation,
        page_index,
        zoom,
        dpi_scale,
        reuse_overlay_size,
        overlay: overlay_spec(ui, dpi_scale),
    });
}

/// Overlay content for "now": live local time, current IP, current moire level and phase.
fn overlay_spec(ui: &ReaderUi, dpi_scale: f64) -> OverlaySpec {
    let (moire_level, moire_phase) = {
        let state = ui.state.borrow();
        (state.moire, state.moire_phase)
    };
    OverlaySpec {
        hidden_watermark: ui.session.hidden_watermark_enabled(),
        visible_watermark: ui.session.visible_watermark_enabled(),
        moire_level,
        moire_phase,
        watermark_text: ui
            .session
            .screen_watermark_text(&LocalDateTime::now().format_short()),
        ipv4: ui.session.current_ip(),
        unix_seconds: unix_seconds_now(),
        dpi_scale,
    }
}

/// UI-thread continuation of a finished page render.
pub fn apply_page_result(ui: &ReaderUi, result: PageResult) {
    let (intent, prev_shown) = {
        let state = ui.state.borrow();
        // Checked first: once locked (or superseded) nothing may be put back on screen.
        if state.locked || state.generation != result.job.generation {
            return;
        }
        (state.scroll_intent, state.shown)
    };
    let page_number = result.job.page_index + 1;
    let rendered = match result.outcome {
        Ok(rendered) => rendered,
        Err(msg) => {
            clear_page_content(ui);
            show_info(
                &ui.alert,
                InfoBarSeverity::Error,
                "页面无法显示",
                &format!("第 {page_number} 页无法显示：{msg}"),
            );
            return;
        }
    };
    let layout = rendered.layout;

    // Overlay first: if the watermark cannot be shown, the page is not shown either.
    let overlay_shown = if rendered.reused_live_overlay {
        let has_live = ui
            .state
            .borrow()
            .overlay_bitmap
            .as_ref()
            .is_some_and(|(_, w, h)| *w == layout.px_width && *h == layout.px_height);
        if has_live {
            let _ = ui.overlay_img.SetVisibility(Visibility::Visible);
        }
        has_live
    } else {
        match rendered.overlay_pixels {
            Ok(Some(pixels)) => {
                show_overlay_pixels(ui, layout.px_width, layout.px_height, &pixels).is_ok()
            }
            Ok(None) => {
                let _ = ui.overlay_img.SetVisibility(Visibility::Collapsed);
                true
            }
            Err(_) => false,
        }
    };
    if !overlay_shown {
        lock_and_close(ui, WATERMARK_FAILURE_REASON);
        return;
    }

    if let Err(err) = show_page_pixels(ui, layout.px_width, layout.px_height, &rendered.page_pixels)
    {
        clear_page_content(ui);
        show_info(
            &ui.alert,
            InfoBarSeverity::Error,
            "页面无法显示",
            &format!(
                "第 {page_number} 页无法创建页面位图（错误代码 0x{:08X}）",
                err.code().0.cast_unsigned()
            ),
        );
        return;
    }

    let size_changed = prev_shown.is_none_or(|prev| {
        prev.px_width != layout.px_width
            || prev.px_height != layout.px_height
            || (prev.effective_scale - layout.effective_scale()).abs() > 1e-4
    });
    if size_changed {
        for img in [&ui.page_img, &ui.overlay_img] {
            let _ = img.SetWidth(layout.dip_width);
            let _ = img.SetHeight(layout.dip_height);
        }
    }
    ui.state.borrow_mut().shown = Some(ShownFrame {
        px_width: layout.px_width,
        px_height: layout.px_height,
        effective_scale: layout.effective_scale(),
    });
    apply_scroll_intent(ui, intent, size_changed);
}

/// Uploads page pixels, reusing the existing `WriteableBitmap` while dimensions are unchanged.
fn show_page_pixels(
    ui: &ReaderUi,
    width: i32,
    height: i32,
    pixels: &[u8],
) -> windows_core::Result<()> {
    let reusable = ui
        .state
        .borrow()
        .page_bitmap
        .as_ref()
        .filter(|(_, w, h)| *w == width && *h == height)
        .map(|(bitmap, _, _)| bitmap.clone());
    if let Some(bitmap) = reusable {
        write_bgra_into_bitmap(&bitmap, pixels)
    } else {
        let bitmap = writeable_bitmap_from_bgra(width, height, pixels)?;
        ui.page_img.SetSource(&bitmap)?;
        ui.state.borrow_mut().page_bitmap = Some((bitmap, width, height));
        Ok(())
    }
}

fn apply_scroll_intent(ui: &ReaderUi, intent: ScrollIntent, size_changed: bool) {
    let sv = &ui.scroller;
    if size_changed {
        let _ = sv.UpdateLayout();
    }
    let (horizontal, vertical) = match intent {
        ScrollIntent::Top => (None, Some(0.0)),
        ScrollIntent::Bottom => (None, Some(sv.ScrollableHeight().unwrap_or(0.0))),
        ScrollIntent::KeepCenter { ratio_x, ratio_y } => (
            Some(centered_offset(
                ratio_x,
                sv.ExtentWidth().unwrap_or(0.0),
                sv.ViewportWidth().unwrap_or(0.0),
            )),
            Some(centered_offset(
                ratio_y,
                sv.ExtentHeight().unwrap_or(0.0),
                sv.ViewportHeight().unwrap_or(0.0),
            )),
        ),
    };
    let _ = sv.ChangeViewWithOptionalAnimation(horizontal, vertical, None, true);
}

/// Asks the worker for a fresh overlay frame for the page bitmap currently on screen.
fn request_overlay(ui: &ReaderUi) {
    let (generation, shown) = {
        let state = ui.state.borrow();
        if state.locked {
            return;
        }
        let Some(shown) = state.shown else {
            return;
        };
        (state.generation, shown)
    };
    let spec = overlay_spec(ui, shown.effective_scale);
    if spec.is_empty() {
        let _ = ui.overlay_img.SetVisibility(Visibility::Collapsed);
        return;
    }
    ui.worker.submit_overlay(OverlayJob {
        generation,
        px_width: shown.px_width,
        px_height: shown.px_height,
        spec,
    });
}

/// UI-thread continuation of a finished overlay frame.
pub fn apply_overlay_result(ui: &ReaderUi, result: OverlayResult) {
    {
        let state = ui.state.borrow();
        let matches_shown = state
            .shown
            .is_some_and(|s| s.px_width == result.px_width && s.px_height == result.px_height);
        if state.locked || state.generation != result.generation || !matches_shown {
            return;
        }
    }
    let shown = match result.pixels {
        Ok(Some(pixels)) => {
            show_overlay_pixels(ui, result.px_width, result.px_height, &pixels).is_ok()
        }
        Ok(None) => {
            let _ = ui.overlay_img.SetVisibility(Visibility::Collapsed);
            true
        }
        Err(_) => false,
    };
    if !shown {
        lock_and_close(ui, WATERMARK_FAILURE_REASON);
    }
}

/// Uploads overlay pixels, reusing the existing bitmap while the size is unchanged.
fn show_overlay_pixels(
    ui: &ReaderUi,
    width: i32,
    height: i32,
    pixels: &[u8],
) -> windows_core::Result<()> {
    let reusable = ui
        .state
        .borrow()
        .overlay_bitmap
        .as_ref()
        .filter(|(_, w, h)| *w == width && *h == height)
        .map(|(bitmap, _, _)| bitmap.clone());
    if let Some(bitmap) = reusable {
        write_bgra_into_bitmap(&bitmap, pixels)?;
    } else {
        let bitmap = writeable_bitmap_from_bgra(width, height, pixels)?;
        ui.overlay_img.SetSource(&bitmap)?;
        ui.state.borrow_mut().overlay_bitmap = Some((bitmap, width, height));
    }
    ui.overlay_img.SetVisibility(Visibility::Visible)
}

/// One-second tick: advances the moire phase and refreshes the clock in the watermarks.
fn overlay_tick(ui: &ReaderUi) {
    {
        let mut state = ui.state.borrow_mut();
        // Nothing is visible behind the curtain; skip the work.
        if state.locked || state.curtain_shown {
            return;
        }
        state.moire_phase = (state.moire_phase + 1) % MOIRE_PHASE_STEPS;
    }
    if (ui.state.borrow().dpi_scale - ui.win.dpi_scale()).abs() > 0.001 {
        // Moved to a display with another scale: re-render the page (with its overlay).
        rerender_if_dpi_changed(ui);
        return;
    }
    request_overlay(ui);
}

// ------------------------------------------------------------------ status display

/// Refreshes the allowance text, the print button and its label from the session's quota.
pub fn update_quota_display(ui: &ReaderUi) {
    let quota = ui.session.quota_status();
    let expires = ui
        .session
        .policy
        .expires_at_utc_ticks
        .map(format_dotnet_ticks_local);
    let _ = ui.quota_tb.SetText(&HSTRING::from(quota_status_text(
        quota.max_prints,
        quota.remaining_prints,
        expires.as_deref(),
    )));
    let _ = ui.print_label.SetText(&HSTRING::from(print_button_text(
        quota.max_prints,
        quota.remaining_prints,
    )));
    let enabled = {
        let state = ui.state.borrow();
        !state.locked && state.print_phase == PrintPhase::Idle && quota.remaining_prints > 0
    };
    let _ = ui.print_btn.SetIsEnabled(enabled);
}

fn update_trace_status(ui: &ReaderUi) {
    let _ = ui.trace_tb.SetText(&HSTRING::from(trace_status_text(
        &ui.session.device.machine_code,
        &ui.session.current_ip(),
    )));
}

// ------------------------------------------------------------------ housekeeping

fn expiry_reason(expires_utc_ticks: i64) -> String {
    format!(
        "文档授权已过期（有效期至 {}）。",
        format_dotnet_ticks_local(expires_utc_ticks)
    )
}

/// Precise one-shot check armed for the expiry instant.
fn check_expiry_now(ui: &ReaderUi) {
    let Some(expires) = ui.session.policy.expires_at_utc_ticks else {
        return;
    };
    let (locked, print_busy) = {
        let state = ui.state.borrow();
        (state.locked, state.print_phase != PrintPhase::Idle)
    };
    // A print in progress is not interrupted; printing re-checks the expiry itself and the
    // periodic check locks the window right after.
    if locked || print_busy {
        return;
    }
    let verdict = evaluate_housekeeping(
        ui.session.unlock_utc_ticks,
        ui.session.unlock_instant.elapsed().as_secs(),
        utc_now_dotnet_ticks(),
        Some(expires),
        0,
    );
    if verdict == HousekeepingVerdict::Expired {
        lock_and_close(ui, &expiry_reason(expires));
    }
}

/// 30-second routine: capture protection, expiry (wall clock and monotonic clock), and in the
/// background the current IP plus a periodic full authorisation re-check.
fn housekeeping_tick(ui: &ReaderUi) {
    let (tick, print_busy) = {
        let mut state = ui.state.borrow_mut();
        if state.locked {
            return;
        }
        state.housekeeping_ticks += 1;
        (
            state.housekeeping_ticks,
            state.print_phase != PrintPhase::Idle,
        )
    };
    let Ok(protection) = reverify_protection(&ui.win) else {
        lock_and_close(
            ui,
            "系统已无法为本窗口保持防截屏保护，为避免文档被截屏或录屏，已停止显示并锁定文档。",
        );
        return;
    };
    let _ = ui
        .protect_tb
        .SetText(&HSTRING::from(protection_status_text(protection)));

    let expires = ui.session.policy.expires_at_utc_ticks;
    let verdict = evaluate_housekeeping(
        ui.session.unlock_utc_ticks,
        ui.session.unlock_instant.elapsed().as_secs(),
        utc_now_dotnet_ticks(),
        expires,
        tick,
    );
    let reverify_store = match verdict {
        HousekeepingVerdict::Expired if print_busy => false,
        HousekeepingVerdict::Expired => {
            lock_and_close(ui, &expiry_reason(expires.unwrap_or_default()));
            return;
        }
        HousekeepingVerdict::Healthy { reverify_store } => reverify_store && !print_busy,
    };

    // Only the session and the dispatcher queue travel to the background thread.
    let session = Arc::clone(&ui.session);
    let dq = ui.dq.clone();
    std::thread::spawn(move || {
        let (new_ip, _) = enumerate_ipv4_addresses();
        let ip_changed = session.update_ip(&new_ip);
        let verified = reverify_store.then(|| {
            LocalQuotaStore::default()
                .initialize_or_verify(&session.policy, &session.device.machine_hash)
                .map(|status| session.set_quota_status(status))
                .map_err(|e| e.to_string())
        });
        let _ = enqueue_ui(&dq, move || {
            with_reader(|ui| apply_housekeeping_outcome(ui, ip_changed, verified));
        });
    });
}

fn apply_housekeeping_outcome(
    ui: &ReaderUi,
    ip_changed: bool,
    verified: Option<Result<(), String>>,
) {
    if ui.state.borrow().locked {
        return;
    }
    match verified {
        // The authorisation record can no longer be read or trusted: stop showing the document.
        Some(Err(reason)) => {
            lock_and_close(ui, &reason);
            return;
        }
        Some(Ok(())) => update_quota_display(ui),
        None => {}
    }
    if ip_changed {
        update_trace_status(ui);
        request_overlay(ui);
    }
}

// ------------------------------------------------------------------ lock & close

/// Removes everything from the screen, releases the plaintext, then explains why and closes.
pub fn lock_and_close(ui: &ReaderUi, reason: &str) {
    {
        let mut state = ui.state.borrow_mut();
        if state.locked {
            return;
        }
        state.locked = true;
        state.generation += 1;
        state.curtain_shown = true;
        reason.clone_into(&mut state.lock_reason);
    }
    stop_timers(ui);
    ui.worker.shutdown();
    clear_page_content(ui);
    let _ = ui.curtain_title.SetText(&HSTRING::from("文档已锁定"));
    let _ = ui.curtain_msg.SetText(&HSTRING::from(reason));
    let _ = ui.curtain.SetVisibility(Visibility::Visible);
    let _ = ui.print_btn.SetIsEnabled(false);
    release_active_session();
    ui.session.release_plaintext();

    // The notice is shown from a fresh dispatcher turn so it never nests inside the event that
    // triggered the lock (activation change, timer tick, render callback).
    let queued = enqueue_ui(&ui.dq, || {
        with_reader(notify_locked_and_close);
    });
    if !queued {
        let _ = ui.win.window.Close();
    }
}

fn notify_locked_and_close(ui: &ReaderUi) {
    let reason = ui.state.borrow().lock_reason.clone();
    if let Ok(hwnd) = window_hwnd(&ui.win.window) {
        message_box(hwnd, "文档已锁定", &reason, false);
    }
    let _ = ui.win.window.Close();
}

/// Detaches the page and overlay bitmaps from the visual tree and wipes cached page pixels.
fn clear_page_content(ui: &ReaderUi) {
    let _ = ui.page_img.SetSource(None::<&ImageSource>);
    let _ = ui.overlay_img.SetSource(None::<&ImageSource>);
    let _ = ui.overlay_img.SetVisibility(Visibility::Collapsed);
    let old_page = {
        let mut state = ui.state.borrow_mut();
        state.shown = None;
        state.overlay_bitmap = None;
        state.page_bitmap.take()
    };
    if let Some((bmp, w, h)) = old_page
        && let (Ok(wu), Ok(hu)) = (usize::try_from(w), usize::try_from(h))
        && let Some(bytes) = wu.checked_mul(hu).and_then(|px| px.checked_mul(4))
    {
        let zeros = vec![0_u8; bytes];
        let _ = write_bgra_into_bitmap(&bmp, &zeros);
    }
}

/// Tears the reader down: stops timers and the worker, clears the images and releases the
/// decrypted document. Returns whether the window had been fully opened (`None` if no reader
/// window is registered).
pub fn teardown_reader() -> Option<bool> {
    let ui = READER_UI.with(|slot| slot.borrow_mut().take())?;
    let opened = {
        let mut state = ui.state.borrow_mut();
        state.locked = true;
        state.generation += 1;
        state.opened
    };
    stop_timers(&ui);
    ui.worker.shutdown();
    clear_page_content(&ui);
    release_active_session();
    ui.session.release_plaintext();
    Some(opened)
}
