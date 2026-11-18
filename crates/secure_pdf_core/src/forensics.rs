//! O(W) sliding-window anti-leak watermark forensics enhancement and EXIF orientation normalization.

use std::fs;
use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageFormat, RgbaImage};

use crate::error::{CoreError, CoreResult};

/// Maximum pixel count that takes part in forensics processing (50M px).
///
/// Larger photos (100 MP / 200 MP phone captures) are scaled down proportionally on load; the
/// screen content they show has far less effective resolution than that.
pub const MAX_FORENSICS_PIXELS: usize = 50_000_000;

/// Hard ceiling on the encoded image size that will be decoded at all (250M px).
pub const MAX_DECODE_PIXELS: usize = 250_000_000;

/// Forensics enhancement filter mode matching `SecurePdf.Core.WatermarkEnhancer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnhanceMode {
    /// Isolates yellow-green camera chroma modulation (`3*(B-R) + (B-G)`) inside an `r=18` window.
    #[default]
    ChromaStegoExtract,
    /// Removes smooth paper/screen illumination locally via `r=15` sliding window luminance mean.
    AdaptiveBackgroundSubtract,
    /// Applies global 256-bin luminance histogram equalization followed by contrast inversion.
    GlobalEqualizeAndInvert,
}

/// Decoded or enhanced upright pixel image buffer (`width * height * 4` bytes).
#[derive(Debug, Clone)]
pub struct ForensicsImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Straight `RGBA8` pixels (`width * height * 4`) for generic encoding / PNG save.
    pub rgba: Vec<u8>,
}

impl ForensicsImage {
    /// Converts straight `RGBA8` pixels into premultiplied `BGRA8` bytes for `WinUI 3` `WriteableBitmap`.
    #[must_use]
    pub fn to_bgra8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rgba.len());
        for px in self.rgba.as_chunks::<4>().0 {
            out.extend_from_slice(&rgba_to_premultiplied_bgra(*px));
        }
        out
    }
}

/// Rounded `channel * alpha / 255 + white * (255 - alpha) / 255` style 8-bit blend.
fn blend_channel(channel: u8, alpha: u8, backdrop: u8) -> u8 {
    let a = u16::from(alpha);
    let value = (u16::from(channel) * a + u16::from(backdrop) * (255 - a) + 127) / 255;
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// Converts one straight `RGBA8` pixel into a premultiplied `BGRA8` pixel.
#[must_use]
pub fn rgba_to_premultiplied_bgra(px: [u8; 4]) -> [u8; 4] {
    let [r, g, b, alpha] = px;
    if alpha == 255 {
        return [b, g, r, 255];
    }
    [
        blend_channel(b, alpha, 0),
        blend_channel(g, alpha, 0),
        blend_channel(r, alpha, 0),
        alpha,
    ]
}

/// Colour of a straight `RGBA8` pixel as seen on a white page (transparent areas become white).
#[inline]
fn composite_on_white(px: [u8; 4]) -> [u8; 3] {
    let [r, g, b, alpha] = px;
    if alpha == 255 {
        return [r, g, b];
    }
    [
        blend_channel(r, alpha, 255),
        blend_channel(g, alpha, 255),
        blend_channel(b, alpha, 255),
    ]
}

/// Loads `path` into memory and returns an upright, opaque [`ForensicsImage`].
///
/// Honors EXIF orientation tag `0x0112`, scales images above [`MAX_FORENSICS_PIXELS`] down
/// proportionally and flattens transparency onto white.
///
/// # Errors
///
/// Returns [`CoreError::Forensics`] on invalid images, unsupported formats, or images above
/// [`MAX_DECODE_PIXELS`], and [`CoreError::Io`] when the file cannot be read.
pub fn load_image_upright(path: &Path) -> CoreResult<ForensicsImage> {
    let bytes = fs::read(path)?;
    load_image_bytes_upright(&bytes)
}

/// Decodes raw encoded image bytes (`PNG`, `JPEG`, `HEIC`/`HEIF`, `BMP`, `TIFF`, `GIF`) into an
/// upright, opaque [`ForensicsImage`] (see [`load_image_upright`]).
///
/// # Errors
///
/// Returns [`CoreError::Forensics`] if decoding fails or the image exceeds [`MAX_DECODE_PIXELS`].
pub fn load_image_bytes_upright(bytes: &[u8]) -> CoreResult<ForensicsImage> {
    load_image_bytes_with_budget(bytes, MAX_FORENSICS_PIXELS)
}

/// Returns `true` when `bytes` starts with an ISO Base Media File Format `ftyp` box carrying a
/// HEIF / HEIC brand (`heic`, `heix`, `hevc`, `hevx`, `heim`, `heis`, `hevm`, `hevs`, `mif1`, `msf1`).
#[must_use]
pub fn is_heif_container(bytes: &[u8]) -> bool {
    const BRANDS: &[[u8; 4]] = &[
        *b"heic", *b"heix", *b"hevc", *b"hevx", *b"heim", *b"heis", *b"hevm", *b"hevs", *b"mif1",
        *b"msf1",
    ];
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let box_len = usize::try_from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .unwrap_or(0)
        .min(bytes.len());
    if box_len < 12 {
        return false;
    }
    let is_brand = |four: &[u8; 4]| BRANDS.contains(four);
    if <[u8; 4]>::try_from(&bytes[8..12]).is_ok_and(|four| is_brand(&four)) {
        return true;
    }
    bytes
        .get(16..box_len)
        .is_some_and(|compat| compat.as_chunks::<4>().0.iter().any(is_brand))
}

fn load_heic_bytes_with_budget(bytes: &[u8], max_pixels: usize) -> CoreResult<ForensicsImage> {
    let decode_ceiling = u64::try_from(MAX_DECODE_PIXELS).unwrap_or(u64::MAX);
    let opts = heic_rs::DecodeOptions {
        layout: heic_rs::PixelLayout::Rgba8,
        max_pixels: Some(decode_ceiling),
        apply_transforms: true,
        decode_alpha: true,
        ..heic_rs::DecodeOptions::default()
    };

    let decoded = heic_rs::decode(bytes, &opts).map_err(|err| match err {
        heic_rs::Error::PixelLimit { .. } => {
            CoreError::Forensics("HEIC 图片过大，超出可处理的 2.5 亿像素上限。".to_owned())
        }
        other => CoreError::Forensics(format!("HEIC/HEIF 图片解码失败：{other}")),
    })?;

    let (src_w, src_h) = (decoded.width, decoded.height);
    let raw_img = RgbaImage::from_raw(src_w, src_h, decoded.data).ok_or_else(|| {
        CoreError::Forensics("HEIC 解码后的像素缓冲区长度与尺寸不匹配。".to_owned())
    })?;
    let (tw, th) = fit_within_pixel_budget(src_w, src_h, max_pixels);
    // ISO/IEC 23008-12 `clap`/`imir`/`irot` transforms are already applied by `heic_rs::decode`
    // (applying legacy EXIF orientation on top would rotate iPhone photos a second time).
    let sized = if (tw, th) == (src_w, src_h) {
        raw_img
    } else {
        DynamicImage::ImageRgba8(raw_img)
            .thumbnail_exact(tw, th)
            .into_rgba8()
    };
    let (ow, oh) = sized.dimensions();
    let mut rgba = sized.into_raw();
    for px in rgba.as_chunks_mut::<4>().0 {
        let flat = composite_on_white(*px);
        *px = [flat[0], flat[1], flat[2], 255];
    }
    Ok(ForensicsImage {
        width: ow,
        height: oh,
        rgba,
    })
}

/// Target size for an image that must fit `max_pixels`, keeping the aspect ratio (v1.x rule:
/// scale both sides by `sqrt(max / total)`), never below 1 px and never above the budget.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn fit_within_pixel_budget(width: u32, height: u32, max_pixels: usize) -> (u32, u32) {
    let total = u64::from(width) * u64::from(height);
    let budget = u64::try_from(max_pixels).unwrap_or(u64::MAX).max(1);
    if total <= budget {
        return (width, height);
    }
    let scale = (budget as f64 / total as f64).sqrt();
    let mut tw = ((f64::from(width) * scale).round() as u32).max(1);
    let mut th = ((f64::from(height) * scale).round() as u32).max(1);
    // Rounding both sides up can overshoot the budget by a sliver; trim the longer side.
    while u64::from(tw) * u64::from(th) > budget {
        if tw >= th && tw > 1 {
            tw -= 1;
        } else if th > 1 {
            th -= 1;
        } else {
            break;
        }
    }
    (tw, th)
}

