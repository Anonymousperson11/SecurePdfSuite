//! Pure Rust domain logic for `SecurePdfReader`: unlock backoff, zoom/DPI render math,
//! shortcut policy filtering, runtime housekeeping, page navigation rules, print page selection
//! and the anti-camera overlay (moire lattice, trace watermarks, microdot matrix).

use secure_pdf_core::printing::parse_page_ranges;
use secure_pdf_core::watermark::{
    TextLayerStyle, encode_trace_payload, microdot_unit_px, paint_microdots, paint_text_layer,
};
use secure_pdf_core::{CoreError, CoreResult, MoireLevel};

/// Maximum pixels per viewport page render (`32M px`) to cap RAM/VRAM allocation.
pub const MAX_READER_VIEW_PIXELS: u64 = 32_000_000;
/// Minimum zoom factor (`50%`).
pub const MIN_ZOOM: f64 = 0.50;
/// Maximum zoom factor (`400%`).
pub const MAX_ZOOM: f64 = 4.00;
/// Incremental zoom step (`15%`).
pub const ZOOM_STEP: f64 = 0.15;

/// Number of distinct moire phase steps before the lattice animation repeats.
pub const MOIRE_PHASE_STEPS: u32 = 6;
/// Minimum cooldown after a wheel-triggered page turn before another edge notch turns again.
pub const WHEEL_PAGING_PAUSE_MS: u128 = 200;
/// Every this many housekeeping ticks the local authorisation record is fully re-verified.
pub const FULL_VERIFY_EVERY_TICKS: u64 = 4;

/// Virtual key constants (avoiding full `Win32` dependency inside pure logic tests).
pub mod vk {
    pub const ENTER: i32 = 0x0D;
    pub const PRINT_SCREEN: i32 = 0x2C;
    pub const LEFT: i32 = 0x25;
    #[allow(dead_code)]
    pub const UP: i32 = 0x26;
    pub const RIGHT: i32 = 0x27;
    #[allow(dead_code)]
    pub const DOWN: i32 = 0x28;
    pub const PAGE_UP: i32 = 0x21;
    pub const PAGE_DOWN: i32 = 0x22;
    pub const KEY_0: i32 = 0x30;
    pub const KEY_A: i32 = 0x41;
    pub const KEY_C: i32 = 0x43;
    pub const KEY_E: i32 = 0x45;
    pub const KEY_P: i32 = 0x50;
    pub const KEY_S: i32 = 0x53;
    pub const KEY_V: i32 = 0x56;
    pub const KEY_X: i32 = 0x58;
    pub const ADD: i32 = 0x6B;
    pub const SUBTRACT: i32 = 0x6D;
    pub const OEM_PLUS: i32 = 0xBB;
    pub const OEM_MINUS: i32 = 0xBD;
}

/// Progressive brute-force delay tracker (`0 -> 500 -> 1500 -> 3000 -> 5000 ms`).
#[derive(Clone, Debug, Default)]
pub struct UnlockBackoff {
    failures: u32,
}

impl UnlockBackoff {
    /// Records one failed verification-code attempt and returns the required cooldown in milliseconds.
    pub const fn record_failure(&mut self) -> u64 {
        self.failures = self.failures.saturating_add(1);
        Self::delay_for_failures(self.failures)
    }

    /// Clears failure counter after a valid unlock.
    pub const fn reset(&mut self) {
        self.failures = 0;
    }

    /// Maps consecutive failure count to deterministic progressive backoff milliseconds.
    #[must_use]
    pub const fn delay_for_failures(failures: u32) -> u64 {
        match failures {
            0 => 0,
            1 => 500,
            2 => 1500,
            3 => 3000,
            _ => 5000,
        }
    }
}

/// Clamps zoom factor cleanly into `[MIN_ZOOM, MAX_ZOOM]`.
#[must_use]
pub const fn clamp_zoom(zoom: f64) -> f64 {
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

/// Computes DPI-aware bitmap pixel size for `(page_width_pts, page_height_pts)` capped at `MAX_READER_VIEW_PIXELS`.
#[must_use]
pub fn compute_view_render_size(
    page_width_pts: f64,
    page_height_pts: f64,
    zoom: f64,
    dpi_scale: f64,
) -> (i32, i32) {
    let base_scale = (dpi_scale.max(0.5) * 96.0 / 72.0) * clamp_zoom(zoom);
    let mut w = (page_width_pts.max(1.0) * base_scale).round().max(1.0);
    let mut h = (page_height_pts.max(1.0) * base_scale).round().max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let max_px = MAX_READER_VIEW_PIXELS as f64;
    let total = w * h;
    if total > max_px {
        let shrink = (max_px / total).sqrt();
        w = (w * shrink).floor().max(1.0);
        h = (h * shrink).floor().max(1.0);
    }
    #[allow(clippy::cast_possible_truncation)]
    let wi = (w as i64).clamp(1, i64::from(i32::MAX)) as i32;
    #[allow(clippy::cast_possible_truncation)]
    let hi = (h as i64).clamp(1, i64::from(i32::MAX)) as i32;
    (wi, hi)
}

/// Pixel size of the rendered page bitmap and the size it is displayed at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewLayout {
    /// Bitmap width in physical pixels (capped by [`MAX_READER_VIEW_PIXELS`]).
    pub px_width: i32,
    /// Bitmap height in physical pixels.
    pub px_height: i32,
    /// Display width in device-independent pixels; follows the zoom label even when the
    /// bitmap was capped, in which case the bitmap is stretched.
    pub dip_width: f64,
    /// Display height in device-independent pixels.
    pub dip_height: f64,
    /// `true` when the pixel budget forced a lower resolution than one bitmap pixel per
    /// screen pixel.
    pub capped: bool,
}

impl ViewLayout {
    /// Bitmap pixels per device-independent pixel (the display scale unless capped).
    #[must_use]
    pub fn effective_scale(&self) -> f64 {
        f64::from(self.px_width) / self.dip_width.max(1.0)
    }
}

/// Computes bitmap pixels and on-screen size for a page at `zoom` on a display of `dpi_scale`.
#[must_use]
pub fn compute_view_layout(
    page_width_pts: f64,
    page_height_pts: f64,
    zoom: f64,
    dpi_scale: f64,
) -> ViewLayout {
    let scale = dpi_scale.max(0.5);
    let (px_width, px_height) =
        compute_view_render_size(page_width_pts, page_height_pts, zoom, scale);
    let dip_per_pt = 96.0 / 72.0 * clamp_zoom(zoom);
    let uncapped_w = (page_width_pts.max(1.0) * dip_per_pt * scale)
        .round()
        .max(1.0);
    let uncapped_h = (page_height_pts.max(1.0) * dip_per_pt * scale)
        .round()
        .max(1.0);
    let capped = f64::from(px_width) < uncapped_w || f64::from(px_height) < uncapped_h;
    let (dip_width, dip_height) = if capped {
        (
            page_width_pts.max(1.0) * dip_per_pt,
            page_height_pts.max(1.0) * dip_per_pt,
        )
    } else {
        // One bitmap pixel per screen pixel: place on whole device pixels so nothing is resampled.
        (f64::from(px_width) / scale, f64::from(px_height) / scale)
    };
    ViewLayout {
        px_width,
        px_height,
        dip_width,
        dip_height,
        capped,
    }
}

