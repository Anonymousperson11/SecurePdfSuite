//! Pure in-memory `PDFium` wrapper serialized under a process-wide mutex.

use std::ffi::{CString, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use windows_core::{PCSTR, PCWSTR};
use zeroize::Zeroizing;

use crate::bindings;
use crate::error::{CoreError, CoreResult};
use crate::win32_util::{LibraryGuard, to_wide_null};

/// Maximum pixels allowed for a single interactive or print bitmap render (64M px).
pub const MAX_RENDER_PIXELS: u64 = 64_000_000;

const FPDF_ANNOT: c_int = 0x01;
const FPDF_PRINTING: c_int = 0x0800;
const FPDF_BITMAP_BGR: c_int = 2;
const FPDF_BITMAP_BGRX: c_int = 3;

#[repr(C)]
struct FpdfLibraryConfig {
    version: c_int,
    user_font_paths: *const *const i8,
    isolate: *mut c_void,
    v8_embedder_slot: u32,
    platform: *mut c_void,
    renderer_type: c_int,
}

type FnInitWithConfig = unsafe extern "C" fn(*const FpdfLibraryConfig);
type FnLoadMemDocument = unsafe extern "C" fn(*const c_void, c_int, *const i8) -> *mut c_void;
type FnCloseDocument = unsafe extern "C" fn(*mut c_void);
type FnGetPageCount = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnGetPageSizeByIndex = unsafe extern "C" fn(*mut c_void, c_int, *mut f64, *mut f64) -> c_int;
type FnLoadPage = unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void;
type FnClosePage = unsafe extern "C" fn(*mut c_void);
type FnBitmapCreateEx =
    unsafe extern "C" fn(c_int, c_int, c_int, *mut c_void, c_int) -> *mut c_void;
type FnBitmapFillRect = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_int, u64) -> c_int;
type FnRenderPageBitmap =
    unsafe extern "C" fn(*mut c_void, *mut c_void, c_int, c_int, c_int, c_int, c_int, c_int);
type FnBitmapGetBuffer = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type FnBitmapGetStride = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnBitmapDestroy = unsafe extern "C" fn(*mut c_void);
type FnGetLastError = unsafe extern "C" fn() -> u32;

struct PdfiumApi {
    _library: LibraryGuard,
    load_mem_document: FnLoadMemDocument,
    close_document: FnCloseDocument,
    get_page_count: FnGetPageCount,
    get_page_size_by_index: FnGetPageSizeByIndex,
    load_page: FnLoadPage,
    close_page: FnClosePage,
    bitmap_create_ex: FnBitmapCreateEx,
    bitmap_fill_rect: FnBitmapFillRect,
    render_page_bitmap: FnRenderPageBitmap,
    bitmap_get_buffer: FnBitmapGetBuffer,
    bitmap_get_stride: FnBitmapGetStride,
    bitmap_destroy: FnBitmapDestroy,
    get_last_error: FnGetLastError,
}

struct PageGuard<'a> {
    page: *mut c_void,
    close: FnClosePage,
    _api: &'a PdfiumApi,
}

impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        if !self.page.is_null() {
            // SAFETY: Valid `page` handle closed cleanly under held `PDFIUM_GATE`.
            unsafe { (self.close)(self.page) };
        }
    }
}

struct BitmapGuard {
    bmp: *mut c_void,
    destroy: FnBitmapDestroy,
}

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        if !self.bmp.is_null() {
            // SAFETY: Valid `FPDF_BITMAP` destroyed under held `PDFIUM_GATE`.
            unsafe { (self.destroy)(self.bmp) };
        }
    }
}

static PDFIUM_GATE: OnceLock<Mutex<Option<PdfiumApi>>> = OnceLock::new();

fn pdfium_gate() -> &'static Mutex<Option<PdfiumApi>> {
    PDFIUM_GATE.get_or_init(|| Mutex::new(None))
}

/// Locates `pdfium.dll` explicitly next to the current executable or its `x64` subfolder.
#[must_use]
pub fn default_pdfium_candidates() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        list.push(dir.join("pdfium.dll"));
        list.push(dir.join("x64").join("pdfium.dll"));
    }
    list
}

