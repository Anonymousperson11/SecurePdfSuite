//! Trace watermark primitives shared by the reader overlay and the forensics workbench:
//! system-font text layers (visible / hidden) and the decodable microdot trace matrix.

use std::ffi::c_void;
use std::net::Ipv4Addr;
use std::ptr::null_mut;

use crate::error::{CoreError, CoreResult};
use crate::forensics::ForensicsImage;
use crate::win32_util::to_wide_null;

// ---------------------------------------------------------------------------------------------
// Watermark text
// ---------------------------------------------------------------------------------------------

/// Builds the on-screen trace label: optional note, IP, local time and machine code
/// (`"{note} | IP:{ip} | {time} | {code}"`, identical to v1.x).
#[must_use]
pub fn build_screen_watermark_text(
    watermark_note: &str,
    ipv4: &str,
    local_time: &str,
    machine_code: &str,
) -> String {
    let stamp = format!("IP:{ipv4} | {local_time} | {machine_code}");
    let note = watermark_note.trim();
    if note.is_empty() {
        stamp
    } else {
        format!("{note} | {stamp}")
    }
}

/// Appearance of one tiled, rotated text layer.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayerStyle {
    /// Font pixel height.
    pub font_px: i32,
    /// Straight (non-premultiplied) text colour as `[r, g, b]`.
    pub color_rgb: [u8; 3],
    /// Text opacity `0..=255`.
    pub alpha: u8,
    /// Straight colour of the one-pixel offset shadow that keeps text legible on dark pages.
    pub shadow_rgb: [u8; 3],
    /// Shadow opacity `0..=255` (`0` disables the shadow).
    pub shadow_alpha: u8,
    /// Shadow offset in pixels (both axes).
    pub shadow_offset_px: f32,
    /// Minimum horizontal distance between repeats, in pixels.
    pub min_step_x_px: f32,
    /// Horizontal gap added to the measured text width, in pixels.
    pub gap_x_px: f32,
    /// Vertical distance between rows, in pixels.
    pub step_y_px: f32,
    /// Rotation in degrees (negative = text rises towards the right).
    pub angle_deg: f32,
}

impl TextLayerStyle {
    /// Visible watermark: dark slate `ARGB(34, 15, 23, 42)` with a light shadow, 16 DIP bold.
    #[must_use]
    pub fn visible(dpi_scale: f64) -> Self {
        let s = scale_factor(dpi_scale);
        Self {
            font_px: scaled_px(16.0, s),
            color_rgb: [15, 23, 42],
            alpha: 34,
            shadow_rgb: [248, 250, 252],
            shadow_alpha: 18,
            shadow_offset_px: s,
            min_step_x_px: 310.0 * s,
            gap_x_px: 110.0 * s,
            step_y_px: 170.0 * s,
            angle_deg: -26.0,
        }
    }

    /// Hidden trace watermark: low-alpha blue `ARGB(18, 0, 70, 245)`; barely changes luminance
    /// but leaves a `(B - R)` chroma signal that survives re-photographing and JPEG compression.
    #[must_use]
    pub fn hidden(dpi_scale: f64) -> Self {
        let s = scale_factor(dpi_scale);
        Self {
            font_px: scaled_px(18.0, s),
            color_rgb: [0, 70, 245],
            alpha: 18,
            shadow_rgb: [240, 246, 255],
            shadow_alpha: 10,
            shadow_offset_px: s,
            min_step_x_px: 310.0 * s,
            gap_x_px: 75.0 * s,
            step_y_px: 108.0 * s,
            angle_deg: -26.0,
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
const fn scale_factor(dpi_scale: f64) -> f32 {
    if dpi_scale.is_finite() {
        dpi_scale.clamp(0.5, 8.0) as f32
    } else {
        1.0
    }
}

#[allow(clippy::cast_possible_truncation)]
fn scaled_px(dip: f32, scale: f32) -> i32 {
    ((dip * scale).round() as i32).max(8)
}

/// Anti-aliased coverage mask of one line of text rendered with the system UI font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMask {
    /// Mask width in pixels.
    pub width: usize,
    /// Mask height in pixels.
    pub height: usize,
    /// Row-major coverage values (`0` = empty, `255` = fully covered).
    pub coverage: Vec<u8>,
}

const MAX_TEXT_MASK_WIDTH: i32 = 16_384;
const MAX_TEXT_UTF16_UNITS: usize = 512;

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod gdi {
    use core::ffi::c_void;
    use windows_core::{BOOL, PCWSTR};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Size {
        pub cx: i32,
        pub cy: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct BitmapInfoHeader {
        pub size: u32,
        pub width: i32,
        pub height: i32,
        pub planes: u16,
        pub bit_count: u16,
        pub compression: u32,
        pub size_image: u32,
        pub x_pels_per_meter: i32,
        pub y_pels_per_meter: i32,
        pub clr_used: u32,
        pub clr_important: u32,
    }

    windows_core::link!("gdi32.dll" "system" fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void);
    windows_core::link!("gdi32.dll" "system" fn DeleteDC(hdc: *mut c_void) -> BOOL);
    windows_core::link!("gdi32.dll" "system" fn DeleteObject(ho: *mut c_void) -> BOOL);
    windows_core::link!("gdi32.dll" "system" fn SelectObject(hdc: *mut c_void, h: *mut c_void) -> *mut c_void);
    windows_core::link!("gdi32.dll" "system" fn CreateFontW(
        cheight: i32, cwidth: i32, cescapement: i32, corientation: i32, cweight: i32,
        bitalic: u32, bunderline: u32, bstrikeout: u32, icharset: u32, ioutprecision: u32,
        iclipprecision: u32, iquality: u32, ipitchandfamily: u32, pszfacename: PCWSTR,
    ) -> *mut c_void);
    windows_core::link!("gdi32.dll" "system" fn GetTextExtentPoint32W(
        hdc: *mut c_void, lpstring: *const u16, c: i32, psizl: *mut Size,
    ) -> BOOL);
    windows_core::link!("gdi32.dll" "system" fn CreateDIBSection(
        hdc: *mut c_void, pbmi: *const BitmapInfoHeader, usage: u32,
        ppvbits: *mut *mut c_void, hsection: *mut c_void, offset: u32,
    ) -> *mut c_void);
    windows_core::link!("gdi32.dll" "system" fn SetBkMode(hdc: *mut c_void, mode: i32) -> i32);
    windows_core::link!("gdi32.dll" "system" fn SetTextColor(hdc: *mut c_void, color: u32) -> u32);
    windows_core::link!("gdi32.dll" "system" fn TextOutW(
        hdc: *mut c_void, x: i32, y: i32, lpstring: *const u16, c: i32,
    ) -> BOOL);
    windows_core::link!("gdi32.dll" "system" fn GdiFlush() -> BOOL);
}

struct MemoryDc(*mut c_void);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Releases the memory DC created by `CreateCompatibleDC`.
            unsafe {
                let _ = gdi::DeleteDC(self.0);
            }
        }
    }
}

