# 03 — 发送端应用 (`SecurePdf.Creator`)

> [!NOTE]
> 本文档包含发送端/创建端 WPF 应用 `SecurePdf.Creator` 的全部源码（v1.1.0）。
> v1.1.0 新增 Vista+ `IFileOpenDialog` 目录选择器、未保存验证码关窗提醒、非覆盖安全命名 (`_1`/`_2`)、CSV 导出公式注入中和、取证预览降采样护栏与自定义扁平深色按钮模板。

## 目录

- [`src/SecurePdf.Creator/SecurePdf.Creator.csproj`](#srcsecurepdfcreatorsecurepdfcreatorcsproj)
- [`src/SecurePdf.Creator/App.xaml`](#srcsecurepdfcreatorappxaml)
- [`src/SecurePdf.Creator/App.xaml.cs`](#srcsecurepdfcreatorappxamlcs)
- [`src/SecurePdf.Creator/FolderPicker.cs`](#srcsecurepdfcreatorfolderpickercs)
- [`src/SecurePdf.Creator/MainWindow.xaml`](#srcsecurepdfcreatormainwindowxaml)
- [`src/SecurePdf.Creator/MainWindow.xaml.cs`](#srcsecurepdfcreatormainwindowxamlcs)

## `src/SecurePdf.Creator/SecurePdf.Creator.csproj`

**模块说明**：发送端 WPF 项目配置（仅依赖 `SecurePdf.Core`，不打包 `pdfium.dll` 以维持极小发布体积）（共 `22` 行）

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>WinExe</OutputType>
    <TargetFramework>net48</TargetFramework>
    <UseWPF>true</UseWPF>
    <AssemblyName>SecurePdfCreator</AssemblyName>
    <RootNamespace>SecurePdf.Creator</RootNamespace>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
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

## `src/SecurePdf.Creator/App.xaml`

**模块说明**：发送端暗色主题资源与自定义扁平化 `Button` 控件模板定义（共 `39` 行）

```xml
<Application x:Class="SecurePdf.Creator.App"
             xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
             xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
             StartupUri="MainWindow.xaml">
    <Application.Resources>
        <Style TargetType="Button">
            <Setter Property="Padding" Value="12,6" />
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

## `src/SecurePdf.Creator/App.xaml.cs`

**模块说明**：发送端 WPF 入口与 `DispatcherUnhandledException` 全局未处理异常捕获（共 `21` 行）

```csharp
using System.Windows;
using System.Windows.Threading;

namespace SecurePdf.Creator
{
    public partial class App : Application
    {
        protected override void OnStartup(StartupEventArgs e)
        {
            // 创建端崩溃的代价是丢掉表格里尚未导出的验证码，所以遇到未处理的界面异常时提示后继续运行
            DispatcherUnhandledException += OnDispatcherUnhandledException;
            base.OnStartup(e);
        }

        private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
        {
            e.Handled = true;
            MessageBox.Show("操作未能完成：" + e.Exception.Message, "SecurePdf Creator", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
}
```

## `src/SecurePdf.Creator/FolderPicker.cs`

**模块说明**：基于 Windows Vista+ `IFileOpenDialog` (`FOS_PICKFOLDERS`) COM 接口封装的现代目录选择对话框（共 `100` 行）

```csharp
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace SecurePdf.Creator
{
    /// <summary>
    /// 系统自带的“选择文件夹”对话框（IFileOpenDialog + FOS_PICKFOLDERS）。
    /// WPF 在 .NET Framework 下没有文件夹选择框；原版用“另存为”对话框凑合，需要用户随便填个文件名才能选目录。
    /// </summary>
    internal static class FolderPicker
    {
        private const uint FOS_PICKFOLDERS = 0x00000020;
        private const uint FOS_FORCEFILESYSTEM = 0x00000040;
        private const uint FOS_PATHMUSTEXIST = 0x00000800;
        private const uint SIGDN_FILESYSPATH = 0x80058000;
        private const int ERROR_CANCELLED = unchecked((int)0x800704C7);

        /// <summary>返回选中的文件夹完整路径；用户取消时返回 null。</summary>
        public static string? Show(Window owner, string title, string? initialDirectory)
        {
            var dialog = (IFileOpenDialog)new FileOpenDialogCoClass();
            try
            {
                dialog.GetOptions(out uint options);
                dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST);
                dialog.SetTitle(title);

                if (!string.IsNullOrWhiteSpace(initialDirectory) && Directory.Exists(initialDirectory))
                {
                    Guid shellItemId = typeof(IShellItem).GUID;
                    if (SHCreateItemFromParsingName(Path.GetFullPath(initialDirectory), IntPtr.Zero, ref shellItemId, out IShellItem? folder) == 0 && folder != null)
                    {
                        dialog.SetFolder(folder);
                        Marshal.ReleaseComObject(folder);
                    }
                }

                int hr = dialog.Show(new WindowInteropHelper(owner).Handle);
                if (hr == ERROR_CANCELLED) return null;
                if (hr != 0) Marshal.ThrowExceptionForHR(hr);

                dialog.GetResult(out IShellItem result);
                try
                {
                    result.GetDisplayName(SIGDN_FILESYSPATH, out string path);
                    return path;
                }
                finally
                {
                    Marshal.ReleaseComObject(result);
                }
            }
            finally
            {
                Marshal.ReleaseComObject(dialog);
            }
        }

        [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
        private static extern int SHCreateItemFromParsingName(string pszPath, IntPtr pbc, ref Guid riid, out IShellItem? ppv);

        [ComImport, Guid("DC1C5A9C-E88A-4DDE-A5A1-60F82A20AEF7")]
        private class FileOpenDialogCoClass { }

        // 只声明到实际用到的最后一个方法（GetResult）；之前的方法必须按虚表顺序逐个列出
        [ComImport, Guid("42F85136-DB7E-439C-85F1-E4075D135FC8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IFileOpenDialog
        {
            [PreserveSig] int Show(IntPtr hwndOwner);
            void SetFileTypes(uint cFileTypes, IntPtr rgFilterSpec);
            void SetFileTypeIndex(uint iFileType);
            void GetFileTypeIndex(out uint piFileType);
            void Advise(IntPtr pfde, out uint pdwCookie);
            void Unadvise(uint dwCookie);
            void SetOptions(uint fos);
            void GetOptions(out uint pfos);
            void SetDefaultFolder(IShellItem psi);
            void SetFolder(IShellItem psi);
            void GetFolder(out IShellItem ppsi);
            void GetCurrentSelection(out IShellItem ppsi);
            void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string pszName);
            void GetFileName([MarshalAs(UnmanagedType.LPWStr)] out string pszName);
            void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string pszTitle);
            void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string pszText);
            void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string pszLabel);
            void GetResult(out IShellItem ppsi);
        }

        [ComImport, Guid("43826D1E-E718-42EE-BC55-A1E261C37BFE"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IShellItem
        {
            void BindToHandler(IntPtr pbc, ref Guid bhid, ref Guid riid, out IntPtr ppv);
            void GetParent(out IShellItem ppsi);
            void GetDisplayName(uint sigdnName, [MarshalAs(UnmanagedType.LPWStr)] out string ppszName);
        }
    }
}
```

## `src/SecurePdf.Creator/MainWindow.xaml`

**模块说明**：创建端主界面 XAML：PDF 列表、策略参数区（打印次数 / 小数小时有效期 / 摩尔纹与隐形水印强度）、生成结果表格与翻拍取证对比面板（共 `206` 行）

```xml
<Window x:Class="SecurePdf.Creator.MainWindow"
        xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="SecurePdf Creator — 安全 PDF 创建与水印取证端"
        Height="760" Width="1080"
        MinHeight="640" MinWidth="920"
        WindowStartupLocation="CenterScreen"
        Background="#0F172A">
    <Grid Margin="16">
        <TabControl Background="#1E293B" BorderBrush="#334155">
            <TabItem Header="  创建加密 PDF 与自动生码  " FontSize="13">
                <Grid Margin="16">
                    <Grid.RowDefinitions>
                        <RowDefinition Height="Auto" />
                        <RowDefinition Height="Auto" />
                        <RowDefinition Height="*" />
                        <RowDefinition Height="Auto" />
                    </Grid.RowDefinitions>

                    <Border Grid.Row="0" Background="#0F172A" CornerRadius="8" Padding="14" BorderBrush="#334155" BorderThickness="1"
                            AllowDrop="True" Drop="PdfDropZone_Drop" DragOver="PdfDropZone_DragOver">
                        <Grid>
                            <Grid.RowDefinitions>
                                <RowDefinition Height="Auto" />
                                <RowDefinition Height="115" />
                            </Grid.RowDefinitions>
                            <DockPanel LastChildFill="False" Margin="0,0,0,10">
                                <TextBlock Text="待加密的原始 PDF 文件列表（支持拖入多个文件）"
                                           Foreground="#F8FAFC" FontWeight="SemiBold" VerticalAlignment="Center" />
                                <StackPanel DockPanel.Dock="Right" Orientation="Horizontal">
                                    <Button Content="+ 添加 PDF 文件" Background="#0284C7" Foreground="White"
                                            BorderThickness="0" Click="AddPdfFiles_Click" Margin="0,0,8,0" />
                                    <Button Content="移除选中" Background="#334155" Foreground="#E2E8F0"
                                            BorderThickness="0" Click="RemoveSelectedPdf_Click" Margin="0,0,8,0" />
                                    <Button Content="清空列表" Background="#334155" Foreground="#E2E8F0"
                                            BorderThickness="0" Click="ClearPdfList_Click" />
                                </StackPanel>
                            </DockPanel>

                            <ListBox x:Name="SourcePdfList" Grid.Row="1" Background="#1E293B" Foreground="#E2E8F0"
                                     BorderBrush="#334155" SelectionMode="Extended" />
                        </Grid>
                    </Border>

                    <Border Grid.Row="1" Background="#0F172A" CornerRadius="8" Padding="14" Margin="0,12,0,12"
                            BorderBrush="#334155" BorderThickness="1">
                        <Grid>
                            <Grid.RowDefinitions>
                                <RowDefinition Height="Auto" />
                                <RowDefinition Height="Auto" />
                                <RowDefinition Height="Auto" />
                            </Grid.RowDefinitions>
                            <Grid.ColumnDefinitions>
                                <ColumnDefinition Width="*" />
                                <ColumnDefinition Width="*" />
                                <ColumnDefinition Width="*" />
                            </Grid.ColumnDefinitions>

                            <StackPanel Grid.Row="0" Grid.Column="0" Margin="0,0,12,10">
                                <TextBlock Text="允许打印次数（0 = 完全禁止打印）" Foreground="#CBD5E1" FontSize="12" Margin="0,0,0,4" />
                                <TextBox x:Name="MaxPrintsBox" Text="1" Padding="8,6" Background="#1E293B" Foreground="#38BDF8"
                                         BorderBrush="#475569" FontSize="13" />
                            </StackPanel>

                            <StackPanel Grid.Row="0" Grid.Column="1" Margin="0,0,12,10">
                                <TextBlock Text="文档/打印有效期（按小时）" Foreground="#CBD5E1" FontSize="12" Margin="0,0,0,4" />
                                <Grid>
                                    <Grid.ColumnDefinitions>
                                        <ColumnDefinition Width="*" />
                                        <ColumnDefinition Width="75" />
                                    </Grid.ColumnDefinitions>
                                    <ComboBox x:Name="ExpiryCombo" SelectedIndex="0" Height="32" SelectionChanged="ExpiryCombo_SelectionChanged">
                                        <ComboBoxItem Content="永久有效" Tag="0" />
                                        <ComboBoxItem Content="1 小时内有效" Tag="1" />
                                        <ComboBoxItem Content="6 小时内有效" Tag="6" />
                                        <ComboBoxItem Content="24 小时内有效（1天）" Tag="24" />
                                        <ComboBoxItem Content="72 小时内有效（3天）" Tag="72" />
                                        <ComboBoxItem Content="自定义小时" Tag="-1" />
                                    </ComboBox>
                                    <TextBox x:Name="CustomHoursBox" Grid.Column="1" Text="12" Margin="6,0,0,0" Padding="6,5"
                                             Background="#1E293B" Foreground="#F8FAFC" BorderBrush="#475569"
                                             IsEnabled="False" ToolTip="输入有效小时数（支持小数，如 0.5 表示 30 分钟）" />
                                </Grid>
                            </StackPanel>

                            <StackPanel Grid.Row="0" Grid.Column="2" Margin="0,0,0,10">
                                <TextBlock Text="防手机翻拍摩尔纹强度" Foreground="#CBD5E1" FontSize="12" Margin="0,0,0,4" />
                                <ComboBox x:Name="MoireCombo" SelectedIndex="2" Height="32">
                                    <ComboBoxItem Content="关闭" Tag="0" />
                                    <ComboBoxItem Content="低（最清晰）" Tag="1" />
                                    <ComboBoxItem Content="中（推荐平衡）" Tag="2" />
                                    <ComboBoxItem Content="高（强干扰）" Tag="3" />
                                </ComboBox>
                            </StackPanel>

                            <StackPanel Grid.Row="1" Grid.Column="0" Grid.ColumnSpan="2" Margin="0,0,12,10">
                                <TextBlock Text="输出文件夹（留空则输出到每个原文件所在目录）" Foreground="#CBD5E1" FontSize="12" Margin="0,0,0,4" />
                                <Grid>
                                    <Grid.ColumnDefinitions>
                                        <ColumnDefinition Width="*" />
                                        <ColumnDefinition Width="Auto" />
                                    </Grid.ColumnDefinitions>
                                    <TextBox x:Name="OutputDirBox" Padding="8,6" Background="#1E293B" Foreground="#E2E8F0" BorderBrush="#475569" />
                                    <Button Grid.Column="1" Content="选择目录..." Margin="6,0,0,0" Background="#334155"
                                            Foreground="#F8FAFC" BorderThickness="0" Click="ChooseOutputDir_Click" />
                                </Grid>
                            </StackPanel>

                            <StackPanel Grid.Row="1" Grid.Column="2" Margin="0,0,0,10">
                                <TextBlock Text="水印附带发送方备注（可选）" Foreground="#CBD5E1" FontSize="12" Margin="0,0,0,4" />
                                <TextBox x:Name="WatermarkNoteBox" Padding="8,6" Background="#1E293B" Foreground="#F8FAFC"
                                         BorderBrush="#475569" ToolTip="将与接收方IP及实时时间一起嵌入水印" />
                            </StackPanel>

                            <WrapPanel Grid.Row="2" Grid.Column="0" Grid.ColumnSpan="3" Orientation="Horizontal" Margin="0,2,0,0">
                                <CheckBox x:Name="HiddenWatermarkChk" Content="启用隐形防传播水印（实时时间 + IP + 设备码）"
                                          IsChecked="True" Foreground="#E2E8F0" Margin="0,0,18,4" />
                                <CheckBox x:Name="BlurOnUnfocusChk" Content="窗口失焦自动隐藏遮罩"
                                          IsChecked="True" Foreground="#E2E8F0" Margin="0,0,18,4" />
                                <CheckBox x:Name="VisibleWatermarkChk" Content="同时叠加半透明显性水印"
                                          IsChecked="False" Foreground="#E2E8F0" Margin="0,0,18,4" />
                                <CheckBox x:Name="AllowAdjustMoireChk" Content="允许接收方微调摩尔纹强度"
                                          IsChecked="False" Foreground="#E2E8F0" Margin="0,0,0,4" />
                            </WrapPanel>
                        </Grid>
                    </Border>

                    <DataGrid x:Name="ResultsGrid" Grid.Row="2" AutoGenerateColumns="False" IsReadOnly="True"
                              Background="#0F172A" Foreground="#0F172A" BorderBrush="#334155"
                              RowBackground="#F8FAFC" AlternatingRowBackground="#E2E8F0"
                              HeadersVisibility="Column" CanUserAddRows="False">
                        <DataGrid.Columns>
                            <DataGridTextColumn Header="原 PDF 文件" Binding="{Binding SourceFileName}" Width="180" />
                            <DataGridTextColumn Header="自动生成验证码" Binding="{Binding VerificationCode}" Width="165" FontWeight="Bold" />
                            <DataGridTextColumn Header="允许打印" Binding="{Binding MaxPrintsText}" Width="90" />
                            <DataGridTextColumn Header="有效期" Binding="{Binding ExpiryText}" Width="130" />
                            <DataGridTextColumn Header="加密输出路径 (.spdf)" Binding="{Binding OutputPath}" Width="*" />
                            <DataGridTextColumn Header="状态" Binding="{Binding Status}" Width="95" />
                        </DataGrid.Columns>
                    </DataGrid>

                    <DockPanel Grid.Row="3" Margin="0,12,0,0" LastChildFill="False">
                        <Button x:Name="GenerateBtn" Content="一键加密并自动生成对应验证码" Height="38"
                                Background="#059669" Foreground="White" FontWeight="Bold" FontSize="14"
                                BorderThickness="0" Click="GenerateBtn_Click" />
                        <TextBlock x:Name="StatusText" Text="就绪" Foreground="#94A3B8" VerticalAlignment="Center" Margin="14,0,0,0" />

                        <StackPanel DockPanel.Dock="Right" Orientation="Horizontal">
                            <Button Content="复制选中验证码" Background="#334155" Foreground="#F8FAFC"
                                    BorderThickness="0" Margin="0,0,8,0" Click="CopySelectedCode_Click" />
                            <Button Content="复制全部清单" Background="#334155" Foreground="#F8FAFC"
                                    BorderThickness="0" Margin="0,0,8,0" Click="CopyAllCodes_Click" />
                            <Button Content="导出清单 (.csv)" Background="#0284C7" Foreground="White"
                                    BorderThickness="0" Click="ExportCsv_Click" />
                        </StackPanel>
                    </DockPanel>
                </Grid>
            </TabItem>

            <TabItem Header="  隐形水印取证增强工具  " FontSize="13">
                <Grid Margin="16">
                    <Grid.RowDefinitions>
                        <RowDefinition Height="Auto" />
                        <RowDefinition Height="*" />
                    </Grid.RowDefinitions>

                    <DockPanel Grid.Row="0" Margin="0,0,0,12" LastChildFill="False">
                        <Button x:Name="LoadForensicBtn" Content="选择疑似泄密图片 / 翻拍照片..." Background="#0284C7" Foreground="White"
                                BorderThickness="0" Click="LoadForensicImage_Click" />
                        <TextBlock Text="增强算法:" Foreground="#CBD5E1" VerticalAlignment="Center" Margin="16,0,8,0" />
                        <ComboBox x:Name="ForensicModeCombo" Width="260" SelectedIndex="0" SelectionChanged="ForensicModeCombo_SelectionChanged">
                            <ComboBoxItem Content="色度隐形水印提取（专治手机翻拍）" Tag="0" />
                            <ComboBoxItem Content="自适应背景消除 + 对比增强" Tag="1" />
                            <ComboBoxItem Content="全局直方图均衡 + 反相显影" Tag="2" />
                        </ComboBox>
                        <Button x:Name="SaveForensicBtn" DockPanel.Dock="Right" Content="保存增强取证图" Background="#059669" Foreground="White"
                                BorderThickness="0" Click="SaveForensicImage_Click" />
                        <TextBlock x:Name="ForensicStatusText" Text="" Foreground="#94A3B8" VerticalAlignment="Center" Margin="14,0,0,0" />
                    </DockPanel>

                    <Grid Grid.Row="1">
                        <Grid.ColumnDefinitions>
                            <ColumnDefinition Width="*" />
                            <ColumnDefinition Width="12" />
                            <ColumnDefinition Width="*" />
                        </Grid.ColumnDefinitions>

                        <Border Grid.Column="0" Background="#0F172A" BorderBrush="#334155" BorderThickness="1" CornerRadius="6" Padding="8">
                            <DockPanel>
                                <TextBlock DockPanel.Dock="Top" Text="原始图片（整图自适应预览）" Foreground="#94A3B8" Margin="0,0,0,6" />
                                <Image x:Name="ForensicSourceImage" Stretch="Uniform" />
                            </DockPanel>
                        </Border>

                        <Border Grid.Column="2" Background="#0F172A" BorderBrush="#334155" BorderThickness="1" CornerRadius="6" Padding="8">
                            <DockPanel>
                                <TextBlock DockPanel.Dock="Top" Text="增强显影结果（可还原隐藏 IP / 实时时间 / 设备码）" Foreground="#38BDF8" Margin="0,0,0,6" />
                                <Image x:Name="ForensicOutputImage" Stretch="Uniform" />
                            </DockPanel>
                        </Border>
                    </Grid>
                </Grid>
            </TabItem>
        </TabControl>
    </Grid>
</Window>
```

## `src/SecurePdf.Creator/MainWindow.xaml.cs`

**模块说明**：批量加密、小数小时校验、`NextFreeOutputPath` 非覆盖重命名、CSV 公式注入中和、未保存验证码关窗确认、异步取证增强与 `PreviewMaxSide = 3000` 预览保护（共 `558` 行）

```csharp
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Drawing;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Win32;
using SecurePdf.Core.Forensics;
using SecurePdf.Core.Format;

namespace SecurePdf.Creator
{
    public sealed class GeneratedPdfRecord
    {
        public string SourceFileName { get; set; } = string.Empty;
        public string VerificationCode { get; set; } = string.Empty;
        public string MaxPrintsText { get; set; } = string.Empty;
        public string ExpiryText { get; set; } = string.Empty;
        public string OutputPath { get; set; } = string.Empty;
        public string Status { get; set; } = string.Empty;
        public bool Succeeded { get; set; }
    }

    public partial class MainWindow : Window
    {
        /// <summary>取证预览图的最长边。预览只用于界面显示，保存的取证图仍是完整分辨率。</summary>
        private const int PreviewMaxSide = 3000;

        private readonly ObservableCollection<string> _queuedPdfPaths = new ObservableCollection<string>();
        private readonly ObservableCollection<GeneratedPdfRecord> _records = new ObservableCollection<GeneratedPdfRecord>();

        private bool _generating;
        /// <summary>表格里有尚未导出 / 复制的验证码。验证码只存在于这张表里，关掉窗口就再也找不回来。</summary>
        private bool _hasUnsavedCodes;

        private Bitmap? _forensicSourceBitmap;
        private Bitmap? _forensicOutputBitmap;
        private bool _forensicBusy;

        public MainWindow()
        {
            InitializeComponent();
            SourcePdfList.ItemsSource = _queuedPdfPaths;
            ResultsGrid.ItemsSource = _records;
            Closed += (_, __) =>
            {
                _forensicSourceBitmap?.Dispose();
                _forensicOutputBitmap?.Dispose();
            };
        }

        protected override void OnClosing(CancelEventArgs e)
        {
            if (_generating)
            {
                MessageBox.Show(this, "正在加密文档，请等待完成后再关闭。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                e.Cancel = true;
            }
            else if (_hasUnsavedCodes)
            {
                MessageBoxResult answer = MessageBox.Show(this,
                    "表格中的验证码还没有导出或复制。\n验证码只显示在这里，关闭后无法找回，对应的 .spdf 文件将无法打开。\n\n确定要关闭吗？",
                    "验证码尚未保存", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);
                if (answer != MessageBoxResult.Yes) e.Cancel = true;
            }
            base.OnClosing(e);
        }

        // ------------------------------------------------------------------ 待加密列表

        private void AddPdfFiles_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new OpenFileDialog
            {
                Filter = "PDF 文档 (*.pdf)|*.pdf",
                Multiselect = true,
                Title = "选择一个或多个待加密 PDF 文件"
            };
            if (dlg.ShowDialog(this) == true)
            {
                foreach (string file in dlg.FileNames) AddSourceFile(file);
            }
        }

        private void PdfDropZone_DragOver(object sender, DragEventArgs e)
        {
            e.Effects = e.Data.GetDataPresent(DataFormats.FileDrop) ? DragDropEffects.Copy : DragDropEffects.None;
            e.Handled = true;
        }

        private void PdfDropZone_Drop(object sender, DragEventArgs e)
        {
            if (e.Data.GetData(DataFormats.FileDrop) is string[] files)
            {
                foreach (string file in files)
                {
                    if (file.EndsWith(".pdf", StringComparison.OrdinalIgnoreCase) && File.Exists(file)) AddSourceFile(file);
                }
            }
        }

        private void AddSourceFile(string path)
        {
            if (!_queuedPdfPaths.Contains(path, StringComparer.OrdinalIgnoreCase)) _queuedPdfPaths.Add(path);
        }

        private void RemoveSelectedPdf_Click(object sender, RoutedEventArgs e)
        {
            foreach (string item in SourcePdfList.SelectedItems.Cast<string>().ToList()) _queuedPdfPaths.Remove(item);
        }

        private void ClearPdfList_Click(object sender, RoutedEventArgs e) => _queuedPdfPaths.Clear();

        private void ExpiryCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (CustomHoursBox == null) return;
            CustomHoursBox.IsEnabled = ExpiryCombo.SelectedItem is ComboBoxItem item && item.Tag?.ToString() == "-1";
        }

        private void ChooseOutputDir_Click(object sender, RoutedEventArgs e)
        {
            try
            {
                string? picked = FolderPicker.Show(this, "选择 .spdf 文件的输出文件夹", OutputDirBox.Text.Trim());
                if (picked != null) OutputDirBox.Text = picked;
            }
            catch (Exception ex) when (ex is COMException || ex is InvalidCastException || ex is ArgumentException || ex is IOException)
            {
                MessageBox.Show(this, "无法打开文件夹选择框，请直接在输入框中填写完整路径。\n" + ex.Message, "提示", MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }

        // ------------------------------------------------------------------ 加密生成

        private async void GenerateBtn_Click(object sender, RoutedEventArgs e)
        {
            if (_generating) return;

            if (_queuedPdfPaths.Count == 0)
            {
                MessageBox.Show(this, "请先添加至少一个 PDF 文件。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }
            if (!int.TryParse(MaxPrintsBox.Text.Trim(), out int maxPrints) || maxPrints < 0)
            {
                MessageBox.Show(this, "允许打印次数必须是非负整数（0 表示禁止打印）。", "参数错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            double hours = 0;
            if (ExpiryCombo.SelectedItem is ComboBoxItem expItem &&
                double.TryParse(expItem.Tag?.ToString(), NumberStyles.Float, CultureInfo.InvariantCulture, out hours) && hours < 0)
            {
                string rawHours = CustomHoursBox.Text.Trim();
                bool parsed = double.TryParse(rawHours, NumberStyles.Float, CultureInfo.InvariantCulture, out hours) ||
                              double.TryParse(rawHours, NumberStyles.Float, CultureInfo.CurrentCulture, out hours);
                if (!parsed || double.IsNaN(hours) || double.IsInfinity(hours) || hours <= 0 || hours > 87600)
                {
                    MessageBox.Show(this, "请输入有效的自定义小时数（大于 0，最多 87600 小时，支持小数如 0.5）。", "参数错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                    return;
                }
            }

            string customOutDir = OutputDirBox.Text.Trim().Trim('"');
            if (customOutDir.Length > 0)
            {
                try
                {
                    // 相对路径会落到“程序当前目录”这个用户看不见的位置，所以只接受完整路径
                    if (!Path.IsPathRooted(customOutDir)) throw new ArgumentException("请填写完整路径，例如 D:\\加密输出。");
                    customOutDir = Path.GetFullPath(customOutDir);
                    Directory.CreateDirectory(customOutDir);
                }
                catch (Exception ex) when (ex is ArgumentException || ex is IOException || ex is UnauthorizedAccessException || ex is NotSupportedException)
                {
                    MessageBox.Show(this, "输出文件夹无效或无法创建：" + ex.Message, "参数错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                    return;
                }
            }

            // 同一批文档共用同一个到期时刻
            DateTimeOffset? expiresAtUtc = hours > 0 ? DateTimeOffset.UtcNow.AddHours(hours) : (DateTimeOffset?)null;
            string expiryLabel = expiresAtUtc.HasValue ? expiresAtUtc.Value.ToLocalTime().ToString("yyyy-MM-dd HH:mm") : "永久有效";

            MoireStrength moire = MoireStrength.Medium;
            if (MoireCombo.SelectedItem is ComboBoxItem mItem && int.TryParse(mItem.Tag?.ToString(), out int mVal)) moire = (MoireStrength)mVal;

            SpdfSecurityFlags flags = SpdfSecurityFlags.None;
            if (HiddenWatermarkChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableHiddenWatermark;
            if (BlurOnUnfocusChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableBlurOnUnfocus;
            if (VisibleWatermarkChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableVisibleWatermark;
            if (AllowAdjustMoireChk.IsChecked == true) flags |= SpdfSecurityFlags.AllowUserAdjustMoire;

            string note = WatermarkNoteBox.Text.Trim();
            List<string> files = _queuedPdfPaths.ToList();
            string maxPrintsText = maxPrints == 0 ? "禁止打印" : $"{maxPrints} 次";

            _generating = true;
            GenerateBtn.IsEnabled = false;
            int succeeded = 0;

            try
            {
                for (int i = 0; i < files.Count; i++)
                {
                    string srcPath = files[i];
                    string fileName = Path.GetFileName(srcPath);
                    StatusText.Text = $"正在加密生成验证码 ({i + 1} / {files.Count}): {fileName}";

                    GeneratedPdfRecord rec = await Task.Run(() =>
                    {
                        var record = new GeneratedPdfRecord
                        {
                            SourceFileName = fileName,
                            VerificationCode = "--",
                            MaxPrintsText = maxPrintsText,
                            ExpiryText = expiryLabel
                        };

                        try
                        {
                            string targetDir = customOutDir.Length > 0
                                ? customOutDir
                                : Path.GetDirectoryName(Path.GetFullPath(srcPath)) ?? throw new IOException("无法确定源文件所在目录。");

                            // 绝不覆盖已有的 .spdf：被覆盖文件的验证码随之作废，而它可能已经发给了别人
                            string outPath = NextFreeOutputPath(targetDir, Path.GetFileNameWithoutExtension(srcPath));
                            record.OutputPath = outPath;

                            byte[] rawBytes = File.ReadAllBytes(srcPath);
                            var opts = new SpdfCreationOptions
                            {
                                MaxPrints = maxPrints,
                                ExpiresAtUtc = expiresAtUtc,
                                MoireLevel = moire,
                                SecurityFlags = flags,
                                OriginalFileName = fileName,
                                WatermarkNote = note
                            };
                            (_, string code) = SpdfContainer.Create(rawBytes, outPath, opts);

                            record.VerificationCode = code;
                            record.Status = "成功";
                            record.Succeeded = true;
                        }
                        catch (Exception ex)
                        {
                            // 单个文件失败（被占用、不是 PDF、磁盘满、内存不足……）记入表格，不影响这一批里的其它文件
                            record.Status = "失败: " + ex.Message;
                        }
                        return record;
                    });

                    _records.Add(rec);
                    ResultsGrid.ScrollIntoView(rec);
                    if (rec.Succeeded)
                    {
                        succeeded++;
                        _hasUnsavedCodes = true;
                    }
                }

                int failed = files.Count - succeeded;
                StatusText.Text = failed == 0
                    ? $"已完成 {succeeded} 份文档加密，每份文档各有独立验证码。请及时导出或复制验证码。"
                    : $"完成：成功 {succeeded} 份，失败 {failed} 份（失败原因见表格“状态”列）。";
            }
            finally
            {
                _generating = false;
                GenerateBtn.IsEnabled = true;
            }
        }

        /// <summary>“名称.spdf”已存在时依次尝试“名称 (2).spdf”“名称 (3).spdf”……</summary>
        private static string NextFreeOutputPath(string directory, string baseName)
        {
            string candidate = Path.Combine(directory, baseName + ".spdf");
            for (int n = 2; File.Exists(candidate) || Directory.Exists(candidate); n++)
            {
                candidate = Path.Combine(directory, $"{baseName} ({n}).spdf");
            }
            return candidate;
        }

        // ------------------------------------------------------------------ 验证码清单

        /// <summary>剪贴板被其它程序（远程桌面、剪贴板管理器）短暂占用时会抛异常，重试几次。</summary>
        private bool TryCopyToClipboard(string text)
        {
            for (int attempt = 0; attempt < 5; attempt++)
            {
                try
                {
                    Clipboard.SetDataObject(text, true);
                    return true;
                }
                catch (ExternalException)
                {
                    System.Threading.Thread.Sleep(60);
                }
            }
            MessageBox.Show(this, "剪贴板正被其它程序占用，复制失败，请稍后重试。", "提示", MessageBoxButton.OK, MessageBoxImage.Warning);
            return false;
        }

        private void CopySelectedCode_Click(object sender, RoutedEventArgs e)
        {
            if (ResultsGrid.SelectedItem is GeneratedPdfRecord rec && rec.Succeeded)
            {
                if (TryCopyToClipboard(rec.VerificationCode))
                {
                    StatusText.Text = $"已复制 {rec.SourceFileName} 的验证码: {rec.VerificationCode}";
                    if (_records.Count(r => r.Succeeded) == 1) _hasUnsavedCodes = false;
                }
            }
            else
            {
                StatusText.Text = "请先在表格中选中一条加密成功的记录。";
            }
        }

        private void CopyAllCodes_Click(object sender, RoutedEventArgs e)
        {
            List<GeneratedPdfRecord> valid = _records.Where(r => r.Succeeded).ToList();
            if (valid.Count == 0)
            {
                StatusText.Text = "当前没有可复制的成功记录。";
                return;
            }

            var sb = new StringBuilder();
            foreach (GeneratedPdfRecord r in valid)
            {
                sb.AppendLine($"文件: {r.SourceFileName} | 验证码: {r.VerificationCode} | 打印上限: {r.MaxPrintsText} | 有效期: {r.ExpiryText} | 路径: {r.OutputPath}");
            }
            if (TryCopyToClipboard(sb.ToString()))
            {
                _hasUnsavedCodes = false;
                StatusText.Text = $"已复制 {valid.Count} 条加密记录到剪贴板。";
            }
        }

        private void ExportCsv_Click(object sender, RoutedEventArgs e)
        {
            List<GeneratedPdfRecord> valid = _records.Where(r => r.Succeeded).ToList();
            if (valid.Count == 0)
            {
                MessageBox.Show(this, "当前没有可导出的成功记录。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            var dlg = new SaveFileDialog
            {
                Filter = "CSV 清单 (*.csv)|*.csv|文本清单 (*.txt)|*.txt",
                FileName = $"验证码清单_{DateTime.Now:yyyyMMdd_HHmmss}.csv"
            };
            if (dlg.ShowDialog(this) != true) return;

            var sb = new StringBuilder();
            sb.AppendLine("原文件名,验证码,打印限制,有效期,输出路径");
            foreach (GeneratedPdfRecord r in valid)
            {
                sb.AppendLine(string.Join(",", new[] { r.SourceFileName, r.VerificationCode, r.MaxPrintsText, r.ExpiryText, r.OutputPath }.Select(CsvField)));
            }

            try
            {
                File.WriteAllText(dlg.FileName, sb.ToString(), new UTF8Encoding(encoderShouldEmitUTF8Identifier: true));
                _hasUnsavedCodes = false;
                StatusText.Text = $"已导出清单: {dlg.FileName}";
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is NotSupportedException)
            {
                MessageBox.Show(this, "清单导出失败：" + ex.Message, "导出失败", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        /// <summary>
        /// CSV 字段：双引号转义；以 = + - @ 开头的内容前面加单引号，防止 Excel 把文件名当成公式执行。
        /// </summary>
        private static string CsvField(string value)
        {
            string v = value ?? string.Empty;
            if (v.Length > 0 && (v[0] == '=' || v[0] == '+' || v[0] == '-' || v[0] == '@' || v[0] == '\t' || v[0] == '\r'))
            {
                v = "'" + v;
            }
            return "\"" + v.Replace("\"", "\"\"") + "\"";
        }

        // ------------------------------------------------------------------ 水印取证

        private async void LoadForensicImage_Click(object sender, RoutedEventArgs e)
        {
            if (_forensicBusy) return;

            var dlg = new OpenFileDialog
            {
                Filter = "图片文件 (*.png;*.jpg;*.jpeg;*.bmp;*.tif;*.tiff)|*.png;*.jpg;*.jpeg;*.bmp;*.tif;*.tiff",
                Title = "选择待取证分析的截图或照片"
            };
            if (dlg.ShowDialog(this) != true) return;

            string path = dlg.FileName;
            await RunForensicAsync(() => WatermarkEnhancer.LoadImageUpright(path), "正在读取图片...");
        }

        private async void ForensicModeCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_forensicSourceBitmap != null && !_forensicBusy) await RunForensicAsync(null, string.Empty);
        }

        /// <summary>
        /// 取证增强在后台线程进行（大照片要算一两秒，原版在界面线程上算，窗口会卡住）。
        /// <paramref name="loadSource"/> 不为空时先读入新图片；读取失败则保留原来的图片与结果。
        /// </summary>
        private async Task RunForensicAsync(Func<Bitmap>? loadSource, string loadingText)
        {
            _forensicBusy = true;
            LoadForensicBtn.IsEnabled = false;
            SaveForensicBtn.IsEnabled = false;
            ForensicModeCombo.IsEnabled = false;

            try
            {
                if (loadSource != null)
                {
                    ForensicStatusText.Text = loadingText;
                    Bitmap loaded = await Task.Run(loadSource);
                    BitmapSource preview = await Task.Run(() => ToPreviewSource(loaded));

                    _forensicSourceBitmap?.Dispose();
                    _forensicSourceBitmap = loaded;
                    ForensicSourceImage.Source = preview;
                }

                // 处理期间下拉框是禁用的，所以这里读到的就是最终选择
                while (true)
                {
                    ForensicFilterMode mode = SelectedForensicMode();
                    Bitmap source = _forensicSourceBitmap!;
                    ForensicStatusText.Text = $"正在增强 {source.Width}×{source.Height} 图片...";

                    Bitmap enhanced = await Task.Run(() => WatermarkEnhancer.Enhance(source, mode));
                    BitmapSource enhancedPreview = await Task.Run(() => ToPreviewSource(enhanced));

                    _forensicOutputBitmap?.Dispose();
                    _forensicOutputBitmap = enhanced;
                    ForensicOutputImage.Source = enhancedPreview;

                    if (SelectedForensicMode() == mode) break;
                }
                ForensicStatusText.Text = $"完成（{_forensicSourceBitmap!.Width}×{_forensicSourceBitmap.Height}）";
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException ||
                                       ex is ExternalException || ex is OutOfMemoryException || ex is NotSupportedException)
            {
                // GDI+ 对“不是图片 / 图片已损坏”抛的是 ArgumentException 或 OutOfMemoryException
                ForensicStatusText.Text = string.Empty;
                string reason = ex is OutOfMemoryException ? "文件不是受支持的图片格式，或图片过大。" : ex.Message;
                MessageBox.Show(this, "无法处理该图片：" + reason, "水印取证", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
            finally
            {
                _forensicBusy = false;
                LoadForensicBtn.IsEnabled = true;
                SaveForensicBtn.IsEnabled = true;
                ForensicModeCombo.IsEnabled = true;
            }
        }

        private ForensicFilterMode SelectedForensicMode()
        {
            return ForensicModeCombo.SelectedItem is ComboBoxItem item && int.TryParse(item.Tag?.ToString(), out int val)
                ? (ForensicFilterMode)val
                : ForensicFilterMode.ChromaStegoExtract;
        }

        private void SaveForensicImage_Click(object sender, RoutedEventArgs e)
        {
            if (_forensicOutputBitmap == null || _forensicBusy)
            {
                MessageBox.Show(this, "请先加载并增强图片。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            var dlg = new SaveFileDialog
            {
                Filter = "PNG 图片 (*.png)|*.png",
                FileName = $"水印取证_{DateTime.Now:yyyyMMdd_HHmmss}.png"
            };
            if (dlg.ShowDialog(this) != true) return;

            try
            {
                _forensicOutputBitmap.Save(dlg.FileName, ImageFormat.Png);
                ForensicStatusText.Text = $"已保存: {dlg.FileName}";
            }
            catch (Exception ex) when (ex is ExternalException || ex is IOException || ex is UnauthorizedAccessException)
            {
                MessageBox.Show(this, "保存失败（目标文件可能被占用或没有写入权限）。\n" + ex.Message, "保存失败", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        /// <summary>
        /// 把 GDI+ 位图转成界面用的预览图：直接读取像素（不经过 HBITMAP），超过 <see cref="PreviewMaxSide"/> 的先缩小。
        /// 返回值已冻结，可以在后台线程生成后交给界面线程。
        /// </summary>
        private static BitmapSource ToPreviewSource(Bitmap bmp)
        {
            System.Windows.Media.PixelFormat wpfFormat;
            System.Drawing.Imaging.PixelFormat gdiFormat;
            if (bmp.PixelFormat == System.Drawing.Imaging.PixelFormat.Format8bppIndexed)
            {
                gdiFormat = System.Drawing.Imaging.PixelFormat.Format8bppIndexed;
                wpfFormat = PixelFormats.Gray8;       // 取证输出使用线性灰度调色板
            }
            else
            {
                gdiFormat = System.Drawing.Imaging.PixelFormat.Format24bppRgb;
                wpfFormat = PixelFormats.Bgr24;
            }

            BitmapSource full;
            BitmapData data = bmp.LockBits(new Rectangle(0, 0, bmp.Width, bmp.Height), ImageLockMode.ReadOnly, gdiFormat);
            try
            {
                full = BitmapSource.Create(bmp.Width, bmp.Height, 96, 96, wpfFormat, null, data.Scan0, data.Stride * bmp.Height, data.Stride);
            }
            finally
            {
                bmp.UnlockBits(data);
            }

            int longSide = Math.Max(bmp.Width, bmp.Height);
            if (longSide > PreviewMaxSide)
            {
                double scale = PreviewMaxSide / (double)longSide;
                var scaled = new TransformedBitmap(full, new ScaleTransform(scale, scale));
                full = new WriteableBitmap(scaled);
            }

            full.Freeze();
            return full;
        }
    }
}
```

---

[返回总览 README](../README.md)