fn load_symbol<T>(module: bindings::HMODULE, name: &str) -> CoreResult<T> {
    let c_name = CString::new(name)
        .map_err(|_| CoreError::Pdfium(format!("PDFium 符号名称非法：{name}")))?;
    // SAFETY: `module` is a valid loaded `pdfium.dll` handle and `c_name` is NUL-terminated.
    let raw = unsafe { bindings::GetProcAddress(module, PCSTR(c_name.as_ptr().cast())) };
    let proc = raw.ok_or_else(|| CoreError::Pdfium(format!("pdfium.dll 缺少导出符号 {name}")))?;
    if const { size_of::<T>() != size_of::<bindings::FARPROC>() } {
        return Err(CoreError::Pdfium("函数指针大小不匹配。".to_owned()));
    }
    // SAFETY: Caller specifies exact matching `unsafe extern "C"` signature exported by `pdfium.dll`.
    Ok(unsafe { std::mem::transmute_copy::<_, T>(&proc) })
}

fn ensure_pdfium_loaded<'a>(
    guard: &'a mut Option<PdfiumApi>,
    custom_dll_path: Option<&Path>,
) -> CoreResult<&'a PdfiumApi> {
    if guard.is_none() {
        let mut candidates = Vec::new();
        if let Some(p) = custom_dll_path {
            candidates.push(p.to_path_buf());
        }
        candidates.extend(default_pdfium_candidates());

        let found = candidates
            .into_iter()
            .find(|p| p.is_file())
            .ok_or_else(|| {
                CoreError::Pdfium(
                    "未在程序目录找到 pdfium.dll（或 x64\\pdfium.dll），无法渲染 PDF。".to_owned(),
                )
            })?;

        let wide = to_wide_null(&found.to_string_lossy());
        // SAFETY: Explicit absolute filesystem path to `pdfium.dll`; never uses ambiguous search paths.
        let module = unsafe { bindings::LoadLibraryW(PCWSTR(wide.as_ptr())) };
        if module.0.is_null() {
            return Err(CoreError::Pdfium(format!(
                "加载 {} 失败：{}",
                found.display(),
                std::io::Error::last_os_error()
            )));
        }
        let lib = LibraryGuard::new(module);

        let init_with_config: FnInitWithConfig =
            load_symbol(lib.as_hmodule(), "FPDF_InitLibraryWithConfig")?;
        let api = PdfiumApi {
            load_mem_document: load_symbol(lib.as_hmodule(), "FPDF_LoadMemDocument")?,
            close_document: load_symbol(lib.as_hmodule(), "FPDF_CloseDocument")?,
            get_page_count: load_symbol(lib.as_hmodule(), "FPDF_GetPageCount")?,
            get_page_size_by_index: load_symbol(lib.as_hmodule(), "FPDF_GetPageSizeByIndex")?,
            load_page: load_symbol(lib.as_hmodule(), "FPDF_LoadPage")?,
            close_page: load_symbol(lib.as_hmodule(), "FPDF_ClosePage")?,
            bitmap_create_ex: load_symbol(lib.as_hmodule(), "FPDFBitmap_CreateEx")?,
            bitmap_fill_rect: load_symbol(lib.as_hmodule(), "FPDFBitmap_FillRect")?,
            render_page_bitmap: load_symbol(lib.as_hmodule(), "FPDF_RenderPageBitmap")?,
            bitmap_get_buffer: load_symbol(lib.as_hmodule(), "FPDFBitmap_GetBuffer")?,
            bitmap_get_stride: load_symbol(lib.as_hmodule(), "FPDFBitmap_GetStride")?,
            bitmap_destroy: load_symbol(lib.as_hmodule(), "FPDFBitmap_Destroy")?,
            get_last_error: load_symbol(lib.as_hmodule(), "FPDF_GetLastError")?,
            _library: lib,
        };

        let cfg = FpdfLibraryConfig {
            version: 2,
            user_font_paths: null(),
            isolate: null_mut(),
            v8_embedder_slot: 0,
            platform: null_mut(),
            renderer_type: 0,
        };
        // SAFETY: Initialized once under `PDFIUM_GATE` mutex before any `FPDF_*` API calls.
        unsafe { init_with_config(&raw const cfg) };
        *guard = Some(api);
    }

    guard
        .as_ref()
        .ok_or_else(|| CoreError::Pdfium("PDFium 初始化状态异常。".to_owned()))
}

/// Decrypted PDF document bound directly to an in-memory byte buffer with zero temp files.
pub struct MemoryPdfDocument {
    doc_handle: *mut c_void,
    page_count: i32,
    // Kept alive & pinned on heap for the full lifetime of `doc_handle`; zeroized after close.
    _backing_bytes: Zeroizing<Box<[u8]>>,
}