fn load_image_bytes_with_budget(bytes: &[u8], max_pixels: usize) -> CoreResult<ForensicsImage> {
    if is_heif_container(bytes) {
        return load_heic_bytes_with_budget(bytes, max_pixels);
    }
    let orientation = parse_exif_orientation(bytes).unwrap_or(1);
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::Forensics(format!("无法识别图片格式：{e}")))?;

    let (src_w, src_h) = reader
        .into_dimensions()
        .map_err(|e| CoreError::Forensics(format!("无法读取图片尺寸：{e}")))?;
    let total_px = usize::try_from(src_w)
        .ok()
        .and_then(|wi| {
            usize::try_from(src_h)
                .ok()
                .and_then(|hi| wi.checked_mul(hi))
        })
        .ok_or_else(|| CoreError::Forensics("图片尺寸整数溢出。".to_owned()))?;
    if total_px == 0 {
        return Err(CoreError::Forensics(format!(
            "图片尺寸无效（{src_w}x{src_h}）。"
        )));
    }
    if total_px > MAX_DECODE_PIXELS {
        return Err(CoreError::Forensics(format!(
            "图片过大（{src_w}x{src_h}），超出可处理的 2.5 亿像素上限。"
        )));
    }

    let mut image_reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::Forensics(format!("读取图片格式失败：{e}")))?;
    // The pixel count is already bounded above; lift the library's default 512 MiB allocation
    // cap so that legitimate 100-200 MP photos can be decoded before being scaled down.
    let mut limits = image::Limits::default();
    limits.max_alloc = None;
    image_reader.limits(limits);
    let decoded = image_reader
        .decode()
        .map_err(|e| CoreError::Forensics(format!("图片解码失败：{e}")))?;

    let (tw, th) = fit_within_pixel_budget(src_w, src_h, max_pixels);
    let sized = if (tw, th) == (src_w, src_h) {
        decoded
    } else {
        // Area-averaging reduction: linear time and well suited to pure down-scaling.
        decoded.thumbnail_exact(tw, th)
    };

    let oriented = apply_exif_orientation(sized, orientation);
    let (ow, oh) = oriented.dimensions();
    let mut rgba = oriented.into_rgba8().into_raw();
    for px in rgba.as_chunks_mut::<4>().0 {
        let flat = composite_on_white(*px);
        *px = [flat[0], flat[1], flat[2], 255];
    }
    Ok(ForensicsImage {
        width: ow,
        height: oh,
        rgba,
    })
}

/// Applies the requested forensic watermark enhancement algorithm to `src`.
///
/// # Errors
///
/// Returns [`CoreError::Forensics`] if `src` buffer dimensions are inconsistent or exceed 50M px.
pub fn enhance_image(src: &ForensicsImage, mode: EnhanceMode) -> CoreResult<ForensicsImage> {
    let w = usize::try_from(src.width)
        .map_err(|_| CoreError::Forensics("图片宽度溢出。".to_owned()))?;
    let h = usize::try_from(src.height)
        .map_err(|_| CoreError::Forensics("图片高度溢出。".to_owned()))?;
    let count = w
        .checked_mul(h)
        .ok_or_else(|| CoreError::Forensics("图片像素总数溢出。".to_owned()))?;
    if count == 0 || count > MAX_FORENSICS_PIXELS {
        return Err(CoreError::Forensics(
            "图片尺寸非法或超出 50M 像素上限。".to_owned(),
        ));
    }
    if src.rgba.len() != count * 4 {
        return Err(CoreError::Forensics(
            "输入像素缓冲区长度与宽高不匹配。".to_owned(),
        ));
    }

    let out_rgba = match mode {
        EnhanceMode::ChromaStegoExtract => enhance_chroma_watermark(src),
        EnhanceMode::AdaptiveBackgroundSubtract => enhance_adaptive_background(src),
        EnhanceMode::GlobalEqualizeAndInvert => enhance_global_equalize_invert(src),
    };

    Ok(ForensicsImage {
        width: src.width,
        height: src.height,
        rgba: out_rgba,
    })
}

/// Saves `img` cleanly as a lossless PNG file at `path`.
///
/// # Errors
///
/// Returns [`CoreError::Forensics`] or [`CoreError::Io`] if encoding or writing fails.
pub fn save_png(path: &Path, img: &ForensicsImage) -> CoreResult<()> {
    let rgba_buf =
        RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).ok_or_else(|| {
            CoreError::Forensics("图片像素尺寸与缓冲区不一致，无法保存 PNG。".to_owned())
        })?;
    let mut file = fs::File::create(path)?;
    rgba_buf
        .write_to(&mut file, ImageFormat::Png)
        .map_err(|e| CoreError::Forensics(format!("PNG 编码导出失败：{e}")))?;
    Ok(())
}