struct GdiObject(*mut c_void);

impl Drop for GdiObject {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Deletes a GDI object owned by this guard that is no longer selected.
            unsafe {
                let _ = gdi::DeleteObject(self.0);
            }
        }
    }
}

/// Restores the previously selected object when dropped, so owned objects can be deleted.
struct Selection {
    dc: *mut c_void,
    previous: *mut c_void,
}

impl Selection {
    fn select(dc: *mut c_void, object: *mut c_void) -> Self {
        // SAFETY: Selects a live GDI object into a live memory DC.
        let previous = unsafe { gdi::SelectObject(dc, object) };
        Self { dc, previous }
    }
}

impl Drop for Selection {
    fn drop(&mut self) {
        if !self.previous.is_null() {
            // SAFETY: Re-selects the object that was selected before this guard was created.
            unsafe {
                let _ = gdi::SelectObject(self.dc, self.previous);
            }
        }
    }
}

/// Renders `text` once with the bold system UI font (CJK capable) into a coverage mask.
///
/// # Errors
///
/// Returns [`CoreError::Context`] when GDI cannot create the font or bitmap, and
/// [`CoreError::InvalidArgument`] for empty text or a non-positive font size.
pub fn render_text_mask(text: &str, font_px: i32) -> CoreResult<TextMask> {
    if text.is_empty() || font_px <= 0 {
        return Err(CoreError::InvalidArgument(
            "水印文本为空或字号无效。".to_owned(),
        ));
    }
    let gdi_err = |what: &str| CoreError::Context(format!("水印文字渲染失败（{what}）。"));
    let utf16: Vec<u16> = text.encode_utf16().take(MAX_TEXT_UTF16_UNITS).collect();
    let utf16_len = i32::try_from(utf16.len()).map_err(|_| gdi_err("文本过长"))?;

    // SAFETY: A null reference DC creates a memory DC compatible with the screen.
    let dc = MemoryDc(unsafe { gdi::CreateCompatibleDC(null_mut()) });
    if dc.0.is_null() {
        return Err(gdi_err("CreateCompatibleDC"));
    }

    let face = to_wide_null("Microsoft YaHei UI");
    // SAFETY: Creates a bold, anti-aliased TrueType font; `face` is NUL-terminated.
    let font = GdiObject(unsafe {
        gdi::CreateFontW(
            -font_px,
            0,
            0,
            0,
            700, // FW_BOLD
            0,
            0,
            0,
            1, // DEFAULT_CHARSET
            4, // OUT_TT_PRECIS
            0, // CLIP_DEFAULT_PRECIS
            4, // ANTIALIASED_QUALITY
            0, // DEFAULT_PITCH
            windows_core::PCWSTR(face.as_ptr()),
        )
    });
    if font.0.is_null() {
        return Err(gdi_err("CreateFontW"));
    }
    let _font_selection = Selection::select(dc.0, font.0);

    let mut extent = gdi::Size::default();
    // SAFETY: `utf16` holds `utf16_len` code units and `extent` is a live local.
    let measured =
        unsafe { gdi::GetTextExtentPoint32W(dc.0, utf16.as_ptr(), utf16_len, &raw mut extent) };
    if !measured.as_bool() || extent.cx <= 0 || extent.cy <= 0 {
        return Err(gdi_err("GetTextExtentPoint32W"));
    }
    let width_px = extent.cx.min(MAX_TEXT_MASK_WIDTH);
    let height_px = extent.cy;

    let header = gdi::BitmapInfoHeader {
        size: 40,
        width: width_px,
        height: -height_px, // top-down rows
        planes: 1,
        bit_count: 32,
        ..gdi::BitmapInfoHeader::default()
    };
    let mut bits: *mut c_void = null_mut();
    // SAFETY: Creates a zero-initialised 32-bpp top-down DIB section; `bits` receives its pixels.
    let bitmap = GdiObject(unsafe {
        gdi::CreateDIBSection(dc.0, &raw const header, 0, &raw mut bits, null_mut(), 0)
    });
    if bitmap.0.is_null() || bits.is_null() {
        return Err(gdi_err("CreateDIBSection"));
    }
    let _bitmap_selection = Selection::select(dc.0, bitmap.0);

    // SAFETY: Transparent background so only glyph pixels are written.
    unsafe {
        let _ = gdi::SetBkMode(dc.0, 1);
    }
    // SAFETY: White text on the black DIB makes each channel equal to glyph coverage.
    unsafe {
        let _ = gdi::SetTextColor(dc.0, 0x00FF_FFFF);
    }
    // SAFETY: Draws `utf16_len` code units at the origin of the selected DIB section.
    let drawn = unsafe { gdi::TextOutW(dc.0, 0, 0, utf16.as_ptr(), utf16_len) };
    // SAFETY: Ensures batched GDI drawing has reached the DIB before it is read.
    unsafe {
        let _ = gdi::GdiFlush();
    }
    if !drawn.as_bool() {
        return Err(gdi_err("TextOutW"));
    }

    let width = usize::try_from(width_px).map_err(|_| gdi_err("尺寸"))?;
    let height = usize::try_from(height_px).map_err(|_| gdi_err("尺寸"))?;
    // SAFETY: The DIB section owns `width * height` BGRA pixels at `bits` while `bitmap` lives.
    let pixels = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), width * height * 4) };
    let coverage = pixels.as_chunks::<4>().0.iter().map(|px| px[1]).collect();
    Ok(TextMask {
        width,
        height,
        coverage,
    })
}