// SAFETY: All FFI operations on `doc_handle` acquire the global `PDFIUM_GATE` mutex.
unsafe impl Send for MemoryPdfDocument {}
// SAFETY: Same global mutex serialization guarantee.
unsafe impl Sync for MemoryPdfDocument {}

impl Drop for MemoryPdfDocument {
    fn drop(&mut self) {
        if !self.doc_handle.is_null() {
            if let Ok(guard) = pdfium_gate().lock()
                && let Some(api) = guard.as_ref()
            {
                // SAFETY: `self.doc_handle` was created by `FPDF_LoadMemDocument`, protected by
                // `PDFIUM_GATE`, and closed strictly before `_backing_bytes` drops.
                unsafe { (api.close_document)(self.doc_handle) };
            }
            self.doc_handle = null_mut();
        }
    }
}

impl MemoryPdfDocument {
    /// Opens an in-memory PDF buffer using `pdfium.dll` next to the executable.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Pdfium`] if `pdfium.dll` is missing, input exceeds `i32::MAX`, or
    /// `PDFium` rejects the document.
    pub fn open_bytes(pdf_bytes: Zeroizing<Vec<u8>>) -> CoreResult<Self> {
        Self::open_bytes_with_dll(pdf_bytes, None)
    }

    /// Opens an in-memory PDF buffer specifying an explicit `pdfium.dll` path when needed.
    ///
    /// # Errors
    ///
    /// Same as [`Self::open_bytes`].
    pub fn open_bytes_with_dll(
        mut pdf_bytes: Zeroizing<Vec<u8>>,
        pdfium_dll_path: Option<&Path>,
    ) -> CoreResult<Self> {
        if pdf_bytes.is_empty() {
            return Err(CoreError::Pdfium("PDF 内容为空。".to_owned()));
        }
        let len_i32 = c_int::try_from(pdf_bytes.len())
            .map_err(|_| CoreError::Pdfium("PDF 大小超过单文档 2 GiB 解析限制。".to_owned()))?;

        let owned = std::mem::take(&mut *pdf_bytes).into_boxed_slice();
        let backing = Zeroizing::new(owned);

        let (doc_handle, page_count) = {
            let mut gate = pdfium_gate()
                .lock()
                .map_err(|_| CoreError::Pdfium("PDFium 全局锁处于异常状态。".to_owned()))?;
            let api = ensure_pdfium_loaded(&mut gate, pdfium_dll_path)?;

            // SAFETY: `backing` remains alive on heap at fixed address until `Drop` closes `doc_handle`.
            let doc_handle =
                unsafe { (api.load_mem_document)(backing.as_ptr().cast(), len_i32, null()) };
            if doc_handle.is_null() {
                // SAFETY: Thread holds `PDFIUM_GATE` mutex immediately after `FPDF_LoadMemDocument`.
                let err_code = unsafe { (api.get_last_error)() };
                let detail = match err_code {
                    4 => "目标 PDF 本身带有打开密码保护，请先解除原 PDF 密码后再加密。".to_owned(),
                    _ => format!("PDFium 无法解析 PDF 文档（错误码 {err_code}）。"),
                };
                return Err(CoreError::Pdfium(detail));
            }

            // SAFETY: `doc_handle` is non-null and owned under `PDFIUM_GATE`.
            let page_count = unsafe { (api.get_page_count)(doc_handle) };
            if page_count <= 0 {
                // SAFETY: Cleanup non-empty failed handle under lock.
                unsafe { (api.close_document)(doc_handle) };
                return Err(CoreError::Pdfium(
                    "PDF 文档不包含任何可阅读页面。".to_owned(),
                ));
            }
            drop(gate);
            (doc_handle, page_count)
        };

        Ok(Self {
            doc_handle,
            page_count,
            _backing_bytes: backing,
        })
    }

    /// Returns total page count of the loaded document (`>= 1`).
    #[must_use]
    pub const fn page_count(&self) -> i32 {
        self.page_count
    }

    /// Queries logical page `(width_pts, height_pts)` in PDF points (1/72 inch).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Pdfium`] if `page_index` is out of range or `PDFium` query fails.
    pub fn page_size_points(&self, page_index: i32) -> CoreResult<(f64, f64)> {
        if page_index < 0 || page_index >= self.page_count {
            return Err(CoreError::Pdfium(format!(
                "页码索引越界：{page_index}（总页数 {}）",
                self.page_count
            )));
        }
        let gate = pdfium_gate()
            .lock()
            .map_err(|_| CoreError::Pdfium("PDFium 全局锁处于异常状态。".to_owned()))?;
        let api = gate
            .as_ref()
            .ok_or_else(|| CoreError::Pdfium("PDFium 未初始化。".to_owned()))?;
        let mut w = 0.0_f64;
        let mut h = 0.0_f64;
        // SAFETY: Serialized by `PDFIUM_GATE`, valid `doc_handle`, valid out pointers.
        let ok = unsafe {
            (api.get_page_size_by_index)(self.doc_handle, page_index, &raw mut w, &raw mut h)
        };
        drop(gate);
        if ok == 0 || w <= 0.0 || h <= 0.0 {
            return Err(CoreError::Pdfium(format!(
                "无法获取第 {} 页页面尺寸。",
                page_index + 1
            )));
        }
        Ok((w, h))
    }