/// O(W*H) separable box blur repeated `passes` times (3 passes approximate an isotropic Gaussian
/// with zero rectangular halo artifacts).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops
)]
fn blur_box_passes(src: &[f32], w: usize, h: usize, radius: usize, passes: usize) -> Vec<f32> {
    if radius == 0 || passes == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut cur = src.to_vec();
    let mut tmp = vec![0.0_f32; w * h];
    let mut pref = vec![0.0_f64; w.max(h) + 1];

    for _ in 0..passes {
        for y in 0..h {
            let row_off = y * w;
            pref[0] = 0.0;
            for x in 0..w {
                pref[x + 1] = pref[x] + f64::from(cur[row_off + x]);
            }
            for x in 0..w {
                let x0 = x.saturating_sub(radius);
                let x1 = (x + radius).min(w - 1);
                let count = (x1 - x0 + 1) as f64;
                tmp[row_off + x] = ((pref[x1 + 1] - pref[x0]) / count) as f32;
            }
        }
        for x in 0..w {
            pref[0] = 0.0;
            for y in 0..h {
                pref[y + 1] = pref[y] + f64::from(tmp[y * w + x]);
            }
            for y in 0..h {
                let y0 = y.saturating_sub(radius);
                let y1 = (y + radius).min(h - 1);
                let count = (y1 - y0 + 1) as f64;
                cur[y * w + x] = ((pref[y1 + 1] - pref[y0]) / count) as f32;
            }
        }
    }
    cur
}

/// Estimates `(p50, p84, p988)` over `[-16.0, 64.0]` in O(N) time using a 4096-bin histogram.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]
fn signal_percentiles(values: &[f32], weights: &[f32]) -> (f32, f32, f32) {
    const BINS: usize = 4096;
    const MIN_V: f32 = -16.0;
    const SPAN_V: f32 = 80.0;
    const SCALE: f32 = (BINS - 1) as f32 / SPAN_V;

    let mut hist = vec![0_u64; BINS];
    let mut total = 0_u64;
    for (&v, &w) in values.iter().zip(weights) {
        if w > 0.85 {
            let bin = ((v - MIN_V) * SCALE).round().clamp(0.0, (BINS - 1) as f32) as usize;
            hist[bin] += 1;
            total += 1;
        }
    }
    if total == 0 {
        return (0.0, 0.0, 0.0);
    }
    let at = |q: f64| -> f32 {
        let target = ((total as f64) * q).ceil().max(1.0) as u64;
        let mut acc = 0_u64;
        for (idx, &count) in hist.iter().enumerate() {
            acc += count;
            if acc >= target {
                return (idx as f32) / SCALE + MIN_V;
            }
        }
        MIN_V + SPAN_V
    };
    (at(0.50), at(0.84), at(0.988))
}

/// Multi-scale anti-moire watermark isolator for camera photos and screenshots:
/// 1. Suppresses high-frequency LCD RGB subpixel moire with a tight Gaussian pre-filter (`r_inner`).
/// 2. Normalizes blue-excess chromaticity by luminance so screen white-balance and black body ink
///    do not invert or bias local chroma.
/// 3. Inpaints deep dark PDF text/chrome before estimating local paper background (`r_outer`),
///    combining hidden blue chroma and semi-transparent visible watermark contrast onto crisp paper.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::suboptimal_flops
)]
fn enhance_chroma_watermark(src: &ForensicsImage) -> Vec<u8> {
    let w = src.width as usize;
    let h = src.height as usize;
    let n = w * h;
    let longer = w.max(h);

    let mut luma = Vec::with_capacity(n);
    let mut chroma_norm = Vec::with_capacity(n);
    for px in src.rgba.as_chunks::<4>().0 {
        let [r8, g8, b8] = composite_on_white(*px);
        let r = f32::from(r8);
        let g = f32::from(g8);
        let b = f32::from(b8);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        luma.push(y);
        let denom = y.max(48.0);
        chroma_norm.push((3.0 * (b - r) + (b - g)) * (220.0 / denom) * 0.25);
    }

    // Auto-detect high-frequency LCD camera moire energy on non-ink paper pixels so both 1080p/
    // 1440p WeChat photos and 4032p raw HEIC photos smooth subpixel colour fringes without
    // blurring clean 1:1 screenshots or small test images.
    let mut hf_sum = 0.0_f64;
    let mut hf_cnt = 0_u64;
    for y in (0..h).step_by(4) {
        let row = y * w;
        for x in 0..w.saturating_sub(4) {
            if (luma[row + x] - luma[row + x + 3]).abs() < 28.0 && luma[row + x] > 100.0 {
                let c0 = chroma_norm[row + x];
                hf_sum += f64::from(
                    (c0 - chroma_norm[row + x + 2]).abs() + (c0 - chroma_norm[row + x + 3]).abs(),
                );
                hf_cnt += 1;
            }
        }
    }
    let hf_moire = if hf_cnt > 0 {
        (hf_sum / (hf_cnt as f64)) as f32
    } else {
        0.0
    };
    let r_inner = if longer < 256 {
        0
    } else if hf_moire > 2.2 {
        (longer / 780).clamp(3, 5)
    } else {
        (longer / 1_200).min(2)
    };
    let r_halo = (longer / 240).clamp(6, 18);
    let r_outer = (longer / 92).clamp(18, 52);

    let luma_fine = blur_box_passes(&luma, w, h, r_inner, 3);
    let luma_paper = blur_box_passes(&luma_fine, w, h, r_outer, 3);

    let mut raw_ink = vec![0.0_f32; n];
    for i in 0..n {
        let paper_y = luma_paper[i];
        if paper_y < 95.0 {
            raw_ink[i] = 1.0;
        } else {
            let ratio = luma_fine[i] / paper_y;
            if ratio < 0.80 {
                raw_ink[i] = ((0.80 - ratio) / 0.10).clamp(0.0, 1.0);
            }
        }
    }
    let ink_tight = blur_box_passes(&raw_ink, w, h, r_inner + 2, 2);
    let ink_wide = blur_box_passes(&raw_ink, w, h, r_halo, 3);

    let chroma_fine = blur_box_passes(&chroma_norm, w, h, r_inner, 3);
    let chroma_rough_bg = blur_box_passes(&chroma_fine, w, h, r_outer, 2);

    let mut chroma_clean = vec![0.0_f32; n];
    let mut luma_clean = vec![0.0_f32; n];
    let mut chroma_weights = vec![0.0_f32; n];
    let mut luma_weights = vec![0.0_f32; n];
    for i in 0..n {
        let wc = (1.0 - 4.0 * ink_tight[i]).clamp(0.0, 1.0);
        let wl = (1.0 - 12.0 * ink_wide[i]).clamp(0.0, 1.0);
        chroma_weights[i] = wc;
        luma_weights[i] = wl;
        chroma_clean[i] = chroma_fine[i] * wc + chroma_rough_bg[i] * (1.0 - wc);
        luma_clean[i] = luma_fine[i] * wl + luma_paper[i] * (1.0 - wl);
    }
    let chroma_bg = blur_box_passes(&chroma_clean, w, h, r_outer, 3);
    let luma_bg = blur_box_passes(&luma_clean, w, h, r_outer, 3);

    let mut sig = vec![0.0_f32; n];
    for i in 0..n {
        let wc = chroma_weights[i];
        let wl = luma_weights[i];
        let c_band = (chroma_clean[i] - chroma_bg[i]) * wc;
        let paper_y = luma_bg[i].max(48.0);
        let l_drop = ((luma_bg[i] - luma_clean[i]) * (220.0 / paper_y)).max(0.0) * wl;
        sig[i] = c_band + 0.55 * l_drop;
    }

    let (p50, p84, p988) = signal_percentiles(&sig, &chroma_weights);
    let sigma = (p84 - p50).max(0.22);
    let floor = p50 + 0.85 * sigma;
    let ceil = (p50 + 0.88 * (p988 - p50)).max(floor + (2.8 * sigma).max(1.2));
    let span = (ceil - floor).max(1.2);

    let mut out = vec![0xFF_u8; n * 4];
    for i in 0..n {
        let t = ((sig[i] - floor) / span).clamp(0.0, 1.0);
        let dark_curve = t * (2.0 - t);
        let context_ink = if luma_paper[i] >= 95.0 {
            raw_ink[i] * 26.0
        } else {
            16.0
        };
        let mapped = (252.0 - (dark_curve * 236.0).max(context_ink))
            .round()
            .clamp(0.0, 255.0) as u8;
        let dst = i * 4;
        out[dst] = mapped;
        out[dst + 1] = mapped;
        out[dst + 2] = mapped;
    }
    out
}