/// Scroll offset that puts the same relative position (`ratio` of the extent) back in the
/// middle of the viewport after the content size changed.
#[must_use]
pub fn centered_offset(ratio: f64, extent: f64, viewport: f64) -> f64 {
    let max_offset = (extent - viewport).max(0.0);
    ratio
        .clamp(0.0, 1.0)
        .mul_add(extent, -(viewport / 2.0))
        .clamp(0.0, max_offset)
}

/// Relative position of the viewport centre inside the scrollable extent (`fallback` when the
/// extent is empty).
#[must_use]
pub fn viewport_center_ratio(offset: f64, viewport: f64, extent: f64, fallback: f64) -> f64 {
    if extent > 0.0 {
        ((offset + viewport / 2.0) / extent).clamp(0.0, 1.0)
    } else {
        fallback
    }
}

/// Resolves text typed into the page box to a zero-based page index.
///
/// Out-of-range numbers are clamped to the first / last page; anything that is not a plain
/// number keeps the current page.
#[must_use]
pub fn resolve_page_input(text: &str, current_index: i32, page_count: i32) -> i32 {
    let trimmed = text.trim();
    if page_count <= 0 || trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return current_index;
    }
    trimmed.parse::<i64>().map_or(current_index, |number| {
        let clamped = number.clamp(1, i64::from(page_count));
        i32::try_from(clamped - 1).unwrap_or(current_index)
    })
}

/// What a mouse-wheel notch does beyond ordinary scrolling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelAction {
    /// Ordinary scrolling; leave the event alone.
    Scroll,
    /// Already at the edge but the wheel is still spinning: swallow without turning the page.
    Swallow,
    /// Go to the next page (shown from its top).
    NextPage,
    /// Go to the previous page (shown from its bottom).
    PrevPageAtBottom,
}

/// Decides whether a wheel notch at the top / bottom edge of the page turns the page.
///
/// `paused` tells whether enough time passed since the previous notch, so that one long spin
/// does not race through several pages.
#[must_use]
pub fn wheel_paging_action(
    wheel_delta: i32,
    vertical_offset: f64,
    scrollable_height: f64,
    page_index: i32,
    page_count: i32,
    paused: bool,
) -> WheelAction {
    let at_bottom = vertical_offset >= scrollable_height - 0.5;
    let at_top = vertical_offset <= 0.5;
    if wheel_delta < 0 && at_bottom && page_index + 1 < page_count {
        if paused {
            WheelAction::NextPage
        } else {
            WheelAction::Swallow
        }
    } else if wheel_delta > 0 && at_top && page_index > 0 {
        if paused {
            WheelAction::PrevPageAtBottom
        } else {
            WheelAction::Swallow
        }
    } else {
        WheelAction::Scroll
    }
}

/// Which pages the print dialog was asked to print.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSelectionMode {
    /// Every page of the document.
    All,
    /// Only the page currently shown.
    Current,
    /// A custom specification such as `1-3,5`.
    Custom,
}

impl PageSelectionMode {
    /// Maps the print dialog's combo index to a mode.
    #[must_use]
    pub const fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Current,
            2 => Self::Custom,
            _ => Self::All,
        }
    }

    /// Combo index of this mode.
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Self::All => 0,
            Self::Current => 1,
            Self::Custom => 2,
        }
    }
}

/// Resolves the print dialog's page selection to zero-based page indices.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] with a user-facing message for an invalid custom
/// range or an out-of-range current page.
pub fn resolve_print_pages(
    mode: PageSelectionMode,
    custom_spec: &str,
    current_index: i32,
    page_count: i32,
) -> CoreResult<Vec<i32>> {
    match mode {
        PageSelectionMode::All if page_count > 0 => Ok((0..page_count).collect()),
        PageSelectionMode::Current if (0..page_count).contains(&current_index) => {
            Ok(vec![current_index])
        }
        PageSelectionMode::Custom => parse_page_ranges(custom_spec, page_count),
        _ => Err(CoreError::InvalidArgument("没有可打印的页面。".to_owned())),
    }
}

/// Status-bar text describing the print allowance and the expiry (local time), as in v1.x.
#[must_use]
pub fn quota_status_text(max_prints: i32, remaining: i32, expires_local: Option<&str>) -> String {
    let expiry = expires_local.map_or_else(String::new, |t| format!("  |  有效期至: {t}"));
    if max_prints <= 0 {
        format!("打印策略: 发送方已禁止打印 (0 次){expiry}")
    } else {
        format!("剩余打印次数: {} / {max_prints}{expiry}", remaining.max(0))
    }
}

/// Label of the print button for the current allowance.
#[must_use]
pub const fn print_button_text(max_prints: i32, remaining: i32) -> &'static str {
    if max_prints <= 0 {
        "禁止打印"
    } else if remaining > 0 {
        "安全打印"
    } else {
        "打印次数已用尽"
    }
}

/// Status-bar text with the machine code and current IP.
#[must_use]
pub fn trace_status_text(machine_code: &str, ipv4: &str) -> String {
    format!("设备码: {machine_code}  |  IP: {ipv4}  |  禁止复制与另存")
}

/// Screenshot / recording protection actually in effect for a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectionState {
    /// The window is left out of screenshots and recordings entirely.
    ExcludedFromCapture,
    /// The window shows up as a black rectangle in screenshots and recordings.
    BlackInCapture,
    /// Debug builds only: protection deliberately disabled for watermark diagnostics.
    DebugDisabled,
}

/// Toolbar text stating the protection level that is really active.
#[must_use]
pub const fn protection_status_text(state: ProtectionState) -> &'static str {
    match state {
        ProtectionState::ExcludedFromCapture => "防截屏/防录屏已启用（窗口不会出现在截图和录屏中）",
        ProtectionState::BlackInCapture => "防截屏/防录屏已启用（截图和录屏中本窗口显示为黑色）",
        ProtectionState::DebugDisabled => "调试版本：防截屏保护已关闭，请勿用于真实文档",
    }
}

/// One-line description of a selected `.spdf` file: short document id and size.
#[must_use]
pub fn file_summary_text(doc_id_hex: &str, size_bytes: u64) -> String {
    let short_id: String = doc_id_hex
        .chars()
        .take(12)
        .collect::<String>()
        .to_uppercase();
    let kb_tenths = size_bytes.saturating_mul(10) / 1024;
    let size = if kb_tenths >= 10_240 {
        let megabytes = size_bytes.saturating_mul(10) / (1024 * 1024);
        format!("{}.{} MB", megabytes / 10, megabytes % 10)
    } else {
        format!("{}.{} KB", kb_tenths / 10, kb_tenths % 10)
    };
    format!("文档 ID: {short_id}  ·  大小: {size}")
}