/// Tiles `text` across a premultiplied `BGRA8` buffer using the system font.
///
/// # Errors
///
/// Propagates [`render_text_mask`] failures; returns [`CoreError::InvalidArgument`] when the
/// buffer length does not match `width * height * 4`.
pub fn paint_text_layer(
    bgra: &mut [u8],
    width: usize,
    height: usize,
    text: &str,
    style: &TextLayerStyle,
) -> CoreResult<()> {
    if text.trim().is_empty() || width == 0 || height == 0 {
        return Ok(());
    }
    let mask = render_text_mask(text, style.font_px)?;
    paint_text_mask_layer(bgra, width, height, &mask, style)
}

/// Tiles an already rendered [`TextMask`] across a premultiplied `BGRA8` buffer.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] when the buffer length does not match
/// `width * height * 4`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
pub fn paint_text_mask_layer(
    bgra: &mut [u8],
    width: usize,
    height: usize,
    mask: &TextMask,
    style: &TextLayerStyle,
) -> CoreResult<()> {
    if bgra.len() != width * height * 4 {
        return Err(CoreError::InvalidArgument(
            "水印图层缓冲区尺寸不匹配。".to_owned(),
        ));
    }
    if mask.width == 0 || mask.height == 0 || mask.coverage.len() != mask.width * mask.height {
        return Ok(());
    }

    let (sin_a, cos_a) = style.angle_deg.to_radians().sin_cos();
    let text_w = mask.width as f32;
    let text_h = mask.height as f32;
    let step_x = style.min_step_x_px.max(text_w + style.gap_x_px).max(1.0);
    let step_y = style.step_y_px.max(text_h + 1.0);
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;

    let sample = |tx: f32, ty: f32| -> f32 {
        if tx < 0.0 || ty < 0.0 || tx >= text_w || ty >= text_h {
            return 0.0;
        }
        let x0 = tx.floor() as usize;
        let y0 = ty.floor() as usize;
        let x1 = (x0 + 1).min(mask.width - 1);
        let y1 = (y0 + 1).min(mask.height - 1);
        let fx = tx - tx.floor();
        let fy = ty - ty.floor();
        let at = |x: usize, y: usize| f32::from(mask.coverage[y * mask.width + x]);
        let top = at(x0, y0).mul_add(1.0 - fx, at(x1, y0) * fx);
        let bottom = at(x0, y1).mul_add(1.0 - fx, at(x1, y1) * fx);
        top.mul_add(1.0 - fy, bottom * fy) / 255.0
    };
    // Coverage of the tiled text at unrotated layer coordinates `(u, v)`.
    let tiled = |u: f32, v: f32| -> f32 {
        let row = (v / step_y).floor();
        let ty = row.mul_add(-step_y, v);
        if ty >= text_h {
            return 0.0;
        }
        let row_offset = if (row as i64).rem_euclid(2) == 0 {
            0.0
        } else {
            step_x * 0.45
        };
        let tx = (u - row_offset).rem_euclid(step_x);
        sample(tx, ty)
    };

    let layers = [
        (style.shadow_rgb, style.shadow_alpha, style.shadow_offset_px),
        (style.color_rgb, style.alpha, 0.0),
    ];
    for (y, row) in bgra.chunks_exact_mut(width * 4).enumerate() {
        let dy = y as f32 + 0.5 - center_y;
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let dx = x as f32 + 0.5 - center_x;
            // Inverse of "rotate by angle, then translate to centre".
            let u = dx.mul_add(cos_a, dy * sin_a);
            let v = dy.mul_add(cos_a, -(dx * sin_a));
            for (rgb, alpha, offset) in layers {
                if alpha == 0 {
                    continue;
                }
                let cov = tiled(u - offset, v - offset);
                if cov <= 0.0 {
                    continue;
                }
                let a = cov * f32::from(alpha) / 255.0;
                blend_premultiplied(px, rgb, a);
            }
        }
    }
    Ok(())
}