/// Anti-moire Gaussian local illumination removal rendering dark ink and watermarks on clean paper.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::suboptimal_flops
)]
fn enhance_adaptive_background(src: &ForensicsImage) -> Vec<u8> {
    let w = src.width as usize;
    let h = src.height as usize;
    let n = w * h;
    let longer = w.max(h);

    let mut luma = Vec::with_capacity(n);
    for px in src.rgba.as_chunks::<4>().0 {
        let [r, g, b] = composite_on_white(*px);
        luma.push((f32::from(r) * 77.0 + f32::from(g) * 150.0 + f32::from(b) * 29.0) / 256.0);
    }

    let r_inner = if longer < 256 {
        0
    } else {
        (longer / 780).clamp(2, 5)
    };
    let r_outer = (longer / 95).clamp(15, 56);
    let fine = blur_box_passes(&luma, w, h, r_inner, 3);
    let paper = blur_box_passes(&fine, w, h, r_outer, 3);

    let mut out = vec![0xFF_u8; n * 4];
    for i in 0..n {
        let paper_y = paper[i].max(32.0);
        let diff = (paper_y - fine[i]) * (215.0 / paper_y);
        let mapped = (242.0 - diff * 10.0).round().clamp(0.0, 255.0) as u8;
        let dst = i * 4;
        out[dst] = mapped;
        out[dst + 1] = mapped;
        out[dst + 2] = mapped;
    }
    out
}

