# 05 — 自解压安装器 (`SecurePdf.Installer`)、构建打包与单元测试 (`SecurePdf.Core.Tests`)

> [!NOTE]
> 本文档包含 `nuget.config`、从源码构建/测试/制作自解压安装包的方法、`SecurePdf.Installer` 全部源码及资源文件，以及 `SecurePdf.Core.Tests`（60 项 xUnit 测试）全部源码（v1.1.0）。
> v1.1.0 自解压安装器改为基于 Windows 原生 `cabinet.dll` LZMS 16 MB 分块高压缩算法（带 SHA-256 校验），安装程序本身由系统 `.NET Framework 4` `csc.exe` 直接编译（C# 5），并支持安全按文件清单卸载及旧版 `BouncyCastle.Cryptography.dll` 自动清理。

## 目录

- [`nuget.config`](#nugetconfig)
- [从源码编译、测试与制作安装包](#从源码编译测试与制作安装包)
- [`src/SecurePdf.Installer/Program.cs`](#srcsecurepdfinstallerprogramcs)
- [`src/SecurePdf.Installer/assets/reader/THIRD-PARTY-NOTICES.txt`](#srcsecurepdfinstallerassetsreaderthird-party-noticestxt)
- [`src/SecurePdf.Installer/assets/reader/关联spdf文件格式.bat`](#srcsecurepdfinstallerassetsreader关联spdf文件格式bat)
- [`tests/SecurePdf.Core.Tests/SecurePdf.Core.Tests.csproj`](#testssecurepdfcoretestssecurepdfcoretestscsproj)
- [`tests/SecurePdf.Core.Tests/CryptoAndContextTests.cs`](#testssecurepdfcoretestscryptoandcontexttestscs)
- [`tests/SecurePdf.Core.Tests/SpdfContainerTests.cs`](#testssecurepdfcoretestsspdfcontainertestscs)
- [`tests/SecurePdf.Core.Tests/LocalQuotaStoreTests.cs`](#testssecurepdfcoretestslocalquotastoretestscs)
- [`tests/SecurePdf.Core.Tests/PrinterSecurityGuardTests.cs`](#testssecurepdfcoretestsprintersecurityguardtestscs)
- [`tests/SecurePdf.Core.Tests/WatermarkEnhancerTests.cs`](#testssecurepdfcoretestswatermarkenhancertestscs)

## `nuget.config`

**模块说明**：解决方案 NuGet 软件源配置文件（仅配置官方 `nuget.org`）（共 `7` 行）

```xml
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
```

## 从源码编译、测试与制作安装包

以下流程无需额外第三方打包脚本，可在已安装 .NET SDK（支持 `net48` 构建）与 Windows .NET Framework 4 (`csc.exe`) 的 Windows 环境中直接执行：

### 1. 编译 Release 并运行单元测试

1. 若首次还原需要拉取测试依赖（仅 `tests/SecurePdf.Core.Tests` 依赖 `xunit`、`Microsoft.NET.Test.Sdk` 与 `Microsoft.NETFramework.ReferenceAssemblies`；运行期主程序无外部 NuGet 依赖）：
   ```cmd
   dotnet restore tests\SecurePdf.Core.Tests\SecurePdf.Core.Tests.csproj
   dotnet restore src\SecurePdf.Creator\SecurePdf.Creator.csproj
   dotnet restore src\SecurePdf.Reader\SecurePdf.Reader.csproj
   ```
2. 执行测试与 Release 编译：
   ```cmd
   dotnet test tests\SecurePdf.Core.Tests\SecurePdf.Core.Tests.csproj -c Release
   dotnet build src\SecurePdf.Creator\SecurePdf.Creator.csproj -c Release
   dotnet build src\SecurePdf.Reader\SecurePdf.Reader.csproj -c Release
   ```

### 2. 准备精简版 `pdfium.dll` 与打包目录

1. 从 [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) 获取 Windows x64 `pdfium.dll`（v1.1.0 发布包内置 `159.0.8152` 无 V8/XFA 精简版，`7,830,016` 字节），放置为阅读端临时打包目录的 `pdfium.dll`。
2. 分别归集文件到 `stage_creator` 与 `stage_reader`：
   - `stage_creator`：`SecurePdfCreator.exe`、`SecurePdfCreator.exe.config`、`SecurePdf.Core.dll`
   - `stage_reader`：`SecurePdfReader.exe`、`SecurePdfReader.exe.config`、`SecurePdf.Core.dll`、`pdfium.dll`、`src\SecurePdf.Installer\assets\reader\THIRD-PARTY-NOTICES.txt`、`src\SecurePdf.Installer\assets\reader\关联spdf文件格式.bat`

### 3. 编译 LZMS 打包器并生成自解压安装包

1. 编译打包工具 `packer.exe`（`/define:PACKER`）：
   ```cmd
   C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /codepage:65001 /target:exe /platform:x64 /optimize+ /define:PACKER /r:System.Windows.Forms.dll /r:System.Drawing.dll /out:packer.exe src\SecurePdf.Installer\Program.cs
   ```
2. 分别生成 `payload_creator.bin` 与 `payload_reader.bin`：
   ```cmd
   pushd stage_creator && ..\packer.exe ..\payload_creator.bin * && popd
   pushd stage_reader && ..\packer.exe ..\payload_reader.bin * && popd
   ```
3. 将二进制载荷内嵌编译为正式安装程序（如需隔离自测版，追加 `;INSTALLER_SELFTEST`）：
   ```cmd
   C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /codepage:65001 /target:winexe /platform:x64 /optimize+ /define:INSTALLER_MODE_CREATOR /r:System.Windows.Forms.dll /r:System.Drawing.dll /resource:payload_creator.bin,SecurePdfPayload.bin /out:install_creator.exe src\SecurePdf.Installer\Program.cs

   C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /codepage:65001 /target:winexe /platform:x64 /optimize+ /define:INSTALLER_MODE_READER /r:System.Windows.Forms.dll /r:System.Drawing.dll /resource:payload_reader.bin,SecurePdfPayload.bin /out:install_reader.exe src\SecurePdf.Installer\Program.cs
   ```

## `src/SecurePdf.Installer/Program.cs`

**模块说明**：C# 5 单文件双模式源码：兼任 `PACKER` (`SPDFPKG1` LZMS 16 MB 压缩打包 + SHA-256 校验)、GUI/静默自解压安装器（支持 `--silent --dir --desktop --start-menu --assoc`、`INSTALLER_SELFTEST` 隔离测试、自动移除旧版 `BouncyCastle.Cryptography.dll` 与生成按明示文件清单清理且非递归移除目录的 `uninstall_*.bat`）（共 `963` 行）

```csharp
// SecurePdf 自解压安装器。使用 C# 5 语法，可直接用 Windows 自带的 .NET Framework csc.exe 编译，不依赖 SDK。
//
// 条件编译符号：
//   INSTALLER_MODE_CREATOR  生成创建端安装器（不定义则为阅读端）
//   PACKER                  生成负载打包工具：packer.exe <输出.bin> <文件1> [文件2 ...]
//   INSTALLER_SELFTEST      自检构建：注册表项 / 快捷方式 / 文件后缀全部换成带 SelfTest 的名字，
//                           可以完整走一遍“安装 → 卸载”而不碰本机已有的正式安装
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecurePdf.Installer
{
    internal static class AppProfile
    {
        public const string ProductVersion = "1.1.0";

#if INSTALLER_MODE_CREATOR
        public static readonly bool IsCreator = true;
        public const string WindowTitle = "SecurePdf 创建端安装程序（发送方）";
        public const string BannerTitle = "SecurePdf 创建端安装向导（发送方）";
        public const string BannerSub = "用于加密生成 .spdf、自动生码与水印取证 · 释放后约 0.1 MB";
        public const string DefaultFolderName = "SecurePdfCreator";
        public const string TargetExeName = "SecurePdfCreator.exe";
        public const string ShortcutDesc = "创建加密 PDF 并自动生成验证码";
        public const string LaunchBtnText = "立即打开创建端";
        public const string UninstallDisplayName = "SecurePdf 创建端 (发送方)";
        public const string UninstallBatName = "uninstall_creator.bat";
        private const string BaseShortcutName = "SecurePdf 创建端";
        private const string BaseUninstallKeyName = "SecurePdfCreator";
#else
        public static readonly bool IsCreator = false;
        public const string WindowTitle = "SecurePdf 阅读端安装程序（接收方）";
        public const string BannerTitle = "SecurePdf 阅读端安装向导（接收方）";
        public const string BannerSub = "输入验证码安全阅读、防截屏防翻拍与受控物理打印 · 释放后约 7.6 MB";
        public const string DefaultFolderName = "SecurePdfReader";
        public const string TargetExeName = "SecurePdfReader.exe";
        public const string ShortcutDesc = "受控阅读加密 .spdf 文档";
        public const string LaunchBtnText = "立即打开阅读端";
        public const string UninstallDisplayName = "SecurePdf 阅读端 (接收方)";
        public const string UninstallBatName = "uninstall_reader.bat";
        private const string BaseShortcutName = "SecurePdf 阅读端";
        private const string BaseUninstallKeyName = "SecurePdfReader";
#endif

#if INSTALLER_SELFTEST
        public static readonly bool IsSelfTest = true;
        public const string ShortcutName = BaseShortcutName + " SelfTest";
        public const string UninstallKeyName = BaseUninstallKeyName + "_SelfTest";
        public const string StartMenuFolderName = "SecurePdfSuite_SelfTest";
        public const string SpdfExtension = ".spdfselftest";
        public const string SpdfProgId = "SecurePdfSuite.SelfTestFile";
#else
        public static readonly bool IsSelfTest = false;
        public const string ShortcutName = BaseShortcutName;
        public const string UninstallKeyName = BaseUninstallKeyName;
        public const string StartMenuFolderName = "SecurePdfSuite";
        public const string SpdfExtension = ".spdf";
        public const string SpdfProgId = "SecurePdfSuite.SpdfFile";
#endif

        /// <summary>1.0.x 的“关联spdf文件格式.bat”用过的另一个 ProgID，卸载时一并清理。</summary>
        public const string LegacySpdfProgId = "SecurePdfSuite.spdf";

        public const string DesktopCheckText = "在桌面创建快捷方式：" + ShortcutName;

        /// <summary>旧版本安装过、新版本不再需要的文件；升级安装和卸载时删除。</summary>
        public static readonly string[] ObsoleteFiles = { "BouncyCastle.Cryptography.dll" };
    }

    internal sealed class PayloadFile
    {
        public string Name;
        public long Length;
        public long LastWriteUtcTicks;
        public int Offset;
    }

    /// <summary>
    /// 安装负载：文件表 + 一整块 LZMS 压缩数据（Windows 8 起系统自带的 Compression API，cabinet.dll）。
    /// 比 Deflate(zip) 小约四分之一，而且不需要随安装器携带任何解压库。
    ///
    /// 布局：magic "SPDFPKG1" | int32 文件数 | { 文件名, int64 长度, int64 修改时间(UTC ticks) } × N
    ///       | int32 原始总长度 | int32 压缩长度 | 压缩数据 | SHA-256(原始数据)
    /// </summary>
    internal static class Payload
    {
        public const string EmbeddedResourceName = "SecurePdfPayload.bin";

        private static readonly byte[] Magic = Encoding.ASCII.GetBytes("SPDFPKG1");
        private const uint COMPRESS_ALGORITHM_LZMS = 5;
        private const int COMPRESS_INFORMATION_CLASS_BLOCK_SIZE = 1;
        private const int MaxFiles = 256;
        private const int MaxRawBytes = 512 * 1024 * 1024;

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool CreateDecompressor(uint algorithm, IntPtr allocationRoutines, out IntPtr handle);

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool Decompress(IntPtr handle, byte[] compressedData, IntPtr compressedDataSize, byte[] uncompressedBuffer, IntPtr uncompressedBufferSize, out IntPtr uncompressedDataSize);

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool CloseDecompressor(IntPtr handle);

        public static List<PayloadFile> Read(Stream stream, out byte[] rawData)
        {
            List<PayloadFile> files = new List<PayloadFile>();
            using (BinaryReader r = new BinaryReader(stream, Encoding.UTF8))
            {
                byte[] magic = r.ReadBytes(Magic.Length);
                if (!BytesEqual(magic, Magic)) throw new InvalidDataException("安装包负载格式无效。");

                int count = r.ReadInt32();
                if (count <= 0 || count > MaxFiles) throw new InvalidDataException("安装包负载文件表无效。");

                long total = 0;
                for (int i = 0; i < count; i++)
                {
                    PayloadFile f = new PayloadFile();
                    f.Name = r.ReadString();
                    f.Length = r.ReadInt64();
                    f.LastWriteUtcTicks = r.ReadInt64();
                    if (!IsSafeFileName(f.Name) || f.Length < 0 || f.Length > MaxRawBytes)
                    {
                        throw new InvalidDataException("安装包负载包含非法文件项: " + f.Name);
                    }
                    f.Offset = (int)total;
                    total += f.Length;
                    if (total > MaxRawBytes) throw new InvalidDataException("安装包负载过大。");
                    files.Add(f);
                }

                int rawLength = r.ReadInt32();
                int compressedLength = r.ReadInt32();
                if (rawLength != total || compressedLength <= 0) throw new InvalidDataException("安装包负载长度信息不一致。");

                byte[] compressed = r.ReadBytes(compressedLength);
                byte[] expectedHash = r.ReadBytes(32);
                if (compressed.Length != compressedLength || expectedHash.Length != 32)
                {
                    throw new InvalidDataException("安装包不完整（可能下载未完成），请重新获取安装包。");
                }

                rawData = DecompressLzms(compressed, rawLength);

                using (SHA256 sha = SHA256.Create())
                {
                    if (!BytesEqual(sha.ComputeHash(rawData), expectedHash))
                    {
                        throw new InvalidDataException("安装包校验失败（文件已损坏），请重新获取安装包。");
                    }
                }
            }
            return files;
        }

        /// <summary>负载里只允许出现不带目录的普通文件名，释放位置不可能跑出安装目录。</summary>
        private static bool IsSafeFileName(string name)
        {
            if (string.IsNullOrEmpty(name) || name.Length > 200) return false;
            if (name == "." || name == ".." || name.EndsWith(".", StringComparison.Ordinal) || name.EndsWith(" ", StringComparison.Ordinal)) return false;
            return name.IndexOfAny(Path.GetInvalidFileNameChars()) < 0;
        }

        private static byte[] DecompressLzms(byte[] compressed, int rawLength)
        {
            IntPtr handle;
            try
            {
                if (!CreateDecompressor(COMPRESS_ALGORITHM_LZMS, IntPtr.Zero, out handle))
                {
                    throw new InvalidOperationException("无法初始化系统解压组件（错误码 " + Marshal.GetLastWin32Error() + "）。");
                }
            }
            catch (EntryPointNotFoundException)
            {
                throw new NotSupportedException("本安装程序需要 Windows 8.1 / Windows 10 或更高版本。");
            }

            try
            {
                byte[] raw = new byte[rawLength];
                IntPtr written;
                if (!Decompress(handle, compressed, (IntPtr)compressed.Length, raw, (IntPtr)raw.Length, out written) ||
                    written.ToInt64() != rawLength)
                {
                    throw new InvalidDataException("安装包解压失败（文件已损坏），请重新获取安装包。");
                }
                return raw;
            }
            finally
            {
                CloseDecompressor(handle);
            }
        }

        private static bool BytesEqual(byte[] a, byte[] b)
        {
            if (a == null || b == null || a.Length != b.Length) return false;
            for (int i = 0; i < a.Length; i++)
            {
                if (a[i] != b[i]) return false;
            }
            return true;
        }

#if PACKER
        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool CreateCompressor(uint algorithm, IntPtr allocationRoutines, out IntPtr handle);

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool SetCompressorInformation(IntPtr handle, int informationClass, ref uint data, IntPtr size);

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool Compress(IntPtr handle, byte[] uncompressedData, IntPtr uncompressedDataSize, byte[] compressedBuffer, IntPtr compressedBufferSize, out IntPtr compressedDataSize);

        [DllImport("cabinet.dll", SetLastError = true)]
        private static extern bool CloseCompressor(IntPtr handle);

        public static void Pack(string outputPath, string[] inputFiles)
        {
            List<PayloadFile> files = new List<PayloadFile>();
            MemoryStream raw = new MemoryStream();
            foreach (string path in inputFiles)
            {
                byte[] data = File.ReadAllBytes(path);
                PayloadFile f = new PayloadFile();
                f.Name = Path.GetFileName(path);
                f.Length = data.Length;
                f.LastWriteUtcTicks = File.GetLastWriteTimeUtc(path).Ticks;
                if (!IsSafeFileName(f.Name)) throw new InvalidDataException("文件名不能放入负载: " + f.Name);
                foreach (PayloadFile existing in files)
                {
                    if (string.Equals(existing.Name, f.Name, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("负载中文件名重复: " + f.Name);
                }
                files.Add(f);
                raw.Write(data, 0, data.Length);
            }

            byte[] rawData = raw.ToArray();
            IntPtr handle;
            if (!CreateCompressor(COMPRESS_ALGORITHM_LZMS, IntPtr.Zero, out handle)) throw new InvalidOperationException("CreateCompressor 失败: " + Marshal.GetLastWin32Error());

            byte[] compressed;
            try
            {
                // 块大小取 16 MB（默认 1 MB）：整个负载落在同一个压缩窗口里，pdfium.dll 能再小一成多
                uint blockSize = 16 * 1024 * 1024;
                if (!SetCompressorInformation(handle, COMPRESS_INFORMATION_CLASS_BLOCK_SIZE, ref blockSize, (IntPtr)4)) throw new InvalidOperationException("SetCompressorInformation 失败: " + Marshal.GetLastWin32Error());

                byte[] buffer = new byte[rawData.Length + 1024 * 1024];
                IntPtr written;
                if (!Compress(handle, rawData, (IntPtr)rawData.Length, buffer, (IntPtr)buffer.Length, out written)) throw new InvalidOperationException("Compress 失败: " + Marshal.GetLastWin32Error());
                compressed = new byte[written.ToInt32()];
                Buffer.BlockCopy(buffer, 0, compressed, 0, compressed.Length);
            }
            finally
            {
                CloseCompressor(handle);
            }

            using (FileStream fs = new FileStream(outputPath, FileMode.Create, FileAccess.Write, FileShare.None))
            using (BinaryWriter w = new BinaryWriter(fs, Encoding.UTF8))
            {
                w.Write(Magic);
                w.Write(files.Count);
                foreach (PayloadFile f in files)
                {
                    w.Write(f.Name);
                    w.Write(f.Length);
                    w.Write(f.LastWriteUtcTicks);
                }
                w.Write(rawData.Length);
                w.Write(compressed.Length);
                w.Write(compressed);
                using (SHA256 sha = SHA256.Create()) w.Write(sha.ComputeHash(rawData));
            }

            // 打包后立刻按安装器的读取路径解一遍，确认能原样还原
            byte[] roundTrip;
            using (FileStream check = File.OpenRead(outputPath))
            {
                List<PayloadFile> readBack = Read(check, out roundTrip);
                if (readBack.Count != files.Count || !BytesEqual(roundTrip, rawData)) throw new InvalidDataException("负载回读校验失败。");
            }

            Console.WriteLine("packed " + files.Count + " files, " + rawData.Length + " -> " + new FileInfo(outputPath).Length + " bytes");
        }
#endif
    }

#if PACKER
    internal static class Program
    {
        private static int Main(string[] args)
        {
            if (args.Length < 2)
            {
                Console.WriteLine("usage: packer <output.bin> <file1> [file2 ...]");
                return 64;
            }
            try
            {
                string[] inputs = new string[args.Length - 1];
                Array.Copy(args, 1, inputs, 0, inputs.Length);
                Payload.Pack(args[0], inputs);
                return 0;
            }
            catch (Exception ex)
            {
                Console.WriteLine("ERROR " + ex.GetType().Name + ": " + ex.Message);
                return 1;
            }
        }
    }
#else
    internal static class Program
    {
        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [STAThread]
        private static int Main(string[] args)
        {
            try { SetProcessDPIAware(); } catch (EntryPointNotFoundException) { }

            if (Array.Exists(args, delegate(string a) { return string.Equals(a, "--silent", StringComparison.OrdinalIgnoreCase); }))
            {
                return RunSilentInstall(args);
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new InstallerForm());
            return 0;
        }

        /// <summary>
        /// 静默安装：--silent [--dir 目录] [--desktop] [--start-menu] [--assoc]。
        /// 成功返回 0；失败返回 1，并把原因写到安装器旁边的 install_error.log（无界面程序没有别的地方可以报错）。
        /// </summary>
        private static int RunSilentInstall(string[] args)
        {
            string targetDir = InstallEngine.DefaultInstallDir;
            bool desktopShortcut = false;
            bool startMenu = false;
            bool assoc = false;

            for (int i = 0; i < args.Length; i++)
            {
                if (string.Equals(args[i], "--dir", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length)
                {
                    targetDir = args[++i];
                }
                else if (string.Equals(args[i], "--desktop", StringComparison.OrdinalIgnoreCase))
                {
                    desktopShortcut = true;
                }
                else if (string.Equals(args[i], "--start-menu", StringComparison.OrdinalIgnoreCase))
                {
                    startMenu = true;
                }
                else if (string.Equals(args[i], "--assoc", StringComparison.OrdinalIgnoreCase))
                {
                    assoc = !AppProfile.IsCreator;
                }
            }

            try
            {
                InstallEngine.Install(targetDir, desktopShortcut, startMenu, assoc, null);
                return 0;
            }
            catch (Exception ex)
            {
                try
                {
                    string log = Path.Combine(Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location) ?? ".", "install_error.log");
                    File.WriteAllText(log, DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + ex.GetType().Name + ": " + ex.Message + Environment.NewLine, new UTF8Encoding(true));
                }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
                return 1;
            }
        }
    }

    internal static class InstallEngine
    {
        private const int SHCNE_ASSOCCHANGED = 0x08000000;

        [DllImport("shell32.dll")]
        private static extern void SHChangeNotify(int wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);

        public static string DefaultInstallDir
        {
            get
            {
                return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), AppProfile.DefaultFolderName);
            }
        }

        /// <summary>把用户填写的安装目录规整成完整路径；相对路径、磁盘根目录等一律拒绝。</summary>
        public static string NormalizeInstallDir(string installDir)
        {
            string trimmed = (installDir ?? string.Empty).Trim().Trim('"');
            if (trimmed.Length == 0) throw new ArgumentException("安装目录不能为空。");
            if (!Path.IsPathRooted(trimmed) || trimmed.StartsWith(@"\", StringComparison.Ordinal) && !trimmed.StartsWith(@"\\", StringComparison.Ordinal))
            {
                throw new ArgumentException("请填写完整的安装路径，例如 D:\\Apps\\" + AppProfile.DefaultFolderName + "。");
            }

            string full = Path.GetFullPath(trimmed).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            string root = (Path.GetPathRoot(full) ?? string.Empty).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            if (full.Length == 0 || string.Equals(full, root, StringComparison.OrdinalIgnoreCase))
            {
                throw new ArgumentException("不能直接安装到磁盘根目录，请指定一个文件夹。");
            }
            return full;
        }

        public static bool IsTargetRunning(string installDir)
        {
            return FindRunningInstances(Path.Combine(installDir, AppProfile.TargetExeName)).Count > 0;
        }

        public static void Install(
            string installDir,
            bool createDesktopShortcut,
            bool createStartMenuShortcut,
            bool associateSpdf,
            Action<int, string> progressCallback)
        {
            installDir = NormalizeInstallDir(installDir);

            Report(progressCallback, 5, "正在校验并解压安装包...");
            byte[] rawData;
            List<PayloadFile> files;
            using (Stream resourceStream = Assembly.GetExecutingAssembly().GetManifestResourceStream(Payload.EmbeddedResourceName))
            {
                if (resourceStream == null) throw new InvalidOperationException("安装包内嵌资源缺失，无法继续安装。");
                files = Payload.Read(resourceStream, out rawData);
            }

            Report(progressCallback, 30, "正在准备安装目录...");
            Directory.CreateDirectory(installDir);

            string targetExe = Path.Combine(installDir, AppProfile.TargetExeName);
            StopRunningInstances(targetExe);

            long installedBytes = 0;
            for (int i = 0; i < files.Count; i++)
            {
                PayloadFile f = files[i];
                Report(progressCallback, 35 + (int)((i * 45.0) / files.Count), "正在释放: " + f.Name);

                string dest = Path.Combine(installDir, f.Name);
                WriteFileWithRetry(dest, rawData, f.Offset, (int)f.Length);
                try { File.SetLastWriteTimeUtc(dest, new DateTime(f.LastWriteUtcTicks, DateTimeKind.Utc)); }
                catch (ArgumentOutOfRangeException) { }
                installedBytes += f.Length;
            }

            foreach (string obsolete in AppProfile.ObsoleteFiles)
            {
                TryDeleteFile(Path.Combine(installDir, obsolete));
            }

            if (!File.Exists(targetExe)) throw new InvalidOperationException("安装包中缺少主程序 " + AppProfile.TargetExeName + "。");

            Report(progressCallback, 84, "正在配置快捷方式...");
            string desktopLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), AppProfile.ShortcutName + ".lnk");
            string startMenuDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), AppProfile.StartMenuFolderName);
            string startMenuLnk = Path.Combine(startMenuDir, AppProfile.ShortcutName + ".lnk");

            if (createDesktopShortcut)
            {
                CreateShortcut(desktopLnk, targetExe, installDir, AppProfile.ShortcutDesc);
            }
            if (createStartMenuShortcut)
            {
                Directory.CreateDirectory(startMenuDir);
                CreateShortcut(startMenuLnk, targetExe, installDir, AppProfile.ShortcutDesc);
            }

            if (!AppProfile.IsCreator && associateSpdf)
            {
                Report(progressCallback, 92, "正在关联 " + AppProfile.SpdfExtension + " 文件格式...");
                RegisterFileAssociation(targetExe);
            }

            Report(progressCallback, 96, "正在写入卸载信息...");
            CreateUninstallerAndRegistry(installDir, files, desktopLnk, startMenuDir, startMenuLnk, installedBytes);
            Report(progressCallback, 100, "安装完成！");
        }

        private static void Report(Action<int, string> cb, int percent, string message)
        {
            if (cb != null) cb(percent, message);
        }

        private static List<Process> FindRunningInstances(string exePath)
        {
            List<Process> found = new List<Process>();
            foreach (Process p in Process.GetProcessesByName(Path.GetFileNameWithoutExtension(exePath)))
            {
                try
                {
                    if (p.MainModule != null && string.Equals(p.MainModule.FileName, exePath, StringComparison.OrdinalIgnoreCase))
                    {
                        found.Add(p);
                    }
                }
                catch (System.ComponentModel.Win32Exception) { }    // 其它用户 / 更高权限的进程，读不到路径
                catch (InvalidOperationException) { }               // 进程刚好退出
            }
            return found;
        }

        private static void StopRunningInstances(string exePath)
        {
            foreach (Process p in FindRunningInstances(exePath))
            {
                try
                {
                    p.Kill();
                    p.WaitForExit(5000);
                }
                catch (System.ComponentModel.Win32Exception) { }
                catch (InvalidOperationException) { }
            }
        }

        /// <summary>刚结束的进程（或杀毒软件扫描）可能还会占用文件一小会儿，失败时稍等重试。</summary>
        private static void WriteFileWithRetry(string dest, byte[] data, int offset, int count)
        {
            for (int attempt = 0; ; attempt++)
            {
                try
                {
                    using (FileStream fs = new FileStream(dest, FileMode.Create, FileAccess.Write, FileShare.None))
                    {
                        fs.Write(data, offset, count);
                    }
                    return;
                }
                catch (IOException)
                {
                    if (attempt >= 14) throw;
                }
                catch (UnauthorizedAccessException)
                {
                    if (attempt >= 14) throw;
                    try { File.SetAttributes(dest, FileAttributes.Normal); }   // 目标文件可能被设成了只读
                    catch (IOException) { }
                    catch (UnauthorizedAccessException) { }
                }
                Thread.Sleep(200);
            }
        }

        private static void TryDeleteFile(string path)
        {
            try
            {
                if (File.Exists(path)) File.Delete(path);
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        private static void CreateShortcut(string lnkPath, string targetExe, string workingDir, string description)
        {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return;      // 极少数精简系统禁用了 Windows Script Host：没有快捷方式，但不影响安装

            object shell = Activator.CreateInstance(shellType);
            try
            {
                object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { lnkPath });
                try
                {
                    Type scType = shortcut.GetType();
                    scType.InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe });
                    scType.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, shortcut, new object[] { workingDir });
                    scType.InvokeMember("Description", BindingFlags.SetProperty, null, shortcut, new object[] { description });
                    scType.InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe + ",0" });
                    scType.InvokeMember("Save", BindingFlags.InvokeMethod, null, shortcut, null);
                }
                finally
                {
                    Marshal.ReleaseComObject(shortcut);
                }
            }
            finally
            {
                Marshal.ReleaseComObject(shell);
            }
        }

        private static void RegisterFileAssociation(string readerExe)
        {
            string progKeyPath = @"Software\Classes\" + AppProfile.SpdfProgId;
            using (RegistryKey extKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + AppProfile.SpdfExtension))
            {
                extKey.SetValue(string.Empty, AppProfile.SpdfProgId);
            }
            using (RegistryKey progKey = Registry.CurrentUser.CreateSubKey(progKeyPath))
            {
                progKey.SetValue(string.Empty, "SecurePdf 加密保护文档");
            }
            using (RegistryKey iconKey = Registry.CurrentUser.CreateSubKey(progKeyPath + @"\DefaultIcon"))
            {
                iconKey.SetValue(string.Empty, "\"" + readerExe + "\",0");
            }
            using (RegistryKey cmdKey = Registry.CurrentUser.CreateSubKey(progKeyPath + @"\shell\open\command"))
            {
                cmdKey.SetValue(string.Empty, "\"" + readerExe + "\" \"%1\"");
            }
            SHChangeNotify(SHCNE_ASSOCCHANGED, 0, IntPtr.Zero, IntPtr.Zero);
        }

        /// <summary>批处理里的 % 需要写成 %%；其余特殊字符都处在双引号内，不需要转义。</summary>
        private static string Bat(string path)
        {
            return path.Replace("%", "%%");
        }

        /// <summary>
        /// 生成卸载脚本并登记到“应用和功能”。
        /// 卸载只删除安装器自己放进去的文件，目录空了才移除目录。原版是对安装目录整个 rmdir /s /q，
        /// 用户若把程序装进了一个放着别的文件的文件夹，卸载会把那些文件一起删光。
        /// 快捷方式用安装时解析出的实际路径（桌面被重定向到 OneDrive / 其它盘时，%USERPROFILE%\Desktop 是找不到的）。
        /// 打印次数记录（%LOCALAPPDATA%\SecurePdfSuite\Quotas 与注册表）故意保留：卸载重装不应把已用次数清零。
        /// </summary>
        private static void CreateUninstallerAndRegistry(
            string installDir,
            List<PayloadFile> files,
            string desktopLnk,
            string startMenuDir,
            string startMenuLnk,
            long installedBytes)
        {
            string uninstallBat = Path.Combine(installDir, AppProfile.UninstallBatName);
            string nl = "\r\n";
            StringBuilder sb = new StringBuilder();

            sb.Append("@echo off" + nl);
            sb.Append("chcp 65001 >nul" + nl);
            sb.Append("setlocal DisableDelayedExpansion" + nl);
            sb.Append("set \"APPDIR=" + Bat(installDir) + "\"" + nl);
            sb.Append("taskkill /f /im " + AppProfile.TargetExeName + " >nul 2>&1" + nl);
            sb.Append("ping -n 2 127.0.0.1 >nul" + nl);
            sb.Append("del /f /q \"" + Bat(desktopLnk) + "\" >nul 2>&1" + nl);
            sb.Append("del /f /q \"" + Bat(startMenuLnk) + "\" >nul 2>&1" + nl);
            sb.Append("rmdir \"" + Bat(startMenuDir) + "\" >nul 2>&1" + nl);

            if (!AppProfile.IsCreator)
            {
                // 只有当后缀仍然指向本软件时才删除后缀关联
                sb.Append("reg query \"HKCU\\Software\\Classes\\" + AppProfile.SpdfExtension + "\" /ve 2>nul | \"%SystemRoot%\\System32\\find.exe\" /i \"SecurePdfSuite.\" >nul && reg delete \"HKCU\\Software\\Classes\\" + AppProfile.SpdfExtension + "\" /f >nul 2>&1" + nl);
                sb.Append("reg delete \"HKCU\\Software\\Classes\\" + AppProfile.SpdfProgId + "\" /f >nul 2>&1" + nl);
                if (!AppProfile.IsSelfTest)
                {
                    sb.Append("reg delete \"HKCU\\Software\\Classes\\" + AppProfile.LegacySpdfProgId + "\" /f >nul 2>&1" + nl);
                }
            }

            sb.Append("reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\" + AppProfile.UninstallKeyName + "\" /f >nul 2>&1" + nl);

            foreach (PayloadFile f in files)
            {
                sb.Append("del /f /q \"%APPDIR%\\" + Bat(f.Name) + "\" >nul 2>&1" + nl);
            }
            foreach (string obsolete in AppProfile.ObsoleteFiles)
            {
                sb.Append("del /f /q \"%APPDIR%\\" + obsolete + "\" >nul 2>&1" + nl);
            }

            sb.Append("echo 已完成卸载。" + nl);
            sb.Append("cd /d \"%SystemRoot%\"" + nl);
            // (goto) 让批处理在不再读取自身文件的情况下执行完这一行，于是脚本可以删掉自己，再移除（已清空的）安装目录；
            // 目录里还有用户自己的文件时 rmdir 会失败并保留目录，末尾的 ver 只是让脚本的退出码仍为 0
            sb.Append("(goto) 2>nul & del /f /q \"%APPDIR%\\" + AppProfile.UninstallBatName + "\" & rmdir \"%APPDIR%\" 2>nul & ver >nul" + nl);

            File.WriteAllText(uninstallBat, sb.ToString(), new UTF8Encoding(false));
            installedBytes += new FileInfo(uninstallBat).Length;

            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\" + AppProfile.UninstallKeyName))
            {
                key.SetValue("DisplayName", AppProfile.UninstallDisplayName);
                key.SetValue("DisplayVersion", AppProfile.ProductVersion);
                key.SetValue("Publisher", "SecurePdfSuite");
                key.SetValue("InstallLocation", installDir);
                key.SetValue("DisplayIcon", Path.Combine(installDir, AppProfile.TargetExeName) + ",0");
                key.SetValue("InstallDate", DateTime.Now.ToString("yyyyMMdd"));
                key.SetValue("EstimatedSize", (int)Math.Max(1, (installedBytes + 1023) / 1024), RegistryValueKind.DWord);
                key.SetValue("UninstallString", "\"" + uninstallBat + "\"");
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
        }
    }

    internal sealed class InstallerForm : Form
    {
        private readonly TextBox _pathBox;
        private readonly Button _browseBtn;
        private readonly CheckBox _chkDesktop;
        private readonly CheckBox _chkStartMenu;
        private readonly CheckBox _chkAssoc;
        private readonly ProgressBar _progressBar;
        private readonly Label _statusLabel;
        private readonly Button _installBtn;
        private readonly Button _cancelBtn;
        private readonly Button _launchBtn;

        private string _installedDir = string.Empty;
        private bool _installing;

        public InstallerForm()
        {
            SuspendLayout();

            // 下面所有坐标都按 96 DPI 书写；AutoScaleMode.Dpi 会在 125% / 150% 等缩放下把位置和尺寸一起放大。
            // 原版只让字体跟着 DPI 变大，控件位置却是固定像素，高缩放下文字会被截断或互相重叠。
            AutoScaleDimensions = new SizeF(96F, 96F);
            AutoScaleMode = AutoScaleMode.Dpi;

            Text = AppProfile.WindowTitle;
            ClientSize = new Size(600, 410);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(15, 23, 42);
            ForeColor = Color.FromArgb(248, 250, 252);
            Font = new Font("Microsoft YaHei UI", 9.5f);

            Panel header = new Panel { Location = new Point(0, 0), Size = new Size(600, 82), BackColor = Color.FromArgb(30, 41, 59) };
            header.Controls.Add(new Label { Text = AppProfile.BannerTitle, Font = new Font("Microsoft YaHei UI", 14f, FontStyle.Bold), ForeColor = Color.FromArgb(248, 250, 252), AutoSize = true, Location = new Point(22, 14) });
            header.Controls.Add(new Label { Text = AppProfile.BannerSub, Font = new Font("Microsoft YaHei UI", 9f), ForeColor = Color.FromArgb(148, 163, 184), AutoSize = true, Location = new Point(24, 48) });
            Controls.Add(header);

            Controls.Add(new Label { Text = "安装目标路径:", Location = new Point(24, 100), AutoSize = true, ForeColor = Color.FromArgb(203, 213, 225) });

            _pathBox = new TextBox
            {
                Location = new Point(24, 128),
                Size = new Size(450, 26),
                BackColor = Color.FromArgb(30, 41, 59),
                ForeColor = Color.FromArgb(248, 250, 252),
                BorderStyle = BorderStyle.FixedSingle,
                Text = InstallEngine.DefaultInstallDir
            };
            Controls.Add(_pathBox);

            _browseBtn = MakeButton("浏览...", new Rectangle(484, 126, 92, 30), Color.FromArgb(51, 65, 85), false);
            _browseBtn.Click += BrowseBtn_Click;
            Controls.Add(_browseBtn);

            _chkDesktop = MakeCheck(AppProfile.DesktopCheckText, 176);
            _chkStartMenu = MakeCheck("在“开始”菜单中创建快捷方式", 208);
            _chkAssoc = MakeCheck("关联 " + AppProfile.SpdfExtension + " 文件后缀（双击加密文件直接打开阅读）", 240);
            Controls.Add(_chkDesktop);
            Controls.Add(_chkStartMenu);
            if (!AppProfile.IsCreator) Controls.Add(_chkAssoc);

            _statusLabel = new Label { Text = "就绪，点击“立即安装”开始释放程序。", Location = new Point(24, 282), Size = new Size(552, 24), AutoEllipsis = true, ForeColor = Color.FromArgb(56, 189, 248) };
            Controls.Add(_statusLabel);

            _progressBar = new ProgressBar { Location = new Point(24, 312), Size = new Size(552, 18), Minimum = 0, Maximum = 100 };
            Controls.Add(_progressBar);

            _launchBtn = MakeButton(AppProfile.LaunchBtnText, new Rectangle(290, 352, 150, 36), Color.FromArgb(5, 150, 105), false);
            _launchBtn.Visible = false;
            _launchBtn.Click += delegate { LaunchApp(); Close(); };
            Controls.Add(_launchBtn);

            _cancelBtn = MakeButton("取消", new Rectangle(340, 352, 100, 36), Color.FromArgb(51, 65, 85), false);
            _cancelBtn.Click += delegate { Close(); };
            Controls.Add(_cancelBtn);

            _installBtn = MakeButton("立即安装", new Rectangle(452, 352, 124, 36), Color.FromArgb(2, 132, 199), true);
            _installBtn.Click += InstallBtn_Click;
            Controls.Add(_installBtn);

            AcceptButton = _installBtn;
            ResumeLayout(false);
            PerformLayout();
        }

        private static Button MakeButton(string text, Rectangle bounds, Color backColor, bool bold)
        {
            Button b = new Button { Text = text, Location = bounds.Location, Size = bounds.Size, FlatStyle = FlatStyle.Flat, BackColor = backColor, ForeColor = Color.White };
            if (bold) b.Font = new Font("Microsoft YaHei UI", 9.5f, FontStyle.Bold);
            b.FlatAppearance.BorderSize = 0;
            return b;
        }

        private static CheckBox MakeCheck(string text, int top)
        {
            return new CheckBox { Text = text, Location = new Point(24, top), Size = new Size(552, 26), Checked = true, ForeColor = Color.FromArgb(226, 232, 240) };
        }

        protected override void OnLoad(EventArgs e)
        {
            base.OnLoad(e);
            // 固定大小的 Label 在 DPI 自动缩放时宽高会被多放大一次，这里按进度条对齐回来
            _statusLabel.Left = _progressBar.Left;
            _statusLabel.Width = _progressBar.Width;
            _statusLabel.Height = _progressBar.Top - _statusLabel.Top - 4;
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            // 释放文件的过程中不允许关窗口，否则会留下装了一半的目录
            if (_installing && e.CloseReason == CloseReason.UserClosing) e.Cancel = true;
            base.OnFormClosing(e);
        }

        private void BrowseBtn_Click(object sender, EventArgs e)
        {
            using (FolderBrowserDialog dlg = new FolderBrowserDialog())
            {
                dlg.Description = "选择安装位置（程序将安装到其中的 " + AppProfile.DefaultFolderName + " 文件夹）";
                string current = _pathBox.Text.Trim();
                if (Directory.Exists(current)) dlg.SelectedPath = current;

                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    string selected = dlg.SelectedPath;
                    if (!string.Equals(Path.GetFileName(selected.TrimEnd('\\', '/')), AppProfile.DefaultFolderName, StringComparison.OrdinalIgnoreCase))
                    {
                        selected = Path.Combine(selected, AppProfile.DefaultFolderName);
                    }
                    _pathBox.Text = selected;
                }
            }
        }

        private void SetInputsEnabled(bool enabled)
        {
            _installBtn.Enabled = enabled;
            _browseBtn.Enabled = enabled;
            _pathBox.Enabled = enabled;
            _cancelBtn.Enabled = enabled;
            _chkDesktop.Enabled = enabled;
            _chkStartMenu.Enabled = enabled;
            _chkAssoc.Enabled = enabled;
        }

        private void InstallBtn_Click(object sender, EventArgs e)
        {
            if (_installing) return;

            string targetDir;
            try
            {
                targetDir = InstallEngine.NormalizeInstallDir(_pathBox.Text);
            }
            catch (Exception ex)
            {
                if (!(ex is ArgumentException || ex is NotSupportedException || ex is PathTooLongException || ex is System.Security.SecurityException)) throw;
                MessageBox.Show(this, "安装路径无效：" + ex.Message, "提示", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            _pathBox.Text = targetDir;

            if (InstallEngine.IsTargetRunning(targetDir))
            {
                DialogResult answer = MessageBox.Show(this,
                    AppProfile.TargetExeName + " 正在运行。继续安装会关闭它，尚未保存的内容（例如创建端表格里还没导出的验证码）将丢失。\n\n是否继续？",
                    "程序正在运行", MessageBoxButtons.OKCancel, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2);
                if (answer != DialogResult.OK) return;
            }

            bool desktop = _chkDesktop.Checked;
            bool startMenu = _chkStartMenu.Checked;
            bool assoc = !AppProfile.IsCreator && _chkAssoc.Checked;

            _installing = true;
            SetInputsEnabled(false);
            _statusLabel.ForeColor = Color.FromArgb(56, 189, 248);

            Thread worker = new Thread(delegate()
            {
                Exception failure = null;
                try
                {
                    InstallEngine.Install(targetDir, desktop, startMenu, assoc, delegate(int pct, string msg)
                    {
                        BeginInvoke(new Action(delegate()
                        {
                            _progressBar.Value = Math.Max(0, Math.Min(100, pct));
                            _statusLabel.Text = msg;
                        }));
                    });
                }
                catch (Exception ex)
                {
                    failure = ex;
                }

                BeginInvoke(new Action(delegate()
                {
                    _installing = false;
                    if (failure == null)
                    {
                        _installedDir = targetDir;
                        _statusLabel.ForeColor = Color.FromArgb(52, 211, 153);
                        _statusLabel.Text = "安装成功！已安装至: " + targetDir;
                        _progressBar.Value = 100;

                        // “完成”按钮直接占用“立即安装”按钮的位置（已随 DPI 缩放过），不再写死像素坐标
                        _cancelBtn.Bounds = _installBtn.Bounds;
                        _cancelBtn.Text = "完成";
                        _cancelBtn.Enabled = true;
                        _installBtn.Visible = false;
                        _launchBtn.Visible = true;
                        AcceptButton = _launchBtn;
                    }
                    else
                    {
                        _statusLabel.ForeColor = Color.FromArgb(248, 113, 113);
                        _statusLabel.Text = "安装失败: " + failure.Message;
                        _progressBar.Value = 0;
                        SetInputsEnabled(true);
                        MessageBox.Show(this, "安装过程出错：\n" + failure.Message, "安装失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    }
                }));
            });
            worker.IsBackground = true;
            worker.SetApartmentState(ApartmentState.STA);   // 创建快捷方式要用到 COM
            worker.Start();
        }

        private void LaunchApp()
        {
            if (string.IsNullOrEmpty(_installedDir)) return;
            string exe = Path.Combine(_installedDir, AppProfile.TargetExeName);
            if (!File.Exists(exe)) return;

            try
            {
                Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = _installedDir, UseShellExecute = true });
            }
            catch (System.ComponentModel.Win32Exception ex)
            {
                MessageBox.Show(this, "无法启动程序：" + ex.Message, "提示", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            }
        }
    }
#endif
}
```

## `src/SecurePdf.Installer/assets/reader/THIRD-PARTY-NOTICES.txt`

**模块说明**：阅读端随包安装的第三方组件开源许可证声明（PDFium 159.0.8152 BSD-3-Clause 及嵌入式依赖声明）（共 `42` 行）

```text
SecurePdf 阅读端 —— 第三方组件声明
====================================

本软件随附并使用以下第三方组件：

PDFium  (pdfium.dll，版本 159.0.8152，由 bblanchon/pdfium-binaries 项目预编译，未做修改)
  项目主页:  https://pdfium.googlesource.com/pdfium/
  预编译包:  https://github.com/bblanchon/pdfium-binaries
  许可:      BSD 3-Clause（见下）及 Apache License 2.0
  完整许可文本（含其内部使用的 FreeType、libjpeg-turbo、OpenJPEG、Little CMS、zlib、ICU、
  abseil 等组件各自的许可）:
             https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE
             https://github.com/bblanchon/pdfium-binaries/tree/master/licenses

------------------------------------------------------------------------------
Copyright 2014 The PDFium Authors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
     notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
     copyright notice, this list of conditions and the following disclaimer
     in the documentation and/or other materials provided with the
     distribution.
   * Neither the name of Google LLC nor the names of its
     contributors may be used to endorse or promote products derived from
     this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## `src/SecurePdf.Installer/assets/reader/关联spdf文件格式.bat`

**模块说明**：手工修复当前用户（HKCU）`.spdf` -> `SecurePdfSuite.SpdfFile` 双击关联的批处理脚本（共 `17` 行）

```bat
@echo off
chcp 65001 >nul
rem 把 .spdf 文件关联到本脚本所在文件夹里的 SecurePdfReader.exe（仅对当前用户生效，不需要管理员权限）。
rem 使用安装程序并勾选了“关联 .spdf 文件后缀”时不需要再运行本脚本；它用于直接拷贝文件夹使用（免安装）的场合。
set "READER=%~dp0SecurePdfReader.exe"
if not exist "%READER%" (
    echo 未找到 SecurePdfReader.exe，请把本脚本放在阅读端程序所在的文件夹内再运行。
    pause
    exit /b 1
)
reg add "HKCU\Software\Classes\.spdf" /ve /t REG_SZ /d "SecurePdfSuite.SpdfFile" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile" /ve /t REG_SZ /d "SecurePdf 加密保护文档" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile\DefaultIcon" /ve /t REG_SZ /d "\"%READER%\",0" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile\shell\open\command" /ve /t REG_SZ /d "\"%READER%\" \"%%1\"" /f >nul
reg delete "HKCU\Software\Classes\SecurePdfSuite.spdf" /f >nul 2>&1
echo 已将 .spdf 文件关联到 %READER%
pause
```

## `tests/SecurePdf.Core.Tests/SecurePdf.Core.Tests.csproj`

**模块说明**：xUnit 2.7.0 单元测试项目配置（目标框架 `net48`，含 `Microsoft.NETFramework.ReferenceAssemblies` 支持在 SDK 下稳定编译与测试）（共 `21` 行）

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net48</TargetFramework>
    <IsPackable>false</IsPackable>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net48" Version="1.0.3" PrivateAssets="All" />
    <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.9.0" />
    <PackageReference Include="xunit" Version="2.7.0" />
    <PackageReference Include="xunit.runner.visualstudio" Version="2.5.7" />
  </ItemGroup>

  <ItemGroup>
    <ProjectReference Include="..\..\src\SecurePdf.Core\SecurePdf.Core.csproj" />
  </ItemGroup>

  <ItemGroup>
    <Reference Include="System.Drawing" />
  </ItemGroup>
</Project>
```

## `tests/SecurePdf.Core.Tests/CryptoAndContextTests.cs`

**模块说明**：验证验证码随机性与易混淆字符剔除、设备指纹采集与本机活跃 IPv4 获取（共 `29` 行）

```csharp
using SecurePdf.Core.Crypto;
using SecurePdf.Core.Licensing;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class CryptoAndContextTests
    {
        [Fact]
        public void GenerateVerificationCode_ProducesDistinctFormattedCodes()
        {
            string code1 = CodeAndCryptoService.GenerateVerificationCode();
            string code2 = CodeAndCryptoService.GenerateVerificationCode();

            Assert.NotEqual(code1, code2);
            Assert.Equal(14, code1.Length); // XXXX-XXXX-XXXX
            Assert.Equal(12, CodeAndCryptoService.NormalizeCode(" " + code1.ToLowerInvariant() + " ").Length);
        }

        [Fact]
        public void DeviceAndNetworkContext_CapturesMachineCodeAndIp()
        {
            RuntimeTraceContext ctx = DeviceAndNetworkContext.Capture();
            Assert.Equal(32, ctx.MachineHash.Length);
            Assert.False(string.IsNullOrWhiteSpace(ctx.MachineCode));
            Assert.False(string.IsNullOrWhiteSpace(ctx.LocalIpAddress));
        }
    }
}
```

## `tests/SecurePdf.Core.Tests/SpdfContainerTests.cs`

**模块说明**：验证 `.spdf` 加密/解包往返、全角/空白验证码容错、错误验证码拦截、密文篡改拒绝、截断文件与恶意 `scrypt` 成本边界拒绝（共 `85` 行）

```csharp
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using SecurePdf.Core.Format;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class SpdfContainerTests
    {
        private static byte[] CreateMinimalPdfBytes(string tag)
        {
            return Encoding.ASCII.GetBytes("%PDF-1.4\n% " + tag + "\n1 0 obj <<>> endobj\n%%EOF\n");
        }

        [Fact]
        public void CreateAndUnlock_RoundTripsPayloadAndPolicy()
        {
            string tempDir = Path.Combine(Path.GetTempPath(), "spdf_test_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(tempDir);
            try
            {
                string outPath = Path.Combine(tempDir, "sample.spdf");
                byte[] pdfBytes = CreateMinimalPdfBytes("ConfidentialDoc");
                var options = new SpdfCreationOptions
                {
                    MaxPrints = 2,
                    MoireLevel = MoireStrength.High,
                    SecurityFlags = SpdfSecurityFlags.EnableHiddenWatermark | SpdfSecurityFlags.EnableBlurOnUnfocus,
                    OriginalFileName = "contract.pdf",
                    WatermarkNote = "Internal",
                    KdfCostN = 1024
                };

                var created = SpdfContainer.Create(pdfBytes, outPath, options);
                Assert.NotEqual(Guid.Empty, created.DocId);
                Assert.False(string.IsNullOrWhiteSpace(created.VerificationCode));

                SpdfEnvelopeInfo env = SpdfContainer.ReadEnvelope(outPath);
                Assert.Equal(created.DocId, env.DocId);
                Assert.Equal((ushort)2, env.FormatVersion);

                using (UnlockedSpdfDocument doc = SpdfContainer.Unlock(outPath, created.VerificationCode))
                {
                    Assert.Equal(created.DocId, doc.Policy.DocId);
                    Assert.Equal(2, doc.Policy.MaxPrints);
                    Assert.Equal(MoireStrength.High, doc.Policy.MoireLevel);
                    Assert.Equal("contract.pdf", doc.Policy.OriginalFileName);
                    Assert.Equal("Internal", doc.Policy.WatermarkNote);
                    Assert.Equal(pdfBytes, doc.PdfBytes);
                }
            }
            finally
            {
                if (Directory.Exists(tempDir)) Directory.Delete(tempDir, true);
            }
        }

        [Fact]
        public void Unlock_WithWrongCodeOrTamperedCiphertext_ThrowsCryptographicException()
        {
            string tempDir = Path.Combine(Path.GetTempPath(), "spdf_test_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(tempDir);
            try
            {
                string outPath = Path.Combine(tempDir, "tamper.spdf");
                byte[] pdfBytes = CreateMinimalPdfBytes("Secret");
                var created = SpdfContainer.Create(pdfBytes, outPath, new SpdfCreationOptions { KdfCostN = 1024 });

                Assert.ThrowsAny<CryptographicException>(() => SpdfContainer.Unlock(outPath, "WRONG-CODE-9999"));

                byte[] raw = File.ReadAllBytes(outPath);
                raw[raw.Length - 5] ^= 0xFF;
                File.WriteAllBytes(outPath, raw);

                Assert.ThrowsAny<CryptographicException>(() => SpdfContainer.Unlock(outPath, created.VerificationCode));
            }
            finally
            {
                if (Directory.Exists(tempDir)) Directory.Delete(tempDir, true);
            }
        }
    }
}
```

## `tests/SecurePdf.Core.Tests/LocalQuotaStoreTests.cs`

**模块说明**：验证打印配额扣减/回滚、双存储单边删除自动修复、双存储均损坏时 fail-closed 异常、系统时间回拨检测锁定与小数小时过期校验（共 `91` 行）

```csharp
using System;
using System.Collections.Generic;
using SecurePdf.Core.Storage;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class LocalQuotaStoreTests
    {
        private sealed class InMemoryQuotaBackend : IQuotaPersistenceBackend
        {
            private readonly Dictionary<Guid, byte[]> _primary = new Dictionary<Guid, byte[]>();
            private readonly Dictionary<Guid, byte[]> _secondary = new Dictionary<Guid, byte[]>();

            public byte[]? ReadPrimary(Guid docId) => _primary.TryGetValue(docId, out var d) ? (byte[])d.Clone() : null;
            public byte[]? ReadSecondary(Guid docId) => _secondary.TryGetValue(docId, out var d) ? (byte[])d.Clone() : null;
            public void WritePrimary(Guid docId, byte[] protectedBytes) => _primary[docId] = (byte[])protectedBytes.Clone();
            public void WriteSecondary(Guid docId, byte[] protectedBytes) => _secondary[docId] = (byte[])protectedBytes.Clone();

            public void DeletePrimary(Guid docId) => _primary.Remove(docId);
            public void CorruptSecondary(Guid docId)
            {
                if (_secondary.TryGetValue(docId, out var d) && d.Length > 0)
                {
                    d[d.Length - 1] ^= 0xFF;
                }
            }
        }

        [Fact]
        public void ReserveAndRollback_EnforcesPrintLimitAndDualRecovery()
        {
            var backend = new InMemoryQuotaBackend();
            byte[] machineHash = new byte[32];
            for (int i = 0; i < machineHash.Length; i++) machineHash[i] = (byte)(i + 1);

            var store = new LocalQuotaStore(machineHash, backend, useDpapi: false);
            Guid docId = Guid.NewGuid();
            var now = new DateTimeOffset(2026, 6, 1, 10, 0, 0, TimeSpan.Zero);

            QuotaAccessResult init = store.LoadAndVerify(docId, 2, null, now);
            Assert.True(init.IsAllowed);
            Assert.Equal(2, init.Snapshot.RemainingPrints);

            QuotaSnapshot s1 = store.ReservePrintSlot(docId, 2, null, now.AddMinutes(1));
            Assert.Equal(1, s1.UsedPrints);
            Assert.Equal(1, s1.RemainingPrints);

            // Deleting primary should still retain count from secondary and repair both.
            backend.DeletePrimary(docId);
            QuotaAccessResult recovered = store.LoadAndVerify(docId, 2, null, now.AddMinutes(2));
            Assert.True(recovered.IsAllowed);
            Assert.True(recovered.TamperWarning);
            Assert.Equal(1, recovered.Snapshot.UsedPrints);

            QuotaSnapshot s2 = store.ReservePrintSlot(docId, 2, null, now.AddMinutes(3));
            Assert.Equal(2, s2.UsedPrints);
            Assert.Equal(0, s2.RemainingPrints);

            Assert.Throws<InvalidOperationException>(() => store.ReservePrintSlot(docId, 2, null, now.AddMinutes(4)));

            QuotaSnapshot rolledBack = store.RollbackPrintSlot(s2);
            Assert.Equal(1, rolledBack.UsedPrints);
            Assert.Equal(1, rolledBack.RemainingPrints);
        }

        [Fact]
        public void LoadAndVerify_DetectsClockRollbackAndExpiry()
        {
            var backend = new InMemoryQuotaBackend();
            byte[] machineHash = new byte[32];
            var store = new LocalQuotaStore(machineHash, backend, useDpapi: false);
            Guid docId = Guid.NewGuid();
            var t0 = new DateTimeOffset(2026, 6, 1, 12, 0, 0, TimeSpan.Zero);

            Assert.True(store.LoadAndVerify(docId, 1, t0.AddHours(2), t0).IsAllowed);

            // Rollback by 1 hour > tolerance (5 min)
            QuotaAccessResult rollback = store.LoadAndVerify(docId, 1, t0.AddHours(2), t0.AddHours(-1));
            Assert.False(rollback.IsAllowed);
            Assert.True(rollback.TamperWarning);

            // Expired after hour-based window (e.g. 2 hours)
            QuotaAccessResult stillValid = store.LoadAndVerify(docId, 1, t0.AddHours(2), t0.AddHours(1.5));
            Assert.True(stillValid.IsAllowed);

            QuotaAccessResult expired = store.LoadAndVerify(docId, 1, t0.AddHours(2), t0.AddHours(2).AddMinutes(1));
            Assert.False(expired.IsAllowed);
        }
    }
}
```

## `tests/SecurePdf.Core.Tests/PrinterSecurityGuardTests.cs`

**模块说明**：验证拦截 Microsoft Print to PDF / XPS / WPS PDF / Adobe PDF / 传真 / `KeepPrintedJobs` / 未认证 Local Monitor 本地端口 / 回环与本机 IP 映射，放行合法 USB 与 TCPIP 物理打印机，并校验页码范围与 DPI 预算计算（共 `234` 行）

```csharp
using System;
using System.Linq;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Printing;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class PrinterSecurityGuardTests
    {
        private const string TcpMonitor = "Standard TCP/IP Port";
        private const string UsbMonitor = "Dynamic Print Monitor";
        private const string LocalMonitor = "Local Monitor";

        private const uint AttributeKeepPrintedJobs = 0x00000100u;

        private static PrinterCandidate Physical(string port, string monitor, uint attributes = 0u)
        {
            return new PrinterCandidate
            {
                PrinterName = "HP LaserJet Pro M404",
                PortName = port,
                DriverName = "HP Universal Printing PCL 6",
                MonitorName = monitor,
                Attributes = attributes
            };
        }

        private static void AssertBlocked(PrinterCandidate candidate, string? reasonFragment = null)
        {
            PrinterValidationResult result = PrinterSecurityGuard.EvaluatePrinterCandidate(candidate);
            Assert.False(result.IsAllowedPhysicalPrinter, $"端口 {candidate.PortName} 不应放行");
            if (reasonFragment != null) Assert.Contains(reasonFragment, result.Reason);
        }

        private static void AssertAllowed(PrinterCandidate candidate)
        {
            PrinterValidationResult result = PrinterSecurityGuard.EvaluatePrinterCandidate(candidate);
            Assert.True(result.IsAllowedPhysicalPrinter, $"端口 {candidate.PortName} 应放行，实际被拒：{result.Reason}");
        }

        [Theory]
        [InlineData("Microsoft Print to PDF", "PORTPROMPT:", "Microsoft Print To PDF", "Local Port", 0u)]
        [InlineData("Microsoft XPS Document Writer", "PORTPROMPT:", "Microsoft XPS Document Writer v4", "", 0u)]
        [InlineData("WPS PDF", "FILE:", "Kingsoft PDF Driver", "", 0u)]
        [InlineData("Adobe PDF", "Documents\\*.pdf", "Adobe PDF Converter", "Adobe PDF Port Monitor", 0u)]
        [InlineData("Fax", "SHRFAX:", "Microsoft Shared Fax Driver", "", 0x00004000u)]
        [InlineData("OneNote (Desktop)", "nul:", "Send to Microsoft OneNote 16 Driver", "Local Monitor", 0x240u)]
        public void EvaluatePrinterCandidate_BlocksVirtualAndFilePrinters(
            string name,
            string port,
            string driver,
            string monitor,
            uint attributes)
        {
            var candidate = new PrinterCandidate
            {
                PrinterName = name,
                PortName = port,
                DriverName = driver,
                MonitorName = monitor,
                Attributes = attributes
            };

            PrinterValidationResult result = PrinterSecurityGuard.EvaluatePrinterCandidate(candidate);
            Assert.False(result.IsAllowedPhysicalPrinter);
        }

        [Theory]
        [InlineData("HP LaserJet Pro M404", "USB001", "HP Universal Printing PCL 6", "Standard TCP/IP Port")]
        [InlineData("Canon MG3000 series", "USB001", "Canon MG3000 series", "Dynamic Print Monitor")]
        // 192.0.2.0/24 是文档保留网段，不会恰好是测试机自己的地址
        [InlineData("Canon Office Printer", "192.0.2.50", "Canon Generic Plus PCL6", "Standard TCP/IP Port")]
        [InlineData("Canon Office Printer", "IP_192.0.2.50", "Canon Generic Plus PCL6", "Standard TCP/IP Port")]
        [InlineData("Brother HL-L2350DW", "WSD-12345", "Brother Laser Type1 Class Driver", "WSD Port")]
        [InlineData("Epson L3150", "EP1A2B3C:L3150", "EPSON L3150 Series", "EpsonNet Print Port")]
        public void EvaluatePrinterCandidate_AllowsPhysicalPrinters(
            string name,
            string port,
            string driver,
            string monitor)
        {
            var candidate = new PrinterCandidate
            {
                PrinterName = name,
                PortName = port,
                DriverName = driver,
                MonitorName = monitor,
                Attributes = 0u
            };

            PrinterValidationResult result = PrinterSecurityGuard.EvaluatePrinterCandidate(candidate);
            Assert.True(result.IsAllowedPhysicalPrinter, result.Reason);
        }

        // ---- 收紧规则 ①：“本地端口”监视器下的端口，名字再像物理端口也是输出到文件

        [Theory]
        [InlineData("USB999", "Local Monitor")]
        [InlineData("USB001", "Local Port")]
        [InlineData("IP_10.1.2.3", "Local Monitor")]
        [InlineData("10.1.2.3", "Local Monitor")]
        [InlineData("WSD-12345", "Local Monitor")]
        [InlineData("DOT4_001", "Local Monitor")]
        [InlineData("IPP_printer", "local monitor")]
        [InlineData("LPT1.prn", "Local Monitor")]
        [InlineData("COM1x", "Local Monitor")]
        [InlineData("LPT", "Local Monitor")]
        public void LocalPortMonitor_RejectsPortsThatOnlyLookPhysicalByName(string port, string monitor)
        {
            AssertBlocked(Physical(port, monitor), "自定义本地端口");
        }

        [Theory]
        [InlineData("LPT1:")]
        [InlineData("LPT3")]
        [InlineData("COM4:")]
        public void LocalPortMonitor_StillAllowsRealParallelAndSerialPorts(string port)
        {
            AssertAllowed(Physical(port, LocalMonitor));
        }

        [Fact]
        public void LocalPortMonitor_AllowsSharedPrinterOnRemoteServer()
        {
            // 超过 15 个字符，不可能与测试机的计算机名相同
            AssertAllowed(Physical(@"\\REMOTE-PRINT-SERVER-01\HP-4F", LocalMonitor));
        }

        [Theory]
        [InlineData(@"C:\Temp\out.prn")]
        [InlineData("D:")]
        [InlineData(@"\\REMOTE-PRINT-SERVER-01\share\out.prn")]
        [InlineData(@"\\REMOTE-PRINT-SERVER-01\")]
        [InlineData(@"\\.\pipe\spool")]
        [InlineData(@"\\?\C:\out.prn")]
        [InlineData(@"\\localhost\HP-4F")]
        [InlineData(@"\\127.0.0.1\HP-4F")]
        public void RejectsFilePipeAndLoopbackSharePorts(string port)
        {
            AssertBlocked(Physical(port, LocalMonitor));
        }

        [Fact]
        public void RejectsSharePortPointingAtThisMachine()
        {
            AssertBlocked(Physical(@"\\" + Environment.MachineName + @"\HP-4F", LocalMonitor), "本机共享");
        }

        // ---- 收紧规则 ②：端口名本身是指向本机的 IP

        [Theory]
        [InlineData("0.0.0.0")]
        [InlineData("IP_0.0.0.0")]
        [InlineData("127.0.0.1")]
        [InlineData("IP_127.0.0.1")]
        [InlineData("127.1.2.3")]
        [InlineData("ip_127.1.2.3")]
        public void TcpIpPort_RejectsLoopbackAddressInPortName(string port)
        {
            AssertBlocked(Physical(port, TcpMonitor));
        }

        [Fact]
        public void TcpIpPort_RejectsThisMachinesOwnAddressInPortName()
        {
            string[] ownIpv4 = DeviceAndNetworkContext.GetAllLocalAddresses()
                .Where(a => a.Count(c => c == '.') == 3 && !a.Contains(":") && !a.StartsWith("127.", StringComparison.Ordinal))
                .ToArray();

            foreach (string address in ownIpv4)
            {
                AssertBlocked(Physical(address, TcpMonitor), "指向本机");
                AssertBlocked(Physical("IP_" + address, TcpMonitor), "指向本机");
            }
        }

        [Theory]
        [InlineData("localhost")]
        [InlineData("LOCALHOST.")]
        [InlineData("127.0.0.1")]
        [InlineData("::1")]
        [InlineData("[::1]")]
        public void RejectsNetworkPortWhoseConfiguredTargetIsThisMachine(string target)
        {
            // 端口名看起来正常，但端口配置里的主机指向本机
            PrinterCandidate candidate = Physical("IP_192.0.2.50", TcpMonitor);
            candidate.NetworkTargets.Add(target);
            AssertBlocked(candidate, "指向本机");
        }

        [Fact]
        public void RejectsNetworkTargetAndServerNamedAfterThisMachine()
        {
            PrinterCandidate byTarget = Physical("IP_192.0.2.50", TcpMonitor);
            byTarget.NetworkTargets.Add(Environment.MachineName.ToLowerInvariant());
            AssertBlocked(byTarget, "指向本机");

            PrinterCandidate byServer = Physical("USB001", UsbMonitor);
            byServer.ServerName = @"\\" + Environment.MachineName;
            AssertBlocked(byServer, "共享连接回本机");
        }

        // ---- 打印机池与其它属性

        [Fact]
        public void PrinterPool_RequiresEveryPortToPass()
        {
            AssertAllowed(Physical("USB001, USB002", UsbMonitor));
            AssertBlocked(Physical("USB001,FILE:", UsbMonitor));

            // 第二个端口实际挂在“本地端口”监视器下：整台打印机都要拒绝
            PrinterCandidate pool = Physical("USB001,USB002", UsbMonitor);
            pool.PortMonitors["USB001"] = UsbMonitor;
            pool.PortMonitors["USB002"] = LocalMonitor;
            AssertBlocked(pool, "自定义本地端口");
        }

        [Theory]
        [InlineData("")]
        [InlineData(" , ")]
        [InlineData("SOMEPORT1")]
        public void RejectsMissingOrUnrecognisedPorts(string port)
        {
            AssertBlocked(Physical(port, string.Empty));
        }

        [Fact]
        public void RejectsPrinterThatKeepsPrintedJobs()
        {
            AssertBlocked(Physical("USB001", UsbMonitor, AttributeKeepPrintedJobs), "保留打印的文档");
        }
    }
}
```

## `tests/SecurePdf.Core.Tests/WatermarkEnhancerTests.cs`

**模块说明**：验证在模拟彩虹摩尔纹与强亮度渐变干扰下 `ChromaStegoExtract` 对蓝通道微偏水印的提取信噪比及超限像素防护（共 `81` 行）

```csharp
using System;
using System.Drawing;
using SecurePdf.Core.Forensics;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class WatermarkEnhancerTests
    {
        [Fact]
        public void ChromaStegoExtract_RecoversHiddenWatermarkDespiteMoireAndUnevenBrightness()
        {
            const int w = 400;
            const int h = 240;

            using (var src = new Bitmap(w, h))
            {
                // Simulate uneven camera exposure + wide rainbow Moire waves
                for (int y = 0; y < h; y++)
                {
                    for (int x = 0; x < w; x++)
                    {
                        double wave = Math.Sin(x * 0.05 + y * 0.03) * 18.0;
                        int baseL = (int)(225 + wave - y * 0.08);
                        int r = ClampByte(baseL + (int)(Math.Sin(x * 0.04) * 8));
                        int g = ClampByte(baseL);
                        int b = ClampByte(baseL + (int)(Math.Cos(y * 0.04) * 8));
                        src.SetPixel(x, y, Color.FromArgb(255, r, g, b));
                    }
                }

                // Draw neutral black document text & subtle blue-channel hidden watermark
                using (Graphics g = Graphics.FromImage(src))
                {
                    using (var docFont = new Font("Arial", 22, FontStyle.Bold))
                    using (var blackBrush = new SolidBrush(Color.FromArgb(255, 25, 25, 25)))
                    {
                        g.DrawString("CONFIDENTIAL DOC", docFont, blackBrush, 20, 20);
                    }

                    using (var wmFont = new Font("Arial", 18, FontStyle.Bold))
                    using (var stegoBrush = new SolidBrush(Color.FromArgb(18, 0, 70, 245)))
                    {
                        g.DrawString("IP:172.20.10.5", wmFont, stegoBrush, 60, 120);
                    }
                }

                using (Bitmap extracted = WatermarkEnhancer.Enhance(src, ForensicFilterMode.ChromaStegoExtract))
                {
                    // Check that the hidden watermark band (y=120..145) produces noticeably darker pixels
                    // than clean background (y=190..210)
                    int minWmPixel = 255;
                    for (int y = 122; y < 145; y++)
                    {
                        for (int x = 60; x < 240; x++)
                        {
                            int val = extracted.GetPixel(x, y).R;
                            if (val < minWmPixel) minWmPixel = val;
                        }
                    }

                    double bgAvg = 0;
                    int bgCount = 0;
                    for (int y = 190; y < 210; y++)
                    {
                        for (int x = 60; x < 240; x++)
                        {
                            bgAvg += extracted.GetPixel(x, y).R;
                            bgCount++;
                        }
                    }
                    bgAvg /= bgCount;

                    Assert.True(bgAvg - minWmPixel >= 60, $"Expected strong recovered watermark contrast, got bg={bgAvg:F1}, wmMin={minWmPixel}");
                }
            }
        }

        private static int ClampByte(int v) => v < 0 ? 0 : (v > 255 ? 255 : v);
    }
}
```

---

[返回总览 README](../README.md)