/// Human-readable wait time for the unlock cooldown.
#[must_use]
pub fn cooldown_text(wait_ms: u64) -> String {
    if wait_ms.is_multiple_of(1000) {
        format!("{} 秒", wait_ms / 1000)
    } else {
        format!("{}.{} 秒", wait_ms / 1000, (wait_ms % 1000) / 100)
    }
}

/// Action decided by reader shortcut filtering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutAction {
    /// Block and mark event handled to deny exfiltration or screen snapshot keys.
    Block,
    /// Open secure print workflow (`Ctrl+P`).
    Print,
    /// Step zoom in (`Ctrl++`).
    ZoomIn,
    /// Step zoom out (`Ctrl+-`).
    ZoomOut,
    /// Reset zoom to `100%` (`Ctrl+0`).
    ZoomReset,
    /// Navigate to previous page.
    PrevPage,
    /// Navigate to next page.
    NextPage,
    /// Leave key event unhandled for normal UI navigation/input.
    Allow,
}

/// Classifies a reader keystroke according to security and navigation rules.
#[must_use]
pub const fn classify_shortcut(
    virtual_key: i32,
    ctrl: bool,
    focus_in_text_input: bool,
) -> ShortcutAction {
    if virtual_key == vk::PRINT_SCREEN {
        return ShortcutAction::Block;
    }
    if ctrl {
        match virtual_key {
            vk::KEY_P => return ShortcutAction::Print,
            vk::ADD | vk::OEM_PLUS => return ShortcutAction::ZoomIn,
            vk::SUBTRACT | vk::OEM_MINUS => return ShortcutAction::ZoomOut,
            vk::KEY_0 => return ShortcutAction::ZoomReset,
            vk::KEY_C | vk::KEY_X | vk::KEY_S | vk::KEY_A | vk::KEY_E | vk::KEY_V => {
                return if focus_in_text_input {
                    ShortcutAction::Allow
                } else {
                    ShortcutAction::Block
                };
            }
            _ => {}
        }
    }
    if focus_in_text_input {
        return ShortcutAction::Allow;
    }
    match virtual_key {
        vk::PAGE_UP | vk::LEFT => ShortcutAction::PrevPage,
        vk::PAGE_DOWN | vk::RIGHT => ShortcutAction::NextPage,
        _ => ShortcutAction::Allow,
    }
}

/// Everything the anti-camera overlay needs for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlaySpec {
    /// Draw the hidden trace watermark and the microdot matrix (policy `HIDDEN_WATERMARK`).
    pub hidden_watermark: bool,
    /// Draw the visible watermark (policy `VISIBLE_WATERMARK`).
    pub visible_watermark: bool,
    /// Moire interference intensity level.
    pub moire_level: MoireLevel,
    /// Animation phase of the moire lattice (`0..MOIRE_PHASE_STEPS`).
    pub moire_phase: u32,
    /// Trace label (note, IP, live local time, machine code) drawn by both text layers.
    pub watermark_text: String,
    /// Primary IPv4 address encoded into the microdot matrix.
    pub ipv4: String,
    /// Unix seconds encoded into the microdot matrix.
    pub unix_seconds: u32,
    /// Display scale (`1.0` at 96 DPI) used for font sizes, spacing and dot size.
    pub dpi_scale: f64,
}

impl OverlaySpec {
    /// `true` when the frame would be fully transparent, so no overlay bitmap is needed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.hidden_watermark && !self.visible_watermark && self.moire_level == MoireLevel::Off
    }
}

/// Renders the transparent premultiplied `BGRA8` anti-camera overlay for one frame.
///
/// Layers, bottom to top: moire lattice, hidden trace text plus microdot matrix (only with
/// `hidden_watermark`), visible watermark text (only with `visible_watermark`). Returns
/// `Ok(None)` when nothing has to be drawn.
///
/// # Errors
///
/// Returns the core error when the system font text cannot be rendered, or
/// [`CoreError::InvalidArgument`] for a non-positive size.
pub fn render_overlay(width: i32, height: i32, spec: &OverlaySpec) -> CoreResult<Option<Vec<u8>>> {
    if spec.is_empty() {
        return Ok(None);
    }
    let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
        return Err(CoreError::InvalidArgument("叠加层尺寸无效。".to_owned()));
    };
    if w == 0 || h == 0 {
        return Err(CoreError::InvalidArgument("叠加层尺寸无效。".to_owned()));
    }

    let mut bgra = vec![0_u8; w * h * 4];
    paint_moire_lattice(
        &mut bgra,
        w,
        spec.moire_level,
        spec.moire_phase,
        spec.dpi_scale,
    );
    if spec.hidden_watermark {
        paint_text_layer(
            &mut bgra,
            w,
            h,
            &spec.watermark_text,
            &TextLayerStyle::hidden(spec.dpi_scale),
        )?;
        let payload = encode_trace_payload(&spec.ipv4, spec.unix_seconds);
        paint_microdots(&mut bgra, w, h, &payload, microdot_unit_px(spec.dpi_scale))?;
    }
    if spec.visible_watermark {
        paint_text_layer(
            &mut bgra,
            w,
            h,
            &spec.watermark_text,
            &TextLayerStyle::visible(spec.dpi_scale),
        )?;
    }
    Ok(Some(bgra))
}

/// Lattice geometry and opacity for a moire level: `(dark alpha, light alpha, tile size in DIP)`.
const fn moire_params(level: MoireLevel) -> Option<(u8, u8, f64)> {
    match level {
        MoireLevel::Off => None,
        MoireLevel::Low => Some((7, 5, 6.0)),
        MoireLevel::Medium => Some((11, 8, 5.0)),
        MoireLevel::High => Some((18, 13, 4.0)),
    }
}

/// Lattice tile size in physical pixels for a moire level at the given display scale.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn moire_tile_px(level: MoireLevel, dpi_scale: f64) -> Option<usize> {
    let (_, _, tile_dip) = moire_params(level)?;
    let scale = if dpi_scale.is_finite() {
        dpi_scale.clamp(0.5, 8.0)
    } else {
        1.0
    };
    Some(((tile_dip * scale).round() as usize).max(3))
}