    /// Renders `page_index` at exact `(width, height)` pixels to tight premultiplied `BGRA8` (`width*height*4`).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Pdfium`] if dimensions exceed [`MAX_RENDER_PIXELS`] or `PDFium` fails.
    pub fn render_page_bgra(
        &self,
        page_index: i32,
        width: i32,
        height: i32,
    ) -> CoreResult<Vec<u8>> {
        self.render_page_internal(page_index, width, height, FPDF_BITMAP_BGRX, FPDF_ANNOT)
    }

    /// Renders `page_index` for GDI physical printing in DWORD-aligned 24-bpp `BGR` layout.
    ///
    /// Returns `(pixels, row_stride_bytes)`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Pdfium`] if dimensions exceed [`MAX_RENDER_PIXELS`] or `PDFium` fails.
    pub fn render_page_bgr24_for_print(
        &self,
        page_index: i32,
        width: i32,
        height: i32,
    ) -> CoreResult<(Vec<u8>, usize)> {
        let raw = self.render_page_internal(
            page_index,
            width,
            height,
            FPDF_BITMAP_BGR,
            FPDF_ANNOT | FPDF_PRINTING,
        )?;
        let w_usize =
            usize::try_from(width).map_err(|_| CoreError::Pdfium("打印宽度非法。".to_owned()))?;
        let h_usize =
            usize::try_from(height).map_err(|_| CoreError::Pdfium("打印高度非法。".to_owned()))?;
        let src_row_bytes = w_usize * 3;
        let dib_stride = (src_row_bytes + 3) & !3;
        if dib_stride == src_row_bytes {
            return Ok((raw, dib_stride));
        }
        let mut aligned = vec![0_u8; dib_stride * h_usize];
        for row in 0..h_usize {
            let src = &raw[row * src_row_bytes..(row + 1) * src_row_bytes];
            let dst = &mut aligned[row * dib_stride..row * dib_stride + src_row_bytes];
            dst.copy_from_slice(src);
        }
        Ok((aligned, dib_stride))
    }