/// 8-bit luminance (`77 R + 150 G + 29 B`) of a pixel flattened onto white.
fn luma_on_white(px: [u8; 4]) -> usize {
    let [r, g, b] = composite_on_white(px);
    let luma = (u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8;
    usize::try_from(luma.min(255)).unwrap_or(255)
}

fn enhance_global_equalize_invert(src: &ForensicsImage) -> Vec<u8> {
    let total = (src.width as usize) * (src.height as usize);
    let mut hist = [0_u64; 256];

    for px in src.rgba.as_chunks::<4>().0 {
        hist[luma_on_white(*px)] += 1;
    }

    let mut cdf = [0_u64; 256];
    let mut acc = 0_u64;
    let mut cdf_min = None;
    for i in 0..256 {
        acc += hist[i];
        cdf[i] = acc;
        if hist[i] > 0 && cdf_min.is_none() {
            cdf_min = Some(acc);
        }
    }

    let total_u64 = u64::try_from(total).unwrap_or(1);
    let min_c = cdf_min.unwrap_or(0);
    let denom = total_u64.saturating_sub(min_c);

    let mut lut = [0_u8; 256];
    for i in 0..256 {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let eq = if denom == 0 {
            u8::try_from(i).unwrap_or(0)
        } else {
            (((cdf[i].saturating_sub(min_c)) as f64 / denom as f64) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        lut[i] = 255 - eq;
    }

    let mut out = vec![0xFF_u8; src.rgba.len()];
    for (dst, px) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.rgba.as_chunks::<4>().0)
    {
        let v = lut[luma_on_white(*px)];
        dst[0] = v;
        dst[1] = v;
        dst[2] = v;
        dst[3] = 255;
    }
    out
}

fn apply_exif_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Parses EXIF Orientation tag (`0x0112`) directly from JPEG APP1 or TIFF header bytes.
fn parse_exif_orientation(data: &[u8]) -> Option<u16> {
    if data.starts_with(b"\xFF\xD8") {
        let mut i = 2_usize;
        while i + 4 <= data.len() {
            if data[i] != 0xFF {
                break;
            }
            let marker = data[i + 1];
            i += 2;
            if marker == 0xD9 || marker == 0xDA {
                break;
            }
            if (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0x00 {
                continue;
            }
            let seg_len = usize::from(u16::from_be_bytes([*data.get(i)?, *data.get(i + 1)?]));
            if seg_len < 2 || i + seg_len > data.len() {
                break;
            }
            let payload = &data[i + 2..i + seg_len];
            if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
                return parse_tiff_ifd_orientation(&payload[6..]);
            }
            i += seg_len;
        }
        None
    } else {
        parse_tiff_ifd_orientation(data)
    }
}

fn parse_tiff_ifd_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }
    let little = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |off: usize| -> Option<u16> {
        let s = tiff.get(off..off + 2)?;
        Some(if little {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let read_u32 = |off: usize| -> Option<u32> {
        let s = tiff.get(off..off + 4)?;
        Some(if little {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };

    if read_u16(2)? != 42 {
        return None;
    }
    let ifd_off = usize::try_from(read_u32(4)?).ok()?;
    let count = usize::from(read_u16(ifd_off)?);
    for idx in 0..count {
        let entry = ifd_off.checked_add(2)?.checked_add(idx * 12)?;
        let tag = read_u16(entry)?;
        if tag == 0x0112 {
            let val = read_u16(entry + 8)?;
            return Some(val);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chroma_stego_extract_amplifies_faint_blue_channel_stego_text() {
        // Uniform neutral gray background (160, 160, 160) with a subtle delta B=+3, R=-2 ridge
        let w = 80_u32;
        let h = 60_u32;
        let mut rgba = vec![160_u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx + 3] = 255;
                if (28..=30).contains(&y) && (20..=55).contains(&x) {
                    rgba[idx] = 158; // R
                    rgba[idx + 1] = 160; // G
                    rgba[idx + 2] = 163; // B
                }
            }
        }
        let img = ForensicsImage {
            width: w,
            height: h,
            rgba,
        };
        let enhanced = enhance_image(&img, EnhanceMode::ChromaStegoExtract).unwrap();

        let bg_luma = enhanced.rgba[0];
        let stego_luma = enhanced.rgba[((29 * w + 35) * 4) as usize];
        // Stego stripe must darken strongly relative to paper baseline (~225)
        assert!(bg_luma >= 215);
        assert!(stego_luma < bg_luma.saturating_sub(40));
    }

    #[test]
    fn adaptive_background_and_global_eq_work_cleanly_and_save_png() {
        let w = 64_u32;
        let h = 48_u32;
        let mut rgba = vec![235_u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx + 3] = 255;
                if y == 24 {
                    rgba[idx] = 210;
                    rgba[idx + 1] = 210;
                    rgba[idx + 2] = 210;
                }
            }
        }
        let img = ForensicsImage {
            width: w,
            height: h,
            rgba,
        };
        let sub = enhance_image(&img, EnhanceMode::AdaptiveBackgroundSubtract).unwrap();
        let eq = enhance_image(&img, EnhanceMode::GlobalEqualizeAndInvert).unwrap();
        assert_ne!(sub.rgba[((24 * w + 32) * 4) as usize], sub.rgba[0]);
        assert!(eq.rgba[((24 * w + 32) * 4) as usize] > eq.rgba[0]);

        let dir = tempfile::tempdir().unwrap();
        let out_png = dir.path().join("enhanced.png");
        save_png(&out_png, &sub).unwrap();
        let reloaded = load_image_upright(&out_png).unwrap();
        assert_eq!(reloaded.width, w);
        assert_eq!(reloaded.height, h);
    }

    /// Minimal TIFF header carrying only the Orientation tag, in either byte order.
    fn tiff_with_orientation(orientation: u16, little_endian: bool) -> Vec<u8> {
        let u16b = |v: u16| {
            if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let u32b = |v: u32| {
            if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let mut tiff = Vec::new();
        tiff.extend_from_slice(if little_endian { b"II" } else { b"MM" });
        tiff.extend_from_slice(&u16b(42));
        tiff.extend_from_slice(&u32b(8));
        tiff.extend_from_slice(&u16b(1)); // 1 IFD entry
        tiff.extend_from_slice(&u16b(0x0112)); // Orientation
        tiff.extend_from_slice(&u16b(3)); // SHORT
        tiff.extend_from_slice(&u32b(1)); // count
        tiff.extend_from_slice(&u16b(orientation));
        tiff.extend_from_slice(&u16b(0));
        tiff.extend_from_slice(&u32b(0)); // next IFD
        tiff
    }

    #[test]
    fn exif_tiff_orientation_rotates_6_90cw_and_8_270cw() {
        assert_eq!(
            parse_exif_orientation(&tiff_with_orientation(6, true)),
            Some(6)
        );
        assert_eq!(
            parse_exif_orientation(&tiff_with_orientation(8, true)),
            Some(8)
        );
        assert_eq!(
            parse_exif_orientation(&tiff_with_orientation(8, false)),
            Some(8)
        );
        assert_eq!(parse_exif_orientation(b"II*\0"), None);
        assert_eq!(parse_exif_orientation(b"not an image at all"), None);
    }

    /// Encodes a real baseline JPEG and splices a genuine APP1 `Exif` segment after SOI.
    fn jpeg_with_exif_orientation(
        img: &image::RgbImage,
        orientation: u16,
        little_endian: bool,
    ) -> Vec<u8> {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode_image(img)
            .unwrap();
        assert!(jpeg.starts_with(b"\xFF\xD8"));

        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff_with_orientation(orientation, little_endian));
        let seg_len = u16::try_from(payload.len() + 2).unwrap();

        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// 32x16 picture: left half pure red, right half pure blue.
    fn red_left_blue_right() -> image::RgbImage {
        image::RgbImage::from_fn(32, 16, |x, _| {
            if x < 16 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        })
    }

    fn pixel(img: &ForensicsImage, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * img.width + x) * 4) as usize;
        [
            img.rgba[idx],
            img.rgba[idx + 1],
            img.rgba[idx + 2],
            img.rgba[idx + 3],
        ]
    }

    fn is_red(px: [u8; 4]) -> bool {
        px[0] > 180 && px[2] < 80
    }

    fn is_blue(px: [u8; 4]) -> bool {
        px[2] > 180 && px[0] < 80
    }

    #[test]
    fn jpeg_app1_exif_orientation_8_rotates_pixels_270_degrees_clockwise() {
        let jpeg = jpeg_with_exif_orientation(&red_left_blue_right(), 8, true);
        assert_eq!(parse_exif_orientation(&jpeg), Some(8));

        let upright = load_image_bytes_upright(&jpeg).unwrap();
        // 32x16 landscape becomes 16x32 portrait.
        assert_eq!((upright.width, upright.height), (16, 32));
        // Rotating 270 degrees clockwise moves the right (blue) half to the top and the left
        // (red) half to the bottom.
        for x in [2, 8, 13] {
            assert!(
                is_blue(pixel(&upright, x, 3)),
                "{:?}",
                pixel(&upright, x, 3)
            );
            assert!(
                is_red(pixel(&upright, x, 28)),
                "{:?}",
                pixel(&upright, x, 28)
            );
        }
    }

    #[test]
    fn jpeg_app1_exif_orientation_6_big_endian_rotates_pixels_90_degrees_clockwise() {
        let jpeg = jpeg_with_exif_orientation(&red_left_blue_right(), 6, false);
        assert_eq!(parse_exif_orientation(&jpeg), Some(6));

        let upright = load_image_bytes_upright(&jpeg).unwrap();
        assert_eq!((upright.width, upright.height), (16, 32));
        // Rotating 90 degrees clockwise moves the left (red) half to the top.
        for x in [2, 8, 13] {
            assert!(is_red(pixel(&upright, x, 3)));
            assert!(is_blue(pixel(&upright, x, 28)));
        }
    }

    #[test]
    fn jpeg_without_exif_or_with_other_app_segments_keeps_its_orientation() {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode_image(&red_left_blue_right())
            .unwrap();
        assert_eq!(parse_exif_orientation(&jpeg), None);
        let plain = load_image_bytes_upright(&jpeg).unwrap();
        assert_eq!((plain.width, plain.height), (32, 16));
        assert!(is_red(pixel(&plain, 3, 8)));
        assert!(is_blue(pixel(&plain, 28, 8)));

        // An APP1 segment that is not Exif (e.g. XMP) must be skipped, not misparsed.
        let mut xmp = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x0A];
        xmp.extend_from_slice(b"http://x");
        xmp.extend_from_slice(&jpeg[2..]);
        assert_eq!(parse_exif_orientation(&xmp), None);
    }

    /// Maps a stored pixel `(x, y)` of a `w` x `h` image to its upright position.
    type Map = fn(u32, u32, u32, u32) -> (u32, u32);

    #[test]
    fn every_exif_orientation_maps_pixels_to_the_position_the_standard_defines() {
        // 3 wide x 2 high with a unique value per pixel: value = 10 * y + x.
        let (w, h) = (3_u32, 2_u32);
        let src = RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([u8::try_from(10 * y + x).unwrap(), 0, 0, 255])
        });
        // (orientation, upright size, where stored pixel (x, y) must end up)
        let cases: [(u16, (u32, u32), Map); 8] = [
            (1, (3, 2), |x, y, _, _| (x, y)),
            (2, (3, 2), |x, y, w, _| (w - 1 - x, y)),
            (3, (3, 2), |x, y, w, h| (w - 1 - x, h - 1 - y)),
            (4, (3, 2), |x, y, _, h| (x, h - 1 - y)),
            (5, (2, 3), |x, y, _, _| (y, x)),
            (6, (2, 3), |x, y, _, h| (h - 1 - y, x)),
            (7, (2, 3), |x, y, w, h| (h - 1 - y, w - 1 - x)),
            (8, (2, 3), |x, y, w, _| (y, w - 1 - x)),
        ];
        for (orientation, size, map) in cases {
            let out = apply_exif_orientation(DynamicImage::ImageRgba8(src.clone()), orientation)
                .into_rgba8();
            assert_eq!(out.dimensions(), size, "orientation {orientation}");
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = map(x, y, w, h);
                    assert_eq!(
                        out.get_pixel(dx, dy)[0],
                        u8::try_from(10 * y + x).unwrap(),
                        "orientation {orientation}: stored ({x},{y}) -> upright ({dx},{dy})"
                    );
                }
            }
        }
    }

    fn encode_png(img: &RgbaImage) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn pixel_budget_fit_keeps_aspect_ratio_and_never_exceeds_the_budget() {
        assert_eq!(
            fit_within_pixel_budget(4_000, 3_000, 50_000_000),
            (4_000, 3_000)
        );
        // 200 MP (16320 x 12240) -> at most 50 MP with the same 4:3 shape.
        let (w, h) = fit_within_pixel_budget(16_320, 12_240, MAX_FORENSICS_PIXELS);
        assert!(u64::from(w) * u64::from(h) <= 50_000_000);
        assert!(u64::from(w) * u64::from(h) > 49_900_000);
        assert!((f64::from(w) / f64::from(h) - 4.0 / 3.0).abs() < 0.001);
        // Degenerate shapes stay at least one pixel on each side and inside the budget.
        for (iw, ih, budget) in [
            (1_000_000, 1, 1_000),
            (1, 1_000_000, 1_000),
            (7, 7, 1),
            (3, 5, 14),
        ] {
            let (fw, fh) = fit_within_pixel_budget(iw, ih, budget);
            assert!(fw >= 1 && fh >= 1);
            assert!(u64::from(fw) * u64::from(fh) <= u64::try_from(budget).unwrap());
        }
    }

    #[test]
    fn images_above_the_pixel_budget_are_scaled_down_instead_of_rejected() {
        // 400x200 (80 000 px) against a 20 000 px budget -> exactly half size on each side.
        let src = RgbaImage::from_fn(400, 200, |x, _| {
            if x < 200 {
                image::Rgba([250, 250, 250, 255])
            } else {
                image::Rgba([20, 20, 20, 255])
            }
        });
        let png = encode_png(&src);
        let small = load_image_bytes_with_budget(&png, 20_000).unwrap();
        assert_eq!((small.width, small.height), (200, 100));
        assert_eq!(small.rgba.len(), 200 * 100 * 4);
        // Content survives the resize: light left half, dark right half.
        assert!(pixel(&small, 40, 50)[0] > 230);
        assert!(pixel(&small, 160, 50)[0] < 40);
        // The scaled result is accepted by the enhancer.
        assert!(enhance_image(&small, EnhanceMode::GlobalEqualizeAndInvert).is_ok());

        // Within budget: untouched.
        let same = load_image_bytes_with_budget(&png, 80_000).unwrap();
        assert_eq!((same.width, same.height), (400, 200));
    }

    #[test]
    fn real_image_just_above_50_megapixels_is_loaded_and_reduced_to_the_limit() {
        // 8200 x 6200 = 50.84 MP, written directly as an uncompressed 8-bit greyscale BMP so
        // the test does not spend its time in an encoder.
        let (w, h) = (8_200_u32, 6_200_u32);
        let row_len = w as usize; // already a multiple of four, so rows need no padding
        let pixel_offset = 14 + 40 + 256 * 4;
        let mut bytes = Vec::with_capacity(pixel_offset + row_len * h as usize);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // file size (ignored by decoders)
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // reserved
        bytes.extend_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes.extend_from_slice(&40_u32.to_le_bytes()); // BITMAPINFOHEADER
        bytes.extend_from_slice(&i32::try_from(w).unwrap().to_le_bytes());
        bytes.extend_from_slice(&i32::try_from(h).unwrap().to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // planes
        bytes.extend_from_slice(&8_u16.to_le_bytes()); // bits per pixel
        bytes.extend_from_slice(&[0_u8; 24]); // BI_RGB, sizes, resolution, palette counts
        for level in 0..=255_u8 {
            bytes.extend_from_slice(&[level, level, level, 0]);
        }
        let mut row = vec![16_u8; row_len];
        row[..row_len / 2].fill(240);
        for _ in 0..h {
            bytes.extend_from_slice(&row);
        }

        let loaded = load_image_bytes_upright(&bytes).unwrap();
        let total = u64::from(loaded.width) * u64::from(loaded.height);
        assert!(total <= 50_000_000, "{} x {}", loaded.width, loaded.height);
        assert!(total > 49_900_000, "{} x {}", loaded.width, loaded.height);
        assert!(loaded.width < w && loaded.height < h);
        let ratio = f64::from(loaded.width) / f64::from(loaded.height);
        assert!((ratio - f64::from(w) / f64::from(h)).abs() < 0.001);
        assert!(pixel(&loaded, loaded.width / 4, loaded.height / 2)[0] > 220);
        assert!(pixel(&loaded, loaded.width * 3 / 4, loaded.height / 2)[0] < 40);
    }

    #[test]
    fn absurdly_large_headers_are_rejected_before_any_pixel_is_decoded() {
        // A BMP header that claims 20000 x 20000 (400 MP) with no pixel data behind it.
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&0_u32.to_le_bytes()); // file size (ignored)
        bmp.extend_from_slice(&0_u32.to_le_bytes()); // reserved
        bmp.extend_from_slice(&54_u32.to_le_bytes()); // pixel data offset
        bmp.extend_from_slice(&40_u32.to_le_bytes()); // BITMAPINFOHEADER
        bmp.extend_from_slice(&20_000_i32.to_le_bytes());
        bmp.extend_from_slice(&20_000_i32.to_le_bytes());
        bmp.extend_from_slice(&1_u16.to_le_bytes()); // planes
        bmp.extend_from_slice(&24_u16.to_le_bytes()); // bpp
        bmp.extend_from_slice(&[0_u8; 24]);
        let err = load_image_bytes_upright(&bmp).unwrap_err();
        assert!(err.to_string().contains("上限"), "{err}");
    }

    #[test]
    fn transparent_pixels_are_flattened_onto_white_and_previews_are_premultiplied() {
        let src = RgbaImage::from_fn(4, 1, |x, _| match x {
            0 => image::Rgba([0, 0, 0, 0]),        // fully transparent black
            1 => image::Rgba([0, 0, 0, 255]),      // opaque black
            2 => image::Rgba([200, 100, 50, 128]), // half transparent
            _ => image::Rgba([10, 20, 30, 255]),
        });
        let loaded = load_image_bytes_upright(&encode_png(&src)).unwrap();
        assert_eq!(pixel(&loaded, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&loaded, 1, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&loaded, 2, 0), [227, 177, 152, 255]);
        assert_eq!(pixel(&loaded, 3, 0), [10, 20, 30, 255]);

        // Premultiplied BGRA conversion for display.
        assert_eq!(
            rgba_to_premultiplied_bgra([10, 20, 30, 255]),
            [30, 20, 10, 255]
        );
        assert_eq!(
            rgba_to_premultiplied_bgra([200, 100, 50, 128]),
            [25, 50, 100, 128]
        );
        assert_eq!(rgba_to_premultiplied_bgra([255, 255, 255, 0]), [0, 0, 0, 0]);
        let img = ForensicsImage {
            width: 2,
            height: 1,
            rgba: vec![200, 100, 50, 128, 1, 2, 3, 255],
        };
        assert_eq!(img.to_bgra8(), vec![25, 50, 100, 128, 3, 2, 1, 255]);

        // The enhancer sees a transparent pixel as white paper, not as black ink.
        let transparent = ForensicsImage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 0, 255, 255, 255, 255],
        };
        let eq = enhance_image(&transparent, EnhanceMode::GlobalEqualizeAndInvert).unwrap();
        assert_eq!(eq.rgba[0], eq.rgba[4]);
    }

    /// Straightforward O(W*H*r^2) reference for multi-pass separable box blur.
    fn brute_force_box_passes(
        src: &[f32],
        w: usize,
        h: usize,
        radius: usize,
        passes: usize,
    ) -> Vec<f32> {
        let mut cur = src.to_vec();
        for _ in 0..passes {
            let mut horiz = vec![0.0_f32; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (x0, x1) = (x.saturating_sub(radius), (x + radius).min(w - 1));
                    let sum: f64 = (x0..=x1).map(|xx| f64::from(cur[y * w + xx])).sum();
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                    {
                        horiz[y * w + x] = (sum / ((x1 - x0 + 1) as f64)) as f32;
                    }
                }
            }
            for y in 0..h {
                for x in 0..w {
                    let (y0, y1) = (y.saturating_sub(radius), (y + radius).min(h - 1));
                    let sum: f64 = (y0..=y1).map(|yy| f64::from(horiz[yy * w + x])).sum();
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                    {
                        cur[y * w + x] = (sum / ((y1 - y0 + 1) as f64)) as f32;
                    }
                }
            }
        }
        cur
    }

    fn noisy_plane(len: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            out.push(f32::from(u8::try_from(state % 256).unwrap()));
        }
        out
    }

    #[test]
    fn sliding_window_output_is_identical_to_a_brute_force_reference() {
        // Sizes cover: larger than the window, narrower/shorter than the radius, single row,
        // single column and a single pixel.
        for (w, h, seed) in [
            (61, 47, 0x1234_5678),
            (40, 9, 0x0BAD_F00D),
            (7, 53, 0x0C0F_FEE1),
            (5, 4, 0x0000_BEEF),
            (1, 30, 0x0000_0077),
            (30, 1, 0x0000_0099),
            (1, 1, 0x0000_0001),
        ] {
            let plane = noisy_plane(w * h, seed);
            let fast = blur_box_passes(&plane, w, h, 9, 3);
            let reference = brute_force_box_passes(&plane, w, h, 9, 3);
            for (a, b) in fast.iter().zip(&reference) {
                assert!((a - b).abs() <= 1e-4, "{w}x{h}: {a} vs {b}");
            }
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn camera_photo_with_lcd_moire_and_black_body_text_isolates_watermarks_cleanly() {
        // 1200x900 simulated camera photo of a cool-white LCD screen (205, 212, 236) with
        // vertical RGB subpixel moire (period 3 px), dark body text, and a faint blue watermark.
        let (w, h) = (1_200_u32, 900_u32);
        let mut rgba = vec![255_u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let moire = match x % 3 {
                    0 => [10_i16, -4, -8],
                    1 => [-5, 9, -4],
                    _ => [-5, -5, 12],
                };
                let mut r = 205 + moire[0];
                let mut g = 212 + moire[1];
                let mut b = 236 + moire[2];

                // Dark PDF body text line (attenuates all channels multiplicatively by 0.25)
                if (420..=436).contains(&y) && (120..=1_080).contains(&x) && (x % 22 < 14) {
                    r /= 4;
                    g /= 4;
                    b /= 4;
                }
                // Faint hidden blue watermark band (`alpha ~ 18/255` of `[0, 70, 245]`)
                if (200..=214).contains(&y) && (180..=1_020).contains(&x) {
                    r -= 12;
                    g -= 7;
                    b += 2;
                }
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = u8::try_from(r.clamp(0, 255)).unwrap();
                rgba[idx + 1] = u8::try_from(g.clamp(0, 255)).unwrap();
                rgba[idx + 2] = u8::try_from(b.clamp(0, 255)).unwrap();
                rgba[idx + 3] = 255;
            }
        }
        let img = ForensicsImage {
            width: w,
            height: h,
            rgba,
        };
        let enh = enhance_image(&img, EnhanceMode::ChromaStegoExtract).unwrap();
        let paper = pixel(&enh, 600, 100)[0];
        let wm = pixel(&enh, 600, 207)[0];
        let body_ink = pixel(&enh, 600, 428)[0];

        assert!(paper >= 240, "paper should stay crisp white, got {paper}");
        assert!(wm <= 60, "watermark stroke should be dark, got {wm}");
        assert!(
            body_ink >= 200,
            "black PDF body ink should stay faintly suppressed, got {body_ink}"
        );
    }

    #[test]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::many_single_char_names,
        clippy::suboptimal_flops
    )]
    fn wechat_compressed_camera_photo_with_hidden_only_watermark_is_extracted_clearly() {
        // Simulates a 1440x1080 WeChat-compressed phone photo of a screen with ONLY the hidden
        // trace watermark enabled (`[0, 95, 250]`, alpha=15): strong +/-13 period-6 RGB LCD moire,
        // 22 % corner vignetting, dense black text lines, and a heavily ISP-attenuated hidden
        // watermark stroke (`dR = -4.5, dG = -3.0, dB = -0.5` after 70 % ISP chroma loss).
        let (w, h) = (1_440_u32, 1_080_u32);
        let mut rgba = vec![255_u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let xf = x as f32;
                let yf = y as f32;
                let vig =
                    1.0 - 0.22 * (((xf - 720.0) / 720.0).powi(2) + ((yf - 540.0) / 540.0).powi(2));
                let (mr, mg, mb) = match (x + y / 7) % 6 {
                    0 | 1 => (13.0, -5.0, -8.0),
                    2 | 3 => (-6.0, 11.0, -5.0),
                    _ => (-7.0, -6.0, 13.0),
                };
                let mut r = 212.0 * vig + mr;
                let mut g = 218.0 * vig + mg;
                let mut b = 234.0 * vig + mb;

                if (460..=477).contains(&y) && (120..=1_320).contains(&x) && (x % 24 < 16) {
                    r *= 0.22;
                    g *= 0.22;
                    b *= 0.22;
                } else if (230..=243).contains(&y) && (220..=1_180).contains(&x) {
                    r -= 4.5 * vig;
                    g -= 3.0 * vig;
                    b -= 0.5 * vig;
                }
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = r.round().clamp(0.0, 255.0) as u8;
                rgba[idx + 1] = g.round().clamp(0.0, 255.0) as u8;
                rgba[idx + 2] = b.round().clamp(0.0, 255.0) as u8;
                rgba[idx + 3] = 255;
            }
        }
        // Encode and decode through lossy JPEG (quality 78, typical WeChat photo compression) to
        // verify extraction survives 4:2:0 DCT chroma quantization as well.
        let rgb_img = image::RgbImage::from_fn(w, h, |x, y| {
            let idx = ((y * w + x) * 4) as usize;
            image::Rgb([rgba[idx], rgba[idx + 1], rgba[idx + 2]])
        });
        let mut jpeg_bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 78)
            .encode_image(&rgb_img)
            .unwrap();
        let loaded = load_image_bytes_upright(&jpeg_bytes).unwrap();
        let enh = enhance_image(&loaded, EnhanceMode::ChromaStegoExtract).unwrap();

        let paper = pixel(&enh, 720, 120)[0];
        let wm = pixel(&enh, 720, 236)[0];
        let body_ink = pixel(&enh, 720, 468)[0];
        assert!(paper >= 240, "paper should stay white, got {paper}");
        assert!(
            wm <= 95,
            "hidden-only watermark should extract clearly after JPEG-78 + LCD moire, got {wm}"
        );
        assert!(
            body_ink >= 200,
            "body text stays suppressed, got {body_ink}"
        );
    }

    #[test]
    fn heif_container_detection_and_corrupt_heic_error_reporting() {
        let mut fake_heic = Vec::new();
        fake_heic.extend_from_slice(&24_u32.to_be_bytes());
        fake_heic.extend_from_slice(b"ftypheic");
        fake_heic.extend_from_slice(&0_u32.to_be_bytes());
        fake_heic.extend_from_slice(b"mif1heix");
        assert!(is_heif_container(&fake_heic));

        let mut mif1_compat = Vec::new();
        mif1_compat.extend_from_slice(&20_u32.to_be_bytes());
        mif1_compat.extend_from_slice(b"ftypiso5");
        mif1_compat.extend_from_slice(&0_u32.to_be_bytes());
        mif1_compat.extend_from_slice(b"heic");
        assert!(is_heif_container(&mif1_compat));

        assert!(!is_heif_container(b"\x89PNG\r\n\x1a\n00000000"));
        assert!(!is_heif_container(
            b"\x00\x00\x00\x14ftypmp42\x00\x00\x00\x00mp42"
        ));

        let err = load_image_bytes_upright(&fake_heic).unwrap_err();
        assert!(err.to_string().contains("HEIC"), "{err}");
    }

    #[test]
    #[ignore = "diagnostic; decodes SECUREPDF_HEIC_IN and writes forensic PNGs to SECUREPDF_HEIC_OUT_DIR"]
    fn extract_heic_forensics_to_dir() {
        let input = std::path::PathBuf::from(std::env::var_os("SECUREPDF_HEIC_IN").unwrap());
        let out_dir = std::path::PathBuf::from(std::env::var_os("SECUREPDF_HEIC_OUT_DIR").unwrap());
        fs::create_dir_all(&out_dir).unwrap();

        let loaded = load_image_upright(&input).unwrap();
        println!("LOADED_UPRIGHT={}x{}", loaded.width, loaded.height);
        if let Some(found) = crate::watermark::decode_microdots(&loaded) {
            println!(
                "MICRODOTS=OK ip={} unix={} local={}",
                found.payload.ipv4,
                found.payload.unix_seconds,
                crate::localtime::format_unix_seconds_local(found.payload.unix_seconds)
            );
        } else {
            println!("MICRODOTS=NONE (camera capture)");
        }

        // Save downscaled preview of upright original + full/preview of all 3 enhancement modes.
        let save_scaled = |img: &ForensicsImage, name: &str, max_side: u32| {
            let longer = img.width.max(img.height);
            if longer <= max_side {
                save_png(&out_dir.join(name), img).unwrap();
            } else {
                let scale = f64::from(max_side) / f64::from(longer);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let nw = ((f64::from(img.width) * scale).round() as u32).max(1);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let nh = ((f64::from(img.height) * scale).round() as u32).max(1);
                let raw = RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).unwrap();
                let thumb = DynamicImage::ImageRgba8(raw)
                    .thumbnail_exact(nw, nh)
                    .into_rgba8();
                thumb
                    .write_to(
                        &mut fs::File::create(out_dir.join(name)).unwrap(),
                        ImageFormat::Png,
                    )
                    .unwrap();
            }
        };

        save_scaled(&loaded, "0_upright_preview.png", 2000);
        for (idx, mode, tag) in [
            (1, EnhanceMode::ChromaStegoExtract, "chroma"),
            (2, EnhanceMode::AdaptiveBackgroundSubtract, "adaptive"),
            (3, EnhanceMode::GlobalEqualizeAndInvert, "equalize"),
        ] {
            let enh = enhance_image(&loaded, mode).unwrap();
            save_png(&out_dir.join(format!("{idx}_{tag}_full.png")), &enh).unwrap();
            save_scaled(&enh, &format!("{idx}_{tag}_preview.png"), 2000);
            println!("SAVED {idx}_{tag} ({}x{})", enh.width, enh.height);
        }
    }
}