/// Source-over blend of a straight colour with opacity `a` onto a premultiplied BGRA pixel.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn blend_premultiplied(px: &mut [u8; 4], rgb: [u8; 3], a: f32) {
    let a = a.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    let mix = |dst: u8, src: u8| -> u8 {
        f32::from(dst)
            .mul_add(inv, f32::from(src) * a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    px[0] = mix(px[0], rgb[2]);
    px[1] = mix(px[1], rgb[1]);
    px[2] = mix(px[2], rgb[0]);
    px[3] = f32::from(px[3])
        .mul_add(inv, 255.0 * a)
        .round()
        .clamp(0.0, 255.0) as u8;
}

// ---------------------------------------------------------------------------------------------
// Microdot trace matrix
// ---------------------------------------------------------------------------------------------

/// Decoded microdot trace payload: who displayed the page (IPv4) and when (Unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TracePayload {
    /// Primary IPv4 address of the displaying machine.
    pub ipv4: Ipv4Addr,
    /// Unix time (seconds, 32-bit) at which the frame was drawn.
    pub unix_seconds: u32,
}

/// Number of payload bytes (IPv4 + big-endian Unix seconds), identical to v1.x.
pub const TRACE_PAYLOAD_LEN: usize = 8;

/// Microdot tile width in grid units (one grid unit = half of a v1.x layout unit).
pub const MICRODOT_TILE_W: usize = 144;
/// Microdot tile height in grid units.
pub const MICRODOT_TILE_H: usize = 112;

const ANCHOR_SIZE: usize = 6;
const ANCHORS: [(usize, usize); 2] = [(4, 4), (14, 4)];
const DOT_SIZE: usize = 5;
const DOT_PITCH: usize = 10;
const DOT_ORIGIN: (usize, usize) = (8, 18);
/// Eight payload rows plus one CRC-8 row.
const DOT_ROWS: usize = TRACE_PAYLOAD_LEN + 1;
/// Straight microdot colour `ARGB(14, 0, 40, 220)` (v1.x `MicroDotBrush`).
const MICRODOT_RGB: [u8; 3] = [0, 40, 220];
const MICRODOT_ALPHA: u8 = 14;

/// Packs IPv4 + Unix seconds into the 8-byte trace payload (v1.x `BuildCompactTraceBytes`).
/// An unparsable address is encoded as `0.0.0.0`.
#[must_use]
pub fn encode_trace_payload(ipv4: &str, unix_seconds: u32) -> [u8; TRACE_PAYLOAD_LEN] {
    let ip = ipv4
        .trim()
        .parse::<Ipv4Addr>()
        .unwrap_or(Ipv4Addr::UNSPECIFIED);
    let mut out = [0_u8; TRACE_PAYLOAD_LEN];
    out[..4].copy_from_slice(&ip.octets());
    out[4..].copy_from_slice(&unix_seconds.to_be_bytes());
    out
}

/// Unpacks the 8-byte trace payload.
#[must_use]
pub const fn decode_trace_payload(bytes: &[u8; TRACE_PAYLOAD_LEN]) -> TracePayload {
    TracePayload {
        ipv4: Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]),
        unix_seconds: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
    }
}

/// CRC-8 (polynomial `0x07`, init `0x00`) over the payload; drawn as a ninth dot row so that
/// decoding can tell a genuine matrix from noise.
#[must_use]
pub const fn trace_payload_crc8(bytes: &[u8; TRACE_PAYLOAD_LEN]) -> u8 {
    let mut crc = 0_u8;
    let mut i = 0;
    while i < TRACE_PAYLOAD_LEN {
        crc ^= bytes[i];
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x07
            };
            bit += 1;
        }
        i += 1;
    }
    crc
}

/// Physical pixels per grid unit for a given DPI scale (integer so tiles stay pixel-exact).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn microdot_unit_px(dpi_scale: f64) -> usize {
    let scale = if dpi_scale.is_finite() {
        dpi_scale.clamp(0.5, 8.0)
    } else {
        1.0
    };
    ((1.5 * scale).round() as usize).clamp(1, MAX_MICRODOT_UNIT_PX)
}

/// Largest supported pixels-per-grid-unit (covers DPI scales up to 500 %).
pub const MAX_MICRODOT_UNIT_PX: usize = 8;