/// High-frequency diagonal lattice (two dark diagonals and one light half-diagonal per tile)
/// whose phase shifts with `phase`, as in the v1.x overlay. Must be painted first: it writes
/// straight into the still transparent buffer.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn paint_moire_lattice(bgra: &mut [u8], w: usize, level: MoireLevel, phase: u32, dpi_scale: f64) {
    let (Some((alpha_dark, alpha_light, _)), Some(tile)) =
        (moire_params(level), moire_tile_px(level, dpi_scale))
    else {
        return;
    };
    let scale = if dpi_scale.is_finite() {
        dpi_scale.clamp(0.5, 8.0)
    } else {
        1.0
    };
    let shift = (f64::from(phase % MOIRE_PHASE_STEPS) * 0.7 * scale).round() as usize % tile;
    let premul = |rgb: [u8; 3], alpha: u8| -> [u8; 4] {
        let ch = |c: u8| u8::try_from((u16::from(c) * u16::from(alpha) + 127) / 255).unwrap_or(0);
        [ch(rgb[2]), ch(rgb[1]), ch(rgb[0]), alpha]
    };
    let dark = premul([24, 30, 42], alpha_dark);
    let light = premul([248, 250, 252], alpha_light);

    for (y, row) in bgra.chunks_exact_mut(w * 4).enumerate() {
        let ly = (y + tile - shift) % tile;
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let lx = (x + tile - shift) % tile;
            if lx == ly || lx + ly == tile {
                *px = dark;
            } else if lx + ly == tile / 2 {
                *px = light;
            }
        }
    }
}

/// Outcome of the periodic runtime check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HousekeepingVerdict {
    /// Session remains valid; `reverify_store` asks for a full authorisation re-check.
    Healthy {
        /// Re-read and verify the local authorisation record on this tick.
        reverify_store: bool,
    },
    /// The document's validity period is over.
    Expired,
}

/// Evaluates expiry against both the wall clock and the tamper-proof monotonic clock.
///
/// A document counts as expired as soon as either `wall_now_utc_ticks` or the unlock time plus
/// the monotonic `elapsed_secs` reaches `expires_utc_ticks` (the same `>=` rule as
/// `SpdfPolicy::is_expired_at`), so setting the system clock back during reading does not help.
#[must_use]
pub fn evaluate_housekeeping(
    unlock_utc_ticks: i64,
    elapsed_secs: u64,
    wall_now_utc_ticks: i64,
    expires_utc_ticks: Option<i64>,
    tick_count: u64,
) -> HousekeepingVerdict {
    if let Some(expires) = expires_utc_ticks {
        let elapsed_ticks = i64::try_from(elapsed_secs)
            .unwrap_or(i64::MAX)
            .saturating_mul(10_000_000);
        let steady_now = unlock_utc_ticks.saturating_add(elapsed_ticks);
        if steady_now >= expires || wall_now_utc_ticks >= expires {
            return HousekeepingVerdict::Expired;
        }
    }
    HousekeepingVerdict::Healthy {
        reverify_store: tick_count > 0 && tick_count.is_multiple_of(FULL_VERIFY_EVERY_TICKS),
    }
}

