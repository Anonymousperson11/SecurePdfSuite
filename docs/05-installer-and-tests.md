# 05 — 自解压安装器 (`SecurePdf.Installer`) 与单元测试 (`SecurePdf.Core.Tests`)

包含打包 `install_creator.exe` / `install_reader.exe` 的 WinForms 安装器源码（通过条件编译 `INSTALLER_MODE_CREATOR` 区分双端）以及全部 xUnit 自动化测试。

> [!NOTE]
> 安装器采用 C# 5 语法编写，以便直接使用 Windows 系统自带的 `.NET Framework 4.x csc.exe` 编译并将压缩负载内嵌为托管资源，实现零外部运行库安装。

## 目录

1. [`src/SecurePdf.Installer/Program.cs`](#srcsecurepdfinstallerprogramcs) — 支持 GUI 向导、桌面快捷方式生成、`.spdf` 文件关联（仅阅读端）以及 `--silent --dir` 静默升级的自解压安装程序

2. [`tests/SecurePdf.Core.Tests/SecurePdf.Core.Tests.csproj`](#testssecurepdfcoretestssecurepdfcoretestscsproj) — xUnit 测试项目配置（target `net48`）

3. [`tests/SecurePdf.Core.Tests/CryptoAndContextTests.cs`](#testssecurepdfcoretestscryptoandcontexttestscs) — 验证验证码随机性与设备指纹/IP 采集

4. [`tests/SecurePdf.Core.Tests/SpdfContainerTests.cs`](#testssecurepdfcoretestsspdfcontainertestscs) — 验证 `.spdf` 创建/解包往返、错误验证码拦截与密文篡改拒绝

5. [`tests/SecurePdf.Core.Tests/LocalQuotaStoreTests.cs`](#testssecurepdfcoretestslocalquotastoretestscs) — 验证打印配额扣减/回滚、双存储单边删除恢复、系统时间回拨检测与小时级过期

6. [`tests/SecurePdf.Core.Tests/PrinterSecurityGuardTests.cs`](#testssecurepdfcoretestsprintersecurityguardtestscs) — 验证拦截 Microsoft Print to PDF / XPS / WPS PDF / Adobe PDF / Fax 并放行物理打印机

7. [`tests/SecurePdf.Core.Tests/WatermarkEnhancerTests.cs`](#testssecurepdfcoretestswatermarkenhancertestscs) — 验证在模拟彩虹摩尔纹与亮度渐变干扰下 `ChromaStegoExtract` 对蓝通道微偏水印的提取信噪比


---

## `src/SecurePdf.Installer/Program.cs`


**模块说明**：支持 GUI 向导、桌面快捷方式生成、`.spdf` 文件关联（仅阅读端）以及 `--silent --dir` 静默升级的自解压安装程序（共 `469` 行）


```csharp
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace SecurePdf.Installer
{
    internal static class AppProfile
    {
#if INSTALLER_MODE_CREATOR
        public const bool IsCreator = true;
        public const string WindowTitle = "SecurePdf 创建端安装程序（发送方）";
        public const string BannerTitle = "SecurePdf 创建端安装向导（发送方）";
        public const string BannerSub = "用于加密生成 .spdf、自动生码与水印取证 · 释放后约 6.7 MB";
        public const string DefaultFolderName = "SecurePdfCreator";
        public const string TargetExeName = "SecurePdfCreator.exe";
        public const string ShortcutName = "SecurePdf 创建端";
        public const string ShortcutDesc = "创建加密 PDF 并自动生成验证码";
        public const string DesktopCheckText = "在桌面创建快捷方式：SecurePdf 创建端";
        public const string LaunchBtnText = "立即打开创建端";
        public const string UninstallKeyName = "SecurePdfCreator";
        public const string UninstallDisplayName = "SecurePdf 创建端 (发送方)";
        public const string UninstallBatName = "uninstall_creator.bat";
#else
        public const bool IsCreator = false;
        public const string WindowTitle = "SecurePdf 阅读端安装程序（接收方）";
        public const string BannerTitle = "SecurePdf 阅读端安装向导（接收方）";
        public const string BannerSub = "输入验证码安全阅读、防截屏防翻拍与受控物理打印 · 释放后约 22 MB";
        public const string DefaultFolderName = "SecurePdfReader";
        public const string TargetExeName = "SecurePdfReader.exe";
        public const string ShortcutName = "SecurePdf 阅读端";
        public const string ShortcutDesc = "受控阅读加密 .spdf 文档";
        public const string DesktopCheckText = "在桌面创建快捷方式：SecurePdf 阅读端";
        public const string LaunchBtnText = "立即打开阅读端";
        public const string UninstallKeyName = "SecurePdfReader";
        public const string UninstallDisplayName = "SecurePdf 阅读端 (接收方)";
        public const string UninstallBatName = "uninstall_reader.bat";
#endif
    }

    internal static class Program
    {
        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [STAThread]
        private static int Main(string[] args)
        {
            try { SetProcessDPIAware(); } catch { }

            if (args != null && args.Length > 0 && Array.Exists(args, delegate(string a) { return string.Equals(a, "--silent", StringComparison.OrdinalIgnoreCase); }))
            {
                return RunSilentInstall(args);
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new InstallerForm());
            return 0;
        }

        private static int RunSilentInstall(string[] args)
        {
            string targetDir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                AppProfile.DefaultFolderName);
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

            InstallEngine.Install(targetDir, desktopShortcut, startMenu, assoc, null);
            return 0;
        }
    }

    internal static class InstallEngine
    {
        public const string EmbeddedResourceName = "SecurePdfPayload.zip";

        public static void Install(
            string installDir,
            bool createDesktopShortcut,
            bool createStartMenuShortcut,
            bool associateSpdf,
            Action<int, string> progressCallback)
        {
            if (string.IsNullOrEmpty(installDir)) throw new ArgumentException("安装目录不能为空。");

            Report(progressCallback, 5, "正在准备安装目录...");
            Directory.CreateDirectory(installDir);

            string targetExe = Path.Combine(installDir, AppProfile.TargetExeName);
            StopProcessIfRunning(targetExe);

            Assembly asm = Assembly.GetExecutingAssembly();
            using (Stream resourceStream = asm.GetManifestResourceStream(EmbeddedResourceName))
            {
                if (resourceStream == null) throw new InvalidOperationException("安装包内嵌资源缺失，无法继续安装。");

                using (ZipArchive archive = new ZipArchive(resourceStream, ZipArchiveMode.Read))
                {
                    int total = archive.Entries.Count;
                    int index = 0;
                    foreach (ZipArchiveEntry entry in archive.Entries)
                    {
                        index++;
                        string fullDest = Path.GetFullPath(Path.Combine(installDir, entry.FullName));
                        string fullRoot = Path.GetFullPath(installDir);
                        if (!fullRoot.EndsWith(Path.DirectorySeparatorChar.ToString())) fullRoot += Path.DirectorySeparatorChar;
                        if (!fullDest.StartsWith(fullRoot, StringComparison.OrdinalIgnoreCase))
                        {
                            throw new IOException("检测到非法压缩路径: " + entry.FullName);
                        }

                        if (string.IsNullOrEmpty(entry.Name))
                        {
                            Directory.CreateDirectory(fullDest);
                        }
                        else
                        {
                            string parent = Path.GetDirectoryName(fullDest);
                            if (!string.IsNullOrEmpty(parent)) Directory.CreateDirectory(parent);

                            using (Stream entryStream = entry.Open())
                            using (FileStream fs = new FileStream(fullDest, FileMode.Create, FileAccess.Write, FileShare.None))
                            {
                                entryStream.CopyTo(fs);
                            }
                        }

                        int pct = 10 + (int)((index * 70.0) / Math.Max(1, total));
                        Report(progressCallback, pct, "正在释放: " + entry.FullName);
                    }
                }
            }

            Report(progressCallback, 84, "正在配置快捷方式...");
            string desktopDir = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
            if (createDesktopShortcut && File.Exists(targetExe))
            {
                CreateShortcut(Path.Combine(desktopDir, AppProfile.ShortcutName + ".lnk"), targetExe, installDir, AppProfile.ShortcutDesc);
            }

            if (createStartMenuShortcut && File.Exists(targetExe))
            {
                string startDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "SecurePdfSuite");
                Directory.CreateDirectory(startDir);
                CreateShortcut(Path.Combine(startDir, AppProfile.ShortcutName + ".lnk"), targetExe, installDir, AppProfile.ShortcutDesc);
            }

            if (!AppProfile.IsCreator && associateSpdf && File.Exists(targetExe))
            {
                Report(progressCallback, 92, "正在关联 .spdf 文件格式...");
                RegisterFileAssociation(targetExe);
            }

            Report(progressCallback, 96, "正在写入卸载信息...");
            CreateUninstallerAndRegistry(installDir);
            Report(progressCallback, 100, "安装完成！");
        }

        private static void Report(Action<int, string> cb, int percent, string message)
        {
            if (cb != null) cb(percent, message);
        }

        private static void StopProcessIfRunning(string exePath)
        {
            string name = Path.GetFileNameWithoutExtension(exePath);
            try
            {
                foreach (Process p in Process.GetProcessesByName(name))
                {
                    try
                    {
                        if (p.MainModule != null && string.Equals(p.MainModule.FileName, exePath, StringComparison.OrdinalIgnoreCase))
                        {
                            p.Kill();
                            p.WaitForExit(3000);
                        }
                    }
                    catch { }
                }
            }
            catch { }
        }

        private static void CreateShortcut(string lnkPath, string targetExe, string workingDir, string description)
        {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return;

            object shell = Activator.CreateInstance(shellType);
            try
            {
                object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { lnkPath });
                Type scType = shortcut.GetType();
                scType.InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe });
                scType.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, shortcut, new object[] { workingDir });
                scType.InvokeMember("Description", BindingFlags.SetProperty, null, shortcut, new object[] { description });
                scType.InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe + ",0" });
                scType.InvokeMember("Save", BindingFlags.InvokeMethod, null, shortcut, null);
                Marshal.ReleaseComObject(shortcut);
            }
            finally
            {
                Marshal.ReleaseComObject(shell);
            }
        }

        private static void RegisterFileAssociation(string readerExe)
        {
            const string progId = "SecurePdfSuite.SpdfFile";
            using (RegistryKey extKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\.spdf"))
            {
                if (extKey != null) extKey.SetValue(string.Empty, progId);
            }
            using (RegistryKey progKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId))
            {
                if (progKey != null) progKey.SetValue(string.Empty, "SecurePdf Protected Document");
            }
            using (RegistryKey cmdKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId + @"\shell\open\command"))
            {
                if (cmdKey != null) cmdKey.SetValue(string.Empty, "\"" + readerExe + "\" \"%1\"");
            }
        }

        private static void CreateUninstallerAndRegistry(string installDir)
        {
            string uninstallBat = Path.Combine(installDir, AppProfile.UninstallBatName);
            string exeName = AppProfile.TargetExeName;
            string shortcutLnk = AppProfile.ShortcutName + ".lnk";

            string assocCleanup = AppProfile.IsCreator
                ? string.Empty
                : "reg delete \"HKCU\\Software\\Classes\\.spdf\" /f >nul 2>&1" + Environment.NewLine +
                  "reg delete \"HKCU\\Software\\Classes\\SecurePdfSuite.SpdfFile\" /f >nul 2>&1" + Environment.NewLine;

            string content =
                "@echo off" + Environment.NewLine +
                "chcp 65001 >nul" + Environment.NewLine +
                "taskkill /f /im " + exeName + " >nul 2>&1" + Environment.NewLine +
                "del /f /q \"%USERPROFILE%\\Desktop\\" + shortcutLnk + "\" >nul 2>&1" + Environment.NewLine +
                "del /f /q \"%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\SecurePdfSuite\\" + shortcutLnk + "\" >nul 2>&1" + Environment.NewLine +
                assocCleanup +
                "reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\" + AppProfile.UninstallKeyName + "\" /f >nul 2>&1" + Environment.NewLine +
                "echo 已完成卸载，正在清理目录..." + Environment.NewLine +
                "start /b \"\" cmd /c \"timeout /t 2 /nobreak >nul & rmdir /s /q \"\"%~dp0\"\"\"" + Environment.NewLine +
                "exit /b 0";

            File.WriteAllText(uninstallBat, content, new UTF8Encoding(false));

            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\" + AppProfile.UninstallKeyName))
            {
                if (key != null)
                {
                    key.SetValue("DisplayName", AppProfile.UninstallDisplayName);
                    key.SetValue("DisplayVersion", "1.0.1");
                    key.SetValue("Publisher", "SecurePdfSuite");
                    key.SetValue("InstallLocation", installDir);
                    key.SetValue("UninstallString", "\"" + uninstallBat + "\"");
                    key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                    key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                }
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

        public InstallerForm()
        {
            Text = AppProfile.WindowTitle;
            ClientSize = new Size(600, 410);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(15, 23, 42);
            ForeColor = Color.FromArgb(248, 250, 252);
            Font = new Font("Microsoft YaHei UI", 9.5f);

            Panel header = new Panel { Dock = DockStyle.Top, Height = 82, BackColor = Color.FromArgb(30, 41, 59), Padding = new Padding(22, 16, 22, 12) };
            header.Controls.Add(new Label { Text = AppProfile.BannerTitle, Font = new Font("Microsoft YaHei UI", 14f, FontStyle.Bold), ForeColor = Color.FromArgb(248, 250, 252), AutoSize = true, Location = new Point(22, 14) });
            header.Controls.Add(new Label { Text = AppProfile.BannerSub, Font = new Font("Microsoft YaHei UI", 9f), ForeColor = Color.FromArgb(148, 163, 184), AutoSize = true, Location = new Point(24, 46) });
            Controls.Add(header);

            Controls.Add(new Label { Text = "安装目标路径:", Location = new Point(24, 102), AutoSize = true, ForeColor = Color.FromArgb(203, 213, 225) });

            _pathBox = new TextBox
            {
                Location = new Point(24, 128),
                Size = new Size(450, 28),
                BackColor = Color.FromArgb(30, 41, 59),
                ForeColor = Color.FromArgb(248, 250, 252),
                BorderStyle = BorderStyle.FixedSingle,
                Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), AppProfile.DefaultFolderName)
            };
            Controls.Add(_pathBox);

            _browseBtn = new Button { Text = "浏览...", Location = new Point(484, 126), Size = new Size(92, 30), FlatStyle = FlatStyle.Flat, BackColor = Color.FromArgb(51, 65, 85), ForeColor = Color.White };
            _browseBtn.FlatAppearance.BorderSize = 0;
            _browseBtn.Click += BrowseBtn_Click;
            Controls.Add(_browseBtn);

            _chkDesktop = MakeCheck(AppProfile.DesktopCheckText, 178, true);
            _chkStartMenu = MakeCheck("在“开始”菜单中创建快捷方式", 210, true);
            _chkAssoc = MakeCheck("关联 .spdf 文件后缀（双击加密文件直接打开阅读）", 242, true);
            Controls.Add(_chkDesktop);
            Controls.Add(_chkStartMenu);
            if (!AppProfile.IsCreator) Controls.Add(_chkAssoc);

            _statusLabel = new Label { Text = "就绪，点击“立即安装”开始释放程序。", Location = new Point(24, 286), Size = new Size(552, 22), ForeColor = Color.FromArgb(56, 189, 248) };
            Controls.Add(_statusLabel);

            _progressBar = new ProgressBar { Location = new Point(24, 312), Size = new Size(552, 18), Minimum = 0, Maximum = 100 };
            Controls.Add(_progressBar);

            _cancelBtn = new Button { Text = "取消", Location = new Point(340, 352), Size = new Size(100, 36), FlatStyle = FlatStyle.Flat, BackColor = Color.FromArgb(51, 65, 85), ForeColor = Color.White };
            _cancelBtn.FlatAppearance.BorderSize = 0;
            _cancelBtn.Click += delegate { Close(); };
            Controls.Add(_cancelBtn);

            _installBtn = new Button { Text = "立即安装", Location = new Point(452, 352), Size = new Size(124, 36), FlatStyle = FlatStyle.Flat, BackColor = Color.FromArgb(2, 132, 199), ForeColor = Color.White, Font = new Font("Microsoft YaHei UI", 9.5f, FontStyle.Bold) };
            _installBtn.FlatAppearance.BorderSize = 0;
            _installBtn.Click += InstallBtn_Click;
            Controls.Add(_installBtn);

            _launchBtn = new Button { Text = AppProfile.LaunchBtnText, Location = new Point(290, 352), Size = new Size(150, 36), FlatStyle = FlatStyle.Flat, BackColor = Color.FromArgb(5, 150, 105), ForeColor = Color.White, Visible = false };
            _launchBtn.FlatAppearance.BorderSize = 0;
            _launchBtn.Click += delegate { LaunchApp(AppProfile.TargetExeName); Close(); };
            Controls.Add(_launchBtn);
        }

        private CheckBox MakeCheck(string text, int top, bool isChecked)
        {
            return new CheckBox { Text = text, Location = new Point(24, top), Size = new Size(552, 24), Checked = isChecked, ForeColor = Color.FromArgb(226, 232, 240) };
        }

        private void BrowseBtn_Click(object sender, EventArgs e)
        {
            using (FolderBrowserDialog dlg = new FolderBrowserDialog())
            {
                dlg.Description = "选择安装目标文件夹";
                dlg.SelectedPath = _pathBox.Text;
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    string selected = dlg.SelectedPath;
                    if (!selected.EndsWith(AppProfile.DefaultFolderName, StringComparison.OrdinalIgnoreCase))
                    {
                        selected = Path.Combine(selected, AppProfile.DefaultFolderName);
                    }
                    _pathBox.Text = selected;
                }
            }
        }

        private void InstallBtn_Click(object sender, EventArgs e)
        {
            string targetDir = _pathBox.Text.Trim();
            if (string.IsNullOrEmpty(targetDir))
            {
                MessageBox.Show(this, "请先选择有效的安装路径。", "提示", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            bool desktop = _chkDesktop.Checked;
            bool startMenu = _chkStartMenu.Checked;
            bool assoc = !AppProfile.IsCreator && _chkAssoc.Checked;

            _installBtn.Enabled = false;
            _browseBtn.Enabled = false;
            _pathBox.Enabled = false;
            _cancelBtn.Enabled = false;

            Thread worker = new Thread(delegate()
            {
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

                    BeginInvoke(new Action(delegate()
                    {
                        _installedDir = targetDir;
                        _statusLabel.ForeColor = Color.FromArgb(52, 211, 153);
                        _statusLabel.Text = "安装成功！已安装至: " + targetDir;
                        _installBtn.Visible = false;
                        _launchBtn.Visible = true;
                        _cancelBtn.Text = "完成";
                        _cancelBtn.Location = new Point(452, 352);
                        _cancelBtn.Size = new Size(124, 36);
                        _cancelBtn.Enabled = true;
                    }));
                }
                catch (Exception ex)
                {
                    BeginInvoke(new Action(delegate()
                    {
                        _statusLabel.ForeColor = Color.FromArgb(248, 113, 113);
                        _statusLabel.Text = "安装失败: " + ex.Message;
                        _installBtn.Enabled = true;
                        _browseBtn.Enabled = true;
                        _pathBox.Enabled = true;
                        _cancelBtn.Enabled = true;
                        MessageBox.Show(this, "安装过程出错：\n" + ex.Message, "安装失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    }));
                }
            });
            worker.IsBackground = true;
            worker.Start();
        }

        private void LaunchApp(string exeName)
        {
            if (string.IsNullOrEmpty(_installedDir)) return;
            string exe = Path.Combine(_installedDir, exeName);
            if (File.Exists(exe))
            {
                Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = _installedDir, UseShellExecute = true });
            }
        }
    }
}
```


## `tests/SecurePdf.Core.Tests/SecurePdf.Core.Tests.csproj`


**模块说明**：xUnit 测试项目配置（target `net48`）（共 `21` 行）


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


**模块说明**：验证验证码随机性与设备指纹/IP 采集（共 `29` 行）


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


**模块说明**：验证 `.spdf` 创建/解包往返、错误验证码拦截与密文篡改拒绝（共 `85` 行）


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


**模块说明**：验证打印配额扣减/回滚、双存储单边删除恢复、系统时间回拨检测与小时级过期（共 `91` 行）


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


**模块说明**：验证拦截 Microsoft Print to PDF / XPS / WPS PDF / Adobe PDF / Fax 并放行物理打印机（共 `57` 行）


```csharp
using SecurePdf.Core.Printing;
using Xunit;

namespace SecurePdf.Core.Tests
{
    public sealed class PrinterSecurityGuardTests
    {
        [Theory]
        [InlineData("Microsoft Print to PDF", "PORTPROMPT:", "Microsoft Print To PDF", "Local Port", 0u)]
        [InlineData("Microsoft XPS Document Writer", "PORTPROMPT:", "Microsoft XPS Document Writer v4", "", 0u)]
        [InlineData("WPS PDF", "FILE:", "Kingsoft PDF Driver", "", 0u)]
        [InlineData("Adobe PDF", "Documents\\*.pdf", "Adobe PDF Converter", "Adobe PDF Port Monitor", 0u)]
        [InlineData("Fax", "SHRFAX:", "Microsoft Shared Fax Driver", "", 0x00004000u)]
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
        [InlineData("Canon Office Printer", "192.168.1.50", "Canon Generic Plus PCL6", "Standard TCP/IP Port")]
        [InlineData("Brother HL-L2350DW", "WSD-12345", "Brother Laser Type1 Class Driver", "WSD Port")]
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
            Assert.True(result.IsAllowedPhysicalPrinter);
        }
    }
}
```


## `tests/SecurePdf.Core.Tests/WatermarkEnhancerTests.cs`


**模块说明**：验证在模拟彩虹摩尔纹与亮度渐变干扰下 `ChromaStegoExtract` 对蓝通道微偏水印的提取信噪比（共 `81` 行）


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
