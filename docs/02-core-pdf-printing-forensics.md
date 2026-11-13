# 02 — 纯内存 PDF 渲染、物理打印守卫与取证增强

本模块涵盖阅读端的底层渲染隔离、虚拟打印机拦截逻辑以及创建端用于手机翻拍溯源的像素级图像提取算法。

> [!NOTE]
> 为了防止通过“打印为 PDF”或截取临时文件绕过保护，明文 PDF 始终驻留在固定内存 (`MemoryPdfRenderer`)，打印时直接向物理打印机提交光栅页面并叠加可见追踪水印。

## 目录

1. [`src/SecurePdf.Core/Pdf/MemoryPdfRenderer.cs`](#srcsecurepdfcorepdfmemorypdfrenderercs) — 基于 `PdfiumViewer` + `pdfium.dll` 的纯内存 PDF 页面光栅化器，析构时释放内存流

2. [`src/SecurePdf.Core/Printing/PrinterSecurityGuard.cs`](#srcsecurepdfcoreprintingprintersecurityguardcs) — 枚举本地/网络打印机并通过端口名、驱动名、监视器、传真属性及 GDI `DT_METAFILE` 拦截所有虚拟 PDF 打印机

3. [`src/SecurePdf.Core/Forensics/WatermarkEnhancer.cs`](#srcsecurepdfcoreforensicswatermarkenhancercs) — O(W×H) 二维积分图局部背景减除取证算法，支持 `ChromaStegoExtract` 蓝通道差值提取以抵抗彩虹摩尔纹


---

## `src/SecurePdf.Core/Pdf/MemoryPdfRenderer.cs`


**模块说明**：基于 `PdfiumViewer` + `pdfium.dll` 的纯内存 PDF 页面光栅化器，析构时释放内存流（共 `207` 行）


```csharp
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;

namespace SecurePdf.Core.Pdf
{
    public sealed class MemoryPdfRenderer : IDisposable
    {
        private const int FPDF_ANNOT = 0x01;
        private const int FPDF_LCD_TEXT = 0x02;

        private static readonly object InitLock = new object();
        private static bool _libraryInitialized;

        private readonly object _docLock = new object();
        private GCHandle _pinnedPdfHandle;
        private IntPtr _documentHandle;
        private bool _disposed;

        static MemoryPdfRenderer()
        {
            PreloadNativePdfium();
        }

        private MemoryPdfRenderer(GCHandle pinnedHandle, IntPtr documentHandle)
        {
            _pinnedPdfHandle = pinnedHandle;
            _documentHandle = documentHandle;
            PageCount = FPDF_GetPageCount(documentHandle);
        }

        public int PageCount { get; }

        public static MemoryPdfRenderer LoadFromMemory(byte[] pdfBytes)
        {
            if (pdfBytes == null || pdfBytes.Length == 0)
            {
                throw new ArgumentException("PDF data is empty.", nameof(pdfBytes));
            }

            EnsureInitialized();

            GCHandle handle = GCHandle.Alloc(pdfBytes, GCHandleType.Pinned);
            try
            {
                IntPtr ptr = handle.AddrOfPinnedObject();
                IntPtr doc = FPDF_LoadMemDocument(ptr, pdfBytes.Length, null);
                if (doc == IntPtr.Zero)
                {
                    uint err = FPDF_GetLastError();
                    throw new InvalidDataException($"PDFium 无法解析内存中的 PDF 文档 (错误码: {err})。");
                }

                return new MemoryPdfRenderer(handle, doc);
            }
            catch
            {
                if (handle.IsAllocated) handle.Free();
                throw;
            }
        }

        public Bitmap RenderPageToBitmap(int pageIndex, double dpiX = 144.0, double dpiY = 144.0)
        {
            ThrowIfDisposed();
            if (pageIndex < 0 || pageIndex >= PageCount) throw new ArgumentOutOfRangeException(nameof(pageIndex));

            lock (_docLock)
            {
                IntPtr page = FPDF_LoadPage(_documentHandle, pageIndex);
                if (page == IntPtr.Zero) throw new InvalidOperationException($"无法加载 PDF 第 {pageIndex + 1} 页。");

                try
                {
                    double widthPt = FPDF_GetPageWidth(page);
                    double heightPt = FPDF_GetPageHeight(page);

                    int widthPx = Math.Max(1, (int)Math.Round(widthPt * dpiX / 72.0));
                    int heightPx = Math.Max(1, (int)Math.Round(heightPt * dpiY / 72.0));

                    var bitmap = new Bitmap(widthPx, heightPx, PixelFormat.Format32bppArgb);
                    BitmapData? bmpData = null;
                    IntPtr pdfBitmap = IntPtr.Zero;

                    try
                    {
                        bmpData = bitmap.LockBits(new Rectangle(0, 0, widthPx, heightPx), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
                        pdfBitmap = FPDFBitmap_CreateEx(widthPx, heightPx, 4, bmpData.Scan0, bmpData.Stride);
                        if (pdfBitmap == IntPtr.Zero) throw new OutOfMemoryException("无法分配 PDFium 渲染位图缓冲区。");

                        FPDFBitmap_FillRect(pdfBitmap, 0, 0, widthPx, heightPx, 0xFFFFFFFF);
                        FPDF_RenderPageBitmap(pdfBitmap, page, 0, 0, widthPx, heightPx, 0, FPDF_ANNOT | FPDF_LCD_TEXT);
                        return bitmap;
                    }
                    catch
                    {
                        bitmap.Dispose();
                        throw;
                    }
                    finally
                    {
                        if (pdfBitmap != IntPtr.Zero) FPDFBitmap_Destroy(pdfBitmap);
                        if (bmpData != null) bitmap.UnlockBits(bmpData);
                    }
                }
                finally
                {
                    FPDF_ClosePage(page);
                }
            }
        }

        private void ThrowIfDisposed()
        {
            if (_disposed) throw new ObjectDisposedException(nameof(MemoryPdfRenderer));
        }

        public void Dispose()
        {
            lock (_docLock)
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

        private static void EnsureInitialized()
        {
            lock (InitLock)
            {
                if (!_libraryInitialized)
                {
                    FPDF_InitLibrary();
                    _libraryInitialized = true;
                }
            }
        }

        private static void PreloadNativePdfium()
        {
            string baseDir = AppDomain.CurrentDomain.BaseDirectory;
            string[] candidates =
            {
                Path.Combine(baseDir, "pdfium.dll"),
                Path.Combine(baseDir, "x64", "pdfium.dll"),
                Path.Combine(baseDir, "runtimes", "win-x64", "native", "pdfium.dll")
            };

            foreach (string path in candidates)
            {
                if (File.Exists(path) && LoadLibrary(path) != IntPtr.Zero) return;
            }
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
        private static extern void FPDF_CloseDocument(IntPtr document);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr FPDF_LoadPage(IntPtr document, int pageIndex);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_ClosePage(IntPtr page);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern double FPDF_GetPageWidth(IntPtr page);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern double FPDF_GetPageHeight(IntPtr page);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr FPDFBitmap_CreateEx(int width, int height, int format, IntPtr firstScan, int stride);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDFBitmap_FillRect(IntPtr bitmap, int left, int top, int width, int height, uint color);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDF_RenderPageBitmap(IntPtr bitmap, IntPtr page, int startX, int startY, int sizeX, int sizeY, int rotate, int flags);

        [DllImport("pdfium.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FPDFBitmap_Destroy(IntPtr bitmap);
    }
}
```


## `src/SecurePdf.Core/Printing/PrinterSecurityGuard.cs`


**模块说明**：枚举本地/网络打印机并通过端口名、驱动名、监视器、传真属性及 GDI `DT_METAFILE` 拦截所有虚拟 PDF 打印机（共 `494` 行）


```csharp
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Linq;
using System.Runtime.InteropServices;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Storage;

namespace SecurePdf.Core.Printing
{
    public sealed class PrinterCandidate
    {
        public string PrinterName { get; set; } = string.Empty;
        public string PortName { get; set; } = string.Empty;
        public string DriverName { get; set; } = string.Empty;
        public string MonitorName { get; set; } = string.Empty;
        public uint Attributes { get; set; }
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
    }

    public static class PrinterSecurityGuard
    {
        private const uint PRINTER_ENUM_LOCAL = 0x00000002;
        private const uint PRINTER_ENUM_CONNECTIONS = 0x00000004;
        private const uint PRINTER_ATTRIBUTE_FAX = 0x00004000;

        private const int TECHNOLOGY = 2;
        private const int HORZRES = 8;
        private const int VERTRES = 10;
        private const int LOGPIXELSX = 88;
        private const int LOGPIXELSY = 90;
        private const int DT_METAFILE = 5;

        private static readonly string[] BlockedPortExactOrPrefix =
        {
            "PORTPROMPT:", "FILE:", "NUL:", "SHRFAX:", "XPSPORT:",
            "FOXIT", "ADOBE PDF", "PDF", "ONENOTE", "WPS"
        };

        private static readonly string[] BlockedNameOrDriverKeywords =
        {
            "Microsoft Print to PDF", "Microsoft XPS Document Writer",
            "Adobe PDF", "Foxit", "WPS PDF", "Kingsoft PDF", "PDFCreator",
            "CutePDF", "Bullzip", "Nitro PDF", "DoPDF", "PrimoPDF",
            "NovaPDF", "PDF24", "OneNote", "Send To OneNote", "Fax",
            "Snagit", "Virtual", "ImagePrinter", "CAJ", "Print to PDF",
            "Save as PDF", "XPS"
        };

        private static readonly string[] BlockedMonitorKeywords =
        {
            "Document Writer", "PDF", "XPS", "Fax", "OneNote", "AppMon", "Nuance", "Foxit"
        };

        public static PrinterScanSummary ScanPrinters()
        {
            var summary = new PrinterScanSummary();
            Dictionary<string, string> portMonitors = EnumeratePortMonitors();
            List<PrinterCandidate> all = EnumerateRawPrinters(portMonitors);

            foreach (PrinterCandidate candidate in all)
            {
                PrinterValidationResult res = EvaluatePrinterCandidate(candidate);
                if (res.IsAllowedPhysicalPrinter)
                {
                    summary.AllowedPrinters.Add(candidate);
                }
                else
                {
                    summary.BlockedPrinters.Add(res);
                }
            }

            return summary;
        }

        public static PrinterValidationResult EvaluatePrinterCandidate(PrinterCandidate candidate)
        {
            if (candidate == null) throw new ArgumentNullException(nameof(candidate));
            if ((candidate.Attributes & PRINTER_ATTRIBUTE_FAX) != 0)
            {
                return Reject(candidate, "禁止使用传真设备。");
            }

            string port = (candidate.PortName ?? string.Empty).Trim();
            string name = (candidate.PrinterName ?? string.Empty).Trim();
            string driver = (candidate.DriverName ?? string.Empty).Trim();
            string monitor = (candidate.MonitorName ?? string.Empty).Trim();

            if (string.IsNullOrEmpty(name)) return Reject(candidate, "打印机名称为空。");

            string[] portParts = port.Split(new[] { ',' }, StringSplitOptions.RemoveEmptyEntries);
            if (portParts.Length == 0) return Reject(candidate, "打印机未绑定有效物理端口。");

            foreach (string rawPart in portParts)
            {
                string part = rawPart.Trim();
                foreach (string blockedPort in BlockedPortExactOrPrefix)
                {
                    if (part.Equals(blockedPort, StringComparison.OrdinalIgnoreCase) ||
                        part.IndexOf(blockedPort, StringComparison.OrdinalIgnoreCase) >= 0)
                    {
                        return Reject(candidate, $"拦截虚拟/文件输出端口: {part}");
                    }
                }

                if (part.Length >= 3 && char.IsLetter(part[0]) && part[1] == ':' && part[2] == '\\')
                {
                    return Reject(candidate, $"拦截本地文件输出端口: {part}");
                }
                if (part.EndsWith(".pdf", StringComparison.OrdinalIgnoreCase) ||
                    part.EndsWith(".xps", StringComparison.OrdinalIgnoreCase) ||
                    part.EndsWith(".prn", StringComparison.OrdinalIgnoreCase))
                {
                    return Reject(candidate, $"拦截文件后缀输出端口: {part}");
                }
            }

            foreach (string kw in BlockedNameOrDriverKeywords)
            {
                if (name.IndexOf(kw, StringComparison.OrdinalIgnoreCase) >= 0 ||
                    driver.IndexOf(kw, StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    return Reject(candidate, $"拦截虚拟/文档打印机 ({kw})");
                }
            }

            foreach (string mkw in BlockedMonitorKeywords)
            {
                if (monitor.IndexOf(mkw, StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    return Reject(candidate, $"拦截虚拟端口监视器: {monitor}");
                }
            }

            bool looksPhysical = portParts.Any(p =>
            {
                string t = p.Trim();
                return t.StartsWith("USB", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("WSD", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("IP_", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("LPT", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("COM", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith(@"\\", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("192.", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("10.", StringComparison.OrdinalIgnoreCase) ||
                       t.StartsWith("172.", StringComparison.OrdinalIgnoreCase) ||
                       monitor.IndexOf("TCP/IP", StringComparison.OrdinalIgnoreCase) >= 0 ||
                       monitor.IndexOf("USB", StringComparison.OrdinalIgnoreCase) >= 0 ||
                       monitor.IndexOf("WSD", StringComparison.OrdinalIgnoreCase) >= 0;
            });

            if (!looksPhysical)
            {
                return Reject(candidate, $"未识别为物理或标准网络端口 ({port})");
            }

            return new PrinterValidationResult
            {
                Candidate = candidate,
                IsAllowedPhysicalPrinter = true,
                Reason = "已验证物理打印机"
            };
        }

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
            if (zeroBasedPageIndices == null || zeroBasedPageIndices.Count == 0)
            {
                throw new ArgumentException("至少需要选择一页进行打印。", nameof(zeroBasedPageIndices));
            }

            PrinterScanSummary scan = ScanPrinters();
            PrinterCandidate? target = scan.AllowedPrinters.FirstOrDefault(
                p => p.PrinterName.Equals(printerName, StringComparison.OrdinalIgnoreCase));

            if (target == null)
            {
                throw new UnauthorizedAccessException($"拒绝打印：\"{printerName}\" 未通过物理打印机安全校验（可能为虚拟 PDF 打印机）。");
            }

            QuotaSnapshot reserved = quotaStore.ReservePrintSlot(docId, policyMaxPrints, expiresAtUtc);
            bool spoolStarted = false;
            IntPtr hdc = IntPtr.Zero;

            try
            {
                hdc = CreateDC("WINSPOOL", target.PrinterName, null, IntPtr.Zero);
                if (hdc == IntPtr.Zero)
                {
                    throw new InvalidOperationException($"无法创建物理打印机设备上下文（错误码: {Marshal.GetLastWin32Error()}）。");
                }

                int tech = GetDeviceCaps(hdc, TECHNOLOGY);
                if (tech == DT_METAFILE)
                {
                    throw new UnauthorizedAccessException("拒绝打印：目标设备报告为虚拟图元设备。");
                }

                int dpiX = Math.Max(150, Math.Min(300, GetDeviceCaps(hdc, LOGPIXELSX)));
                int dpiY = Math.Max(150, Math.Min(300, GetDeviceCaps(hdc, LOGPIXELSY)));
                int pagePxW = GetDeviceCaps(hdc, HORZRES);
                int pagePxH = GetDeviceCaps(hdc, VERTRES);

                var docInfo = new DOCINFO
                {
                    cbSize = Marshal.SizeOf(typeof(DOCINFO)),
                    lpszDocName = "SecurePdf Protected Document",
                    lpszOutput = IntPtr.Zero,
                    lpszDatatype = IntPtr.Zero,
                    fwType = 0
                };

                int jobId = StartDoc(hdc, ref docInfo);
                if (jobId <= 0)
                {
                    throw new InvalidOperationException($"打印机启动作业失败（错误码: {Marshal.GetLastWin32Error()}）。");
                }

                spoolStarted = true;

                foreach (int pageIndex in zeroBasedPageIndices)
                {
                    if (StartPage(hdc) <= 0)
                    {
                        throw new InvalidOperationException($"打印机开始页面失败（错误码: {Marshal.GetLastWin32Error()}）。");
                    }

                    try
                    {
                        using (Bitmap pageBitmap = renderer.RenderPageToBitmap(pageIndex, dpiX, dpiY))
                        {
                            BurnWatermarkIntoBitmap(pageBitmap, watermarkText);
                            DrawBitmapToPrinterDc(hdc, pageBitmap, pagePxW, pagePxH);
                        }
                    }
                    finally
                    {
                        EndPage(hdc);
                    }
                }

                EndDoc(hdc);
                return reserved;
            }
            catch
            {
                if (!spoolStarted)
                {
                    quotaStore.RollbackPrintSlot(reserved);
                }
                else if (hdc != IntPtr.Zero)
                {
                    AbortDoc(hdc);
                }
                throw;
            }
            finally
            {
                if (hdc != IntPtr.Zero)
                {
                    DeleteDC(hdc);
                }
            }
        }

        private static void BurnWatermarkIntoBitmap(Bitmap bmp, string watermarkText)
        {
            if (string.IsNullOrWhiteSpace(watermarkText)) return;

            using (Graphics g = Graphics.FromImage(bmp))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                float fontSize = Math.Max(14f, bmp.Width / 48f);
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

        private static void DrawBitmapToPrinterDc(IntPtr printerHdc, Bitmap bmp, int targetW, int targetH)
        {
            using (Graphics printerG = Graphics.FromHdc(printerHdc))
            {
                printerG.InterpolationMode = InterpolationMode.HighQualityBicubic;
                float scale = Math.Min(targetW / (float)bmp.Width, targetH / (float)bmp.Height);
                int drawW = (int)(bmp.Width * scale);
                int drawH = (int)(bmp.Height * scale);
                int offsetX = (targetW - drawW) / 2;
                int offsetY = (targetH - drawH) / 2;
                printerG.DrawImage(bmp, offsetX, offsetY, drawW, drawH);
            }
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

        private static Dictionary<string, string> EnumeratePortMonitors()
        {
            var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            uint needed = 0;
            uint returned = 0;
            EnumPorts(null, 2, IntPtr.Zero, 0, ref needed, ref returned);
            if (needed == 0) return map;

            IntPtr buffer = Marshal.AllocHGlobal((int)needed);
            try
            {
                if (EnumPorts(null, 2, buffer, needed, ref needed, ref returned))
                {
                    int structSize = Marshal.SizeOf(typeof(PORT_INFO_2));
                    IntPtr ptr = buffer;
                    for (uint i = 0; i < returned; i++)
                    {
                        var info = Marshal.PtrToStructure<PORT_INFO_2>(ptr);
                        if (!string.IsNullOrEmpty(info.pPortName))
                        {
                            map[info.pPortName!] = info.pMonitorName ?? string.Empty;
                        }
                        ptr = IntPtr.Add(ptr, structSize);
                    }
                }
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
            return map;
        }

        private static List<PrinterCandidate> EnumerateRawPrinters(Dictionary<string, string> portMonitors)
        {
            var list = new List<PrinterCandidate>();
            uint needed = 0;
            uint returned = 0;
            uint flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
            EnumPrinters(flags, null, 2, IntPtr.Zero, 0, ref needed, ref returned);
            if (needed == 0) return list;

            IntPtr buffer = Marshal.AllocHGlobal((int)needed);
            try
            {
                if (EnumPrinters(flags, null, 2, buffer, needed, ref needed, ref returned))
                {
                    int structSize = Marshal.SizeOf(typeof(PRINTER_INFO_2));
                    IntPtr ptr = buffer;
                    for (uint i = 0; i < returned; i++)
                    {
                        var info = Marshal.PtrToStructure<PRINTER_INFO_2>(ptr);
                        string portName = info.pPortName ?? string.Empty;
                        string firstPort = portName.Split(',')[0].Trim();
                        portMonitors.TryGetValue(firstPort, out string? monitorName);

                        list.Add(new PrinterCandidate
                        {
                            PrinterName = info.pPrinterName ?? string.Empty,
                            PortName = portName,
                            DriverName = info.pDriverName ?? string.Empty,
                            MonitorName = monitorName ?? string.Empty,
                            Attributes = info.Attributes
                        });
                        ptr = IntPtr.Add(ptr, structSize);
                    }
                }
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
            return list;
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


**模块说明**：O(W×H) 二维积分图局部背景减除取证算法，支持 `ChromaStegoExtract` 蓝通道差值提取以抵抗彩虹摩尔纹（共 `226` 行）


```csharp
using System;
using System.Drawing;
using System.Drawing.Imaging;

namespace SecurePdf.Core.Forensics
{
    public enum ForensicFilterMode
    {
        ChromaStegoExtract = 0,
        AdaptiveBackgroundSubtract = 1,
        GlobalEqualizeAndInvert = 2
    }

    /// <summary>
    /// Extracts ultra-low-contrast hidden forensic watermarks (IP, timestamp, device code)
    /// from smartphone camera photos or screenshots using O(W*H) integral-image background
    /// subtraction and blue-red chromatic channel separation.
    /// </summary>
    public static class WatermarkEnhancer
    {
        public static Bitmap Enhance(Bitmap source, ForensicFilterMode mode)
        {
            if (source == null) throw new ArgumentNullException(nameof(source));

            int w = source.Width;
            int h = source.Height;
            var result = new Bitmap(w, h, PixelFormat.Format32bppArgb);

            using (var normalized = new Bitmap(w, h, PixelFormat.Format32bppArgb))
            {
                using (Graphics g = Graphics.FromImage(normalized))
                {
                    g.DrawImage(source, 0, 0, w, h);
                }

                BitmapData srcData = normalized.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
                BitmapData dstData = result.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);

                try
                {
                    unsafe
                    {
                        byte* srcPtr = (byte*)srcData.Scan0;
                        byte* dstPtr = (byte*)dstData.Scan0;
                        int stride = srcData.Stride;

                        switch (mode)
                        {
                            case ForensicFilterMode.ChromaStegoExtract:
                                ApplyChromaIntegralResidual(srcPtr, dstPtr, w, h, stride);
                                break;
                            case ForensicFilterMode.AdaptiveBackgroundSubtract:
                                ApplyLuminanceIntegralResidual(srcPtr, dstPtr, w, h, stride);
                                break;
                            case ForensicFilterMode.GlobalEqualizeAndInvert:
                            default:
                                ApplyHistogramEqualize(srcPtr, dstPtr, w, h, stride, invert: true);
                                break;
                        }
                    }
                }
                finally
                {
                    normalized.UnlockBits(srcData);
                    result.UnlockBits(dstData);
                }
            }

            return result;
        }

        /// <summary>
        /// Isolates blue-tinted hidden watermark strokes while cancelling neutral black text,
        /// uneven monitor illumination, and wide camera rainbow Moire waves.
        /// </summary>
        private static unsafe void ApplyChromaIntegralResidual(byte* src, byte* dst, int w, int h, int stride)
        {
            double[] signal = new double[w * h];
            for (int y = 0; y < h; y++)
            {
                byte* row = src + y * stride;
                int rowOff = y * w;
                for (int x = 0; x < w; x++)
                {
                    int idx = x * 4;
                    int b = row[idx];
                    int g = row[idx + 1];
                    int r = row[idx + 2];
                    // Blue-channel watermark yields positive (B - R) and (B - G) while neutral text/paper ~ 0
                    signal[rowOff + x] = (b - r) * 0.75 + (b - g) * 0.25;
                }
            }

            ApplyIntegralHighPassToOutput(signal, dst, w, h, stride, radius: 18, gain: 11.0, invertSign: true);
        }

        /// <summary>
        /// Removes uneven exposure and wide Moire bands using a smooth 2D box-average subtraction.
        /// </summary>
        private static unsafe void ApplyLuminanceIntegralResidual(byte* src, byte* dst, int w, int h, int stride)
        {
            double[] lum = new double[w * h];
            for (int y = 0; y < h; y++)
            {
                byte* row = src + y * stride;
                int rowOff = y * w;
                for (int x = 0; x < w; x++)
                {
                    int idx = x * 4;
                    lum[rowOff + x] = (row[idx + 2] * 77 + row[idx + 1] * 150 + row[idx] * 29) / 256.0;
                }
            }

            ApplyIntegralHighPassToOutput(lum, dst, w, h, stride, radius: 15, gain: 9.0, invertSign: false);
        }

        private static unsafe void ApplyIntegralHighPassToOutput(
            double[] field,
            byte* dst,
            int w,
            int h,
            int stride,
            int radius,
            double gain,
            bool invertSign)
        {
            int iw = w + 1;
            double[] integral = new double[(w + 1) * (h + 1)];

            for (int y = 1; y <= h; y++)
            {
                double rowSum = 0;
                int srcRow = (y - 1) * w;
                int intRow = y * iw;
                int prevIntRow = (y - 1) * iw;
                for (int x = 1; x <= w; x++)
                {
                    rowSum += field[srcRow + (x - 1)];
                    integral[intRow + x] = integral[prevIntRow + x] + rowSum;
                }
            }

            for (int y = 0; y < h; y++)
            {
                int y0 = Math.Max(0, y - radius);
                int y1 = Math.Min(h - 1, y + radius);
                int rowOff = y * w;
                byte* dRow = dst + y * stride;

                for (int x = 0; x < w; x++)
                {
                    int x0 = Math.Max(0, x - radius);
                    int x1 = Math.Min(w - 1, x + radius);

                    int count = (y1 - y0 + 1) * (x1 - x0 + 1);
                    double sum = integral[(y1 + 1) * iw + (x1 + 1)]
                               - integral[y0 * iw + (x1 + 1)]
                               - integral[(y1 + 1) * iw + x0]
                               + integral[y0 * iw + x0];

                    double localMean = sum / count;
                    double diff = field[rowOff + x] - localMean;
                    if (invertSign)
                    {
                        diff = -diff;
                    }

                    // Stretch subtle watermark contrast cleanly around white/dark paper
                    double mapped = (invertSign ? 225.0 : 128.0) + diff * gain;
                    byte v = (byte)Math.Max(0, Math.Min(255, (int)Math.Round(mapped)));

                    int idx = x * 4;
                    dRow[idx] = v;
                    dRow[idx + 1] = v;
                    dRow[idx + 2] = v;
                    dRow[idx + 3] = 255;
                }
            }
        }

        private static unsafe void ApplyHistogramEqualize(byte* src, byte* dst, int w, int h, int stride, bool invert)
        {
            int[] hist = new int[256];
            int total = w * h;

            for (int y = 0; y < h; y++)
            {
                byte* row = src + y * stride;
                for (int x = 0; x < w; x++)
                {
                    int idx = x * 4;
                    int lum = (row[idx + 2] * 77 + row[idx + 1] * 150 + row[idx] * 29) >> 8;
                    hist[lum]++;
                }
            }

            byte[] lut = new byte[256];
            int cdf = 0;
            int cdfMin = 0;
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
                byte* sRow = src + y * stride;
                byte* dRow = dst + y * stride;
                for (int x = 0; x < w; x++)
                {
                    int idx = x * 4;
                    int lum = (sRow[idx + 2] * 77 + sRow[idx + 1] * 150 + sRow[idx] * 29) >> 8;
                    byte v = lut[lum];
                    dRow[idx] = v;
                    dRow[idx + 1] = v;
                    dRow[idx + 2] = v;
                    dRow[idx + 3] = 255;
                }
            }
        }
    }
}
```


---

[返回总览 README](../README.md)