    fn render_page_internal(
        &self,
        page_index: i32,
        width: i32,
        height: i32,
        format: c_int,
        render_flags: c_int,
    ) -> CoreResult<Vec<u8>> {
        if page_index < 0 || page_index >= self.page_count {
            return Err(CoreError::Pdfium(format!("页码越界：{page_index}")));
        }
        if width <= 0 || height <= 0 {
            return Err(CoreError::Pdfium("渲染像素宽高必须为正整数。".to_owned()));
        }
        let pixels = u64::try_from(width).unwrap_or(0) * u64::try_from(height).unwrap_or(0);
        if pixels > MAX_RENDER_PIXELS {
            return Err(CoreError::Pdfium(format!(
                "渲染尺寸 {width}x{height} 超出 64M 像素上限。"
            )));
        }

        let bpp = if format == FPDF_BITMAP_BGR {
            3_usize
        } else {
            4_usize
        };
        let w_usize =
            usize::try_from(width).map_err(|_| CoreError::Pdfium("渲染宽度溢出。".to_owned()))?;
        let h_usize =
            usize::try_from(height).map_err(|_| CoreError::Pdfium("渲染高度溢出。".to_owned()))?;
        let tight_stride = w_usize
            .checked_mul(bpp)
            .ok_or_else(|| CoreError::Pdfium("步长计算溢出。".to_owned()))?;

        let gate = pdfium_gate()
            .lock()
            .map_err(|_| CoreError::Pdfium("PDFium 全局锁异常。".to_owned()))?;
        let api = gate
            .as_ref()
            .ok_or_else(|| CoreError::Pdfium("PDFium 未初始化。".to_owned()))?;

        // SAFETY: Valid `doc_handle` under `PDFIUM_GATE`.
        let page = unsafe { (api.load_page)(self.doc_handle, page_index) };
        if page.is_null() {
            return Err(CoreError::Pdfium(format!(
                "加载第 {} 页失败。",
                page_index + 1
            )));
        }
        let page_guard = PageGuard {
            page,
            close: api.close_page,
            _api: api,
        };

        // SAFETY: Allocates unmanaged PDFium bitmap of exact pixel dimensions and format.
        let bitmap = unsafe { (api.bitmap_create_ex)(width, height, format, null_mut(), 0) };
        if bitmap.is_null() {
            return Err(CoreError::Pdfium("创建 PDFium 位图缓冲区失败。".to_owned()));
        }
        let bmp_guard = BitmapGuard {
            bmp: bitmap,
            destroy: api.bitmap_destroy,
        };

        // SAFETY: Fills bitmap background solid opaque white before rendering PDF page strokes.
        unsafe {
            let _ = (api.bitmap_fill_rect)(bmp_guard.bmp, 0, 0, width, height, 0xFFFF_FFFF);
        }
        // SAFETY: Renders `page_guard.page` into `bmp_guard.bmp` cleanly under `PDFIUM_GATE`.
        unsafe {
            (api.render_page_bitmap)(
                bmp_guard.bmp,
                page_guard.page,
                0,
                0,
                width,
                height,
                0,
                render_flags,
            );
        }

        // SAFETY: Queries PDFium's owned pixel buffer pointer & stride for `bmp_guard.bmp`.
        let src_ptr = unsafe { (api.bitmap_get_buffer)(bmp_guard.bmp) }.cast::<u8>();
        // SAFETY: Queries stride for `bmp_guard.bmp`.
        let src_stride_i32 = unsafe { (api.bitmap_get_stride)(bmp_guard.bmp) };
        if src_ptr.is_null() || src_stride_i32 <= 0 {
            return Err(CoreError::Pdfium("PDFium 位图导出缓冲区无效。".to_owned()));
        }
        let src_stride = usize::try_from(src_stride_i32)
            .map_err(|_| CoreError::Pdfium("位图 stride 非法。".to_owned()))?;
        if src_stride < tight_stride {
            return Err(CoreError::Pdfium(
                "PDFium 返回的 stride 小于行字节宽度。".to_owned(),
            ));
        }

        let mut out = vec![0_u8; tight_stride * h_usize];
        for row in 0..h_usize {
            // SAFETY: `row < height` keeps offset inside bitmap backing allocation.
            let row_ptr = unsafe { src_ptr.add(row * src_stride) };
            // SAFETY: `tight_stride <= src_stride` bounds readable row bytes cleanly.
            let src_row = unsafe { std::slice::from_raw_parts(row_ptr, tight_stride) };
            let dst_row = &mut out[row * tight_stride..(row + 1) * tight_stride];
            dst_row.copy_from_slice(src_row);
            if format == FPDF_BITMAP_BGRX {
                for px in dst_row.as_chunks_mut::<4>().0 {
                    px[3] = 0xFF;
                }
            }
        }
        drop(bmp_guard);
        drop(page_guard);
        drop(gate);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_workspace_pdfium() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("assets")
            .join("pdfium.dll")
    }

    #[test]
    fn opens_and_renders_minimal_pdf_via_pdfium() {
        let dll = find_workspace_pdfium();
        if !dll.is_file() {
            return;
        }
        // Valid minimal 1-page blank PDF
        let pdf = b"%PDF-1.4\n\
            1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
            2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
            3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] >> endobj\n\
            xref\n0 4\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000058 00000 n \n\
            0000000115 00000 n \n\
            trailer << /Size 4 /Root 1 0 R >>\nstartxref\n190\n%%EOF\n"
            .to_vec();

        let doc = MemoryPdfDocument::open_bytes_with_dll(Zeroizing::new(pdf), Some(&dll)).unwrap();
        assert_eq!(doc.page_count(), 1);
        let (w_pt, h_pt) = doc.page_size_points(0).unwrap();
        assert!((w_pt - 200.0).abs() < 0.1);
        assert!((h_pt - 100.0).abs() < 0.1);

        let bgra = doc.render_page_bgra(0, 100, 50).unwrap();
        assert_eq!(bgra.len(), 100 * 50 * 4);
        assert_eq!(&bgra[0..4], &[255, 255, 255, 255]);

        let (bgr, stride) = doc.render_page_bgr24_for_print(0, 101, 50).unwrap();
        assert_eq!(stride, 304); // (101 * 3 + 3) & !3 == 304
        assert_eq!(bgr.len(), 304 * 50);
    }
}
