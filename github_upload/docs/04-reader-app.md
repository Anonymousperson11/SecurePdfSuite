# 04 — 接收端阅读器 (`SecurePdf.Reader`)

> [!NOTE]
> 本文档包含接收端/阅读端 WPF 应用 `SecurePdf.Reader` 的全部源码（v1.1.0）。
> v1.1.0 采用 `WriteableBitmap` DPI 感知纯内存渲染（`ScreenPixelBudget = 32M`）、增强防截屏失效即退出 (`fail-closed`)、新增文本框快捷键豁免、30 秒周期性有效期/时钟/IP 巡检与微点动态水印修复。

## 目录

- [`src/SecurePdf.Reader/SecurePdf.Reader.csproj`](#srcsecurepdfreadersecurepdfreadercsproj)
- [`src/SecurePdf.Reader/App.xaml`](#srcsecurepdfreaderappxaml)
- [`src/SecurePdf.Reader/App.xaml.cs`](#srcsecurepdfreaderappxamlcs)
- [`src/SecurePdf.Reader/Security/WindowCaptureProtector.cs`](#srcsecurepdfreadersecuritywindowcaptureprotectorcs)
- [`src/SecurePdf.Reader/Watermark/AntiCameraOverlay.cs`](#srcsecurepdfreaderwatermarkanticameraoverlaycs)
- [`src/SecurePdf.Reader/Views/UnlockWindow.xaml`](#srcsecurepdfreaderviewsunlockwindowxaml)
- [`src/SecurePdf.Reader/Views/UnlockWindow.xaml.cs`](#srcsecurepdfreaderviewsunlockwindowxamlcs)
- [`src/SecurePdf.Reader/Views/ReaderWindow.xaml`](#srcsecurepdfreaderviewsreaderwindowxaml)
- [`src/SecurePdf.Reader/Views/ReaderWindow.xaml.cs`](#srcsecurepdfreaderviewsreaderwindowxamlcs)
- [`src/SecurePdf.Reader/Views/SecurePrintDialog.xaml`](#srcsecurepdfreaderviewssecureprintdialogxaml)
- [`src/SecurePdf.Reader/Views/SecurePrintDialog.xaml.cs`](#srcsecurepdfreaderviewssecureprintdialogxamlcs)

## `src/SecurePdf.Reader/SecurePdf.Reader.csproj`

**模块说明**：阅读端 WPF 项目配置（运行期加载由安装器部署的无 V8/XFA 精简版 `pdfium.dll`，移除原生包引用）（共 `21` 行）

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

**模块说明**：阅读端应用暗色样式与自定义按钮控件模板（共 `39` 行）

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
            <!-- 扁平按钮模板：系统默认模板在深色主题下，悬停 / 禁用时会变成浅色底，白色文字看不清 -->
            <Setter Property="Template">
                <Setter.Value>
                    <ControlTemplate TargetType="Button">
                        <Border x:Name="Chrome" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}"
                                BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="3" SnapsToDevicePixels="True">
                            <ContentPresenter Margin="{TemplateBinding Padding}" RecognizesAccessKey="True"
                                              HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}"
                                              VerticalAlignment="{TemplateBinding VerticalContentAlignment}" />
                        </Border>
                        <ControlTemplate.Triggers>
                            <Trigger Property="IsMouseOver" Value="True">
                                <Setter TargetName="Chrome" Property="Opacity" Value="0.85" />
                            </Trigger>
                            <Trigger Property="IsPressed" Value="True">
                                <Setter TargetName="Chrome" Property="Opacity" Value="0.7" />
                            </Trigger>
                            <Trigger Property="IsEnabled" Value="False">
                                <Setter TargetName="Chrome" Property="Opacity" Value="0.4" />
                            </Trigger>
                        </ControlTemplate.Triggers>
                    </ControlTemplate>
                </Setter.Value>
            </Setter>
        </Style>
        <Style TargetType="TextBlock">
            <Setter Property="FontFamily" Value="Microsoft YaHei UI" />
        </Style>
    </Application.Resources>
</Application>
```

## `src/SecurePdf.Reader/App.xaml.cs`

**模块说明**：支持命令行与双击 `.spdf` 参数传递至 `UnlockWindow`（不再覆写用户 `.spdf` 关联），异常时关闭所有窗口释放明文内存（共 `61` 行）

```csharp
using System;
using System.IO;
using System.Linq;
using System.Windows;
using System.Windows.Threading;
using SecurePdf.Reader.Views;

namespace SecurePdf.Reader
{
    public partial class App : Application
    {
        private bool _crashReported;

        protected override void OnStartup(StartupEventArgs e)
        {
            base.OnStartup(e);
            DispatcherUnhandledException += OnDispatcherUnhandledException;

            // .spdf 文件关联由安装程序（或“关联spdf文件格式.bat”）负责，阅读端不再在每次启动时改写注册表
            string? initialPath = null;
            if (e.Args.Length > 0 && File.Exists(e.Args[0]))
            {
                initialPath = e.Args[0];
            }

            try
            {
                new UnlockWindow(initialPath).Show();
            }
            catch (Exception ex)
            {
                MessageBox.Show("阅读端启动失败：" + ex.Message, "SecurePdf Reader", MessageBoxButton.OK, MessageBoxImage.Error);
                Shutdown(1);
            }
        }

        /// <summary>
        /// 未处理异常：给出提示后关闭全部窗口再退出。阅读窗口的 Closed 处理会释放渲染器并清零解密后的文档，
        /// 不让明文随着崩溃对话框一直留在内存里。
        /// </summary>
        private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
        {
            e.Handled = true;
            if (_crashReported) return;
            _crashReported = true;

            try
            {
                MessageBox.Show("程序遇到意外错误，即将关闭：" + e.Exception.Message, "SecurePdf Reader", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            catch (InvalidOperationException) { }

            foreach (Window window in Windows.Cast<Window>().ToList())
            {
                try { window.Close(); }
                catch (InvalidOperationException) { }
            }
            Shutdown(1);
        }
    }
}
```

## `src/SecurePdf.Reader/Security/WindowCaptureProtector.cs`

**模块说明**：调用 `user32.dll` `SetWindowDisplayAffinity`（`WDA_EXCLUDEFROMCAPTURE` 及 `WDA_MONITOR` 回退），并在保护不可用或失效时触发 `ProtectionFailed`（共 `56` 行）

```csharp
using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
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

        /// <summary>
        /// 让窗口内容不出现在截图 / 录屏 / 远程采集里。窗口句柄创建后才能设置，所以实际生效在 SourceInitialized。
        /// 两种保护模式都设置失败时调用 <paramref name="onProtectionUnavailable"/>，由调用方决定是否拒绝显示内容。
        /// </summary>
        public static void ApplyProtection(Window window, Action? onProtectionUnavailable = null)
        {
            if (window == null) throw new ArgumentNullException(nameof(window));

            window.SourceInitialized += (_, __) =>
            {
                IntPtr hwnd = new WindowInteropHelper(window).Handle;
                bool applied = hwnd != IntPtr.Zero &&
                               (SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) ||    // Windows 10 2004 及以上：截图里窗口直接消失
                                SetWindowDisplayAffinity(hwnd, WDA_MONITOR));                // 更早的系统：截图里窗口是黑块
                if (!applied) onProtectionUnavailable?.Invoke();
            };

            window.PreviewKeyDown += (_, e) =>
            {
                Key key = e.Key == Key.System ? e.SystemKey : e.Key;
                if (key == Key.Snapshot)
                {
                    e.Handled = true;
                    return;
                }

                // 输入框（验证码、页码）里的复制 / 全选是正常编辑操作，不拦
                if (e.OriginalSource is TextBoxBase || e.OriginalSource is PasswordBox) return;

                if ((Keyboard.Modifiers & ModifierKeys.Control) == ModifierKeys.Control &&
                    (key == Key.C || key == Key.X || key == Key.S || key == Key.A || key == Key.E))
                {
                    e.Handled = true;
                }
            };
        }
    }
}
```

## `src/SecurePdf.Reader/Watermark/AntiCameraOverlay.cs`

**模块说明**：动态高频摩尔纹网格 + 蓝通道微偏移隐形水印；使用冻结画刷与 `Typeface`、`Viewbox(0,0,72,56)` 微点铺底修正，并在窗口最小化/不可见时跳过无效重绘（共 `305` 行）

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
    /// 盖在页面上的防拍叠加层：
    /// 1) 高频、相位不断平移的摩尔纹干扰网格（针对手机摄像头）；
    /// 2) 低亮度蓝色隐形溯源水印（实时时间、IP、设备码），能扛住翻拍模糊与 JPEG 压缩，
    ///    供创建端“水印取证”的 ChromaStegoExtract 模式提取；
    /// 3) 可选的半透明可见水印。
    /// </summary>
    public sealed class AntiCameraOverlay : FrameworkElement
    {
        // 画笔 / 画刷 / 字体每帧都一样，冻结后复用，不再每 350 毫秒重新分配一批
        private static readonly Typeface WatermarkTypeface = new Typeface(
            new FontFamily("Microsoft YaHei UI"), FontStyles.Normal, FontWeights.Bold, FontStretches.Normal);
        private static readonly Brush HiddenPrimaryBrush = Frozen(Color.FromArgb(18, 0, 70, 245));
        private static readonly Brush HiddenShadowBrush = Frozen(Color.FromArgb(10, 240, 246, 255));
        private static readonly Brush VisiblePrimaryBrush = Frozen(Color.FromArgb(34, 15, 23, 42));
        private static readonly Brush VisibleShadowBrush = Frozen(Color.FromArgb(18, 248, 250, 252));
        private static readonly Brush MicroDotBrush = Frozen(Color.FromArgb(14, 0, 40, 220));

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
                // 窗口最小化或叠加层不可见时不重绘，省掉后台的持续 CPU 占用
                if (!IsVisible || Window.GetWindow(this)?.WindowState == WindowState.Minimized) return;

                _phaseStep = (_phaseStep + 1) % 6;
                DateTime now = DateTime.Now;
                if (now.Second != _currentLocalTime.Second || _moireStrength != MoireStrength.Off)
                {
                    _currentLocalTime = now;
                    InvalidateVisual();
                }
            };
        }

        private static Brush Frozen(Color color)
        {
            var brush = new SolidColorBrush(color);
            brush.Freeze();
            return brush;
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

            _currentLocalTime = DateTime.Now;

            // 1. 高频摩尔纹干扰网格
            if (_moireStrength != MoireStrength.Off)
            {
                DrawMoireInterference(dc, w, h);
            }

            // 2. 隐形溯源水印（时间 + IP + 设备码）
            if (_enableHiddenWatermark)
            {
                DrawForensicWatermarks(dc, w, h, visibleMode: false);
                DrawMicroDotTraceMatrix(dc, w, h);
            }

            // 3. 可选的可见水印
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

            Typeface typeface = WatermarkTypeface;

            // 隐形模式用偏蓝的低透明度粗体字：人眼几乎察觉不到亮度变化，
            // 但 (B - R) 色差信号能在手机翻拍的摩尔纹和 JPEG 压缩后保留下来
            Brush primaryBrush = visibleMode ? VisiblePrimaryBrush : HiddenPrimaryBrush;
            Brush shadowBrush = visibleMode ? VisibleShadowBrush : HiddenShadowBrush;

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

        /// <summary>
        /// 微点阵：8 字节（IPv4 + Unix 秒）逐位排成 8×8 的点，平铺在整个页面上。
        /// 画刷的取景框固定为一个瓦片（72×56），再整体等比放大 3 倍。
        /// 原版没有固定取景框，WPF 会按“当前有哪些点”的包围盒去拉伸，点阵每秒都在变形，横竖比例也不一致。
        /// </summary>
        private void DrawMicroDotTraceMatrix(DrawingContext dc, double width, double height)
        {
            byte[] payload = BuildCompactTraceBytes();

            const double cell = 5.0;
            const double tileW = 72.0;
            const double tileH = 56.0;
            const double tileScale = 3.0;

            var dg = new DrawingGroup();
            using (DrawingContext tdc = dg.Open())
            {
                // 左上角两个定位点
                tdc.DrawRectangle(MicroDotBrush, null, new Rect(2, 2, 3, 3));
                tdc.DrawRectangle(MicroDotBrush, null, new Rect(7, 2, 3, 3));

                for (int byteIdx = 0; byteIdx < payload.Length; byteIdx++)
                {
                    byte b = payload[byteIdx];
                    for (int bit = 0; bit < 8; bit++)
                    {
                        if (((b >> bit) & 1) == 1)
                        {
                            double dx = 4 + bit * cell;
                            double dy = 9 + byteIdx * cell;
                            tdc.DrawRectangle(MicroDotBrush, null, new Rect(dx, dy, 2.5, 2.5));
                        }
                    }
                }
            }
            dg.Freeze();

            var brush = new DrawingBrush(dg)
            {
                TileMode = TileMode.Tile,
                Stretch = Stretch.Fill,
                Viewbox = new Rect(0, 0, tileW, tileH),
                ViewboxUnits = BrushMappingMode.Absolute,
                Viewport = new Rect(0, 0, tileW * tileScale, tileH * tileScale),
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

            uint unixSec = (uint)new DateTimeOffset(_currentLocalTime).ToUnixTimeSeconds();
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

**模块说明**：验证码输入、文档选择与状态反馈窗口界面（共 `52` 行）

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

**模块说明**：防重入异步校验验证码、细分捕获 `QuotaStoreException`/过期/篡改/错误密码异常，并提供渐进式错误退避延迟（共 `172` 行）

```csharp
using System;
using System.IO;
using System.Security.Cryptography;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Input;
using Microsoft.Win32;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Storage;
using SecurePdf.Reader.Security;

namespace SecurePdf.Reader.Views
{
    public partial class UnlockWindow : Window
    {
        private readonly RuntimeTraceContext _traceContext;
        private int _failedAttempts;
        private bool _busy;

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
            if (dlg.ShowDialog(this) == true) LoadEnvelopePreview(dlg.FileName);
        }

        private void Window_DragOver(object sender, DragEventArgs e)
        {
            e.Effects = DragDropEffects.None;
            if (!_busy && e.Data.GetDataPresent(DataFormats.FileDrop) &&
                e.Data.GetData(DataFormats.FileDrop) is string[] files &&
                files.Length > 0 && files[0].EndsWith(".spdf", StringComparison.OrdinalIgnoreCase))
            {
                e.Effects = DragDropEffects.Copy;
            }
            e.Handled = true;
        }

        private void Window_Drop(object sender, DragEventArgs e)
        {
            if (_busy) return;
            if (e.Data.GetData(DataFormats.FileDrop) is string[] files && files.Length > 0 && File.Exists(files[0]))
            {
                LoadEnvelopePreview(files[0]);
            }
        }

        private void LoadEnvelopePreview(string path)
        {
            FilePathBox.Text = path;
            ErrorText.Text = string.Empty;
            try
            {
                SpdfEnvelopeInfo env = SpdfContainer.ReadEnvelope(path);
                double kb = env.FileSizeBytes / 1024.0;
                string size = kb >= 1024.0 ? $"{kb / 1024.0:F1} MB" : $"{kb:F1} KB";
                DocInfoText.Text = $"文档 ID: {env.DocId.ToString("N").Substring(0, 12).ToUpperInvariant()}  ·  大小: {size}";
                CodeBox.Focus();
            }
            catch (Exception ex) when (ex is InvalidDataException || ex is IOException || ex is UnauthorizedAccessException || ex is NotSupportedException)
            {
                // 注意：InvalidDataException（不是 .spdf / 文件不完整）并不继承自 IOException，必须单独列出
                DocInfoText.Text = "无法读取该文件";
                ErrorText.Text = ex.Message;
            }
        }

        private void CodeBox_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Enter)
            {
                e.Handled = true;
                UnlockButton_Click(sender, e);
            }
        }

        private async void UnlockButton_Click(object sender, RoutedEventArgs e)
        {
            if (_busy) return;      // 回车键会绕过按钮的禁用状态，这里再挡一次

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

            _busy = true;
            UnlockButton.IsEnabled = false;
            ErrorText.Text = "正在校验验证码并解密文档...";

            UnlockedSpdfDocument? doc = null;
            MemoryPdfRenderer? renderer = null;
            bool handedOver = false;

            try
            {
                var quotaStore = new LocalQuotaStore(_traceContext.MachineHash);
                QuotaAccessResult? access = null;

                // 解密、授权校验、PDF 解析都放到后台线程：大文件不卡界面。
                // PDF 在创建阅读窗口之前就解析好，解析失败时不会留下一个从未显示、却让进程无法退出的窗口。
                await Task.Run(() =>
                {
                    doc = SpdfContainer.Unlock(path, code);
                    access = quotaStore.LoadAndVerify(doc.Policy.DocId, doc.Policy.MaxPrints, doc.Policy.ExpiresAtUtc);
                    if (access.IsAllowed) renderer = MemoryPdfRenderer.LoadFromMemory(doc.PdfBytes);
                });

                if (!access!.IsAllowed)
                {
                    ErrorText.Text = access.BlockReason;
                    return;
                }

                var readerWindow = new ReaderWindow(doc!, renderer!, quotaStore, _traceContext, access.Snapshot, access.TamperWarning);
                handedOver = true;          // 从这里起文档与渲染器由阅读窗口负责释放
                readerWindow.Show();
                Close();
            }
            catch (CryptographicException ex)
            {
                // 只有“验证码不对 / 密文被改”才算一次失败尝试，并逐次加长等待，拖慢逐个试码
                _failedAttempts++;
                ErrorText.Text = ex.Message;
                await Task.Delay(Math.Min(4000, _failedAttempts * 500));
            }
            catch (Exception ex) when (ex is InvalidDataException || ex is IOException || ex is UnauthorizedAccessException || ex is NotSupportedException ||
                                       ex is QuotaStoreException || ex is InvalidOperationException)
            {
                // 文件读不了 / 格式不对 / 授权状态读写失败 / PDF 解析失败 / 缺少渲染组件：核心库给出的都是可直接展示的中文说明
                ErrorText.Text = ex.Message;
            }
            catch (Exception ex)
            {
                ErrorText.Text = "无法打开该文档：" + ex.Message;
            }
            finally
            {
                if (!handedOver)
                {
                    renderer?.Dispose();
                    doc?.Dispose();
                }
                _busy = false;
                UnlockButton.IsEnabled = true;
            }
        }
    }
}
```

## `src/SecurePdf.Reader/Views/ReaderWindow.xaml`

**模块说明**：受控阅读主界面（含防截屏标识、缩放/翻页控件、失焦隐私幕布与底部配额/有效期状态栏）（共 `106` 行）

```xml
<Window x:Class="SecurePdf.Reader.Views.ReaderWindow"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        xmlns:wm="clr-namespace:SecurePdf.Reader.Watermark"
        Title="SecurePdf Reader — 受控阅读模式"
        Height="860" Width="1120"
        MinHeight="600" MinWidth="820"
        WindowStartupLocation="CenterScreen"
        UseLayoutRounding="True"
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
                    <Image x:Name="PageImage" Stretch="Fill" RenderOptions.BitmapScalingMode="HighQuality" />
                </Border>
            </ScrollViewer>

            <wm:AntiCameraOverlay x:Name="CameraOverlay" />

            <TextBlock x:Name="RenderErrorText" Visibility="Collapsed" Foreground="#F87171" FontSize="14"
                       TextWrapping="Wrap" MaxWidth="560" HorizontalAlignment="Center" VerticalAlignment="Center" />

            <Border x:Name="PrivacyCurtain" Background="#0F172A" Visibility="Collapsed">
                <StackPanel VerticalAlignment="Center" HorizontalAlignment="Center">
                    <TextBlock x:Name="CurtainTitleText" Text="内容已临时遮蔽" FontSize="22" FontWeight="Bold" Foreground="#F8FAFC" HorizontalAlignment="Center" />
                    <TextBlock x:Name="CurtainHintText" Text="当前阅读窗口失去焦点，为防止后台窃取或旁观，点击窗口即可恢复显示。"
                               FontSize="13" Foreground="#94A3B8" Margin="0,8,0,0" HorizontalAlignment="Center" />
                </StackPanel>
            </Border>
        </Grid>

        <Border Grid.Row="2" Background="#1E293B" BorderBrush="#334155" BorderThickness="0,1,0,0" Padding="14,0">
            <Grid>
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="Auto" />
                    <ColumnDefinition Width="*" />
                    <ColumnDefinition Width="Auto" />
                </Grid.ColumnDefinitions>
                <TextBlock x:Name="QuotaStatusText" Text="剩余打印次数: --" FontSize="12" Foreground="#38BDF8" VerticalAlignment="Center" />
                <TextBlock x:Name="TamperNoticeText" Grid.Column="1" Visibility="Collapsed" FontSize="11" Foreground="#FBBF24"
                           Text="提示：本机授权记录曾缺失或不一致，已按较严格的一份自动修复"
                           Margin="16,0" VerticalAlignment="Center" TextTrimming="CharacterEllipsis" />
                <TextBlock x:Name="TraceStatusText" Grid.Column="2" Text="" FontSize="11" Foreground="#64748B" VerticalAlignment="Center" />
            </Grid>
        </Border>
    </Grid>
</Window>
```

## `src/SecurePdf.Reader/Views/ReaderWindow.xaml.cs`

**模块说明**：DPI 感知 `WriteableBitmap` 内存渲染、`OnDpiChanged` 重绘、滚轮翻页节流、文本输入与截屏快捷键分离控制、30 秒周期巡检（时钟/重验配额/刷新 IP）及 fail-closed 保护退出（共 `470` 行）

```csharp
using System;
using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using SecurePdf.Core.Format;
using SecurePdf.Core.Licensing;
using SecurePdf.Core.Pdf;
using SecurePdf.Core.Storage;
using SecurePdf.Reader.Security;

namespace SecurePdf.Reader.Views
{
    public partial class ReaderWindow : Window
    {
        /// <summary>100% 缩放时每英寸页面占 120 个设备无关像素，与旧版的显示大小保持一致。</summary>
        private const double BaseDipPerInch = 120.0;
        /// <summary>屏幕渲染的像素预算（约 128 MB）。A0 图纸放大后超出预算时降低渲染分辨率，由界面拉伸显示。</summary>
        private const double ScreenPixelBudget = 32.0 * 1000 * 1000;
        private const double MinZoom = 0.5;
        private const double MaxZoom = 2.5;
        private const double ZoomStep = 0.15;

        private static readonly TimeSpan HousekeepingInterval = TimeSpan.FromSeconds(30);
        /// <summary>每隔几次例行检查做一次完整的授权复核（读写本地授权记录）。</summary>
        private const int FullVerifyEveryTicks = 4;
        private static readonly TimeSpan WheelPagingPause = TimeSpan.FromMilliseconds(250);

        private readonly UnlockedSpdfDocument _document;
        private readonly LocalQuotaStore _quotaStore;
        private readonly RuntimeTraceContext _traceContext;
        private readonly MemoryPdfRenderer _renderer;
        private QuotaSnapshot _quotaSnapshot;

        private readonly DispatcherTimer _housekeepingTimer;
        private readonly DateTimeOffset _openedAtUtc = DateTimeOffset.UtcNow;
        private readonly Stopwatch _sinceOpened = Stopwatch.StartNew();
        private int _housekeepingTicks;

        private int _currentPageIndex;
        private double _zoomFactor = 1.0;
        private DateTime _lastWheelUtc;
        private bool _printDialogOpen;
        private bool _locked;

        /// <summary>
        /// <paramref name="document"/> 与 <paramref name="renderer"/> 自此归本窗口所有，窗口关闭时释放并清零。
        /// </summary>
        public ReaderWindow(
            UnlockedSpdfDocument document,
            MemoryPdfRenderer renderer,
            LocalQuotaStore quotaStore,
            RuntimeTraceContext traceContext,
            QuotaSnapshot initialSnapshot,
            bool tamperWarning = false)
        {
            _document = document ?? throw new ArgumentNullException(nameof(document));
            _renderer = renderer ?? throw new ArgumentNullException(nameof(renderer));
            _quotaStore = quotaStore ?? throw new ArgumentNullException(nameof(quotaStore));
            _traceContext = traceContext ?? throw new ArgumentNullException(nameof(traceContext));
            _quotaSnapshot = initialSnapshot ?? throw new ArgumentNullException(nameof(initialSnapshot));

            _housekeepingTimer = new DispatcherTimer(DispatcherPriority.Background) { Interval = HousekeepingInterval };
            _housekeepingTimer.Tick += HousekeepingTimer_Tick;

            InitializeComponent();
            Closed += ReaderWindow_Closed;

            try
            {
                WindowCaptureProtector.ApplyProtection(this, OnCaptureProtectionUnavailable);
                if (tamperWarning) TamperNoticeText.Visibility = Visibility.Visible;

                InitializeUiState();
                RenderCurrentPage(resetScroll: true);
                _housekeepingTimer.Start();
            }
            catch
            {
                // 窗口对象一创建就登记在 Application.Windows 里；初始化失败时必须关掉它，否则进程不会退出
                Close();
                throw;
            }
        }

        private void InitializeUiState()
        {
            DocTitleText.Text = string.IsNullOrWhiteSpace(_document.Policy.OriginalFileName)
                ? "受保护文档"
                : _document.Policy.OriginalFileName;

            TotalPagesText.Text = $"/ {_renderer.PageCount}";
            UpdateQuotaDisplay();
            UpdateTraceStatus();

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
                Activated += (_, __) => { if (!_locked) PrivacyCurtain.Visibility = Visibility.Collapsed; };
            }

            PreviewKeyDown += ReaderWindow_PreviewKeyDown;
        }

        private void UpdateTraceStatus()
        {
            TraceStatusText.Text = $"设备码: {_traceContext.MachineCode}  |  IP: {_traceContext.LocalIpAddress}  |  禁止复制与另存";
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

        // ------------------------------------------------------------------ 页面渲染

        /// <summary>
        /// 按“缩放 × 屏幕 DPI”把当前页直接渲染进 WriteableBitmap 的后台缓冲区，位图像素与屏幕像素一一对应，
        /// 高分屏上不再发虚，也省掉了 GDI 位图到 WPF 位图的两次整页拷贝。
        /// </summary>
        private void RenderCurrentPage(bool resetScroll, bool scrollToBottom = false)
        {
            if (_locked) return;

            // 缩放时保持视口中心对着页面上的同一位置
            double centerX = PageScrollViewer.ExtentWidth > 0 ? (PageScrollViewer.HorizontalOffset + PageScrollViewer.ViewportWidth / 2.0) / PageScrollViewer.ExtentWidth : 0.5;
            double centerY = PageScrollViewer.ExtentHeight > 0 ? (PageScrollViewer.VerticalOffset + PageScrollViewer.ViewportHeight / 2.0) / PageScrollViewer.ExtentHeight : 0.0;

            try
            {
                _renderer.GetPageSizeInPoints(_currentPageIndex, out double widthPt, out double heightPt);
                double dipW = widthPt / 72.0 * BaseDipPerInch * _zoomFactor;
                double dipH = heightPt / 72.0 * BaseDipPerInch * _zoomFactor;

                DpiScale dpi = VisualTreeHelper.GetDpi(this);
                double pxW = dipW * dpi.DpiScaleX;
                double pxH = dipH * dpi.DpiScaleY;
                double shrink = pxW * pxH > ScreenPixelBudget ? Math.Sqrt(ScreenPixelBudget / (pxW * pxH)) : 1.0;

                int widthPx = Math.Max(1, (int)Math.Round(pxW * shrink));
                int heightPx = Math.Max(1, (int)Math.Round(pxH * shrink));

                var bitmap = new WriteableBitmap(widthPx, heightPx, 96.0 * dpi.DpiScaleX * shrink, 96.0 * dpi.DpiScaleY * shrink, PixelFormats.Bgr32, null);
                bitmap.Lock();
                try
                {
                    _renderer.RenderPageToBuffer(_currentPageIndex, bitmap.BackBuffer, bitmap.BackBufferStride, widthPx, heightPx);
                    bitmap.AddDirtyRect(new Int32Rect(0, 0, widthPx, heightPx));
                }
                finally
                {
                    bitmap.Unlock();
                }
                bitmap.Freeze();

                PageImage.Source = bitmap;
                // 未降分辨率时按整数设备像素摆放，避免再被缩放一次；降了分辨率则拉伸回应有的显示尺寸
                PageImage.Width = shrink < 1.0 ? dipW : widthPx / dpi.DpiScaleX;
                PageImage.Height = shrink < 1.0 ? dipH : heightPx / dpi.DpiScaleY;
                RenderErrorText.Visibility = Visibility.Collapsed;
            }
            catch (Exception ex) when (ex is InvalidOperationException || ex is ArgumentException || ex is OutOfMemoryException)
            {
                PageImage.Source = null;
                RenderErrorText.Text = $"第 {_currentPageIndex + 1} 页无法显示：{ex.Message}";
                RenderErrorText.Visibility = Visibility.Visible;
            }

            PageInputBox.Text = (_currentPageIndex + 1).ToString();
            ZoomText.Text = $"{Math.Round(_zoomFactor * 100)}%";
            PrevPageBtn.IsEnabled = _currentPageIndex > 0;
            NextPageBtn.IsEnabled = _currentPageIndex < _renderer.PageCount - 1;

            PageScrollViewer.UpdateLayout();
            if (scrollToBottom)
            {
                PageScrollViewer.ScrollToVerticalOffset(PageScrollViewer.ScrollableHeight);
            }
            else if (resetScroll)
            {
                PageScrollViewer.ScrollToTop();
            }
            else
            {
                PageScrollViewer.ScrollToHorizontalOffset(centerX * PageScrollViewer.ExtentWidth - PageScrollViewer.ViewportWidth / 2.0);
                PageScrollViewer.ScrollToVerticalOffset(centerY * PageScrollViewer.ExtentHeight - PageScrollViewer.ViewportHeight / 2.0);
            }
        }

        protected override void OnDpiChanged(DpiScale oldDpi, DpiScale newDpi)
        {
            base.OnDpiChanged(oldDpi, newDpi);
            RenderCurrentPage(resetScroll: false);
        }

        private bool GoToPage(int pageIndex, bool scrollToBottom = false)
        {
            if (pageIndex < 0 || pageIndex >= _renderer.PageCount || pageIndex == _currentPageIndex) return false;
            _currentPageIndex = pageIndex;
            RenderCurrentPage(resetScroll: true, scrollToBottom: scrollToBottom);
            return true;
        }

        private void ChangeZoom(double delta)
        {
            double zoom = Math.Max(MinZoom, Math.Min(MaxZoom, _zoomFactor + delta));
            if (Math.Abs(zoom - _zoomFactor) < 1e-9) return;
            _zoomFactor = zoom;
            RenderCurrentPage(resetScroll: false);
        }

        private void PrevPageBtn_Click(object sender, RoutedEventArgs e) => GoToPage(_currentPageIndex - 1);
        private void NextPageBtn_Click(object sender, RoutedEventArgs e) => GoToPage(_currentPageIndex + 1);

        private void PageInputBox_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key != Key.Enter) return;
            e.Handled = true;

            if (int.TryParse(PageInputBox.Text.Trim(), out int pageNum))
            {
                GoToPage(Math.Max(1, Math.Min(_renderer.PageCount, pageNum)) - 1);
            }
            PageInputBox.Text = (_currentPageIndex + 1).ToString();     // 无效或越界的输入改回实际页码
        }

        private void ZoomOutBtn_Click(object sender, RoutedEventArgs e) => ChangeZoom(-ZoomStep);
        private void ZoomInBtn_Click(object sender, RoutedEventArgs e) => ChangeZoom(ZoomStep);

        private void PageScrollViewer_PreviewMouseWheel(object sender, MouseWheelEventArgs e)
        {
            if ((Keyboard.Modifiers & ModifierKeys.Control) == ModifierKeys.Control)
            {
                e.Handled = true;
                ChangeZoom(e.Delta > 0 ? ZoomStep : -ZoomStep);
                return;
            }

            // 已经滚到页底（页顶）后再滚一下滚轮 → 翻到下一页（上一页）。
            // 要求与上一次滚轮动作之间有短暂停顿，连续滚动到头不会把后面几页一口气翻过去。
            DateTime now = DateTime.UtcNow;
            bool paused = now - _lastWheelUtc >= WheelPagingPause;
            _lastWheelUtc = now;

            bool atBottom = PageScrollViewer.VerticalOffset >= PageScrollViewer.ScrollableHeight - 0.5;
            bool atTop = PageScrollViewer.VerticalOffset <= 0.5;

            if (e.Delta < 0 && atBottom && _currentPageIndex < _renderer.PageCount - 1)
            {
                e.Handled = true;
                if (paused) GoToPage(_currentPageIndex + 1);
            }
            else if (e.Delta > 0 && atTop && _currentPageIndex > 0)
            {
                e.Handled = true;
                if (paused) GoToPage(_currentPageIndex - 1, scrollToBottom: true);
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
                return;
            }

            // 光标在页码输入框里时，左右方向键是移动光标，不翻页
            if (e.OriginalSource is TextBoxBase) return;

            if (e.Key == Key.Left || e.Key == Key.PageUp)
            {
                e.Handled = true;
                GoToPage(_currentPageIndex - 1);
            }
            else if (e.Key == Key.Right || e.Key == Key.PageDown)
            {
                e.Handled = true;
                GoToPage(_currentPageIndex + 1);
            }
        }

        // ------------------------------------------------------------------ 打印

        private void PrintBtn_Click(object sender, RoutedEventArgs e) => OpenSecurePrintDialog();

        private void OpenSecurePrintDialog()
        {
            if (_locked || _printDialogOpen) return;

            QuotaAccessResult access;
            try
            {
                access = _quotaStore.LoadAndVerify(_document.Policy.DocId, _document.Policy.MaxPrints, _document.Policy.ExpiresAtUtc);
            }
            catch (QuotaStoreException ex)
            {
                MessageBox.Show(this, ex.Message, "安全校验失败", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            _quotaSnapshot = access.Snapshot;
            UpdateQuotaDisplay();

            if (!access.IsAllowed)
            {
                LockAndClose(access.BlockReason);
                return;
            }
            if (_quotaSnapshot.MaxPrints == 0)
            {
                MessageBox.Show(this, "发送方已禁止打印该文档。", "禁止打印", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            if (_quotaSnapshot.RemainingPrints <= 0)
            {
                MessageBox.Show(this, "当前文档已无剩余打印次数。", "禁止打印", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            var dlg = new SecurePrintDialog(_renderer, _document.Policy, _quotaStore, _traceContext, _currentPageIndex, _quotaSnapshot) { Owner = this };
            _printDialogOpen = true;
            try
            {
                dlg.ShowDialog();
            }
            finally
            {
                _printDialogOpen = false;
            }
            _quotaSnapshot = dlg.LatestSnapshot;
            UpdateQuotaDisplay();
        }

        // ------------------------------------------------------------------ 阅读期间的例行检查

        /// <summary>
        /// 原版只在解锁和打开打印对话框时检查有效期：在到期前打开的文档，只要不关窗口就能一直看下去。
        /// 这里每 30 秒检查一次是否到期，并定期做完整的授权复核（含系统时间回拨检测）；顺带刷新水印里的本机 IP。
        /// </summary>
        private void HousekeepingTimer_Tick(object? sender, EventArgs e)
        {
            if (_locked) return;

            string ip = DeviceAndNetworkContext.GetPrimaryLocalIpAddress();
            if (!string.Equals(ip, _traceContext.LocalIpAddress, StringComparison.Ordinal))
            {
                _traceContext.LocalIpAddress = ip;      // 叠加层水印与打印水印读取的是同一个对象
                UpdateTraceStatus();
                CameraOverlay.InvalidateVisual();
            }

            DateTimeOffset? expires = _document.Policy.ExpiresAtUtc;
            if (!expires.HasValue || _printDialogOpen) return;     // 打印进行中不打断；打印本身会再校验一次

            // 同时看系统时钟和不受改时间影响的计时器：阅读期间把系统时间往回拨也拖不过有效期
            DateTimeOffset wallNow = DateTimeOffset.UtcNow;
            DateTimeOffset steadyNow = _openedAtUtc + _sinceOpened.Elapsed;
            if (wallNow > expires.Value || steadyNow > expires.Value)
            {
                LockAndClose($"文档授权已过期（有效期至 {expires.Value.ToLocalTime():yyyy-MM-dd HH:mm}）。");
                return;
            }

            if (++_housekeepingTicks % FullVerifyEveryTicks != 0) return;

            try
            {
                QuotaAccessResult access = _quotaStore.LoadAndVerify(_document.Policy.DocId, _document.Policy.MaxPrints, expires);
                if (!access.IsAllowed)
                {
                    LockAndClose(access.BlockReason);
                    return;
                }
                _quotaSnapshot = access.Snapshot;
                UpdateQuotaDisplay();
            }
            catch (QuotaStoreException ex)
            {
                LockAndClose(ex.Message);       // 授权状态读写不了时不能继续放行
            }
        }

        /// <summary>先把内容从屏幕上撤掉，再提示原因并关闭窗口。</summary>
        private void LockAndClose(string reason)
        {
            if (_locked) return;
            _locked = true;

            _housekeepingTimer.Stop();
            CameraOverlay.Stop();
            PageImage.Source = null;
            CurtainTitleText.Text = "文档已锁定";
            CurtainHintText.Text = reason;
            PrivacyCurtain.Visibility = Visibility.Visible;

            MessageBox.Show(this, reason, "文档已锁定", MessageBoxButton.OK, MessageBoxImage.Warning);
            Close();
        }

        /// <summary>
        /// 系统拒绝为本窗口启用防截屏保护：界面上写着“防截屏已启用”，实际却没有，这时不能照常显示内容。
        /// </summary>
        private void OnCaptureProtectionUnavailable()
        {
            _locked = true;
            PageImage.Source = null;
            CurtainTitleText.Text = "无法启用防截屏保护";
            CurtainHintText.Text = "当前系统环境不支持窗口内容保护，为避免文档被截屏 / 录屏，已停止显示。";
            PrivacyCurtain.Visibility = Visibility.Visible;

            Dispatcher.BeginInvoke(new Action(() =>
            {
                MessageBox.Show(this, CurtainHintText.Text, "安全校验失败", MessageBoxButton.OK, MessageBoxImage.Error);
                Close();
            }));
        }

        private void ReaderWindow_Closed(object? sender, EventArgs e)
        {
            _locked = true;
            _housekeepingTimer.Stop();
            CameraOverlay.Stop();
            PageImage.Source = null;
            _renderer.Dispose();
            _document.Dispose();
        }
    }
}
```

## `src/SecurePdf.Reader/Views/SecurePrintDialog.xaml`

**模块说明**：受控物理打印机选择与页码范围配置弹窗界面（共 `43` 行）

```xml
<Window x:Class="SecurePdf.Reader.Views.SecurePrintDialog"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="安全打印（已禁用虚拟 PDF 打印机）"
        Height="450" Width="500"
        WindowStartupLocation="CenterOwner"
        ResizeMode="NoResize"
        Background="#0F172A">
    <Border Margin="18" Padding="20" Background="#1E293B" CornerRadius="8" BorderBrush="#334155" BorderThickness="1">
        <StackPanel>
            <TextBlock Text="受控物理打印" FontSize="18" FontWeight="Bold" Foreground="#F8FAFC" />
            <TextBlock x:Name="QuotaBannerText" Text="剩余打印次数: --" FontSize="13" Foreground="#38BDF8" Margin="0,6,0,14" />

            <TextBlock Text="选择已验证的物理打印机" FontSize="12" Foreground="#CBD5E1" Margin="0,0,0,6" />
            <ComboBox x:Name="PrinterCombo" Height="34" FontSize="13" Padding="8,4" DisplayMemberPath="Display" />
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

            <TextBlock Text="提示：每次提交打印任务将扣减 1 次本地额度（一页都没送出就失败时自动退还），打印纸张会自动嵌入实时时间、IP 与设备水印。"
                       FontSize="11" Foreground="#FBBF24" TextWrapping="Wrap" Margin="0,0,0,16" />

            <StackPanel Orientation="Horizontal" HorizontalAlignment="Right">
                <Button x:Name="CancelBtn" Content="取消" Width="90" Background="#334155" Foreground="#F8FAFC"
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

**模块说明**：展示过滤后的物理打印机列表与剩余配额/有效期，打印期间禁用关闭并区分 `SecurePrintException` 配额回滚提示与打印错误（共 `228` 行）

```csharp
using System;
using System.Collections.Generic;
using System.ComponentModel;
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
        private sealed class PrinterItem
        {
            public PrinterItem(string name, string display)
            {
                Name = name;
                Display = display;
            }

            public string Name { get; }
            public string Display { get; }
        }

        private const string ConfirmButtonText = "确认打印 (消耗 1 次)";

        private readonly MemoryPdfRenderer _renderer;
        private readonly SpdfPolicy _policy;
        private readonly LocalQuotaStore _quotaStore;
        private readonly RuntimeTraceContext _traceContext;
        private readonly int _currentPageIndex;
        private bool _printing;

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
            LoadPhysicalPrinters(null);
        }

        private void UpdateQuotaBannerText()
        {
            string expiryHint = _policy.ExpiresAtUtc.HasValue
                ? $"（有效期至 {_policy.ExpiresAtUtc.Value.ToLocalTime():yyyy-MM-dd HH:mm}）"
                : string.Empty;
            QuotaBannerText.Text = $"当前剩余打印次数: {LatestSnapshot.RemainingPrints} / {LatestSnapshot.MaxPrints}{expiryHint}";
        }

        private void UpdateConfirmButton()
        {
            ConfirmPrintBtn.IsEnabled = !_printing && LatestSnapshot.RemainingPrints > 0 && PrinterCombo.Items.Count > 0;
        }

        /// <summary>列出通过校验的物理打印机；优先选中 <paramref name="preferredPrinter"/>，其次是系统默认打印机。</summary>
        private void LoadPhysicalPrinters(string? preferredPrinter)
        {
            PrinterScanSummary scan = PrinterSecurityGuard.ScanPrinters();

            List<PrinterItem> items = scan.AllowedPrinters
                .Select(p => new PrinterItem(p.PrinterName, p.IsOffline ? p.PrinterName + "（脱机）" : p.PrinterName))
                .ToList();
            PrinterCombo.ItemsSource = items;

            if (items.Count > 0)
            {
                int index = items.FindIndex(i => string.Equals(i.Name, preferredPrinter, StringComparison.OrdinalIgnoreCase));
                if (index < 0) index = items.FindIndex(i => string.Equals(i.Name, scan.DefaultPrinterName, StringComparison.OrdinalIgnoreCase));
                PrinterCombo.SelectedIndex = Math.Max(0, index);
            }
            UpdateConfirmButton();

            BlockedSummaryText.ToolTip = null;
            if (scan.ErrorMessage.Length > 0)
            {
                BlockedSummaryText.Text = scan.ErrorMessage;
            }
            else if (scan.BlockedPrinters.Count > 0)
            {
                string blockedNames = string.Join("、", scan.BlockedPrinters.Take(3).Select(b => b.Candidate.PrinterName));
                string extra = scan.BlockedPrinters.Count > 3 ? $" 等共 {scan.BlockedPrinters.Count} 台" : $" 共 {scan.BlockedPrinters.Count} 台";
                BlockedSummaryText.Text = items.Count == 0
                    ? $"未检测到可用的物理打印机。已拦截：{blockedNames}{extra}（鼠标悬停查看原因）。"
                    : $"已屏蔽不符合要求的打印机：{blockedNames}{extra}（鼠标悬停查看原因）。";
                BlockedSummaryText.ToolTip = string.Join(Environment.NewLine,
                    scan.BlockedPrinters.Select(b => $"{b.Candidate.PrinterName}：{b.Reason}"));
            }
            else
            {
                BlockedSummaryText.Text = items.Count == 0 ? "未检测到可用的本地或网络物理打印机。" : string.Empty;
            }
        }

        private void CustomRangeBox_GotFocus(object sender, RoutedEventArgs e) => CustomPagesRadio.IsChecked = true;
        private void CancelBtn_Click(object sender, RoutedEventArgs e) => Close();

        /// <summary>打印进行中不允许关闭对话框：后台线程还在往打印机送页，结果（及扣减后的次数）必须交还给阅读窗口。</summary>
        protected override void OnClosing(CancelEventArgs e)
        {
            if (_printing) e.Cancel = true;
            base.OnClosing(e);
        }

        private void SetPrintingState(bool printing)
        {
            _printing = printing;
            CancelBtn.IsEnabled = !printing;
            PrinterCombo.IsEnabled = !printing;
            ConfirmPrintBtn.Content = printing ? "正在安全打印..." : ConfirmButtonText;
            UpdateConfirmButton();
        }

        private async void ConfirmPrintBtn_Click(object sender, RoutedEventArgs e)
        {
            if (_printing) return;

            string? selectedPrinter = (PrinterCombo.SelectedItem as PrinterItem)?.Name;
            if (string.IsNullOrWhiteSpace(selectedPrinter))
            {
                MessageBox.Show(this, "请先选择一台物理打印机。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            List<int> pageIndices;
            try { pageIndices = ResolvePageIndices(); }
            catch (ArgumentException ex)
            {
                MessageBox.Show(this, ex.Message, "页码格式错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            string stamp = $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} | IP:{_traceContext.LocalIpAddress} | {_traceContext.MachineCode}";
            string printWatermark = string.IsNullOrWhiteSpace(_policy.WatermarkNote) ? stamp : $"{_policy.WatermarkNote} | {stamp}";

            SetPrintingState(true);
            try
            {
                QuotaSnapshot updated = await Task.Run(() => PrinterSecurityGuard.PrintDocumentSecurely(
                    _renderer, selectedPrinter!, pageIndices, printWatermark,
                    _quotaStore, _policy.DocId, _policy.MaxPrints, _policy.ExpiresAtUtc));

                LatestSnapshot = updated;
                SetPrintingState(false);
                UpdateQuotaBannerText();
                MessageBox.Show(this, $"打印任务已提交至物理打印机（共 {pageIndices.Count} 页）。\n剩余打印次数：{updated.RemainingPrints} / {updated.MaxPrints}", "打印完成", MessageBoxButton.OK, MessageBoxImage.Information);
                Close();
            }
            catch (Exception ex)
            {
                // SecurePrintException：打印阶段失败，消息里已说明本次是否扣减了次数；
                // 其余（额度不足 / 已过期 / 授权状态读写失败）发生在预占额度之前，没有扣减
                SetPrintingState(false);
                RefreshSnapshot();
                LoadPhysicalPrinters(selectedPrinter);
                MessageBox.Show(this, ex.Message, ex is SecurePrintException ? "打印失败" : "打印被拒绝", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private void RefreshSnapshot()
        {
            try
            {
                LatestSnapshot = _quotaStore.LoadAndVerify(_policy.DocId, _policy.MaxPrints, _policy.ExpiresAtUtc).Snapshot;
            }
            catch (QuotaStoreException) { }     // 读不到就保留手里的旧值；下次操作时阅读窗口会再校验
            UpdateQuotaBannerText();
            UpdateConfirmButton();
        }

        private List<int> ResolvePageIndices()
        {
            if (AllPagesRadio.IsChecked == true) return Enumerable.Range(0, _renderer.PageCount).ToList();
            if (CurrentPageRadio.IsChecked == true) return new List<int> { _currentPageIndex };

            string spec = CustomRangeBox.Text.Trim();
            if (string.IsNullOrWhiteSpace(spec)) throw new ArgumentException("请输入自定义页码范围，例如 1-3,5");

            var pages = new SortedSet<int>();
            foreach (string token in spec.Split(new[] { ',', '，', ';', '；', ' ' }, StringSplitOptions.RemoveEmptyEntries))
            {
                string t = token.Trim().Replace('－', '-').Replace('—', '-').Replace('~', '-');
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