/// Rectangles `(x, y, w, h)` in grid units that are "on" for `payload` inside one tile.
fn microdot_rects(payload: [u8; TRACE_PAYLOAD_LEN]) -> Vec<(usize, usize, usize, usize)> {
    let mut rects: Vec<_> = ANCHORS
        .iter()
        .map(|&(x, y)| (x, y, ANCHOR_SIZE, ANCHOR_SIZE))
        .collect();
    let rows = payload
        .iter()
        .copied()
        .chain(std::iter::once(trace_payload_crc8(&payload)));
    for (row, byte) in rows.enumerate() {
        for bit in 0..8 {
            if (byte >> bit) & 1 == 1 {
                rects.push((
                    DOT_ORIGIN.0 + bit * DOT_PITCH,
                    DOT_ORIGIN.1 + row * DOT_PITCH,
                    DOT_SIZE,
                    DOT_SIZE,
                ));
            }
        }
    }
    rects
}

/// Tiles the microdot trace matrix across a premultiplied `BGRA8` buffer.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] when the buffer length does not match
/// `width * height * 4` or `unit_px` is out of range.
pub fn paint_microdots(
    bgra: &mut [u8],
    width: usize,
    height: usize,
    payload: &[u8; TRACE_PAYLOAD_LEN],
    unit_px: usize,
) -> CoreResult<()> {
    if bgra.len() != width * height * 4 || unit_px == 0 || unit_px > MAX_MICRODOT_UNIT_PX {
        return Err(CoreError::InvalidArgument("微点图层参数无效。".to_owned()));
    }
    let tile_w = MICRODOT_TILE_W * unit_px;
    let tile_h = MICRODOT_TILE_H * unit_px;
    let alpha = f32::from(MICRODOT_ALPHA) / 255.0;
    let rects = microdot_rects(*payload);

    for tile_y in (0..height).step_by(tile_h) {
        for tile_x in (0..width).step_by(tile_w) {
            for &(rx, ry, rw, rh) in &rects {
                let x0 = tile_x + rx * unit_px;
                let y0 = tile_y + ry * unit_px;
                let x1 = (x0 + rw * unit_px).min(width);
                let y1 = (y0 + rh * unit_px).min(height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let idx = (y * width + x) * 4;
                        if let Some(px) = bgra.get_mut(idx..idx + 4)
                            && let Ok(px) = <&mut [u8; 4]>::try_from(px)
                        {
                            blend_premultiplied(px, MICRODOT_RGB, alpha);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Result of locating and decoding a microdot matrix in a captured image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicrodotDecode {
    /// Decoded IPv4 + time.
    pub payload: TracePayload,
    /// Pixels per grid unit at which the matrix was found.
    pub unit_px: usize,
    /// Mean `(B - R)` chroma lift of the anchor dots over the empty background (8-bit units).
    pub signal: f32,
}

/// Minimum anchor-over-background chroma lift accepted as a real matrix (8-bit units).
const MIN_MICRODOT_SIGNAL: f32 = 3.0;
/// Number of best-scoring alignments tried per scale.
const ALIGNMENT_CANDIDATES: usize = 12;

/// Locates and decodes the microdot trace matrix in a pixel-exact capture.
///
/// Works on a screenshot (or a crop of one) at its original scale. Re-photographed or resampled
/// images are not supported; use the chroma enhancement modes to read the hidden text watermark
/// in those cases.
#[must_use]
pub fn decode_microdots(image: &ForensicsImage) -> Option<MicrodotDecode> {
    let width = usize::try_from(image.width).ok()?;
    let height = usize::try_from(image.height).ok()?;
    if width == 0 || height == 0 || image.rgba.len() != width * height * 4 {
        return None;
    }
    // Signed chroma signal: grey page content cancels out, the blue dots do not.
    let chroma: Vec<f32> = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| f32::from(px[2]) - f32::from(px[0]))
        .collect();

    let mut best: Option<MicrodotDecode> = None;
    for unit_px in 1..=MAX_MICRODOT_UNIT_PX {
        let tile_w = MICRODOT_TILE_W * unit_px;
        let tile_h = MICRODOT_TILE_H * unit_px;
        if width < tile_w || height < tile_h {
            break;
        }
        let folded = FoldedTile::build(&chroma, width, height, tile_w, tile_h);
        if let Some(found) = folded.decode(unit_px)
            && best.is_none_or(|b| found.signal > b.signal)
        {
            best = Some(found);
        }
    }
    best
}

/// One microdot tile averaged over every repeat in the image, with a wrap-around summed-area
/// table for O(1) rectangle means at any alignment.
struct FoldedTile {
    tile_w: usize,
    tile_h: usize,
    /// Summed-area table over the tile repeated 2x2, `(2*tile_w + 1) * (2*tile_h + 1)` entries.
    sat: Vec<f64>,
}

impl FoldedTile {
    #[allow(clippy::cast_precision_loss)]
    fn build(chroma: &[f32], width: usize, height: usize, tile_w: usize, tile_h: usize) -> Self {
        let mut sum = vec![0.0_f64; tile_w * tile_h];
        let mut count = vec![0_u32; tile_w * tile_h];
        for (y, row) in chroma.chunks_exact(width).enumerate().take(height) {
            let ty = y % tile_h;
            for (x, &value) in row.iter().enumerate() {
                let idx = ty * tile_w + x % tile_w;
                sum[idx] += f64::from(value);
                count[idx] += 1;
            }
        }
        let mean: Vec<f64> = sum
            .iter()
            .zip(&count)
            .map(|(&s, &c)| if c == 0 { 0.0 } else { s / f64::from(c) })
            .collect();

        let sat_w = 2 * tile_w + 1;
        let sat_h = 2 * tile_h + 1;
        let mut sat = vec![0.0_f64; sat_w * sat_h];
        for y in 0..2 * tile_h {
            let mut row_acc = 0.0_f64;
            for x in 0..2 * tile_w {
                row_acc += mean[(y % tile_h) * tile_w + x % tile_w];
                sat[(y + 1) * sat_w + x + 1] = sat[y * sat_w + x + 1] + row_acc;
            }
        }
        Self {
            tile_w,
            tile_h,
            sat,
        }
    }

    /// Mean over the rectangle at pixel `(x, y)` (wrapping) of size `w * h` pixels.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn rect_mean(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let sat_w = 2 * self.tile_w + 1;
        let x0 = x % self.tile_w;
        let y0 = y % self.tile_h;
        let x1 = x0 + w.min(self.tile_w);
        let y1 = y0 + h.min(self.tile_h);
        let at = |px: usize, py: usize| self.sat[py * sat_w + px];
        let total = at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0);
        (total / ((x1 - x0) * (y1 - y0)) as f64) as f32
    }

    /// Mean over a rectangle given in grid units relative to the alignment offset `(ox, oy)`.
    fn unit_rect_mean(
        &self,
        unit_px: usize,
        (ox, oy): (usize, usize),
        (ux, uy, uw, uh): (usize, usize, usize, usize),
    ) -> f32 {
        self.rect_mean(
            ox + ux * unit_px,
            oy + uy * unit_px,
            uw * unit_px,
            uh * unit_px,
        )
    }

    /// Levels of the two anchors and the always-empty regions for an alignment.
    fn levels(&self, unit_px: usize, offset: (usize, usize)) -> (f32, f32) {
        let anchor = ANCHORS
            .iter()
            .map(|&(x, y)| self.unit_rect_mean(unit_px, offset, (x, y, ANCHOR_SIZE, ANCHOR_SIZE)))
            .fold(f32::INFINITY, f32::min);
        // Regions that never contain a dot: right band, strip right of the anchors, left margin.
        let empties = [
            (88, 0, MICRODOT_TILE_W - 88, MICRODOT_TILE_H),
            (24, 0, 64, 14),
            (0, 16, 6, MICRODOT_TILE_H - 16),
        ];
        let empty = empties
            .iter()
            .map(|&rect| self.unit_rect_mean(unit_px, offset, rect))
            .fold(f32::NEG_INFINITY, f32::max);
        (anchor, empty)
    }

    fn decode(&self, unit_px: usize) -> Option<MicrodotDecode> {
        // Rank every alignment by anchor contrast, keeping the best few.
        let mut candidates: Vec<(f32, (usize, usize))> = Vec::new();
        for oy in 0..self.tile_h {
            for ox in 0..self.tile_w {
                let (anchor, empty) = self.levels(unit_px, (ox, oy));
                let score = anchor - empty;
                if score < MIN_MICRODOT_SIGNAL {
                    continue;
                }
                if candidates.len() < ALIGNMENT_CANDIDATES {
                    candidates.push((score, (ox, oy)));
                } else if let Some(worst) = candidates.iter_mut().min_by(|a, b| a.0.total_cmp(&b.0))
                    && score > worst.0
                {
                    *worst = (score, (ox, oy));
                }
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

        for (score, offset) in candidates {
            let (anchor, empty) = self.levels(unit_px, offset);
            let threshold = f32::midpoint(anchor, empty);
            let mut rows = [0_u8; DOT_ROWS];
            for (row, byte) in rows.iter_mut().enumerate() {
                for bit in 0..8 {
                    let level = self.unit_rect_mean(
                        unit_px,
                        offset,
                        (
                            DOT_ORIGIN.0 + bit * DOT_PITCH,
                            DOT_ORIGIN.1 + row * DOT_PITCH,
                            DOT_SIZE,
                            DOT_SIZE,
                        ),
                    );
                    if level > threshold {
                        *byte |= 1 << bit;
                    }
                }
            }
            let mut payload = [0_u8; TRACE_PAYLOAD_LEN];
            payload.copy_from_slice(&rows[..TRACE_PAYLOAD_LEN]);
            if trace_payload_crc8(&payload) == rows[TRACE_PAYLOAD_LEN] && payload != [0; 8] {
                return Some(MicrodotDecode {
                    payload: decode_trace_payload(&payload),
                    unit_px,
                    signal: score,
                });
            }
        }
        None
    }
}

/// Composites a premultiplied `BGRA8` overlay over an opaque straight `RGBA8` page image, as the
/// screen would show it (used by tests and diagnostics to simulate a capture).
#[must_use]
pub fn composite_overlay_over_page(page_rgba: &[u8], overlay_bgra: &[u8]) -> Vec<u8> {
    page_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(overlay_bgra.as_chunks::<4>().0)
        .flat_map(|(page, over)| {
            let inv = 255 - u16::from(over[3]);
            let mix = |dst: u8, src_premul: u8| -> u8 {
                let value = u16::from(src_premul) + (u16::from(dst) * inv + 127) / 255;
                u8::try_from(value.min(255)).unwrap_or(u8::MAX)
            };
            [
                mix(page[0], over[2]),
                mix(page[1], over[1]),
                mix(page[2], over[0]),
                255,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_watermark_text_contains_note_ip_time_and_machine_code() {
        let text = build_screen_watermark_text(
            " 张三专用 ",
            "192.168.1.20",
            "26-11-16 09:30:00",
            "ABCD-EFGH-JKLM",
        );
        assert_eq!(
            text,
            "张三专用 | IP:192.168.1.20 | 26-11-16 09:30:00 | ABCD-EFGH-JKLM"
        );
        let bare =
            build_screen_watermark_text("", "10.0.0.8", "26-11-16 09:30:00", "WXYZ-2345-6789");
        assert_eq!(bare, "IP:10.0.0.8 | 26-11-16 09:30:00 | WXYZ-2345-6789");
    }

    #[test]
    fn styles_scale_with_dpi_and_visible_text_is_dark_enough_to_read() {
        let v1 = TextLayerStyle::visible(1.0);
        let v2 = TextLayerStyle::visible(2.0);
        assert_eq!(v1.font_px, 16);
        assert_eq!(v2.font_px, 32);
        assert!(2.0_f32.mul_add(-v1.step_y_px, v2.step_y_px).abs() < 0.01);
        // Dark slate at alpha 34 (v1.x `VisiblePrimaryBrush`), not a pale tint.
        assert_eq!((v1.color_rgb, v1.alpha), ([15, 23, 42], 34));
        let hidden = TextLayerStyle::hidden(1.5);
        assert_eq!(hidden.font_px, 27);
        assert!(hidden.color_rgb[2] > hidden.color_rgb[0] + 200);
    }

    #[test]
    fn system_font_mask_renders_cjk_text_with_real_glyph_coverage() {
        let latin = render_text_mask("IP:10.0.0.8", 24).unwrap();
        let cjk = render_text_mask("张三专用", 24).unwrap();
        let blank = render_text_mask("    ", 24).unwrap();
        let ink = |m: &TextMask| m.coverage.iter().filter(|&&c| c > 128).count();
        assert!(latin.height >= 24 && cjk.height >= 24);
        // Four CJK glyphs are about four ems wide; a missing-glyph fallback would not be.
        assert!(cjk.width >= 24 * 3, "cjk width {}", cjk.width);
        assert!(ink(&cjk) > 24 * 24 / 2, "cjk ink {}", ink(&cjk));
        assert!(ink(&latin) > 100);
        assert_eq!(ink(&blank), 0);
        // Different CJK strings must produce different glyph shapes (not identical tofu boxes).
        let other = render_text_mask("李四内部", 24).unwrap();
        assert_ne!(cjk.coverage, other.coverage);
        assert!(render_text_mask("", 24).is_err());
    }

    #[test]
    fn visible_layer_darkens_a_white_page_noticeably_and_repeats_across_it() {
        let (w, h) = (900_usize, 600_usize);
        let mut overlay = vec![0_u8; w * h * 4];
        let style = TextLayerStyle::visible(1.0);
        paint_text_layer(
            &mut overlay,
            w,
            h,
            "张三专用 | IP:10.0.0.8 | 26-11-16 09:30:00",
            &style,
        )
        .unwrap();
        let page = vec![255_u8; w * h * 4];
        let shown = composite_overlay_over_page(&page, &overlay);

        // Fully covered glyph pixels drop a white page by alpha * (255 - colour) / 255 ≈ 30 levels.
        let darkest = shown.as_chunks::<4>().0.iter().map(|p| p[0]).min().unwrap();
        assert!(darkest <= 255 - 24, "darkest red channel {darkest}");
        let marked = shown
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] < 245)
            .count();
        assert!(marked > 1_500, "only {marked} visibly marked pixels");
        // Text must appear in all four quadrants (tiled), not once.
        for (qx, qy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let mut hits = 0;
            for y in (qy * h / 2)..((qy + 1) * h / 2) {
                for x in (qx * w / 2)..((qx + 1) * w / 2) {
                    if shown[(y * w + x) * 4] < 245 {
                        hits += 1;
                    }
                }
            }
            assert!(hits > 100, "quadrant ({qx},{qy}) has {hits} marked pixels");
        }
        assert!(paint_text_layer(&mut overlay[..8], w, h, "x", &style).is_err());
    }

    #[test]
    fn trace_payload_roundtrips_and_crc_detects_corruption() {
        let bytes = encode_trace_payload("192.168.31.207", 1_794_825_045);
        assert_eq!(bytes, [192, 168, 31, 207, 0x6A, 0xFA, 0xDB, 0x55]);
        let decoded = decode_trace_payload(&bytes);
        assert_eq!(decoded.ipv4, Ipv4Addr::new(192, 168, 31, 207));
        assert_eq!(decoded.unix_seconds, 1_794_825_045);
        assert_eq!(encode_trace_payload("not-an-ip", 7)[..4], [0, 0, 0, 0]);

        let crc = trace_payload_crc8(&bytes);
        for i in 0..TRACE_PAYLOAD_LEN {
            let mut flipped = bytes;
            flipped[i] ^= 0x10;
            assert_ne!(trace_payload_crc8(&flipped), crc);
        }
        // CRC-8/SMBUS check value for "12345678".
        assert_eq!(trace_payload_crc8(b"12345678"), 0xC7);
    }

    #[test]
    fn microdot_unit_is_integral_and_grows_with_dpi() {
        assert_eq!(microdot_unit_px(1.0), 2);
        assert_eq!(microdot_unit_px(1.5), 2);
        assert_eq!(microdot_unit_px(2.0), 3);
        assert_eq!(microdot_unit_px(3.0), 5);
        assert_eq!(microdot_unit_px(f64::NAN), 2);
        assert_eq!(microdot_unit_px(100.0), MAX_MICRODOT_UNIT_PX);
    }

    /// Deterministic grey "document" with text-like dark strokes and paper noise.
    fn fake_page(w: usize, h: usize) -> Vec<u8> {
        let mut state = 0x2545_F491_u32;
        let mut page = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = u8::try_from(state % 5).unwrap();
                let stroke = (y / 9) % 3 == 0 && (x / 7 + y / 9) % 4 != 0;
                let grey = if stroke { 40 + noise } else { 250 - noise };
                page.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        page
    }

    fn capture_with_overlay(w: usize, h: usize, payload: [u8; 8], unit: usize) -> ForensicsImage {
        let mut overlay = vec![0_u8; w * h * 4];
        // Hidden text first, then microdots — the same order the reader overlay uses.
        paint_text_layer(
            &mut overlay,
            w,
            h,
            "IP:192.168.31.207 | 26-11-16 18:30:45 | YVN8-2N88-7RN5",
            &TextLayerStyle::hidden(1.0),
        )
        .unwrap();
        paint_microdots(&mut overlay, w, h, &payload, unit).unwrap();
        ForensicsImage {
            width: u32::try_from(w).unwrap(),
            height: u32::try_from(h).unwrap(),
            rgba: composite_overlay_over_page(&fake_page(w, h), &overlay),
        }
    }

    fn crop(img: &ForensicsImage, x0: usize, y0: usize, w: usize, h: usize) -> ForensicsImage {
        let src_w = usize::try_from(img.width).unwrap();
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in y0..y0 + h {
            rgba.extend_from_slice(&img.rgba[(y * src_w + x0) * 4..(y * src_w + x0 + w) * 4]);
        }
        ForensicsImage {
            width: u32::try_from(w).unwrap(),
            height: u32::try_from(h).unwrap(),
            rgba,
        }
    }

    #[test]
    fn microdots_roundtrip_encode_render_decode_recovers_ip_and_time() {
        let payload = encode_trace_payload("192.168.31.207", 1_794_825_045);
        for unit in [2_usize, 3] {
            let capture = capture_with_overlay(1_280, 900, payload, unit);
            let found = decode_microdots(&capture).expect("matrix must decode");
            assert_eq!(found.unit_px, unit);
            assert_eq!(found.payload.ipv4, Ipv4Addr::new(192, 168, 31, 207));
            assert_eq!(found.payload.unix_seconds, 1_794_825_045);
            assert!(found.signal >= 8.0, "signal {}", found.signal);
        }
    }

    #[test]
    fn microdots_decode_from_an_arbitrarily_cropped_capture() {
        let payload = encode_trace_payload("10.20.30.40", 0xDEAD_BEEF);
        let capture = capture_with_overlay(1_400, 1_000, payload, 2);
        let cropped = crop(&capture, 137, 61, 1_000, 780);
        let found = decode_microdots(&cropped).expect("cropped matrix must decode");
        assert_eq!(found.payload.ipv4, Ipv4Addr::new(10, 20, 30, 40));
        assert_eq!(found.payload.unix_seconds, 0xDEAD_BEEF);
    }

    #[test]
    fn microdot_decoder_rejects_images_without_a_matrix() {
        let (w, h) = (1_000_usize, 700_usize);
        let plain = ForensicsImage {
            width: 1_000,
            height: 700,
            rgba: fake_page(w, h),
        };
        assert_eq!(decode_microdots(&plain), None);

        // Hidden text watermark alone (no microdots) must not decode to a payload either.
        let mut overlay = vec![0_u8; w * h * 4];
        paint_text_layer(
            &mut overlay,
            w,
            h,
            "IP:192.168.31.207 | 26-11-16 18:30:45 | YVN8-2N88-7RN5",
            &TextLayerStyle::hidden(1.0),
        )
        .unwrap();
        let text_only = ForensicsImage {
            width: 1_000,
            height: 700,
            rgba: composite_overlay_over_page(&fake_page(w, h), &overlay),
        };
        assert_eq!(decode_microdots(&text_only), None);

        let tiny = ForensicsImage {
            width: 10,
            height: 10,
            rgba: vec![255; 400],
        };
        assert_eq!(decode_microdots(&tiny), None);
    }
}
