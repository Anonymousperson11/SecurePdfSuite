# 02 — 纯内存 PDF 渲染、物理打印守卫与取证增强

> [!NOTE]
> 本文档对应核心库 `SecurePdf.Core` 中的 PDF 内存光栅化、物理打印守卫与拍照/翻拍取证增强模块（v1.1.0）。
> v1.1.0 改用直接 P/Invoke 调用精简版无 V8/XFA 的 `pdfium.dll`，重构 `WatermarkEnhancer` 为 O(W) 滚动均值过滤并支持 EXIF 扶正，同时增强了本地端口/回环共享打印机识别并修复高 DPI 物理打印缩放bug。

## 目录

- [`src/SecurePdf.Core/Pdf/MemoryPdfRenderer.cs`](#srcsecurepdfcorepdfmemorypdfrenderercs)
- [`src/SecurePdf.Core/Printing/PrinterSecurityGuard.cs`](#srcsecurepdfcoreprintingprintersecurityguardcs)
- [`src/SecurePdf.Core/Forensics/WatermarkEnhancer.cs`](#srcsecurepdfcoreforensicswatermarkenhancercs)

## `src/SecurePdf.Core/Pdf/MemoryPdfRenderer.cs`

**模块说明**：直接基于原生 `pdfium.dll` P/Invoke 的纯内存 PDF 页面光栅化器；含全局 `PdfiumLock` 锁、基目录/`x64` 显式 DLL 加载、`MaxRenderPixels = 64M` 限制、屏幕防发糊 BGRx 渲染及打印 BGR24 旋转渲染（共 `297` 行）

```csharp
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;

namespace SecurePdf.Core.Pdf
{
    /// <summary>
    /// 纯内存 PDF 光栅化器（PDFium）。明文 PDF 只存在于被固定的托管数组中，不落盘。
    /// PDFium 本身不是线程安全的，所有调用（包括跨文档）都通过同一把全局锁串行化。
    /// </summary>
    public sealed class MemoryPdfRenderer : IDisposable
    {
        /// <summary>单次渲染的像素数硬上限（防止超大页面 / 超高缩放把内存耗尽）。调用方应按自身预算先行缩放。</summary>
        public const long MaxRenderPixels = 64L * 1000 * 1000;

        private const int FPDF_ANNOT = 0x01;
        private const int FPDF_PRINTING = 0x800;
        private const int FPDFBitmap_BGR = 2;
        private const int FPDFBitmap_BGRx = 3;
        private const uint FPDF_ERR_PASSWORD = 4;

        private static readonly object PdfiumLock = new object();
        private static bool _libraryInitialized;

        private GCHandle _pinnedPdfHandle;
        private IntPtr _documentHandle;
        private bool _disposed;

        private MemoryPdfRenderer(GCHandle pinnedHandle, IntPtr documentHandle, int pageCount)
        {
            _pinnedPdfHandle = pinnedHandle;
            _documentHandle = documentHandle;
            PageCount = pageCount;
        }

        public int PageCount { get; }

        public static MemoryPdfRenderer LoadFromMemory(byte[] pdfBytes)
        {
            if (pdfBytes == null || pdfBytes.Length == 0)
            {
                throw new ArgumentException("PDF 数据为空。", nameof(pdfBytes));
            }

            lock (PdfiumLock)
            {
                EnsureInitialized();

                GCHandle handle = GCHandle.Alloc(pdfBytes, GCHandleType.Pinned);
                try
                {
                    IntPtr doc = FPDF_LoadMemDocument(handle.AddrOfPinnedObject(), pdfBytes.Length, null);
                    if (doc == IntPtr.Zero)
                    {
                        uint err = FPDF_GetLastError();
                        throw new InvalidDataException(err == FPDF_ERR_PASSWORD
                            ? "该 PDF 自带打开密码，阅读端无法显示。请发送方先去除 PDF 密码后重新生成 .spdf。"
                            : $"无法解析文档中的 PDF 内容（PDFium 错误码 {err}），原始 PDF 可能已损坏。");
                    }

                    int pageCount = FPDF_GetPageCount(doc);
                    if (pageCount <= 0)
                    {
                        FPDF_CloseDocument(doc);
                        throw new InvalidDataException("该 PDF 不包含任何页面。");
                    }

                    return new MemoryPdfRenderer(handle, doc, pageCount);
                }
                catch
                {
                    if (handle.IsAllocated) handle.Free();
                    throw;
                }
            }
        }

        /// <summary>页面尺寸（单位：磅，1/72 英寸），已计入页面自身的 /Rotate 旋转。</summary>
        public void GetPageSizeInPoints(int pageIndex, out double widthPt, out double heightPt)
        {
            lock (PdfiumLock)
            {
                ThrowIfDisposed();
                CheckPageIndex(pageIndex);
                if (FPDF_GetPageSizeByIndex(_documentHandle, pageIndex, out widthPt, out heightPt) == 0 ||
                    !(widthPt > 0) || !(heightPt > 0))
                {
                    throw new InvalidOperationException($"无法读取 PDF 第 {pageIndex + 1} 页的页面尺寸。");
                }
            }
        }

        /// <summary>
        /// 屏幕渲染：直接写入调用方提供的 32 位 BGRx 自上而下缓冲区（例如 WPF WriteableBitmap 的后台缓冲），零拷贝。
        /// 使用灰度抗锯齿文字（不启用 LCD 子像素），避免文字彩边干扰蓝通道隐形水印取证。
        /// </summary>
        public void RenderPageToBuffer(int pageIndex, IntPtr scan0, int stride, int widthPx, int heightPx)
        {
            if (scan0 == IntPtr.Zero) throw new ArgumentNullException(nameof(scan0));
            if (stride < widthPx * 4) throw new ArgumentOutOfRangeException(nameof(stride));
            RenderCore(pageIndex, scan0, stride, widthPx, heightPx, FPDFBitmap_BGRx, 0, FPDF_ANNOT);
        }

        /// <summary>打印渲染：输出 24 位位图。<paramref name="rotate90"/> 为 true 时顺时针旋转 90°（横向页面铺满纵向纸张）。</summary>
        public Bitmap RenderPageForPrint(int pageIndex, int widthPx, int heightPx, bool rotate90)
        {
            CheckRenderSize(widthPx, heightPx);
            var bitmap = new Bitmap(widthPx, heightPx, PixelFormat.Format24bppRgb);
            try
            {
                BitmapData data = bitmap.LockBits(new Rectangle(0, 0, widthPx, heightPx), ImageLockMode.WriteOnly, PixelFormat.Format24bppRgb);
                try
                {
                    RenderCore(pageIndex, data.Scan0, data.Stride, widthPx, heightPx, FPDFBitmap_BGR, rotate90 ? 1 : 0, FPDF_ANNOT | FPDF_PRINTING);
                }
                finally
                {
                    bitmap.UnlockBits(data);
                }
                return bitmap;
            }
            catch
            {
                bitmap.Dispose();
                throw;
            }
        }

        /// <summary>按 DPI 渲染为 32 位位图（供测试与工具使用）。</summary>
        public Bitmap RenderPageToBitmap(int pageIndex, double dpiX = 144.0, double dpiY = 144.0)
        {
            GetPageSizeInPoints(pageIndex, out double widthPt, out double heightPt);
            int widthPx = Math.Max(1, (int)Math.Round(widthPt * dpiX / 72.0));
            int heightPx = Math.Max(1, (int)Math.Round(heightPt * dpiY / 72.0));
            CheckRenderSize(widthPx, heightPx);

            var bitmap = new Bitmap(widthPx, heightPx, PixelFormat.Format32bppRgb);
            try
            {
                BitmapData data = bitmap.LockBits(new Rectangle(0, 0, widthPx, heightPx), ImageLockMode.WriteOnly, PixelFormat.Format32bppRgb);
                try
                {
                    RenderCore(pageIndex, data.Scan0, data.Stride, widthPx, heightPx, FPDFBitmap_BGRx, 0, FPDF_ANNOT);
                }
                finally
                {
                    bitmap.UnlockBits(data);
                }
                return bitmap;
            }
            catch
            {
                bitmap.Dispose();
                throw;
            }
        }

        private void RenderCore(int pageIndex, IntPtr scan0, int stride, int widthPx, int heightPx, int format, int rotate, int flags)
        {
            CheckRenderSize(widthPx, heightPx);

            lock (PdfiumLock)
            {
                // 必须在锁内检查：窗口关闭（Dispose）与后台打印线程可能同时发生
                ThrowIfDisposed();
                CheckPageIndex(pageIndex);

                IntPtr page = FPDF_LoadPage(_documentHandle, pageIndex);
                if (page == IntPtr.Zero) throw new InvalidOperationException($"无法加载 PDF 第 {pageIndex + 1} 页。");

                IntPtr pdfBitmap = IntPtr.Zero;
                try
                {
                    pdfBitmap = FPDFBitmap_CreateEx(widthPx, heightPx, format, scan0, stride);
                    if (pdfBitmap == IntPtr.Zero) throw new OutOfMemoryException("无法创建 PDF 渲染缓冲区。");

                    FPDFBitmap_FillRect(pdfBitmap, 0, 0, widthPx, heightPx, 0xFFFFFFFF);
                    FPDF_RenderPageBitmap(pdfBitmap, page, 0, 0, widthPx, heightPx, rotate, flags);
                }
                finally
                {
                    if (pdfBitmap != IntPtr.Zero) FPDFBitmap_Destroy(pdfBitmap);
                    FPDF_ClosePage(page);
                }
            }
        }

        private static void CheckRenderSize(int widthPx, int heightPx)
        {
            if (widthPx <= 0) throw new ArgumentOutOfRangeException(nameof(widthPx));
            if (heightPx <= 0) throw new ArgumentOutOfRangeException(nameof(heightPx));
            if ((long)widthPx * heightPx > MaxRenderPixels)
            {
                throw new ArgumentOutOfRangeException(nameof(widthPx), "渲染尺寸超出上限。");
            }
        }

        private void CheckPageIndex(int pageIndex)
        {
            if (pageIndex < 0 || pageIndex >= PageCount) throw new ArgumentOutOfRangeException(nameof(pageIndex));
        }

        private void ThrowIfDisposed()
        {
            if (_disposed) throw new ObjectDisposedException(nameof(MemoryPdfRenderer));
        }

        public void Dispose()
        {
            lock (PdfiumLock)
            {
                if (_disposed) return;
                _disposed = true;

                if (_documentHandle != IntPtr.Zero)
                {
                    FPDF_CloseDocument(_documentHandle);
                    _documentHandle = IntPtr.Zero;
                }

                if (_pinnedPdfHandle.IsAllocated) _pinnedPdfHandle.Free();
            }
        }

        /// <summary>只从程序自身目录按完整路径加载 pdfium.dll，不走系统 DLL 搜索顺序（防止被当前目录 / PATH 中的同名文件劫持）。</summary>
        private static void EnsureInitialized()
        {
            if (_libraryInitialized) return;

            string baseDir = AppDomain.CurrentDomain.BaseDirectory;
            string[] candidates =
            {
                Path.Combine(baseDir, "pdfium.dll"),
                Path.Combine(baseDir, "x64", "pdfium.dll")
            };

            string? found = null;
            foreach (string path in candidates)
            {
                if (File.Exists(path)) { found = path; break; }
            }
            if (found == null)
            {
                throw new FileNotFoundException("缺少渲染组件 pdfium.dll，请重新安装阅读端。", candidates[0]);
            }
            if (LoadLibrary(found) == IntPtr.Zero)
            {
                int error = Marshal.GetLastWin32Error();
                throw new InvalidOperationException($"无法加载渲染组件 pdfium.dll（系统错误码 {error}）。本软件需要 64 位 Windows 10 或更高版本。");
            }

            FPDF_InitLibrary();
            _libraryInitialized = true;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr LoadLibrary(string lpFileName);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_InitLibrary();

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
        private static extern IntPtr FPDF_LoadMemDocument(IntPtr dataBuf, int size, string? password);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint FPDF_GetLastError();

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int FPDF_GetPageCount(IntPtr document);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int FPDF_GetPageSizeByIndex(IntPtr document, int pageIndex, out double width, out double height);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_CloseDocument(IntPtr document);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr FPDF_LoadPage(IntPtr document, int pageIndex);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_ClosePage(IntPtr page);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr FPDFBitmap_CreateEx(int width, int height, int format, IntPtr firstScan, int stride);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int FPDFBitmap_FillRect(IntPtr bitmap, int left, int top, int width, int height, uint color);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_RenderPageBitmap(IntPtr bitmap, IntPtr page, int startX, int startY, int sizeX, int sizeY, int rotate, int flags);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDFBitmap_Destroy(IntPtr bitmap);
    }
}
```

## `src/SecurePdf.Core/Printing/PrinterSecurityGuard.cs`

**模块说明**：基于端口白名单与端口/名称/驱动/监视器/传真/`KeepPrintedJobs`/本地文件端口/回环映射黑名单的物理打印守卫；修复 `GraphicsUnit.Pixel` 600 DPI 缩放错误，限定 `MaxPrintDpi = 300` / `MaxPrintPixels = 36M`，并在零页发送失败时通过 `SecurePrintException` 自动回滚配额（共 `736` 行）

```csharp
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Storage;

namespace SecurePdf.Core.Printing
{
    public sealed class PrinterCandidate
    {
        public string PrinterName { get; set; } = string.Empty;
        /// <summary>共享打印机所在的服务器（形如 \\SERVER）；本地打印机为空。</summary>
        public string ServerName { get; set; } = string.Empty;
        public string PortName { get; set; } = string.Empty;
        public string DriverName { get; set; } = string.Empty;
        /// <summary>端口监视器名称。打印机池含多个端口时，各端口的监视器记录在 <see cref="PortMonitors"/>。</summary>
        public string MonitorName { get; set; } = string.Empty;
        public uint Attributes { get; set; }
        public uint Status { get; set; }

        public Dictionary<string, string> PortMonitors { get; } = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        /// <summary>网络端口实际指向的主机名 / IP（取自端口配置），用于识别“指向本机”的假网络打印机。</summary>
        public List<string> NetworkTargets { get; } = new List<string>();

        public bool IsOffline => (Attributes & 0x00000400) != 0 || (Status & 0x00000080) != 0;
    }

    public sealed class PrinterValidationResult
    {
        public PrinterCandidate Candidate { get; set; } = new PrinterCandidate();
        public bool IsAllowedPhysicalPrinter { get; set; }
        public string Reason { get; set; } = string.Empty;
    }

    public sealed class PrinterScanSummary
    {
        public List<PrinterCandidate> AllowedPrinters { get; } = new List<PrinterCandidate>();
        public List<PrinterValidationResult> BlockedPrinters { get; } = new List<PrinterValidationResult>();
        public string DefaultPrinterName { get; set; } = string.Empty;
        /// <summary>无法枚举打印机时的原因（例如打印服务未运行）；正常时为空。</summary>
        public string ErrorMessage { get; set; } = string.Empty;
    }

    /// <summary>打印阶段失败。<see cref="QuotaConsumed"/> 指明本次失败是否已扣减打印次数。</summary>
    public sealed class SecurePrintException : Exception
    {
        public SecurePrintException(string message, bool quotaConsumed, Exception? innerException = null)
            : base(message, innerException)
        {
            QuotaConsumed = quotaConsumed;
        }

        public bool QuotaConsumed { get; }
    }

    /// <summary>
    /// 物理打印机校验与受控打印。校验以“端口白名单”为主（只放行 USB / WSD / IPP / 标准 TCP/IP / 并口串口 / 共享打印机），
    /// 再叠加名称、驱动、端口监视器黑名单；任何不认识的端口类型一律拒绝。
    /// </summary>
    public static class PrinterSecurityGuard
    {
        private const uint PRINTER_ENUM_LOCAL = 0x00000002;
        private const uint PRINTER_ENUM_CONNECTIONS = 0x00000004;
        private const uint PRINTER_ATTRIBUTE_KEEPPRINTEDJOBS = 0x00000100;
        private const uint PRINTER_ATTRIBUTE_FAX = 0x00004000;
        private const int ERROR_INSUFFICIENT_BUFFER = 122;

        private const int TECHNOLOGY = 2;
        private const int HORZRES = 8;
        private const int VERTRES = 10;
        private const int LOGPIXELSX = 88;
        private const int LOGPIXELSY = 90;
        private const int DT_METAFILE = 5;

        /// <summary>页面光栅化的分辨率上限。300 DPI 对文字文档已足够，继续提高只会让假脱机文件成倍变大。</summary>
        private const double MaxPrintDpi = 300.0;
        /// <summary>单页光栅的像素预算（A3@300DPI ≈ 17.4M）。超出时自动降低分辨率，保证大幅面也不会内存溢出。</summary>
        private const double MaxPrintPixels = 36.0 * 1000 * 1000;

        private static readonly string[] BlockedPortKeywords =
        {
            "PORTPROMPT:", "FILE:", "NUL:", "SHRFAX:", "XPSPORT:",
            "FOXIT", "PDF", "ONENOTE", "WPS", "127.0.0.", "LOCALHOST"
        };

        private static readonly string[] BlockedNameOrDriverKeywords =
        {
            "PDF", "XPS Document Writer", "Foxit", "Kingsoft", "CutePDF", "Bullzip", "Bluebeam",
            "OneNote", "Fax", "Snagit", "Virtual", "ImagePrinter", "Image Writer", "Document Converter",
            "CAJ", "Remote Desktop Easy Print", "ThinPrint", "TP Output Gateway", "Evernote",
            "虚拟", "传真"
        };

        private static readonly string[] BlockedMonitorKeywords =
        {
            "Document Writer", "PDF", "XPS", "Fax", "OneNote", "AppMon", "Nuance", "Foxit", "Redirect", "ThinPrint"
        };

        private static readonly string[] PhysicalPortPrefixes =
        {
            "USB", "WSD", "IP_", "IPP", "DOT4", "CNBJNP_", "CNMFNP_"
        };

        private static readonly string[] PhysicalMonitorKeywords =
        {
            "TCP/IP", "USB", "WSD", "IPP", "BJNP", "EpsonNet"
        };

        // ------------------------------------------------------------------ 枚举与校验

        public static PrinterScanSummary ScanPrinters()
        {
            var summary = new PrinterScanSummary();
            try
            {
                Dictionary<string, string> portMonitors = EnumeratePortMonitors();
                foreach (PrinterCandidate candidate in EnumerateRawPrinters(portMonitors))
                {
                    PrinterValidationResult res = EvaluatePrinterCandidate(candidate);
                    if (res.IsAllowedPhysicalPrinter) summary.AllowedPrinters.Add(candidate);
                    else summary.BlockedPrinters.Add(res);
                }
                summary.DefaultPrinterName = ReadDefaultPrinterName();
            }
            catch (System.ComponentModel.Win32Exception ex)
            {
                summary.ErrorMessage = $"无法读取系统打印机列表（请确认 Print Spooler 打印服务正在运行）：{ex.Message}";
            }
            return summary;
        }

        public static PrinterValidationResult EvaluatePrinterCandidate(PrinterCandidate candidate)
        {
            if (candidate == null) throw new ArgumentNullException(nameof(candidate));

            string name = (candidate.PrinterName ?? string.Empty).Trim();
            string driver = (candidate.DriverName ?? string.Empty).Trim();
            string port = (candidate.PortName ?? string.Empty).Trim();
            string server = (candidate.ServerName ?? string.Empty).Trim().TrimStart('\\');

            if ((candidate.Attributes & PRINTER_ATTRIBUTE_FAX) != 0) return Reject(candidate, "禁止使用传真设备。");
            if (string.IsNullOrEmpty(name)) return Reject(candidate, "打印机名称为空。");
            if ((candidate.Attributes & PRINTER_ATTRIBUTE_KEEPPRINTEDJOBS) != 0)
            {
                return Reject(candidate, "该打印机开启了“保留打印的文档”，打印内容会留存在磁盘上。请在打印机属性的“高级”页关闭后重试。");
            }

            HashSet<string> localAddresses = DeviceAndNetworkContext.GetAllLocalAddresses();
            if (server.Length > 0 && IsLocalHost(server, localAddresses))
            {
                return Reject(candidate, "禁止使用通过共享连接回本机的打印机。");
            }
            foreach (string target in candidate.NetworkTargets)
            {
                if (IsLocalHost(target, localAddresses)) return Reject(candidate, $"拦截指向本机的网络打印端口 ({target})");
            }

            foreach (string kw in BlockedNameOrDriverKeywords)
            {
                if (ContainsIgnoreCase(name, kw) || ContainsIgnoreCase(driver, kw))
                {
                    return Reject(candidate, $"拦截虚拟 / 文档类打印机 ({kw})");
                }
            }

            string[] portParts = port.Split(new[] { ',' }, StringSplitOptions.RemoveEmptyEntries);
            if (portParts.Length == 0) return Reject(candidate, "打印机未绑定有效物理端口。");

            // 打印机池：必须每一个端口都通过校验
            foreach (string rawPart in portParts)
            {
                string part = rawPart.Trim();
                string monitor = MonitorFor(candidate, part);

                foreach (string blocked in BlockedPortKeywords)
                {
                    if (ContainsIgnoreCase(part, blocked)) return Reject(candidate, $"拦截虚拟 / 文件输出端口: {part}");
                }
                foreach (string mkw in BlockedMonitorKeywords)
                {
                    if (ContainsIgnoreCase(monitor, mkw)) return Reject(candidate, $"拦截虚拟端口监视器: {monitor}");
                }

                if (part.Length >= 2 && char.IsLetter(part[0]) && part[1] == ':' && (part.Length == 2 || part[2] == '\\' || part[2] == '/'))
                {
                    return Reject(candidate, $"拦截本地文件输出端口: {part}");
                }

                bool isUnc = part.StartsWith(@"\\", StringComparison.Ordinal);
                if (isUnc)
                {
                    // 只接受 \\服务器\共享名 形式的共享打印机；更深的路径是“输出到网络文件”
                    string[] unc = part.Substring(2).Split(new[] { '\\' }, StringSplitOptions.None);
                    if (unc.Length != 2 || unc[0].Length == 0 || unc[1].Length == 0 || unc[0] == "." || unc[0] == "?")
                    {
                        return Reject(candidate, $"拦截文件 / 管道输出端口: {part}");
                    }
                    if (IsLocalHost(unc[0], localAddresses))
                    {
                        return Reject(candidate, $"拦截指向本机共享的打印端口: {part}");
                    }
                }

                bool isParallelOrSerial = IsParallelOrSerialPort(part);
                if (IsLocalPortMonitor(monitor) && !isParallelOrSerial && !isUnc)
                {
                    // “本地端口”监视器下除并口 / 串口 / 共享打印机外的端口本质是把打印数据写入文件；
                    // 即使故意取名为 USB999、IP_1.2.3.4 也一样（真正的 USB / 网络端口由各自的监视器管理）
                    return Reject(candidate, $"拦截自定义本地端口（输出到文件）: {part}");
                }

                string literalHost = part.StartsWith("IP_", StringComparison.OrdinalIgnoreCase) ? part.Substring(3) : part;
                if (LooksLikeIpv4(literalHost) && IsLocalHost(literalHost, localAddresses))
                {
                    return Reject(candidate, $"拦截指向本机的网络打印端口 ({part})");
                }

                bool looksPhysical = isParallelOrSerial || isUnc ||
                                     StartsWithAny(part, PhysicalPortPrefixes) ||
                                     LooksLikeIpv4(part) ||
                                     PhysicalMonitorKeywords.Any(k => ContainsIgnoreCase(monitor, k));
                if (!looksPhysical)
                {
                    return Reject(candidate, $"未识别为物理或标准网络端口 ({part})");
                }
            }

            return new PrinterValidationResult
            {
                Candidate = candidate,
                IsAllowedPhysicalPrinter = true,
                Reason = "已验证物理打印机"
            };
        }

        private static string MonitorFor(PrinterCandidate candidate, string port)
        {
            if (candidate.PortMonitors.TryGetValue(port, out string? monitor) && !string.IsNullOrEmpty(monitor)) return monitor!;
            return (candidate.MonitorName ?? string.Empty).Trim();
        }

        private static bool ContainsIgnoreCase(string text, string keyword)
        {
            return text.IndexOf(keyword, StringComparison.OrdinalIgnoreCase) >= 0;
        }

        private static bool StartsWithAny(string text, string[] prefixes)
        {
            foreach (string p in prefixes)
            {
                if (text.StartsWith(p, StringComparison.OrdinalIgnoreCase)) return true;
            }
            return false;
        }

        private static bool IsLocalPortMonitor(string monitor)
        {
            return monitor.Equals("Local Monitor", StringComparison.OrdinalIgnoreCase) ||
                   monitor.Equals("Local Port", StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>LPT1: / COM3 这类真实的并口、串口。</summary>
        private static bool IsParallelOrSerialPort(string port)
        {
            if (!port.StartsWith("LPT", StringComparison.OrdinalIgnoreCase) &&
                !port.StartsWith("COM", StringComparison.OrdinalIgnoreCase)) return false;

            string rest = port.Substring(3).TrimEnd(':');
            return rest.Length > 0 && rest.Length <= 3 && rest.All(c => c >= '0' && c <= '9');
        }

        private static bool LooksLikeIpv4(string text)
        {
            string[] parts = text.Split('.');
            if (parts.Length != 4) return false;
            foreach (string p in parts)
            {
                if (p.Length == 0 || p.Length > 3 || !p.All(c => c >= '0' && c <= '9') || int.Parse(p) > 255) return false;
            }
            return true;
        }

        private static bool IsLocalHost(string host, HashSet<string> localAddresses)
        {
            host = (host ?? string.Empty).Trim().Trim('[', ']').TrimEnd('.');
            if (host.Length == 0) return false;

            if (host.Equals("localhost", StringComparison.OrdinalIgnoreCase) || host == "." || host == "::1" ||
                host == "0.0.0.0" || host.StartsWith("127.", StringComparison.Ordinal))
            {
                return true;
            }

            string machine = Environment.MachineName;
            if (host.Equals(machine, StringComparison.OrdinalIgnoreCase) ||
                host.StartsWith(machine + ".", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            return localAddresses.Contains(host);
        }

        private static PrinterValidationResult Reject(PrinterCandidate candidate, string reason)
        {
            return new PrinterValidationResult
            {
                Candidate = candidate,
                IsAllowedPhysicalPrinter = false,
                Reason = reason
            };
        }

        // ------------------------------------------------------------------ 受控打印

        /// <summary>
        /// 校验目标打印机 → 预占一次打印额度 → 逐页光栅化并叠加追踪水印后送往打印机。
        /// 一页都没有送出就失败时自动归还额度；已有页面送出后失败则额度保持扣减。
        /// </summary>
        public static QuotaSnapshot PrintDocumentSecurely(
            MemoryPdfRenderer renderer,
            string printerName,
            IReadOnlyList<int> zeroBasedPageIndices,
            string watermarkText,
            LocalQuotaStore quotaStore,
            Guid docId,
            int policyMaxPrints,
            DateTimeOffset? expiresAtUtc)
        {
            if (renderer == null) throw new ArgumentNullException(nameof(renderer));
            if (quotaStore == null) throw new ArgumentNullException(nameof(quotaStore));
            if (zeroBasedPageIndices == null || zeroBasedPageIndices.Count == 0)
            {
                throw new ArgumentException("至少需要选择一页进行打印。", nameof(zeroBasedPageIndices));
            }
            foreach (int pageIndex in zeroBasedPageIndices)
            {
                if (pageIndex < 0 || pageIndex >= renderer.PageCount)
                {
                    throw new ArgumentOutOfRangeException(nameof(zeroBasedPageIndices), "页码超出文档范围。");
                }
            }

            // 打印前重新扫描一次，防止在选择打印机之后端口被偷偷改成文件输出
            PrinterScanSummary scan = ScanPrinters();
            PrinterCandidate? target = scan.AllowedPrinters.FirstOrDefault(
                p => p.PrinterName.Equals(printerName, StringComparison.OrdinalIgnoreCase));
            if (target == null)
            {
                throw new SecurePrintException($"拒绝打印：“{printerName}”未通过物理打印机安全校验（可能为虚拟 PDF 打印机，或已被移除）。", false);
            }

            QuotaSnapshot reserved = quotaStore.ReservePrintSlot(docId, policyMaxPrints, expiresAtUtc);

            IntPtr hdc = IntPtr.Zero;
            bool docStarted = false;
            int pagesSent = 0;

            try
            {
                hdc = CreateDC("WINSPOOL", target.PrinterName, null, IntPtr.Zero);
                if (hdc == IntPtr.Zero)
                {
                    throw new InvalidOperationException($"无法连接打印机（系统错误码 {Marshal.GetLastWin32Error()}）。");
                }
                if (GetDeviceCaps(hdc, TECHNOLOGY) == DT_METAFILE)
                {
                    throw new InvalidOperationException("目标设备报告为虚拟图元设备，已拒绝输出。");
                }

                var docInfo = new DOCINFO
                {
                    cbSize = Marshal.SizeOf(typeof(DOCINFO)),
                    lpszDocName = "SecurePdf Protected Document",
                    lpszOutput = IntPtr.Zero,          // 永远不指定输出文件
                    lpszDatatype = IntPtr.Zero,
                    fwType = 0
                };

                if (StartDoc(hdc, ref docInfo) <= 0)
                {
                    throw new InvalidOperationException($"打印机拒绝了打印作业（系统错误码 {Marshal.GetLastWin32Error()}）。");
                }
                docStarted = true;

                foreach (int pageIndex in zeroBasedPageIndices)
                {
                    if (StartPage(hdc) <= 0)
                    {
                        throw new InvalidOperationException($"打印第 {pageIndex + 1} 页时打印机返回错误（系统错误码 {Marshal.GetLastWin32Error()}）。");
                    }

                    DrawPageToDevice(hdc, renderer, pageIndex, watermarkText);

                    if (EndPage(hdc) <= 0)
                    {
                        throw new InvalidOperationException($"提交第 {pageIndex + 1} 页时打印机返回错误（系统错误码 {Marshal.GetLastWin32Error()}）。");
                    }
                    pagesSent++;
                }

                if (EndDoc(hdc) <= 0)
                {
                    throw new InvalidOperationException($"提交打印作业时打印机返回错误（系统错误码 {Marshal.GetLastWin32Error()}）。");
                }
                docStarted = false;
                return reserved;
            }
            catch (Exception ex)
            {
                if (docStarted && hdc != IntPtr.Zero) AbortDoc(hdc);

                bool quotaConsumed = true;
                if (pagesSent == 0)
                {
                    try
                    {
                        quotaStore.RollbackPrintSlot(reserved);
                        quotaConsumed = false;
                    }
                    catch (QuotaStoreException) { }
                }

                string suffix = quotaConsumed
                    ? $"已有 {pagesSent} 页送往打印机，本次打印次数已扣减。"
                    : "尚未送出任何页面，本次打印次数未扣减。";
                throw new SecurePrintException($"{ex.Message}{Environment.NewLine}{suffix}", quotaConsumed, ex);
            }
            finally
            {
                if (hdc != IntPtr.Zero) DeleteDC(hdc);
            }
        }

        /// <summary>
        /// 把一页 PDF 等比缩放、居中绘制到打印机可打印区域内；横向页面在纵向纸张上（或反之）自动旋转 90°。
        /// </summary>
        private static void DrawPageToDevice(IntPtr hdc, MemoryPdfRenderer renderer, int pageIndex, string watermarkText)
        {
            int deviceW = GetDeviceCaps(hdc, HORZRES);
            int deviceH = GetDeviceCaps(hdc, VERTRES);
            int dpiX = GetDeviceCaps(hdc, LOGPIXELSX);
            int dpiY = GetDeviceCaps(hdc, LOGPIXELSY);
            if (deviceW <= 0 || deviceH <= 0 || dpiX <= 0 || dpiY <= 0)
            {
                throw new InvalidOperationException("打印机返回了无效的纸张尺寸。");
            }

            renderer.GetPageSizeInPoints(pageIndex, out double pageWPt, out double pageHPt);

            double paperWIn = deviceW / (double)dpiX;
            double paperHIn = deviceH / (double)dpiY;

            bool pageIsLandscape = pageWPt > pageHPt * 1.02;
            bool pageIsPortrait = pageHPt > pageWPt * 1.02;
            bool paperIsLandscape = paperWIn > paperHIn;
            bool rotate = (pageIsLandscape && !paperIsLandscape) || (pageIsPortrait && paperIsLandscape);

            double pageWIn = (rotate ? pageHPt : pageWPt) / 72.0;
            double pageHIn = (rotate ? pageWPt : pageHPt) / 72.0;
            double fit = Math.Min(paperWIn / pageWIn, paperHIn / pageHIn);
            double drawWIn = pageWIn * fit;
            double drawHIn = pageHIn * fit;

            int destW = Math.Max(1, Math.Min(deviceW, (int)Math.Round(drawWIn * dpiX)));
            int destH = Math.Max(1, Math.Min(deviceH, (int)Math.Round(drawHIn * dpiY)));
            int destX = (deviceW - destW) / 2;
            int destY = (deviceH - destH) / 2;

            double rasterDpi = Math.Min(MaxPrintDpi, Math.Max(dpiX, dpiY));
            double pixels = drawWIn * drawHIn * rasterDpi * rasterDpi;
            if (pixels > MaxPrintPixels) rasterDpi *= Math.Sqrt(MaxPrintPixels / pixels);

            int rasterW = Math.Max(1, (int)Math.Round(drawWIn * rasterDpi));
            int rasterH = Math.Max(1, (int)Math.Round(drawHIn * rasterDpi));

            using (Bitmap pageBitmap = renderer.RenderPageForPrint(pageIndex, rasterW, rasterH, rotate))
            {
                BurnWatermarkIntoBitmap(pageBitmap, watermarkText);

                using (Graphics g = Graphics.FromHdc(hdc))
                using (var attributes = new ImageAttributes())
                {
                    // 打印机 DC 上 GDI+ 的默认单位是 1/100 英寸，必须切到设备像素，否则 600 DPI 下画面会被放大 6 倍
                    g.PageUnit = GraphicsUnit.Pixel;
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.Half;
                    attributes.SetWrapMode(WrapMode.TileFlipXY);
                    g.DrawImage(pageBitmap, new Rectangle(destX, destY, destW, destH), 0, 0, rasterW, rasterH, GraphicsUnit.Pixel, attributes);
                }
            }
        }

        private static void BurnWatermarkIntoBitmap(Bitmap bmp, string watermarkText)
        {
            if (string.IsNullOrWhiteSpace(watermarkText)) return;

            using (Graphics g = Graphics.FromImage(bmp))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.TextRenderingHint = TextRenderingHint.AntiAlias;
                float fontSize = Math.Max(14f, Math.Min(bmp.Width, bmp.Height) / 44f);
                using (var font = new Font("Microsoft YaHei UI", fontSize, FontStyle.Bold, GraphicsUnit.Pixel))
                using (var brush = new SolidBrush(Color.FromArgb(42, 120, 120, 120)))
                {
                    SizeF textSize = g.MeasureString(watermarkText, font);
                    float stepX = Math.Max(220f, textSize.Width * 1.35f);
                    float stepY = Math.Max(160f, textSize.Height * 3.8f);

                    g.TranslateTransform(bmp.Width / 2f, bmp.Height / 2f);
                    g.RotateTransform(-30f);
                    float diag = (float)Math.Sqrt(bmp.Width * (double)bmp.Width + bmp.Height * (double)bmp.Height);

                    for (float y = -diag; y < diag; y += stepY)
                    {
                        for (float x = -diag; x < diag; x += stepX)
                        {
                            g.DrawString(watermarkText, font, brush, x, y);
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------------ Win32 枚举

        private static Dictionary<string, string> EnumeratePortMonitors()
        {
            var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            EnumerateSpooler<PORT_INFO_2>(
                (IntPtr buffer, uint size, ref uint needed, ref uint returned) => EnumPorts(null, 2, buffer, size, ref needed, ref returned),
                info =>
                {
                    if (!string.IsNullOrEmpty(info.pPortName)) map[info.pPortName!] = info.pMonitorName ?? string.Empty;
                });
            return map;
        }

        private static List<PrinterCandidate> EnumerateRawPrinters(Dictionary<string, string> portMonitors)
        {
            var list = new List<PrinterCandidate>();
            const uint flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;

            EnumerateSpooler<PRINTER_INFO_2>(
                (IntPtr buffer, uint size, ref uint needed, ref uint returned) => EnumPrinters(flags, null, 2, buffer, size, ref needed, ref returned),
                info =>
                {
                    var candidate = new PrinterCandidate
                    {
                        PrinterName = info.pPrinterName ?? string.Empty,
                        ServerName = info.pServerName ?? string.Empty,
                        PortName = info.pPortName ?? string.Empty,
                        DriverName = info.pDriverName ?? string.Empty,
                        Attributes = info.Attributes,
                        Status = info.Status
                    };

                    foreach (string rawPort in candidate.PortName.Split(new[] { ',' }, StringSplitOptions.RemoveEmptyEntries))
                    {
                        string port = rawPort.Trim();
                        if (portMonitors.TryGetValue(port, out string? monitorName) && monitorName != null)
                        {
                            candidate.PortMonitors[port] = monitorName;
                            if (candidate.MonitorName.Length == 0) candidate.MonitorName = monitorName;
                        }
                        AddNetworkTargets(port, candidate.NetworkTargets);
                    }

                    list.Add(candidate);
                });
            return list;
        }

        private delegate bool SpoolerEnumCall(IntPtr buffer, uint size, ref uint needed, ref uint returned);

        private static void EnumerateSpooler<T>(SpoolerEnumCall call, Action<T> onItem) where T : struct
        {
            uint needed = 0;
            uint returned = 0;

            // 列表可能在两次调用之间变化，缓冲区不够时重试
            for (int attempt = 0; attempt < 4; attempt++)
            {
                if (call(IntPtr.Zero, 0, ref needed, ref returned)) return;      // 没有任何条目
                int error = Marshal.GetLastWin32Error();
                if (error != ERROR_INSUFFICIENT_BUFFER) throw new System.ComponentModel.Win32Exception(error);
                if (needed == 0) return;

                IntPtr buffer = Marshal.AllocHGlobal((int)needed);
                try
                {
                    if (call(buffer, needed, ref needed, ref returned))
                    {
                        int structSize = Marshal.SizeOf(typeof(T));
                        for (uint i = 0; i < returned; i++)
                        {
                            onItem((T)Marshal.PtrToStructure(IntPtr.Add(buffer, (int)i * structSize), typeof(T)));
                        }
                        return;
                    }

                    error = Marshal.GetLastWin32Error();
                    if (error != ERROR_INSUFFICIENT_BUFFER) throw new System.ComponentModel.Win32Exception(error);
                }
                finally
                {
                    Marshal.FreeHGlobal(buffer);
                }
            }
        }

        /// <summary>读取网络端口配置里的目标主机（标准 TCP/IP 端口等把 HostName / IPAddress 存在端口监视器的注册表项下）。</summary>
        private static void AddNetworkTargets(string portName, List<string> targets)
        {
            try
            {
                using (RegistryKey? monitors = Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Control\Print\Monitors"))
                {
                    if (monitors == null) return;
                    foreach (string monitorName in monitors.GetSubKeyNames())
                    {
                        using (RegistryKey? portKey = monitors.OpenSubKey(monitorName + @"\Ports\" + portName))
                        {
                            if (portKey == null) continue;
                            foreach (string valueName in new[] { "HostName", "IPAddress" })
                            {
                                if (portKey.GetValue(valueName) is string value && value.Trim().Length > 0)
                                {
                                    targets.Add(value.Trim());
                                }
                            }
                        }
                    }
                }
            }
            catch (Exception ex) when (ex is System.Security.SecurityException || ex is UnauthorizedAccessException || ex is System.IO.IOException || ex is ArgumentException) { }
        }

        private static string ReadDefaultPrinterName()
        {
            int size = 0;
            GetDefaultPrinter(null, ref size);
            if (size <= 0) return string.Empty;

            var sb = new StringBuilder(size);
            return GetDefaultPrinter(sb, ref size) ? sb.ToString() : string.Empty;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct PRINTER_INFO_2
        {
            public string? pServerName;
            public string? pPrinterName;
            public string? pShareName;
            public string? pPortName;
            public string? pDriverName;
            public string? pComment;
            public string? pLocation;
            public IntPtr pDevMode;
            public string? pSepFile;
            public string? pPrintProcessor;
            public string? pDatatype;
            public string? pParameters;
            public IntPtr pSecurityDescriptor;
            public uint Attributes;
            public uint Priority;
            public uint DefaultPriority;
            public uint StartTime;
            public uint UntilTime;
            public uint Status;
            public uint cJobs;
            public uint AveragePPM;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct PORT_INFO_2
        {
            public string? pPortName;
            public string? pMonitorName;
            public string? pDescription;
            public uint fPortType;
            public uint Reserved;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct DOCINFO
        {
            public int cbSize;
            public string lpszDocName;
            public IntPtr lpszOutput;
            public IntPtr lpszDatatype;
            public uint fwType;
        }

        [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool EnumPrinters(uint Flags, string? Name, uint Level, IntPtr pPrinterEnum, uint cbBuf, ref uint pcbNeeded, ref uint pcReturned);

        [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool EnumPorts(string? pName, uint Level, IntPtr pPorts, uint cbBuf, ref uint pcbNeeded, ref uint pcReturned);

        [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool GetDefaultPrinter(StringBuilder? pszBuffer, ref int pcchBuffer);

        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateDC(string lpszDriver, string lpszDevice, string? lpszOutput, IntPtr lpInitData);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern bool DeleteDC(IntPtr hdc);

        [DllImport("gdi32.dll")]
        private static extern int GetDeviceCaps(IntPtr hdc, int nIndex);

        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern int StartDoc(IntPtr hdc, ref DOCINFO lpdi);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern int EndDoc(IntPtr hdc);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern int AbortDoc(IntPtr hdc);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern int StartPage(IntPtr hdc);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern int EndPage(IntPtr hdc);
    }
}
```

## `src/SecurePdf.Core/Forensics/WatermarkEnhancer.cs`

**模块说明**：低内存 O(W) 滚动环形窗口局部背景减除与蓝/绿通道微偏移提取 (`ChromaStegoExtract`)；支持 EXIF `0x0112` 自动扶正、`MaxPixels = 50M` 保护与 8 位灰度调色板输出（共 `292` 行）

```csharp
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

namespace SecurePdf.Core.Forensics
{
    public enum ForensicFilterMode
    {
        ChromaStegoExtract = 0,
        AdaptiveBackgroundSubtract = 1,
        GlobalEqualizeAndInvert = 2
    }

    /// <summary>
    /// 从手机翻拍照片 / 截图中提取阅读端叠加的低对比度隐形溯源水印（IP、时间、设备码）。
    /// 局部背景减除用“滑动窗口列累加”实现：时间 O(W×H)，额外内存只有 O(W)，
    /// 4800 万像素的照片也不需要再为积分图分配数百 MB。输出为 8 位灰度图。
    /// </summary>
    public static class WatermarkEnhancer
    {
        /// <summary>参与取证计算的最大像素数。更大的照片（1 亿 / 2 亿像素直出）先等比缩小，屏幕内容本身远没有这么高的有效分辨率。</summary>
        public const long MaxPixels = 50L * 1000 * 1000;

        private const int ExifOrientationId = 0x0112;

        /// <summary>
        /// 读取图片并规整为 24 位位图：按 EXIF 方向转正（竖拍照片不再横着显示）、超大图等比缩小、透明区域铺白。
        /// 文件整体读入内存后解码，不会一直占用原文件。
        /// </summary>
        public static Bitmap LoadImageUpright(string path)
        {
            byte[] data = File.ReadAllBytes(path);
            using (var ms = new MemoryStream(data, writable: false))
            using (var decoded = new Bitmap(ms))
            {
                int w = decoded.Width;
                int h = decoded.Height;
                double scale = (long)w * h > MaxPixels ? Math.Sqrt(MaxPixels / ((double)w * h)) : 1.0;
                int targetW = Math.Max(1, (int)Math.Round(w * scale));
                int targetH = Math.Max(1, (int)Math.Round(h * scale));

                var upright = new Bitmap(targetW, targetH, PixelFormat.Format24bppRgb);
                try
                {
                    DrawNormalized(decoded, upright, scale < 1.0);

                    RotateFlipType orientation = ReadExifOrientation(decoded);
                    if (orientation != RotateFlipType.RotateNoneFlipNone) upright.RotateFlip(orientation);
                    return upright;
                }
                catch
                {
                    upright.Dispose();
                    throw;
                }
            }
        }

        public static Bitmap Enhance(Bitmap source, ForensicFilterMode mode)
        {
            if (source == null) throw new ArgumentNullException(nameof(source));

            int w = source.Width;
            int h = source.Height;
            var rect = new Rectangle(0, 0, w, h);

            Bitmap? normalized = null;
            Bitmap? result = null;
            try
            {
                Bitmap work = source;
                if (source.PixelFormat != PixelFormat.Format24bppRgb)
                {
                    normalized = new Bitmap(w, h, PixelFormat.Format24bppRgb);
                    DrawNormalized(source, normalized, false);
                    work = normalized;
                }

                result = new Bitmap(w, h, PixelFormat.Format8bppIndexed);
                ColorPalette palette = result.Palette;
                for (int i = 0; i < 256; i++) palette.Entries[i] = Color.FromArgb(i, i, i);
                result.Palette = palette;

                BitmapData srcData = work.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format24bppRgb);
                try
                {
                    BitmapData dstData = result.LockBits(rect, ImageLockMode.WriteOnly, PixelFormat.Format8bppIndexed);
                    try
                    {
                        unsafe
                        {
                            byte* src = (byte*)srcData.Scan0;
                            byte* dst = (byte*)dstData.Scan0;

                            switch (mode)
                            {
                                case ForensicFilterMode.ChromaStegoExtract:
                                    ApplyLocalResidual(src, srcData.Stride, dst, dstData.Stride, w, h, chroma: true);
                                    break;
                                case ForensicFilterMode.AdaptiveBackgroundSubtract:
                                    ApplyLocalResidual(src, srcData.Stride, dst, dstData.Stride, w, h, chroma: false);
                                    break;
                                default:
                                    ApplyHistogramEqualize(src, srcData.Stride, dst, dstData.Stride, w, h, invert: true);
                                    break;
                            }
                        }
                    }
                    finally
                    {
                        result.UnlockBits(dstData);
                    }
                }
                finally
                {
                    work.UnlockBits(srcData);
                }

                Bitmap finished = result;
                result = null;
                return finished;
            }
            finally
            {
                result?.Dispose();
                normalized?.Dispose();
            }
        }

        private static void DrawNormalized(Image source, Bitmap target, bool highQualityScale)
        {
            using (Graphics g = Graphics.FromImage(target))
            using (var attributes = new ImageAttributes())
            {
                g.Clear(Color.White);
                g.InterpolationMode = highQualityScale ? InterpolationMode.HighQualityBicubic : InterpolationMode.NearestNeighbor;
                g.PixelOffsetMode = PixelOffsetMode.Half;
                attributes.SetWrapMode(WrapMode.TileFlipXY);
                g.DrawImage(source, new Rectangle(0, 0, target.Width, target.Height), 0, 0, source.Width, source.Height, GraphicsUnit.Pixel, attributes);
            }
        }

        private static RotateFlipType ReadExifOrientation(Image image)
        {
            try
            {
                if (Array.IndexOf(image.PropertyIdList, ExifOrientationId) < 0) return RotateFlipType.RotateNoneFlipNone;

                PropertyItem item = image.GetPropertyItem(ExifOrientationId);
                if (item?.Value == null || item.Value.Length < 2) return RotateFlipType.RotateNoneFlipNone;

                switch (BitConverter.ToUInt16(item.Value, 0))
                {
                    case 2: return RotateFlipType.RotateNoneFlipX;
                    case 3: return RotateFlipType.Rotate180FlipNone;
                    case 4: return RotateFlipType.Rotate180FlipX;
                    case 5: return RotateFlipType.Rotate90FlipX;
                    case 6: return RotateFlipType.Rotate90FlipNone;
                    case 7: return RotateFlipType.Rotate270FlipX;
                    case 8: return RotateFlipType.Rotate270FlipNone;
                    default: return RotateFlipType.RotateNoneFlipNone;
                }
            }
            catch (ArgumentException)
            {
                return RotateFlipType.RotateNoneFlipNone;   // 图片不含该属性
            }
        }

        /// <summary>
        /// 每个像素减去以它为中心的 (2r+1)² 邻域均值后放大显示。
        /// 色度模式取信号 (B−R)·¾ + (B−G)·¼：蓝色偏移的水印笔画为正，中性的黑字 / 白纸 / 明暗渐变接近 0，
        /// 再减去局部均值即可抵消屏幕亮度不均与宽幅彩虹摩尔纹；亮度模式用于照片本身接近灰度、没有色差的情况。
        /// 信号以整数保存（色度单位 1/4，亮度单位 1/256），窗口求和没有累计误差。
        /// </summary>
        private static unsafe void ApplyLocalResidual(byte* src, int srcStride, byte* dst, int dstStride, int w, int h, bool chroma)
        {
            int radius = chroma ? 18 : 15;
            double gain = chroma ? 11.0 : 9.0;
            double unit = chroma ? 0.25 : 1.0 / 256.0;
            double baseLevel = chroma ? 225.0 : 128.0;

            long[] columnSums = new long[w];
            long[] prefix = new long[w + 1];
            int[] rowSignal = new int[w];
            int firstRow = 0;      // columnSums 当前覆盖的行区间 [firstRow, lastRow]
            int lastRow = -1;

            for (int y = 0; y < h; y++)
            {
                int wantFirst = Math.Max(0, y - radius);
                int wantLast = Math.Min(h - 1, y + radius);

                while (lastRow < wantLast)
                {
                    lastRow++;
                    ReadSignalRow(src + (long)lastRow * srcStride, w, chroma, rowSignal);
                    for (int x = 0; x < w; x++) columnSums[x] += rowSignal[x];
                }
                while (firstRow < wantFirst)
                {
                    ReadSignalRow(src + (long)firstRow * srcStride, w, chroma, rowSignal);
                    for (int x = 0; x < w; x++) columnSums[x] -= rowSignal[x];
                    firstRow++;
                }

                long running = 0;
                for (int x = 0; x < w; x++)
                {
                    running += columnSums[x];
                    prefix[x + 1] = running;
                }

                ReadSignalRow(src + (long)y * srcStride, w, chroma, rowSignal);
                int rows = wantLast - wantFirst + 1;
                byte* outRow = dst + (long)y * dstStride;

                for (int x = 0; x < w; x++)
                {
                    int x0 = Math.Max(0, x - radius);
                    int x1 = Math.Min(w - 1, x + radius);
                    int count = rows * (x1 - x0 + 1);

                    double localMean = (prefix[x1 + 1] - prefix[x0]) * unit / count;
                    double diff = rowSignal[x] * unit - localMean;
                    if (chroma) diff = -diff;       // 水印显示为深色笔画，背景接近白色

                    int mapped = (int)Math.Round(baseLevel + diff * gain);
                    outRow[x] = (byte)(mapped < 0 ? 0 : (mapped > 255 ? 255 : mapped));
                }
            }
        }

        private static unsafe void ReadSignalRow(byte* row, int w, bool chroma, int[] signal)
        {
            if (chroma)
            {
                for (int x = 0; x < w; x++, row += 3)
                {
                    int b = row[0];
                    signal[x] = 3 * (b - row[2]) + (b - row[1]);
                }
            }
            else
            {
                for (int x = 0; x < w; x++, row += 3)
                {
                    signal[x] = row[2] * 77 + row[1] * 150 + row[0] * 29;
                }
            }
        }

        private static unsafe void ApplyHistogramEqualize(byte* src, int srcStride, byte* dst, int dstStride, int w, int h, bool invert)
        {
            int[] hist = new int[256];
            long total = (long)w * h;

            for (int y = 0; y < h; y++)
            {
                byte* row = src + (long)y * srcStride;
                for (int x = 0; x < w; x++, row += 3)
                {
                    hist[(row[2] * 77 + row[1] * 150 + row[0] * 29) >> 8]++;
                }
            }

            byte[] lut = new byte[256];
            long cdf = 0;
            long cdfMin = 0;
            for (int i = 0; i < 256; i++)
            {
                cdf += hist[i];
                if (cdfMin == 0 && cdf > 0) cdfMin = cdf;
                int mapped = total > cdfMin ? (int)Math.Round((cdf - cdfMin) * 255.0 / (total - cdfMin)) : 0;
                mapped = Math.Max(0, Math.Min(255, mapped));
                lut[i] = (byte)(invert ? 255 - mapped : mapped);
            }

            for (int y = 0; y < h; y++)
            {
                byte* row = src + (long)y * srcStride;
                byte* outRow = dst + (long)y * dstStride;
                for (int x = 0; x < w; x++, row += 3)
                {
                    outRow[x] = lut[(row[2] * 77 + row[1] * 150 + row[0] * 29) >> 8];
                }
            }
        }
    }
}
```

---

[返回总览 README](../README.md)