/// Milliseconds from `now_utc_ticks` until the expiry instant (`0` when already reached).
#[must_use]
pub fn millis_until_expiry(now_utc_ticks: i64, expires_utc_ticks: i64) -> u64 {
    u64::try_from(expires_utc_ticks.saturating_sub(now_utc_ticks) / 10_000).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use secure_pdf_core::ForensicsImage;
    use secure_pdf_core::watermark::{
        MICRODOT_TILE_H, MICRODOT_TILE_W, composite_overlay_over_page, decode_microdots,
    };

    use super::*;

    #[test]
    fn unlock_backoff_progresses_strictly_through_tiers() {
        let mut b = UnlockBackoff::default();
        assert_eq!(b.record_failure(), 500);
        assert_eq!(b.record_failure(), 1500);
        assert_eq!(b.record_failure(), 3000);
        assert_eq!(b.record_failure(), 5000);
        assert_eq!(b.record_failure(), 5000);
        b.reset();
        assert_eq!(b.record_failure(), 500);
    }

    #[test]
    fn cooldown_text_is_plain_seconds() {
        assert_eq!(cooldown_text(500), "0.5 秒");
        assert_eq!(cooldown_text(1500), "1.5 秒");
        assert_eq!(cooldown_text(3000), "3 秒");
        assert_eq!(cooldown_text(5000), "5 秒");
    }

    #[test]
    fn view_render_size_caps_at_32m_pixels_keeping_aspect() {
        let (w, h) = compute_view_render_size(6000.0, 8000.0, 4.0, 2.0);
        let px = i64::from(w) * i64::from(h);
        assert!(px <= i64::try_from(MAX_READER_VIEW_PIXELS).unwrap_or(i64::MAX));
        assert!(h > w);
        // The cap is used, not undershot by more than rounding, and the 3:4 aspect survives.
        assert!(px > 31_900_000, "{px}");
        assert!((f64::from(w) / f64::from(h) - 0.75).abs() < 0.001);
    }

    #[test]
    fn a4_page_renders_one_bitmap_pixel_per_screen_pixel_at_normal_dpi() {
        // A4 = 595 x 842 pt; 100 % zoom shows 96 DIP per inch.
        let at_100 = compute_view_layout(595.0, 842.0, 1.0, 1.0);
        assert_eq!((at_100.px_width, at_100.px_height), (793, 1123));
        assert!(!at_100.capped);
        assert!((at_100.dip_width - 793.0).abs() < 1e-9);
        assert!((at_100.dip_height - 1123.0).abs() < 1e-9);

        let at_150 = compute_view_layout(595.0, 842.0, 1.0, 1.5);
        assert_eq!((at_150.px_width, at_150.px_height), (1190, 1684));
        assert!(!at_150.capped);
        assert!((at_150.dip_width - 1190.0 / 1.5).abs() < 1e-9);

        let zoomed = compute_view_layout(595.0, 842.0, 2.0, 2.0);
        assert_eq!((zoomed.px_width, zoomed.px_height), (3173, 4491));
        assert!((zoomed.dip_width - 1586.5).abs() < 1e-9);
    }

    #[test]
    fn capped_render_keeps_the_display_size_matching_the_zoom_label() {
        let layout = compute_view_layout(6000.0, 8000.0, 4.0, 2.0);
        assert!(layout.capped);
        let px = i64::from(layout.px_width) * i64::from(layout.px_height);
        assert!(px <= i64::try_from(MAX_READER_VIEW_PIXELS).unwrap());
        // 6000 pt * (96 / 72) * 400 % = 32000 DIP wide, regardless of the lower bitmap resolution.
        assert!((layout.dip_width - 32_000.0).abs() < 1e-6);
        assert!((layout.dip_height - 42_666.666_666_666_664).abs() < 1e-6);
        assert!(f64::from(layout.px_width) < layout.dip_width * 2.0);
    }

    #[test]
    fn zoom_keeps_the_viewport_centre_on_the_same_spot() {
        // Viewport 800 wide at offset 600 in an extent of 2000: centre at 1000 = 50 %.
        let ratio = viewport_center_ratio(600.0, 800.0, 2000.0, 0.5);
        assert!((ratio - 0.5).abs() < 1e-12);
        // After zooming the extent to 3000 the centre must be at 1500: offset 1100.
        assert!((centered_offset(ratio, 3000.0, 800.0) - 1100.0).abs() < 1e-9);
        // Clamped at both ends and harmless when the content fits the viewport.
        assert!(centered_offset(0.0, 3000.0, 800.0).abs() < 1e-12);
        assert!((centered_offset(1.0, 3000.0, 800.0) - 2200.0).abs() < 1e-9);
        assert!(centered_offset(0.5, 500.0, 800.0).abs() < 1e-12);
        assert!((viewport_center_ratio(0.0, 800.0, 0.0, 0.25) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn page_input_is_clamped_and_invalid_text_keeps_the_current_page() {
        assert_eq!(resolve_page_input("3", 0, 10), 2);
        assert_eq!(resolve_page_input(" 10 ", 0, 10), 9);
        assert_eq!(resolve_page_input("0", 4, 10), 0);
        assert_eq!(resolve_page_input("999", 4, 10), 9);
        assert_eq!(resolve_page_input("99999999999999999999", 4, 10), 4);
        for bad in ["", "  ", "abc", "-2", "3.5", "+4", "2a"] {
            assert_eq!(resolve_page_input(bad, 4, 10), 4, "{bad:?}");
        }
        assert_eq!(resolve_page_input("1", 0, 0), 0);
    }

    #[test]
    fn wheel_turns_pages_only_at_the_edge_and_after_a_pause() {
        // Middle of the page: ordinary scrolling in both directions.
        assert_eq!(
            wheel_paging_action(-120, 300.0, 900.0, 2, 5, true),
            WheelAction::Scroll
        );
        assert_eq!(
            wheel_paging_action(120, 300.0, 900.0, 2, 5, true),
            WheelAction::Scroll
        );
        // Bottom edge, scrolling down.
        assert_eq!(
            wheel_paging_action(-120, 899.7, 900.0, 2, 5, true),
            WheelAction::NextPage
        );
        assert_eq!(
            wheel_paging_action(-120, 900.0, 900.0, 2, 5, false),
            WheelAction::Swallow
        );
        assert_eq!(
            wheel_paging_action(-120, 900.0, 900.0, 4, 5, true),
            WheelAction::Scroll
        );
        // Top edge, scrolling up.
        assert_eq!(
            wheel_paging_action(120, 0.2, 900.0, 2, 5, true),
            WheelAction::PrevPageAtBottom
        );
        assert_eq!(
            wheel_paging_action(120, 0.0, 900.0, 2, 5, false),
            WheelAction::Swallow
        );
        assert_eq!(
            wheel_paging_action(120, 0.0, 900.0, 0, 5, true),
            WheelAction::Scroll
        );
        // A page that fits the viewport is at both edges at once.
        assert_eq!(
            wheel_paging_action(-120, 0.0, 0.0, 0, 2, true),
            WheelAction::NextPage
        );
        assert_eq!(
            wheel_paging_action(120, 0.0, 0.0, 1, 2, true),
            WheelAction::PrevPageAtBottom
        );
    }

    #[test]
    fn shortcut_filter_blocks_exfil_keys_unless_text_input_focused() {
        assert_eq!(
            classify_shortcut(vk::PRINT_SCREEN, false, true),
            ShortcutAction::Block
        );
        assert_eq!(
            classify_shortcut(vk::KEY_C, true, false),
            ShortcutAction::Block
        );
        assert_eq!(
            classify_shortcut(vk::KEY_S, true, false),
            ShortcutAction::Block
        );
        assert_eq!(
            classify_shortcut(vk::KEY_E, true, false),
            ShortcutAction::Block
        );
        assert_eq!(
            classify_shortcut(vk::KEY_C, true, true),
            ShortcutAction::Allow
        );
        assert_eq!(
            classify_shortcut(vk::KEY_P, true, false),
            ShortcutAction::Print
        );
    }

    #[test]
    fn shortcut_filter_covers_every_mapped_key_and_modifier_combination() {
        use ShortcutAction::{Allow, Block, NextPage, PrevPage, Print, ZoomIn, ZoomOut, ZoomReset};
        // Print Screen is blocked in every state.
        for (ctrl, text) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(classify_shortcut(vk::PRINT_SCREEN, ctrl, text), Block);
        }
        // Copy / cut / save / select-all / paste family without a text box: blocked.
        for key in [
            vk::KEY_C,
            vk::KEY_X,
            vk::KEY_S,
            vk::KEY_A,
            vk::KEY_E,
            vk::KEY_V,
        ] {
            assert_eq!(classify_shortcut(key, true, false), Block, "ctrl+{key:#x}");
            assert_eq!(
                classify_shortcut(key, true, true),
                Allow,
                "ctrl+{key:#x} in text"
            );
            assert_eq!(
                classify_shortcut(key, false, false),
                Allow,
                "plain {key:#x}"
            );
        }
        // Ctrl+P prints even while the page box has the focus; plain P is just a letter.
        assert_eq!(classify_shortcut(vk::KEY_P, true, true), Print);
        assert_eq!(classify_shortcut(vk::KEY_P, false, false), Allow);
        // Zoom on both the main keyboard row and the numeric pad.
        for key in [vk::ADD, vk::OEM_PLUS] {
            assert_eq!(classify_shortcut(key, true, false), ZoomIn);
            assert_eq!(classify_shortcut(key, false, false), Allow);
        }
        for key in [vk::SUBTRACT, vk::OEM_MINUS] {
            assert_eq!(classify_shortcut(key, true, false), ZoomOut);
            assert_eq!(classify_shortcut(key, false, false), Allow);
        }
        assert_eq!(classify_shortcut(vk::KEY_0, true, false), ZoomReset);
        assert_eq!(classify_shortcut(vk::KEY_0, false, false), Allow);
        // Paging keys; inside a text box the arrows move the caret instead.
        for key in [vk::LEFT, vk::PAGE_UP] {
            assert_eq!(classify_shortcut(key, false, false), PrevPage);
            assert_eq!(classify_shortcut(key, false, true), Allow);
        }
        for key in [vk::RIGHT, vk::PAGE_DOWN] {
            assert_eq!(classify_shortcut(key, false, false), NextPage);
            assert_eq!(classify_shortcut(key, false, true), Allow);
        }
        // Up / Down scroll the page and Enter is left to the focused control.
        for key in [vk::UP, vk::DOWN, vk::ENTER] {
            assert_eq!(classify_shortcut(key, false, false), Allow);
        }
    }

    #[test]
    fn housekeeping_checks_monotonic_expiry_and_4th_tick_reverify() {
        let start = 638_000_000_000_000_000_i64;
        let exp = start + 120 * 10_000_000;
        assert_eq!(
            evaluate_housekeeping(start, 30, start + 30 * 10_000_000, Some(exp), 4),
            HousekeepingVerdict::Healthy {
                reverify_store: true
            }
        );
        assert_eq!(
            evaluate_housekeeping(start, 125, start + 125 * 10_000_000, Some(exp), 5),
            HousekeepingVerdict::Expired
        );
    }

    #[test]
    fn housekeeping_boundaries_no_expiry_exact_expiry_and_clock_rollback() {
        const SEC: i64 = 10_000_000;
        let start = 638_000_000_000_000_000_i64;
        let healthy = |reverify_store| HousekeepingVerdict::Healthy { reverify_store };

        // No expiry: never expires, however long the document stays open.
        let ten_years = 10 * 365 * 24 * 3600_u64;
        assert_eq!(
            evaluate_housekeeping(start, ten_years, i64::MAX, None, 1),
            healthy(false)
        );

        let exp = start + 60 * SEC;
        // One second before expiry on both clocks: still readable.
        assert_eq!(
            evaluate_housekeeping(start, 59, start + 59 * SEC, Some(exp), 1),
            healthy(false)
        );
        // Wall clock one tick (100 ns) before expiry: still readable.
        assert_eq!(
            evaluate_housekeeping(start, 59, exp - 1, Some(exp), 1),
            healthy(false)
        );
        // Exactly at expiry on the wall clock: expired (same `>=` rule as the policy check).
        assert_eq!(
            evaluate_housekeeping(start, 59, exp, Some(exp), 1),
            HousekeepingVerdict::Expired
        );
        // Exactly at expiry on the monotonic clock while the wall clock was set back an hour.
        assert_eq!(
            evaluate_housekeeping(start, 60, start - 3600 * SEC, Some(exp), 1),
            HousekeepingVerdict::Expired
        );
        // Wall clock jumped forward past the expiry although little time really passed.
        assert_eq!(
            evaluate_housekeeping(start, 5, exp + SEC, Some(exp), 1),
            HousekeepingVerdict::Expired
        );
        // Already expired when opened.
        assert_eq!(
            evaluate_housekeeping(start, 0, start, Some(start), 0),
            HousekeepingVerdict::Expired
        );

        // Full re-verification on every fourth tick only, never on tick 0.
        for (tick, expected) in [
            (0, false),
            (1, false),
            (3, false),
            (4, true),
            (5, false),
            (8, true),
        ] {
            assert_eq!(
                evaluate_housekeeping(start, 1, start + SEC, Some(exp), tick),
                healthy(expected),
                "tick {tick}"
            );
        }

        assert_eq!(millis_until_expiry(start, exp), 60_000);
        assert_eq!(millis_until_expiry(exp, exp), 0);
        assert_eq!(millis_until_expiry(exp + SEC, exp), 0);
    }

    #[test]
    fn print_page_selection_supports_all_current_and_custom_ranges() {
        assert_eq!(
            resolve_print_pages(PageSelectionMode::All, "ignored", 2, 4).unwrap(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            resolve_print_pages(PageSelectionMode::Current, "", 2, 4).unwrap(),
            vec![2]
        );
        assert_eq!(
            resolve_print_pages(PageSelectionMode::Custom, "1-3,5", 0, 10).unwrap(),
            vec![0, 1, 2, 4]
        );
        // "Print page 3" yields exactly one page.
        assert_eq!(
            resolve_print_pages(PageSelectionMode::Custom, "3", 0, 10).unwrap(),
            vec![2]
        );
        for bad in ["", "0", "11", "4-2", "a-b", "1,,x"] {
            assert!(
                matches!(
                    resolve_print_pages(PageSelectionMode::Custom, bad, 0, 10),
                    Err(CoreError::InvalidArgument(_))
                ),
                "{bad:?}"
            );
        }
        assert!(resolve_print_pages(PageSelectionMode::Current, "", 4, 4).is_err());
        assert!(resolve_print_pages(PageSelectionMode::Current, "", -1, 4).is_err());
        assert!(resolve_print_pages(PageSelectionMode::All, "", 0, 0).is_err());

        for mode in [
            PageSelectionMode::All,
            PageSelectionMode::Current,
            PageSelectionMode::Custom,
        ] {
            assert_eq!(PageSelectionMode::from_index(mode.index()), mode);
        }
        assert_eq!(PageSelectionMode::from_index(-1), PageSelectionMode::All);
    }

    #[test]
    fn status_texts_show_remaining_of_total_expiry_and_trace_identity() {
        assert_eq!(quota_status_text(3, 2, None), "剩余打印次数: 2 / 3");
        assert_eq!(
            quota_status_text(3, 0, Some("2026-11-16 18:30:00 (UTC+08:00)")),
            "剩余打印次数: 0 / 3  |  有效期至: 2026-11-16 18:30:00 (UTC+08:00)"
        );
        assert_eq!(
            quota_status_text(0, 0, Some("2026-11-16 18:30:00 (UTC+08:00)")),
            "打印策略: 发送方已禁止打印 (0 次)  |  有效期至: 2026-11-16 18:30:00 (UTC+08:00)"
        );
        assert_eq!(quota_status_text(3, -1, None), "剩余打印次数: 0 / 3");

        assert_eq!(print_button_text(0, 0), "禁止打印");
        assert_eq!(print_button_text(3, 1), "安全打印");
        assert_eq!(print_button_text(3, 0), "打印次数已用尽");

        assert_eq!(
            trace_status_text("YVN8-2N88-7RN5", "192.168.1.20"),
            "设备码: YVN8-2N88-7RN5  |  IP: 192.168.1.20  |  禁止复制与另存"
        );
        assert_eq!(
            file_summary_text("75f1d8efa4054c07984d8e7fa2060d7a", 325),
            "文档 ID: 75F1D8EFA405  ·  大小: 0.3 KB"
        );
        assert_eq!(
            file_summary_text(
                "75f1d8efa4054c07984d8e7fa2060d7a",
                5 * 1024 * 1024 + 512 * 1024
            ),
            "文档 ID: 75F1D8EFA405  ·  大小: 5.5 MB"
        );
    }

    #[test]
    fn protection_status_names_the_level_actually_in_effect() {
        let excluded = protection_status_text(ProtectionState::ExcludedFromCapture);
        let black = protection_status_text(ProtectionState::BlackInCapture);
        let debug = protection_status_text(ProtectionState::DebugDisabled);
        assert!(excluded.contains("不会出现在截图和录屏中"));
        assert!(black.contains("显示为黑色"));
        assert!(debug.contains("已关闭"));
        for text in [excluded, black, debug] {
            assert!(!text.contains("硬件"), "{text}");
        }
    }

    // ---------------------------------------------------------------- overlay

    const TRACE_IP: &str = "192.168.31.207";
    const TRACE_UNIX: u32 = 1_794_825_045;
    const TRACE_TEXT: &str = "张三专用 | IP:192.168.31.207 | 26-11-16 18:30:45 | YVN8-2N88-7RN5";

    fn spec(hidden: bool, visible: bool, moire: MoireLevel, dpi_scale: f64) -> OverlaySpec {
        OverlaySpec {
            hidden_watermark: hidden,
            visible_watermark: visible,
            moire_level: moire,
            moire_phase: 0,
            watermark_text: TRACE_TEXT.to_owned(),
            ipv4: TRACE_IP.to_owned(),
            unix_seconds: TRACE_UNIX,
            dpi_scale,
        }
    }

    fn dims(width: i32, height: i32) -> (usize, usize) {
        (
            usize::try_from(width).unwrap(),
            usize::try_from(height).unwrap(),
        )
    }

    /// White page with grey text-like strokes, as a straight RGBA image.
    fn fake_page(width: i32, height: i32) -> Vec<u8> {
        let (w, h) = dims(width, height);
        let mut page = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let stroke = (y / 11) % 3 == 0 && (x / 9 + y / 11) % 5 != 0;
                let grey = if stroke { 48 } else { 255 };
                page.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        page
    }

    fn capture(width: i32, height: i32, overlay: &[u8], page: &[u8]) -> ForensicsImage {
        ForensicsImage {
            width: u32::try_from(width).unwrap(),
            height: u32::try_from(height).unwrap(),
            rgba: composite_overlay_over_page(page, overlay),
        }
    }

    fn opaque_pixels(overlay: &[u8]) -> usize {
        overlay
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 0)
            .count()
    }

    #[test]
    fn overlay_is_absent_when_no_layer_is_enabled_and_rejects_empty_sizes() {
        assert!(spec(false, false, MoireLevel::Off, 1.0).is_empty());
        assert_eq!(
            render_overlay(640, 480, &spec(false, false, MoireLevel::Off, 1.0)).unwrap(),
            None
        );
        assert!(!spec(true, false, MoireLevel::Off, 1.0).is_empty());
        assert!(render_overlay(0, 480, &spec(true, false, MoireLevel::Off, 1.0)).is_err());
        assert!(render_overlay(640, -1, &spec(false, false, MoireLevel::Low, 1.0)).is_err());
    }

    #[test]
    fn microdots_roundtrip_through_the_reader_overlay_with_every_layer_enabled() {
        // Real page sizes: A4 at 100 % on a 100 % display and on a 200 % display.
        for dpi_scale in [1.0, 2.0] {
            let layout = compute_view_layout(595.0, 842.0, 1.0, dpi_scale);
            let (w, h) = (layout.px_width, layout.px_height);
            let overlay = render_overlay(w, h, &spec(true, true, MoireLevel::High, dpi_scale))
                .unwrap()
                .expect("overlay must be drawn");
            assert_eq!(overlay.len(), dims(w, h).0 * dims(w, h).1 * 4);

            let found = decode_microdots(&capture(w, h, &overlay, &fake_page(w, h)))
                .expect("microdot matrix must decode from the displayed frame");
            assert_eq!(found.payload.ipv4, TRACE_IP.parse::<Ipv4Addr>().unwrap());
            assert_eq!(found.payload.unix_seconds, TRACE_UNIX);
            assert_eq!(found.unit_px, microdot_unit_px(dpi_scale));
        }
    }

    #[test]
    fn hidden_only_overlay_carries_blue_trace_text_and_decodable_microdots_but_no_lattice() {
        let (w, h) = (900, 700);
        let overlay = render_overlay(w, h, &spec(true, false, MoireLevel::Off, 1.0))
            .unwrap()
            .unwrap();
        // Everything drawn is the faint blue trace layer (or its near-white shadow): blue never
        // below red, and never more opaque than text, shadow and a microdot stacked together.
        for px in overlay.as_chunks::<4>().0 {
            assert!(px[0] >= px[2], "blue {} < red {}", px[0], px[2]);
            assert!(px[3] <= 24, "alpha {}", px[3]);
        }
        // The anchor dot of the first microdot tile is present at its fixed position.
        let unit = microdot_unit_px(1.0);
        let (wu, _) = dims(w, h);
        let anchor = (5 * unit * wu + 5 * unit) * 4;
        assert!(overlay[anchor + 3] >= 7 && overlay[anchor] > overlay[anchor + 2]);
        // Both the tiled text and the dots are there, well beyond a handful of pixels.
        assert!(
            opaque_pixels(&overlay) > 12_000,
            "{}",
            opaque_pixels(&overlay)
        );

        let found = decode_microdots(&capture(w, h, &overlay, &fake_page(w, h))).unwrap();
        assert_eq!(found.payload.ipv4, Ipv4Addr::new(192, 168, 31, 207));
        assert_eq!(found.payload.unix_seconds, TRACE_UNIX);

        // On a white page the hidden layer stays near-imperceptible: even where a glyph crosses a
        // microdot the red channel drops by less than 8 %.
        let white = vec![255_u8; dims(w, h).0 * dims(w, h).1 * 4];
        let shown = composite_overlay_over_page(&white, &overlay);
        let darkest = shown.as_chunks::<4>().0.iter().map(|p| p[0]).min().unwrap();
        assert!(darkest >= 235, "hidden layer too visible: {darkest}");
    }

    #[test]
    fn visible_only_overlay_is_legible_on_white_and_contains_no_microdots() {
        let (w, h) = (900, 700);
        let overlay = render_overlay(w, h, &spec(false, true, MoireLevel::Off, 1.0))
            .unwrap()
            .unwrap();
        let white = vec![255_u8; dims(w, h).0 * dims(w, h).1 * 4];
        let shown = capture(w, h, &overlay, &white);
        // Subtle slate at alpha 18 takes a white page down by about 16 levels on glyph pixels.
        let darkest = shown
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[0])
            .min()
            .unwrap();
        assert!(darkest <= 241, "visible watermark too faint: {darkest}");
        let marked = shown
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] <= 247)
            .count();
        assert!(marked > 1_200, "only {marked} clearly marked pixels");
        assert_eq!(decode_microdots(&shown), None);
        // No microdot tile grid: nothing at the anchor position of the first tile is blue.
        let unit = microdot_unit_px(1.0);
        let (wu, _) = dims(w, h);
        let anchor = (5 * unit * wu + 5 * unit) * 4;
        assert!(overlay[anchor] <= overlay[anchor + 2] + 8);
    }

    #[test]
    fn visible_watermark_text_changes_the_rendered_pixels_including_chinese_notes() {
        let (width, height) = (900, 500);
        let with_note = spec(false, true, MoireLevel::Off, 1.0);
        let mut other_note = with_note.clone();
        other_note.watermark_text = TRACE_TEXT.replace("张三专用", "李四专用");
        let mut no_note = with_note.clone();
        no_note.watermark_text = TRACE_TEXT.replace("张三专用 | ", "");

        let zhang = render_overlay(width, height, &with_note).unwrap().unwrap();
        let li = render_overlay(width, height, &other_note).unwrap().unwrap();
        let bare = render_overlay(width, height, &no_note).unwrap().unwrap();
        // Two different Chinese notes of the same length must not render identically (they
        // would if the note were dropped or drawn as missing-glyph boxes).
        assert_ne!(zhang, li);
        assert_ne!(zhang, bare);
        assert_ne!(li, bare);
    }

    #[test]
    fn overlay_text_scales_with_the_display_scale() {
        let small = render_overlay(800, 600, &spec(false, true, MoireLevel::Off, 1.0))
            .unwrap()
            .unwrap();
        let large = render_overlay(1600, 1200, &spec(false, true, MoireLevel::Off, 2.0))
            .unwrap()
            .unwrap();
        // Twice the scale on twice the canvas: about four times the inked pixels.
        let ratio = f64::from(u32::try_from(opaque_pixels(&large)).unwrap())
            / f64::from(u32::try_from(opaque_pixels(&small)).unwrap());
        assert!((3.0..=5.0).contains(&ratio), "ink ratio {ratio}");
    }

    #[test]
    fn moire_only_overlay_is_exactly_the_lattice_and_carries_no_trace_data() {
        // Medium at 100 %: 5 px tiles, each with a 5 px diagonal, a 4 px anti-diagonal and a
        // 2 px light half-diagonal (its third pixel lies on the dark diagonal).
        assert_eq!(moire_tile_px(MoireLevel::Low, 1.0), Some(6));
        assert_eq!(moire_tile_px(MoireLevel::Medium, 1.0), Some(5));
        assert_eq!(moire_tile_px(MoireLevel::High, 1.0), Some(4));
        assert_eq!(moire_tile_px(MoireLevel::High, 2.0), Some(8));
        assert_eq!(moire_tile_px(MoireLevel::Off, 1.0), None);

        let (w, h) = (100, 100);
        let overlay = render_overlay(w, h, &spec(false, false, MoireLevel::Medium, 1.0))
            .unwrap()
            .unwrap();
        let dark = [2_u8, 1, 1, 11]; // premultiplied (42, 30, 24) at alpha 11
        let light = [8_u8, 8, 8, 8]; // premultiplied (252, 250, 248) at alpha 8
        let mut dark_count = 0;
        let mut light_count = 0;
        for px in overlay.as_chunks::<4>().0 {
            if *px == dark {
                dark_count += 1;
            } else if *px == light {
                light_count += 1;
            } else {
                assert_eq!(*px, [0, 0, 0, 0], "unexpected overlay pixel {px:?}");
            }
        }
        assert_eq!(dark_count, 20 * 20 * 9);
        assert_eq!(light_count, 20 * 20 * 2);

        let large = render_overlay(900, 700, &spec(false, false, MoireLevel::High, 1.0))
            .unwrap()
            .unwrap();
        assert_eq!(
            decode_microdots(&capture(900, 700, &large, &fake_page(900, 700))),
            None
        );
    }

    #[test]
    fn moire_strength_and_phase_change_the_lattice() {
        let render = |level, phase| {
            let mut s = spec(false, false, level, 1.0);
            s.moire_phase = phase;
            render_overlay(120, 120, &s).unwrap().unwrap()
        };
        let low = render(MoireLevel::Low, 0);
        let medium = render(MoireLevel::Medium, 0);
        let high = render(MoireLevel::High, 0);
        // Stronger levels are denser and more opaque.
        assert!(opaque_pixels(&high) > opaque_pixels(&medium));
        assert!(opaque_pixels(&medium) > opaque_pixels(&low));
        let max_alpha = |o: &[u8]| o.as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap();
        assert_eq!(
            (max_alpha(&low), max_alpha(&medium), max_alpha(&high)),
            (7, 11, 18)
        );

        // The phase moves the lattice diagonally by round(0.7 * phase) pixels and wraps.
        let phase1 = render(MoireLevel::Medium, 1);
        assert_ne!(medium, phase1);
        let at = |o: &[u8], x: usize, y: usize| -> [u8; 4] {
            let i = (y * 120 + x) * 4;
            [o[i], o[i + 1], o[i + 2], o[i + 3]]
        };
        for (x, y) in [(10, 10), (11, 14), (37, 52), (64, 3), (118, 118)] {
            assert_eq!(at(&phase1, x, y), at(&medium, x - 1, y - 1), "({x},{y})");
        }
        assert_eq!(render(MoireLevel::Medium, MOIRE_PHASE_STEPS), medium);
    }

    /// Decodes the microdot matrix from a captured frame of the reader window
    /// (`SECUREPDF_CAPTURE_PNG`), as produced by a debug build with capture protection off.
    #[test]
    #[ignore = "diagnostic; needs a captured frame, see SECUREPDF_CAPTURE_PNG"]
    fn decode_captured_reader_frame() {
        let path = std::env::var_os("SECUREPDF_CAPTURE_PNG").unwrap();
        let image =
            secure_pdf_core::forensics::load_image_upright(std::path::Path::new(&path)).unwrap();
        println!("CAPTURE_SIZE={}x{}", image.width, image.height);
        let found = decode_microdots(&image).expect("no microdot matrix found in the capture");
        println!(
            "DECODED ip={} unix={} local={} unit_px={} signal={:.1}",
            found.payload.ipv4,
            found.payload.unix_seconds,
            secure_pdf_core::localtime::format_unix_seconds_local(found.payload.unix_seconds),
            found.unit_px,
            found.signal
        );
        println!(
            "NOW unix={} primary_ip={}",
            secure_pdf_core::localtime::unix_seconds_now(),
            secure_pdf_core::licensing::primary_ipv4()
        );
    }

    #[test]
    fn microdot_tiles_repeat_across_the_page() {
        let (w, h) = (1_000, 600);
        let overlay = render_overlay(w, h, &spec(true, false, MoireLevel::Off, 1.0))
            .unwrap()
            .unwrap();
        let unit = microdot_unit_px(1.0);
        let (wu, hu) = dims(w, h);
        let tile_w = MICRODOT_TILE_W * unit;
        let tile_h = MICRODOT_TILE_H * unit;
        let mut tiles = 0;
        for ty in (0..hu - 12 * unit).step_by(tile_h) {
            for tx in (0..wu - 12 * unit).step_by(tile_w) {
                // Centre of the first anchor dot in every tile.
                let idx = ((ty + 6 * unit) * wu + tx + 6 * unit) * 4;
                assert!(
                    overlay[idx + 3] >= 7,
                    "anchor missing in tile at ({tx},{ty})"
                );
                tiles += 1;
            }
        }
        assert_eq!(tiles, 4 * 3);
    }
}
