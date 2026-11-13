# 04 — 接收端阅读器 (`SecurePdf.Reader`)

接收端通过验证码解密 `.spdf`，在防截屏保护窗口中渲染内容，并叠加防手机翻拍动态摩尔纹与实时隐形水印。

> [!NOTE]
> `AntiCameraOverlay` 采用 `18.0` 加粗字号与低饱和蓝色通道微偏移墨色（主色 `ARGB(18, 0, 70, 245)`）渲染实时时间/IP/设备码，人眼几乎无感但可通过创建端蓝通道差值算法高对比提取。

## 目录

1. [`src/SecurePdf.Reader/SecurePdf.Reader.csproj`](#srcsecurepdfreadersecurepdfreadercsproj) — 阅读端 WPF 项目配置，引用 `PdfiumViewer.Native.x86_64.v8-xfa` 提供 x64 `pdfium.dll`

2. [`src/SecurePdf.Reader/App.xaml`](#srcsecurepdfreaderappxaml) — 阅读端应用样式与资源

3. [`src/SecurePdf.Reader/App.xaml.cs`](#srcsecurepdfreaderappxamlcs) — 支持双击 `.spdf` 命令行参数直传至 `UnlockWindow`

4. [`src/SecurePdf.Reader/Security/WindowCaptureProtector.cs`](#srcsecurepdfreadersecuritywindowcaptureprotectorcs) — 调用 `user32.dll` `SetWindowDisplayAffinity`（支持 `WDA_EXCLUDEFROMCAPTURE` 及旧版 `WDA_MONITOR` 回退）

5. [`src/SecurePdf.Reader/Watermark/AntiCameraOverlay.cs`](#srcsecurepdfreaderwatermarkanticameraoverlaycs) — 动态高频摩尔纹叠加层 + 蓝通道微偏移隐形水印 + 60 秒实时时间/IP 自动刷新定时器

6. [`src/SecurePdf.Reader/Views/UnlockWindow.xaml`](#srcsecurepdfreaderviewsunlockwindowxaml) — 验证码输入与文档选择窗口界面

7. [`src/SecurePdf.Reader/Views/UnlockWindow.xaml.cs`](#srcsecurepdfreaderviewsunlockwindowxamlcs) — 异步校验验证码、检查小时级过期与篡改状态并带渐进式错误退避延迟

8. [`src/SecurePdf.Reader/Views/ReaderWindow.xaml`](#srcsecurepdfreaderviewsreaderwindowxaml) — 受控阅读主界面（含防截屏标识、缩放/翻页控件、失焦隐私幕布与底部配额/有效期状态栏）

9. [`src/SecurePdf.Reader/Views/ReaderWindow.xaml.cs`](#srcsecurepdfreaderviewsreaderwindowxamlcs) — 内存渲染、快捷键控制（`Ctrl+P` 安全打印）、实时展示截止时间并阻止直接系统打印

10. [`src/SecurePdf.Reader/Views/SecurePrintDialog.xaml`](#srcsecurepdfreaderviewssecureprintdialogxaml) — 物理打印机选择与页码范围配置弹窗

11. [`src/SecurePdf.Reader/Views/SecurePrintDialog.xaml.cs`](#srcsecurepdfreaderviewssecureprintdialogxamlcs) — 展示过滤后的物理打印机与剩余配额/有效期，并原子化预占/回滚打印配额


---

## `src/SecurePdf.Reader/SecurePdf.Reader.csproj`


**模块说明**：阅读端 WPF 项目配置，引用 `PdfiumViewer.Native.x86_64.v8-xfa` 提供 x64 `pdfium.dll`（共 `21` 行）


```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>WinExe</OutputType>
    <TargetFramework>net48</TargetFramework>
    <UseWPF>true</UseWPF>
    <AssemblyName>SecurePdfReader</AssemblyName>
    <RootNamespace>SecurePdf.Reader</RootNamespace>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net48" Version="1.0.3" PrivateAssets="All" />
  </ItemGroup>

  <ItemGroup>
    <ProjectReference Include="..\SecurePdf.Core\SecurePdf.Core.csproj" />
  </ItemGroup>

  <ItemGroup>
    <Reference Include="System.Drawing" />
  </ItemGroup>
</Project>
```


## `src/SecurePdf.Reader/App.xaml`


**模块说明**：阅读端应用样式与资源（共 `15` 行）


```xml
<Application x:Class="SecurePdf.Reader.App"
             xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
             ShutdownMode="OnLastWindowClose">
    <Application.Resources>
        <Style TargetType="Button">
            <Setter Property="Padding" Value="14,7" />
            <Setter Property="Cursor" Value="Hand" />
            <Setter Property="FontFamily" Value="Microsoft YaHei UI" />
        </Style>
        <Style TargetType="TextBlock">
            <Setter Property="FontFamily" Value="Microsoft YaHei UI" />
        </Style>
    </Application.Resources>
</Application>
```


## `src/SecurePdf.Reader/App.xaml.cs`


**模块说明**：支持双击 `.spdf` 命令行参数直传至 `UnlockWindow`（共 `51` 行）


```csharp
using System;
using System.IO;
using System.Reflection;
using System.Windows;
using Microsoft.Win32;
using SecurePdf.Reader.Views;

namespace SecurePdf.Reader
{
    public partial class App : Application
    {
        protected override void OnStartup(StartupEventArgs e)
        {
            base.OnStartup(e);
            EnsureSpdfFileAssociation();

            string? initialPath = null;
            if (e.Args != null && e.Args.Length > 0 && File.Exists(e.Args[0]))
            {
                initialPath = e.Args[0];
            }

            var unlockWindow = new UnlockWindow(initialPath);
            unlockWindow.Show();
        }

        private static void EnsureSpdfFileAssociation()
        {
            try
            {
                string exePath = Assembly.GetExecutingAssembly().Location;
                if (string.IsNullOrEmpty(exePath) || !File.Exists(exePath)) return;

                const string progId = "SecurePdfSuite.SpdfFile";
                using (RegistryKey? extKey = Registry.CurrentUser.CreateSubKey(@"Software\Classes\.spdf"))
                {
                    extKey?.SetValue(string.Empty, progId);
                }
                using (RegistryKey? progKey = Registry.CurrentUser.CreateSubKey($@"Software\Classes\{progId}"))
                {
                    progKey?.SetValue(string.Empty, "SecurePdf Protected Document");
                }
                using (RegistryKey? cmdKey = Registry.CurrentUser.CreateSubKey($@"Software\Classes\{progId}\shell\open\command"))
                {
                    cmdKey?.SetValue(string.Empty, $"\"{exePath}\" \"%1\"");
                }
            }
            catch { }
        }
    }
}
```


## `src/SecurePdf.Reader/Security/WindowCaptureProtector.cs`


**模块说明**：调用 `user32.dll` `SetWindowDisplayAffinity`（支持 `WDA_EXCLUDEFROMCAPTURE` 及旧版 `WDA_MONITOR` 回退）（共 `53` 行）


```csharp
using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;

namespace SecurePdf.Reader.Security
{
    public static class WindowCaptureProtector
    {
        private const uint WDA_MONITOR = 0x00000001;
        private const uint WDA_EXCLUDEFROMCAPTURE = 0x00000011;

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool SetWindowDisplayAffinity(IntPtr hwnd, uint dwAffinity);

        public static bool ApplyProtection(Window window)
        {
            if (window == null) return false;

            bool applied = false;
            window.SourceInitialized += (_, __) =>
            {
                IntPtr hwnd = new WindowInteropHelper(window).Handle;
                if (hwnd != IntPtr.Zero)
                {
                    applied = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
                    if (!applied) applied = SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
                }
            };

            window.PreviewKeyDown += (_, e) =>
            {
                Key key = e.Key == Key.System ? e.SystemKey : e.Key;
                if (key == Key.Snapshot || key == Key.PrintScreen)
                {
                    e.Handled = true;
                    return;
                }

                if ((Keyboard.Modifiers & ModifierKeys.Control) == ModifierKeys.Control)
                {
                    if (key == Key.C || key == Key.X || key == Key.S || key == Key.A || key == Key.E)
                    {
                        e.Handled = true;
                    }
                }
            };

            return applied;
        }
    }
}
```


## `src/SecurePdf.Reader/Watermark/AntiCameraOverlay.cs`


**模块说明**：动态高频摩尔纹叠加层 + 蓝通道微偏移隐形水印 + 60 秒实时时间/IP 自动刷新定时器（共 `286` 行）


```csharp
using System;
using System.Globalization;
using System.Windows;
using System.Windows.Media;
using System.Windows.Threading;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;

namespace SecurePdf.Reader.Watermark
{
    /// <summary>
    /// Renders:
    /// 1) High-frequency phase-shifting Moire interference grid against smartphone cameras;
    /// 2) Chromatic low-luminance hidden forensic watermark (real-time clock, IP, device code)
    ///    tuned to survive phone camera blur/JPEG compression and pop cleanly in ChromaStegoExtract;
    /// 3) Optional standard semi-transparent visible watermark.
    /// </summary>
    public sealed class AntiCameraOverlay : FrameworkElement
    {
        private readonly DispatcherTimer _clockAndPhaseTimer;
        private int _phaseStep;
        private DateTime _currentLocalTime = DateTime.Now;

        private MoireStrength _moireStrength = MoireStrength.Medium;
        private bool _enableHiddenWatermark = true;
        private bool _enableVisibleWatermark;
        private RuntimeTraceContext _traceContext = new RuntimeTraceContext();
        private string _watermarkNote = string.Empty;

        public AntiCameraOverlay()
        {
            IsHitTestVisible = false;
            SnapsToDevicePixels = true;

            _clockAndPhaseTimer = new DispatcherTimer(DispatcherPriority.Render)
            {
                Interval = TimeSpan.FromMilliseconds(350)
            };
            _clockAndPhaseTimer.Tick += (_, __) =>
            {
                _phaseStep = (_phaseStep + 1) % 6;
                DateTime now = DateTime.Now;
                if (now.Second != _currentLocalTime.Second || _moireStrength != MoireStrength.Off)
                {
                    _currentLocalTime = now;
                    InvalidateVisual();
                }
            };
        }

        public void Configure(
            MoireStrength moireStrength,
            bool enableHiddenWatermark,
            bool enableVisibleWatermark,
            RuntimeTraceContext traceContext,
            string watermarkNote)
        {
            _moireStrength = moireStrength;
            _enableHiddenWatermark = enableHiddenWatermark;
            _enableVisibleWatermark = enableVisibleWatermark;
            _traceContext = traceContext ?? new RuntimeTraceContext();
            _watermarkNote = watermarkNote ?? string.Empty;
            InvalidateVisual();
        }

        public void SetMoireStrength(MoireStrength strength)
        {
            _moireStrength = strength;
            InvalidateVisual();
        }

        public void Start() => _clockAndPhaseTimer.Start();
        public void Stop() => _clockAndPhaseTimer.Stop();

        protected override void OnRender(DrawingContext dc)
        {
            base.OnRender(dc);
            double w = ActualWidth;
            double h = ActualHeight;
            if (w <= 4 || h <= 4)
            {
                return;
            }

            // 1. High-frequency Moire optical interference grid
            if (_moireStrength != MoireStrength.Off)
            {
                DrawMoireInterference(dc, w, h);
            }

            // 2. Hidden chromatic forensic watermark (time + IP + device code)
            if (_enableHiddenWatermark)
            {
                DrawForensicWatermarks(dc, w, h, visibleMode: false);
                DrawMicroDotTraceMatrix(dc, w, h);
            }

            // 3. Optional visible deterrent watermark
            if (_enableVisibleWatermark)
            {
                DrawForensicWatermarks(dc, w, h, visibleMode: true);
            }
        }

        private void DrawMoireInterference(DrawingContext dc, double width, double height)
        {
            byte alphaDark;
            byte alphaLight;
            double tileSize;

            switch (_moireStrength)
            {
                case MoireStrength.Low:
                    alphaDark = 7;
                    alphaLight = 5;
                    tileSize = 6.0;
                    break;
                case MoireStrength.High:
                    alphaDark = 18;
                    alphaLight = 13;
                    tileSize = 4.0;
                    break;
                case MoireStrength.Medium:
                default:
                    alphaDark = 11;
                    alphaLight = 8;
                    tileSize = 5.0;
                    break;
            }

            var darkPen = new Pen(new SolidColorBrush(Color.FromArgb(alphaDark, 24, 30, 42)), 1.0);
            var lightPen = new Pen(new SolidColorBrush(Color.FromArgb(alphaLight, 248, 250, 252)), 1.0);
            darkPen.Freeze();
            lightPen.Freeze();

            var dg = new DrawingGroup();
            using (DrawingContext tileDc = dg.Open())
            {
                tileDc.DrawLine(darkPen, new Point(0, 0), new Point(tileSize, tileSize));
                tileDc.DrawLine(lightPen, new Point(tileSize / 2.0, 0), new Point(0, tileSize / 2.0));
                tileDc.DrawLine(darkPen, new Point(tileSize, 0), new Point(0, tileSize));
            }
            dg.Freeze();

            double shift = _phaseStep * 0.7;
            var brush = new DrawingBrush(dg)
            {
                TileMode = TileMode.Tile,
                Viewport = new Rect(shift, shift, tileSize, tileSize),
                ViewportUnits = BrushMappingMode.Absolute
            };
            brush.Freeze();

            dc.DrawRectangle(brush, null, new Rect(0, 0, width, height));
        }

        private void DrawForensicWatermarks(DrawingContext dc, double width, double height, bool visibleMode)
        {
            string timeText = _currentLocalTime.ToString("yy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture);
            string label = string.IsNullOrWhiteSpace(_watermarkNote)
                ? $"IP:{_traceContext.LocalIpAddress} | {timeText} | {_traceContext.MachineCode}"
                : $"{_watermarkNote} | IP:{_traceContext.LocalIpAddress} | {timeText} | {_traceContext.MachineCode}";

            var typeface = new Typeface(
                new FontFamily("Microsoft YaHei UI"),
                FontStyles.Normal,
                FontWeights.Bold,
                FontStretches.Normal);

            // Hidden mode uses bold 18px blue-shifted chromatic ink (minimal luminance impact to human eye,
            // strong (B - R) signal surviving smartphone camera Moire & JPEG compression).
            Brush primaryBrush = visibleMode
                ? new SolidColorBrush(Color.FromArgb(34, 15, 23, 42))
                : new SolidColorBrush(Color.FromArgb(18, 0, 70, 245));
            Brush shadowBrush = visibleMode
                ? new SolidColorBrush(Color.FromArgb(18, 248, 250, 252))
                : new SolidColorBrush(Color.FromArgb(10, 240, 246, 255));
            primaryBrush.Freeze();
            shadowBrush.Freeze();

            double fontSize = visibleMode ? 16.0 : 18.0;
            double dpi = VisualTreeHelper.GetDpi(this).PixelsPerDip;

            var primaryText = new FormattedText(
                label,
                CultureInfo.CurrentUICulture,
                FlowDirection.LeftToRight,
                typeface,
                fontSize,
                primaryBrush,
                dpi);

            var shadowText = new FormattedText(
                label,
                CultureInfo.CurrentUICulture,
                FlowDirection.LeftToRight,
                typeface,
                fontSize,
                shadowBrush,
                dpi);

            double stepX = Math.Max(310, primaryText.Width + (visibleMode ? 110 : 75));
            double stepY = visibleMode ? 170 : 108;

            dc.PushTransform(new TranslateTransform(width / 2.0, height / 2.0));
            dc.PushTransform(new RotateTransform(-26.0));

            double diag = Math.Sqrt(width * width + height * height);
            for (double y = -diag; y < diag; y += stepY)
            {
                double rowOffset = ((int)Math.Round(y / stepY) % 2 == 0) ? 0 : stepX * 0.45;
                for (double x = -diag + rowOffset; x < diag; x += stepX)
                {
                    dc.DrawText(shadowText, new Point(x + 1, y + 1));
                    dc.DrawText(primaryText, new Point(x, y));
                }
            }

            dc.Pop();
            dc.Pop();
        }

        private void DrawMicroDotTraceMatrix(DrawingContext dc, double width, double height)
        {
            byte[] payload = BuildCompactTraceBytes();
            var dotBrush = new SolidColorBrush(Color.FromArgb(14, 0, 40, 220));
            dotBrush.Freeze();

            const double cell = 5.0;
            const double tileW = 72.0;
            const double tileH = 56.0;

            var dg = new DrawingGroup();
            using (DrawingContext tdc = dg.Open())
            {
                tdc.DrawRectangle(dotBrush, null, new Rect(2, 2, 3, 3));
                tdc.DrawRectangle(dotBrush, null, new Rect(7, 2, 3, 3));

                for (int byteIdx = 0; byteIdx < payload.Length; byteIdx++)
                {
                    byte b = payload[byteIdx];
                    for (int bit = 0; bit < 8; bit++)
                    {
                        if (((b >> bit) & 1) == 1)
                        {
                            double dx = 4 + bit * cell;
                            double dy = 9 + byteIdx * cell;
                            tdc.DrawRectangle(dotBrush, null, new Rect(dx, dy, 2.5, 2.5));
                        }
                    }
                }
            }
            dg.Freeze();

            var brush = new DrawingBrush(dg)
            {
                TileMode = TileMode.Tile,
                Viewport = new Rect(0, 0, tileW * 3, tileH * 3),
                ViewportUnits = BrushMappingMode.Absolute
            };
            brush.Freeze();

            dc.DrawRectangle(brush, null, new Rect(0, 0, width, height));
        }

        private byte[] BuildCompactTraceBytes()
        {
            byte[] data = new byte[8];
            string[] parts = (_traceContext.LocalIpAddress ?? "127.0.0.1").Split('.');
            if (parts.Length == 4)
            {
                for (int i = 0; i < 4; i++)
                {
                    byte.TryParse(parts[i], out data[i]);
                }
            }

            uint unixSec = (uint)DateTimeOffset.UtcNow.ToUnixTimeSeconds();
            data[4] = (byte)(unixSec >> 24);
            data[5] = (byte)(unixSec >> 16);
            data[6] = (byte)(unixSec >> 8);
            data[7] = (byte)unixSec;
            return data;
        }
    }
}
```


## `src/SecurePdf.Reader/Views/UnlockWindow.xaml`


**模块说明**：验证码输入与文档选择窗口界面（共 `52` 行）


```xml
<Window x:Class="SecurePdf.Reader.Views.UnlockWindow"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="SecurePdf Reader — 安全文档验证"
        Height="430" Width="520"
        WindowStartupLocation="CenterScreen"
        ResizeMode="CanMinimize"
        Background="#0F172A"
        AllowDrop="True"
        Drop="Window_Drop"
        DragOver="Window_DragOver">
    <Border Margin="22" Padding="24" Background="#1E293B" CornerRadius="10" BorderBrush="#334155" BorderThickness="1">
        <StackPanel>
            <TextBlock Text="安全 PDF 受控阅读器" FontSize="20" FontWeight="Bold" Foreground="#F8FAFC" />
            <TextBlock Text="仅限本软件解密阅读 · 防截屏录屏 · 禁止虚拟打印机输出"
                       FontSize="12" Foreground="#94A3B8" Margin="0,4,0,18" />

            <TextBlock Text="受保护文件 (.spdf)" FontSize="12" Foreground="#CBD5E1" Margin="0,0,0,6" />
            <Grid Margin="0,0,0,14">
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="*" />
                    <ColumnDefinition Width="Auto" />
                </Grid.ColumnDefinitions>
                <TextBox x:Name="FilePathBox" IsReadOnly="True" Padding="10,8"
                         Background="#0F172A" Foreground="#E2E8F0" BorderBrush="#475569"
                         FontSize="13" VerticalContentAlignment="Center" />
                <Button Grid.Column="1" Content="浏览文件..." Margin="8,0,0,0"
                        Background="#334155" Foreground="#F8FAFC" BorderThickness="0"
                        Click="BrowseButton_Click" />
            </Grid>

            <TextBlock x:Name="DocInfoText" Text="请选择或拖入 .spdf 文档"
                       FontSize="12" Foreground="#64748B" Margin="0,0,0,14" />

            <TextBlock Text="文档验证码" FontSize="12" Foreground="#CBD5E1" Margin="0,0,0,6" />
            <TextBox x:Name="CodeBox" Padding="10,9" FontSize="15" FontFamily="Consolas"
                     CharacterCasing="Upper"
                     Background="#0F172A" Foreground="#38BDF8" BorderBrush="#475569"
                     KeyDown="CodeBox_KeyDown" />

            <TextBlock x:Name="ErrorText" Text="" Foreground="#F87171" FontSize="12"
                       TextWrapping="Wrap" Margin="0,10,0,0" MinHeight="34" />

            <Button x:Name="UnlockButton" Content="验证并解密打开" Height="40" Margin="0,12,0,0"
                    Background="#0284C7" Foreground="White" FontWeight="SemiBold" FontSize="14"
                    BorderThickness="0" Click="UnlockButton_Click" />

            <TextBlock x:Name="DeviceHintText" Text="" FontSize="11" Foreground="#64748B"
                       Margin="0,14,0,0" HorizontalAlignment="Center" />
        </StackPanel>
    </Border>
</Window>
```


## `src/SecurePdf.Reader/Views/UnlockWindow.xaml.cs`


**模块说明**：异步校验验证码、检查小时级过期与篡改状态并带渐进式错误退避延迟（共 `132` 行）


```csharp
using System;
using System.IO;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Input;
using Microsoft.Win32;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Storage;
using SecurePdf.Reader.Security;

namespace SecurePdf.Reader.Views
{
    public partial class UnlockWindow : Window
    {
        private readonly RuntimeTraceContext _traceContext;
        private int _failedAttempts;

        public UnlockWindow(string? initialSpdfPath = null)
        {
            InitializeComponent();
            WindowCaptureProtector.ApplyProtection(this);

            _traceContext = DeviceAndNetworkContext.Capture();
            DeviceHintText.Text = $"本机设备码: {_traceContext.MachineCode}  |  本机IP: {_traceContext.LocalIpAddress}";

            if (!string.IsNullOrWhiteSpace(initialSpdfPath) && File.Exists(initialSpdfPath))
            {
                LoadEnvelopePreview(initialSpdfPath!);
            }
        }

        private void BrowseButton_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new OpenFileDialog { Filter = "安全 PDF 文档 (*.spdf)|*.spdf", Title = "选择受保护的 .spdf 文件" };
            if (dlg.ShowDialog() == true) LoadEnvelopePreview(dlg.FileName);
        }

        private void Window_DragOver(object sender, DragEventArgs e)
        {
            if (e.Data.GetDataPresent(DataFormats.FileDrop))
            {
                string[]? files = e.Data.GetData(DataFormats.FileDrop) as string[];
                if (files != null && files.Length > 0 && files[0].EndsWith(".spdf", StringComparison.OrdinalIgnoreCase))
                {
                    e.Effects = DragDropEffects.Copy;
                    e.Handled = true;
                    return;
                }
            }
            e.Effects = DragDropEffects.None;
            e.Handled = true;
        }

        private void Window_Drop(object sender, DragEventArgs e)
        {
            string[]? files = e.Data.GetData(DataFormats.FileDrop) as string[];
            if (files != null && files.Length > 0 && File.Exists(files[0])) LoadEnvelopePreview(files[0]);
        }

        private void LoadEnvelopePreview(string path)
        {
            FilePathBox.Text = path;
            ErrorText.Text = string.Empty;
            try
            {
                SpdfEnvelopeInfo env = SpdfContainer.ReadEnvelope(path);
                double kb = env.FileSizeBytes / 1024.0;
                DocInfoText.Text = $"文档 ID: {env.DocId.ToString("N").Substring(0, 12).ToUpperInvariant()}  ·  大小: {kb:F1} KB";
                CodeBox.Focus();
            }
            catch (Exception ex)
            {
                DocInfoText.Text = "文件格式无效";
                ErrorText.Text = $"无法读取文件头: {ex.Message}";
            }
        }

        private void CodeBox_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Enter) UnlockButton_Click(sender, e);
        }

        private async void UnlockButton_Click(object sender, RoutedEventArgs e)
        {
            string path = FilePathBox.Text.Trim();
            string code = CodeBox.Text.Trim();

            if (string.IsNullOrEmpty(path) || !File.Exists(path))
            {
                ErrorText.Text = "请先选择有效的 .spdf 文档。";
                return;
            }
            if (string.IsNullOrEmpty(code))
            {
                ErrorText.Text = "请输入该文档对应的验证码。";
                return;
            }

            UnlockButton.IsEnabled = false;
            ErrorText.Text = "正在校验验证码并解密文档...";

            try
            {
                UnlockedSpdfDocument doc = await Task.Run(() => SpdfContainer.Unlock(path, code));
                var quotaStore = new LocalQuotaStore(_traceContext.MachineHash);
                QuotaAccessResult access = quotaStore.LoadAndVerify(doc.Policy.DocId, doc.Policy.MaxPrints, doc.Policy.ExpiresAtUtc);

                if (!access.IsAllowed)
                {
                    doc.Dispose();
                    ErrorText.Text = access.BlockReason;
                    return;
                }

                var readerWindow = new ReaderWindow(doc, quotaStore, _traceContext, access.Snapshot);
                readerWindow.Show();
                Close();
            }
            catch (Exception ex)
            {
                _failedAttempts++;
                ErrorText.Text = $"验证码错误或文档已损坏：{ex.Message}";
                await Task.Delay(Math.Min(4000, _failedAttempts * 500));
            }
            finally
            {
                UnlockButton.IsEnabled = true;
            }
        }
    }
}
```


## `src/SecurePdf.Reader/Views/ReaderWindow.xaml`


**模块说明**：受控阅读主界面（含防截屏标识、缩放/翻页控件、失焦隐私幕布与底部配额/有效期状态栏）（共 `94` 行）


```xml
<Window x:Class="SecurePdf.Reader.Views.ReaderWindow"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        xmlns:wm="clr-namespace:SecurePdf.Reader.Watermark"
        Title="SecurePdf Reader — 受控阅读模式"
        Height="860" Width="1120"
        MinHeight="600" MinWidth="820"
        WindowStartupLocation="CenterScreen"
        Background="#0F172A">
    <Grid>
        <Grid.RowDefinitions>
            <RowDefinition Height="52" />
            <RowDefinition Height="*" />
            <RowDefinition Height="32" />
        </Grid.RowDefinitions>

        <Border Grid.Row="0" Background="#1E293B" BorderBrush="#334155" BorderThickness="0,0,0,1" Padding="14,0">
            <Grid>
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="Auto" />
                    <ColumnDefinition Width="*" />
                    <ColumnDefinition Width="Auto" />
                </Grid.ColumnDefinitions>

                <StackPanel Orientation="Horizontal" VerticalAlignment="Center">
                    <TextBlock x:Name="DocTitleText" Text="文档" FontSize="14" FontWeight="SemiBold"
                               Foreground="#F8FAFC" VerticalAlignment="Center" MaxWidth="280" TextTrimming="CharacterEllipsis" />
                    <Border Background="#065F46" CornerRadius="4" Padding="8,3" Margin="12,0,0,0">
                        <TextBlock Text="防截屏/防录屏已启用" FontSize="11" Foreground="#A7F3D0" />
                    </Border>
                </StackPanel>

                <StackPanel Grid.Column="1" Orientation="Horizontal" HorizontalAlignment="Center" VerticalAlignment="Center">
                    <Button x:Name="PrevPageBtn" Content="上一页" Background="#334155" Foreground="#F8FAFC"
                            BorderThickness="0" Padding="10,5" Click="PrevPageBtn_Click" />
                    <TextBox x:Name="PageInputBox" Width="46" Text="1" TextAlignment="Center" VerticalContentAlignment="Center"
                             Margin="8,0,4,0" Background="#0F172A" Foreground="#F8FAFC" BorderBrush="#475569"
                             KeyDown="PageInputBox_KeyDown" />
                    <TextBlock x:Name="TotalPagesText" Text="/ 1" Foreground="#94A3B8" VerticalAlignment="Center" Margin="0,0,10,0" />
                    <Button x:Name="NextPageBtn" Content="下一页" Background="#334155" Foreground="#F8FAFC"
                            BorderThickness="0" Padding="10,5" Click="NextPageBtn_Click" />

                    <Rectangle Width="1" Height="22" Fill="#334155" Margin="16,0" />

                    <Button Content="-" Width="30" Background="#334155" Foreground="#F8FAFC" BorderThickness="0" Padding="0,4" Click="ZoomOutBtn_Click" />
                    <TextBlock x:Name="ZoomText" Text="100%" Foreground="#E2E8F0" VerticalAlignment="Center" Margin="10,0" MinWidth="42" TextAlignment="Center" />
                    <Button Content="+" Width="30" Background="#334155" Foreground="#F8FAFC" BorderThickness="0" Padding="0,4" Click="ZoomInBtn_Click" />
                </StackPanel>

                <StackPanel Grid.Column="2" Orientation="Horizontal" VerticalAlignment="Center">
                    <StackPanel x:Name="MoireAdjustPanel" Orientation="Horizontal" VerticalAlignment="Center" Margin="0,0,12,0" Visibility="Collapsed">
                        <TextBlock Text="摩尔纹:" Foreground="#94A3B8" VerticalAlignment="Center" Margin="0,0,6,0" FontSize="12" />
                        <ComboBox x:Name="MoireCombo" Width="76" SelectedIndex="2" SelectionChanged="MoireCombo_SelectionChanged">
                            <ComboBoxItem Content="关闭" Tag="0" />
                            <ComboBoxItem Content="低" Tag="1" />
                            <ComboBoxItem Content="中" Tag="2" />
                            <ComboBoxItem Content="高" Tag="3" />
                        </ComboBox>
                    </StackPanel>

                    <Button x:Name="PrintBtn" Content="安全打印" Background="#0284C7" Foreground="White"
                            BorderThickness="0" Padding="14,6" FontWeight="SemiBold" Click="PrintBtn_Click" />
                </StackPanel>
            </Grid>
        </Border>

        <Grid Grid.Row="1" ClipToBounds="True">
            <ScrollViewer x:Name="PageScrollViewer" Background="#0B1120"
                          HorizontalScrollBarVisibility="Auto" VerticalScrollBarVisibility="Auto"
                          PreviewMouseWheel="PageScrollViewer_PreviewMouseWheel">
                <Border Margin="24" HorizontalAlignment="Center" VerticalAlignment="Center" Background="White">
                    <Image x:Name="PageImage" Stretch="None" RenderOptions.BitmapScalingMode="HighQuality" />
                </Border>
            </ScrollViewer>

            <wm:AntiCameraOverlay x:Name="CameraOverlay" />

            <Border x:Name="PrivacyCurtain" Background="#EE0F172A" Visibility="Collapsed">
                <StackPanel VerticalAlignment="Center" HorizontalAlignment="Center">
                    <TextBlock Text="内容已临时遮蔽" FontSize="22" FontWeight="Bold" Foreground="#F8FAFC" HorizontalAlignment="Center" />
                    <TextBlock Text="当前阅读窗口失去焦点，为防止后台窃取或旁观，点击窗口即可恢复显示。"
                               FontSize="13" Foreground="#94A3B8" Margin="0,8,0,0" HorizontalAlignment="Center" />
                </StackPanel>
            </Border>
        </Grid>

        <Border Grid.Row="2" Background="#1E293B" BorderBrush="#334155" BorderThickness="0,1,0,0" Padding="14,0">
            <Grid>
                <TextBlock x:Name="QuotaStatusText" Text="剩余打印次数: --" FontSize="12" Foreground="#38BDF8" VerticalAlignment="Center" />
                <TextBlock x:Name="TraceStatusText" Text="" FontSize="11" Foreground="#64748B" HorizontalAlignment="Right" VerticalAlignment="Center" />
            </Grid>
        </Border>
    </Grid>
</Window>
```


## `src/SecurePdf.Reader/Views/ReaderWindow.xaml.cs`


**模块说明**：内存渲染、快捷键控制（`Ctrl+P` 安全打印）、实时展示截止时间并阻止直接系统打印（共 `216` 行）


```csharp
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Storage;
using SecurePdf.Reader.Security;

namespace SecurePdf.Reader.Views
{
    public partial class ReaderWindow : Window
    {
        private readonly UnlockedSpdfDocument _document;
        private readonly LocalQuotaStore _quotaStore;
        private readonly RuntimeTraceContext _traceContext;
        private readonly MemoryPdfRenderer _renderer;
        private QuotaSnapshot _quotaSnapshot;

        private int _currentPageIndex;
        private double _zoomFactor = 1.0;

        [DllImport("gdi32.dll")]
        private static extern bool DeleteObject(IntPtr hObject);

        public ReaderWindow(
            UnlockedSpdfDocument document,
            LocalQuotaStore quotaStore,
            RuntimeTraceContext traceContext,
            QuotaSnapshot initialSnapshot)
        {
            InitializeComponent();
            WindowCaptureProtector.ApplyProtection(this);

            _document = document ?? throw new ArgumentNullException(nameof(document));
            _quotaStore = quotaStore ?? throw new ArgumentNullException(nameof(quotaStore));
            _traceContext = traceContext ?? throw new ArgumentNullException(nameof(traceContext));
            _quotaSnapshot = initialSnapshot ?? throw new ArgumentNullException(nameof(initialSnapshot));

            _renderer = MemoryPdfRenderer.LoadFromMemory(_document.PdfBytes);

            InitializeUiState();
            RenderCurrentPage();
        }

        private void InitializeUiState()
        {
            DocTitleText.Text = string.IsNullOrWhiteSpace(_document.Policy.OriginalFileName)
                ? "受保护文档"
                : _document.Policy.OriginalFileName;

            TotalPagesText.Text = $"/ {_renderer.PageCount}";
            UpdateQuotaDisplay();
            TraceStatusText.Text = $"设备码: {_traceContext.MachineCode}  |  IP: {_traceContext.LocalIpAddress}  |  禁止复制与另存";

            bool allowHidden = (_document.Policy.SecurityFlags & SpdfSecurityFlags.EnableHiddenWatermark) != 0;
            bool allowVisible = (_document.Policy.SecurityFlags & SpdfSecurityFlags.EnableVisibleWatermark) != 0;
            bool allowAdjustMoire = (_document.Policy.SecurityFlags & SpdfSecurityFlags.AllowUserAdjustMoire) != 0;

            CameraOverlay.Configure(_document.Policy.MoireLevel, allowHidden, allowVisible, _traceContext, _document.Policy.WatermarkNote);
            CameraOverlay.Start();

            if (allowAdjustMoire)
            {
                MoireAdjustPanel.Visibility = Visibility.Visible;
                MoireCombo.SelectedIndex = (int)_document.Policy.MoireLevel;
            }

            if ((_document.Policy.SecurityFlags & SpdfSecurityFlags.EnableBlurOnUnfocus) != 0)
            {
                Deactivated += (_, __) => PrivacyCurtain.Visibility = Visibility.Visible;
                Activated += (_, __) => PrivacyCurtain.Visibility = Visibility.Collapsed;
            }

            PreviewKeyDown += ReaderWindow_PreviewKeyDown;
            Closed += ReaderWindow_Closed;
        }

        private void UpdateQuotaDisplay()
        {
            string expirySuffix = _document.Policy.ExpiresAtUtc.HasValue
                ? $"  |  有效期至: {_document.Policy.ExpiresAtUtc.Value.ToLocalTime():yyyy-MM-dd HH:mm}"
                : string.Empty;

            if (_quotaSnapshot.MaxPrints == 0)
            {
                QuotaStatusText.Text = $"打印策略: 发送方已禁止打印 (0 次){expirySuffix}";
                PrintBtn.IsEnabled = false;
                PrintBtn.Content = "禁止打印";
            }
            else
            {
                QuotaStatusText.Text = $"剩余打印次数: {_quotaSnapshot.RemainingPrints} / {_quotaSnapshot.MaxPrints}{expirySuffix}";
                PrintBtn.IsEnabled = _quotaSnapshot.RemainingPrints > 0;
                PrintBtn.Content = _quotaSnapshot.RemainingPrints > 0 ? "安全打印" : "打印次数已用尽";
            }
        }

        private void RenderCurrentPage()
        {
            if (_renderer.PageCount <= 0) return;

            double dpi = 120.0 * _zoomFactor;
            using (Bitmap bmp = _renderer.RenderPageToBitmap(_currentPageIndex, dpi, dpi))
            {
                IntPtr hBitmap = bmp.GetHbitmap();
                try
                {
                    BitmapSource source = Imaging.CreateBitmapSourceFromHBitmap(hBitmap, IntPtr.Zero, Int32Rect.Empty, BitmapSizeOptions.FromEmptyOptions());
                    source.Freeze();
                    PageImage.Source = source;
                }
                finally
                {
                    DeleteObject(hBitmap);
                }
            }

            PageInputBox.Text = (_currentPageIndex + 1).ToString();
            ZoomText.Text = $"{Math.Round(_zoomFactor * 100)}%";
            PrevPageBtn.IsEnabled = _currentPageIndex > 0;
            NextPageBtn.IsEnabled = _currentPageIndex < _renderer.PageCount - 1;
            PageScrollViewer.ScrollToTop();
        }

        private void PrevPageBtn_Click(object sender, RoutedEventArgs e)
        {
            if (_currentPageIndex > 0) { _currentPageIndex--; RenderCurrentPage(); }
        }

        private void NextPageBtn_Click(object sender, RoutedEventArgs e)
        {
            if (_currentPageIndex < _renderer.PageCount - 1) { _currentPageIndex++; RenderCurrentPage(); }
        }

        private void PageInputBox_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Enter && int.TryParse(PageInputBox.Text.Trim(), out int pageNum))
            {
                int target = Math.Max(1, Math.Min(_renderer.PageCount, pageNum)) - 1;
                if (target != _currentPageIndex) { _currentPageIndex = target; RenderCurrentPage(); }
            }
        }

        private void ZoomOutBtn_Click(object sender, RoutedEventArgs e) { _zoomFactor = Math.Max(0.5, _zoomFactor - 0.15); RenderCurrentPage(); }
        private void ZoomInBtn_Click(object sender, RoutedEventArgs e) { _zoomFactor = Math.Min(2.5, _zoomFactor + 0.15); RenderCurrentPage(); }

        private void PageScrollViewer_PreviewMouseWheel(object sender, MouseWheelEventArgs e)
        {
            if ((Keyboard.Modifiers & ModifierKeys.Control) == ModifierKeys.Control)
            {
                e.Handled = true;
                _zoomFactor = e.Delta > 0 ? Math.Min(2.5, _zoomFactor + 0.15) : Math.Max(0.5, _zoomFactor - 0.15);
                RenderCurrentPage();
            }
        }

        private void MoireCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (CameraOverlay != null && MoireCombo.SelectedItem is ComboBoxItem item && int.TryParse(item.Tag?.ToString(), out int val))
            {
                CameraOverlay.SetMoireStrength((MoireStrength)val);
            }
        }

        private void ReaderWindow_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if ((Keyboard.Modifiers & ModifierKeys.Control) == ModifierKeys.Control && e.Key == Key.P)
            {
                e.Handled = true;
                OpenSecurePrintDialog();
            }
            else if (e.Key == Key.Left || e.Key == Key.PageUp) PrevPageBtn_Click(sender, e);
            else if (e.Key == Key.Right || e.Key == Key.PageDown) NextPageBtn_Click(sender, e);
        }

        private void PrintBtn_Click(object sender, RoutedEventArgs e) => OpenSecurePrintDialog();

        private void OpenSecurePrintDialog()
        {
            QuotaAccessResult access = _quotaStore.LoadAndVerify(_document.Policy.DocId, _document.Policy.MaxPrints, _document.Policy.ExpiresAtUtc);
            _quotaSnapshot = access.Snapshot;
            UpdateQuotaDisplay();

            if (!access.IsAllowed)
            {
                MessageBox.Show(this, access.BlockReason, "安全校验失败", MessageBoxButton.OK, MessageBoxImage.Error);
                Close();
                return;
            }
            if (_quotaSnapshot.RemainingPrints <= 0)
            {
                MessageBox.Show(this, "当前文档已无剩余打印次数。", "禁止打印", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            var dlg = new SecurePrintDialog(_renderer, _document.Policy, _quotaStore, _traceContext, _currentPageIndex, _quotaSnapshot) { Owner = this };
            dlg.ShowDialog();
            _quotaSnapshot = dlg.LatestSnapshot;
            UpdateQuotaDisplay();
        }

        private void ReaderWindow_Closed(object? sender, EventArgs e)
        {
            CameraOverlay.Stop();
            PageImage.Source = null;
            _renderer.Dispose();
            _document.Dispose();
        }
    }
}
```


## `src/SecurePdf.Reader/Views/SecurePrintDialog.xaml`


**模块说明**：物理打印机选择与页码范围配置弹窗（共 `43` 行）


```xml
<Window x:Class="SecurePdf.Reader.Views.SecurePrintDialog"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="安全打印（已禁用虚拟 PDF 打印机）"
        Height="430" Width="500"
        WindowStartupLocation="CenterOwner"
        ResizeMode="NoResize"
        Background="#0F172A">
    <Border Margin="18" Padding="20" Background="#1E293B" CornerRadius="8" BorderBrush="#334155" BorderThickness="1">
        <StackPanel>
            <TextBlock Text="受控物理打印" FontSize="18" FontWeight="Bold" Foreground="#F8FAFC" />
            <TextBlock x:Name="QuotaBannerText" Text="剩余打印次数: --" FontSize="13" Foreground="#38BDF8" Margin="0,6,0,14" />

            <TextBlock Text="选择已验证的物理打印机" FontSize="12" Foreground="#CBD5E1" Margin="0,0,0,6" />
            <ComboBox x:Name="PrinterCombo" Height="34" FontSize="13" Padding="8,4" />
            <TextBlock x:Name="BlockedSummaryText" Text="" FontSize="11" Foreground="#94A3B8"
                       TextWrapping="Wrap" Margin="0,6,0,14" />

            <TextBlock Text="打印页面范围" FontSize="12" Foreground="#CBD5E1" Margin="0,0,0,6" />
            <StackPanel Margin="0,0,0,14">
                <RadioButton x:Name="AllPagesRadio" Content="全部页面" IsChecked="True" Foreground="#E2E8F0" Margin="0,0,0,6" />
                <RadioButton x:Name="CurrentPageRadio" Content="仅当前页" Foreground="#E2E8F0" Margin="0,0,0,6" />
                <StackPanel Orientation="Horizontal">
                    <RadioButton x:Name="CustomPagesRadio" Content="自定义页码 (如 1-3,5):" Foreground="#E2E8F0" VerticalAlignment="Center" />
                    <TextBox x:Name="CustomRangeBox" Width="150" Margin="8,0,0,0" Padding="6,4"
                             Background="#0F172A" Foreground="#F8FAFC" BorderBrush="#475569"
                             GotFocus="CustomRangeBox_GotFocus" />
                </StackPanel>
            </StackPanel>

            <TextBlock Text="提示：每次提交打印任务将扣减 1 次本地额度，且打印纸张会自动嵌入实时时间、IP 与设备水印。"
                       FontSize="11" Foreground="#FBBF24" TextWrapping="Wrap" Margin="0,0,0,16" />

            <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">
                <Button Content="取消" Width="90" Background="#334155" Foreground="#F8FAFC"
                        BorderThickness="0" Margin="0,0,10,0" Click="CancelBtn_Click" />
                <Button x:Name="ConfirmPrintBtn" Content="确认打印 (消耗 1 次)" Width="160"
                        Background="#0284C7" Foreground="White" FontWeight="SemiBold"
                        BorderThickness="0" Click="ConfirmPrintBtn_Click" />
            </StackPanel>
        </StackPanel>
    </Border>
</Window>
```


## `src/SecurePdf.Reader/Views/SecurePrintDialog.xaml.cs`


**模块说明**：展示过滤后的物理打印机与剩余配额/有效期，并原子化预占/回滚打印配额（共 `172` 行）


```csharp
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using System.Windows;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Printing;
using SecurePdf.Core.Storage;
using SecurePdf.Reader.Security;

namespace SecurePdf.Reader.Views
{
    public partial class SecurePrintDialog : Window
    {
        private readonly MemoryPdfRenderer _renderer;
        private readonly SpdfPolicy _policy;
        private readonly LocalQuotaStore _quotaStore;
        private readonly RuntimeTraceContext _traceContext;
        private readonly int _currentPageIndex;

        public QuotaSnapshot LatestSnapshot { get; private set; }

        public SecurePrintDialog(
            MemoryPdfRenderer renderer,
            SpdfPolicy policy,
            LocalQuotaStore quotaStore,
            RuntimeTraceContext traceContext,
            int currentPageIndex,
            QuotaSnapshot currentSnapshot)
        {
            InitializeComponent();
            WindowCaptureProtector.ApplyProtection(this);

            _renderer = renderer;
            _policy = policy;
            _quotaStore = quotaStore;
            _traceContext = traceContext;
            _currentPageIndex = currentPageIndex;
            LatestSnapshot = currentSnapshot;

            UpdateQuotaBannerText();
            CurrentPageRadio.Content = $"仅当前页 (第 {currentPageIndex + 1} 页)";
            LoadPhysicalPrinters();
        }

        private void UpdateQuotaBannerText()
        {
            string expiryHint = _policy.ExpiresAtUtc.HasValue
                ? $"（有效期至 {_policy.ExpiresAtUtc.Value.ToLocalTime():yyyy-MM-dd HH:mm}）"
                : string.Empty;
            QuotaBannerText.Text = $"当前剩余打印次数: {LatestSnapshot.RemainingPrints} / {LatestSnapshot.MaxPrints}{expiryHint}";
        }

        private void LoadPhysicalPrinters()
        {
            PrinterScanSummary scan = PrinterSecurityGuard.ScanPrinters();
            PrinterCombo.ItemsSource = scan.AllowedPrinters.Select(p => p.PrinterName).ToList();

            if (scan.AllowedPrinters.Count > 0)
            {
                PrinterCombo.SelectedIndex = 0;
                ConfirmPrintBtn.IsEnabled = LatestSnapshot.RemainingPrints > 0;
            }
            else
            {
                ConfirmPrintBtn.IsEnabled = false;
            }

            if (scan.BlockedPrinters.Count > 0)
            {
                string blockedNames = string.Join("、", scan.BlockedPrinters.Take(3).Select(b => b.Candidate.PrinterName));
                string extra = scan.BlockedPrinters.Count > 3 ? $" 等共 {scan.BlockedPrinters.Count} 台" : $" 共 {scan.BlockedPrinters.Count} 台";
                BlockedSummaryText.Text = scan.AllowedPrinters.Count == 0
                    ? $"未检测到物理打印机。已自动拦截虚拟/PDF 打印机：{blockedNames}{extra}。"
                    : $"已自动屏蔽虚拟/PDF 打印机：{blockedNames}{extra}。";
            }
            else if (scan.AllowedPrinters.Count == 0)
            {
                BlockedSummaryText.Text = "未检测到可用的本地或网络物理打印机。";
            }
        }

        private void CustomRangeBox_GotFocus(object sender, RoutedEventArgs e) => CustomPagesRadio.IsChecked = true;
        private void CancelBtn_Click(object sender, RoutedEventArgs e) => Close();

        private async void ConfirmPrintBtn_Click(object sender, RoutedEventArgs e)
        {
            string? selectedPrinter = PrinterCombo.SelectedItem as string;
            if (string.IsNullOrWhiteSpace(selectedPrinter))
            {
                MessageBox.Show(this, "请先选择一台物理打印机。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            List<int> pageIndices;
            try { pageIndices = ResolvePageIndices(); }
            catch (Exception ex)
            {
                MessageBox.Show(this, ex.Message, "页码格式错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            ConfirmPrintBtn.IsEnabled = false;
            ConfirmPrintBtn.Content = "正在安全打印...";

            try
            {
                string printWatermark = string.IsNullOrWhiteSpace(_policy.WatermarkNote)
                    ? $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} | IP:{_traceContext.LocalIpAddress} | {_traceContext.MachineCode}"
                    : $"{_policy.WatermarkNote} | {DateTime.Now:yyyy-MM-dd HH:mm:ss} | IP:{_traceContext.LocalIpAddress} | {_traceContext.MachineCode}";

                QuotaSnapshot updated = await Task.Run(() => PrinterSecurityGuard.PrintDocumentSecurely(
                    _renderer, selectedPrinter!, pageIndices, printWatermark,
                    _quotaStore, _policy.DocId, _policy.MaxPrints, _policy.ExpiresAtUtc));

                LatestSnapshot = updated;
                MessageBox.Show(this, $"打印任务已提交至物理打印机。\n剩余打印次数：{updated.RemainingPrints} / {updated.MaxPrints}", "打印完成", MessageBoxButton.OK, MessageBoxImage.Information);
                Close();
            }
            catch (Exception ex)
            {
                QuotaAccessResult refreshed = _quotaStore.LoadAndVerify(_policy.DocId, _policy.MaxPrints, _policy.ExpiresAtUtc);
                LatestSnapshot = refreshed.Snapshot;
                UpdateQuotaBannerText();
                MessageBox.Show(this, $"打印失败：{ex.Message}", "打印被拒绝", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            finally
            {
                ConfirmPrintBtn.IsEnabled = LatestSnapshot.RemainingPrints > 0 && PrinterCombo.Items.Count > 0;
                ConfirmPrintBtn.Content = "确认打印 (消耗 1 次)";
            }
        }

        private List<int> ResolvePageIndices()
        {
            if (AllPagesRadio.IsChecked == true) return Enumerable.Range(0, _renderer.PageCount).ToList();
            if (CurrentPageRadio.IsChecked == true) return new List<int> { _currentPageIndex };

            string spec = CustomRangeBox.Text.Trim();
            if (string.IsNullOrWhiteSpace(spec)) throw new ArgumentException("请输入自定义页码范围，例如 1-3,5");

            var pages = new SortedSet<int>();
            foreach (string token in spec.Split(new[] { ',', '，' }, StringSplitOptions.RemoveEmptyEntries))
            {
                string t = token.Trim();
                int dashIdx = t.IndexOf('-');
                if (dashIdx > 0)
                {
                    if (!int.TryParse(t.Substring(0, dashIdx), out int start) ||
                        !int.TryParse(t.Substring(dashIdx + 1), out int end) ||
                        start < 1 || end < start || end > _renderer.PageCount)
                    {
                        throw new ArgumentException($"无效页码区间: {t}（文档共 {_renderer.PageCount} 页）");
                    }
                    for (int p = start; p <= end; p++) pages.Add(p - 1);
                }
                else
                {
                    if (!int.TryParse(t, out int single) || single < 1 || single > _renderer.PageCount)
                    {
                        throw new ArgumentException($"无效页码: {t}（文档共 {_renderer.PageCount} 页）");
                    }
                    pages.Add(single - 1);
                }
            }
            if (pages.Count == 0) throw new ArgumentException("未解析到有效页码。");
            return pages.ToList();
        }
    }
}
```


---

[返回总览 README](../README.md)
