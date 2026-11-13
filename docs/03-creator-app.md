# 03 — 发送端应用 (`SecurePdf.Creator`)

发送端用于将普通 `.pdf` 批量加密为 `.spdf` 并为每个文档生成唯一验证码，同时集成泄漏照片取证增强界面。

> [!NOTE]
> 有效期已从早期“按天计算”升级为“按小时计算”（内置 1h / 6h / 24h / 72h 预设及支持小数的自定义小时），取证图预览移除局部滚动裁剪以完整自适应显示。

## 目录

1. [`src/SecurePdf.Creator/SecurePdf.Creator.csproj`](#srcsecurepdfcreatorsecurepdfcreatorcsproj) — 发送端 WPF 项目配置（不打包 `pdfium.dll` 以压缩体积）

2. [`src/SecurePdf.Creator/App.xaml`](#srcsecurepdfcreatorappxaml) — 发送端应用资源与暗色主题样式定义

3. [`src/SecurePdf.Creator/App.xaml.cs`](#srcsecurepdfcreatorappxamlcs) — 发送端 WPF 入口代码

4. [`src/SecurePdf.Creator/MainWindow.xaml`](#srcsecurepdfcreatormainwindowxaml) — 创建端主界面 XAML：策略参数区（打印次数 / 按小时有效期 / 摩尔纹强度）、结果清单与取证对比面板

5. [`src/SecurePdf.Creator/MainWindow.xaml.cs`](#srcsecurepdfcreatormainwindowxamlcs) — 批量加密生成逻辑、小数小时校验、CSV 导出以及异步调用 `WatermarkEnhancer` 进行取证增强


---

## `src/SecurePdf.Creator/SecurePdf.Creator.csproj`


**模块说明**：发送端 WPF 项目配置（不打包 `pdfium.dll` 以压缩体积）（共 `22` 行）


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


**模块说明**：发送端应用资源与暗色主题样式定义（共 `15` 行）


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
        </Style>
        <Style TargetType="TextBlock">
            <Setter Property="FontFamily" Value="Microsoft YaHei UI" />
        </Style>
    </Application.Resources>
</Application>
```


## `src/SecurePdf.Creator/App.xaml.cs`


**模块说明**：发送端 WPF 入口代码（共 `8` 行）


```csharp
using System.Windows;

namespace SecurePdf.Creator
{
    public partial class App : Application
    {
    }
}
```


## `src/SecurePdf.Creator/MainWindow.xaml`


**模块说明**：创建端主界面 XAML：策略参数区（打印次数 / 按小时有效期 / 摩尔纹强度）、结果清单与取证对比面板（共 `205` 行）


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
                        <Button Content="选择疑似泄密图片 / 翻拍照片..." Background="#0284C7" Foreground="White"
                                BorderThickness="0" Click="LoadForensicImage_Click" />
                        <TextBlock Text="增强算法:" Foreground="#CBD5E1" VerticalAlignment="Center" Margin="16,0,8,0" />
                        <ComboBox x:Name="ForensicModeCombo" Width="260" SelectedIndex="0" SelectionChanged="ForensicModeCombo_SelectionChanged">
                            <ComboBoxItem Content="色度隐形水印提取（专治手机翻拍）" Tag="0" />
                            <ComboBoxItem Content="自适应背景消除 + 对比增强" Tag="1" />
                            <ComboBoxItem Content="全局直方图均衡 + 反相显影" Tag="2" />
                        </ComboBox>
                        <Button DockPanel.Dock="Right" Content="保存增强取证图" Background="#059669" Foreground="White"
                                BorderThickness="0" Click="SaveForensicImage_Click" />
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


**模块说明**：批量加密生成逻辑、小数小时校验、CSV 导出以及异步调用 `WatermarkEnhancer` 进行取证增强（共 `339` 行）


```csharp
using System;
using System.Collections.ObjectModel;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
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
    }

    public partial class MainWindow : Window
    {
        private readonly ObservableCollection<string> _queuedPdfPaths = new ObservableCollection<string>();
        private readonly ObservableCollection<GeneratedPdfRecord> _records = new ObservableCollection<GeneratedPdfRecord>();

        private Bitmap? _forensicSourceBitmap;
        private Bitmap? _forensicOutputBitmap;

        [DllImport("gdi32.dll")]
        private static extern bool DeleteObject(IntPtr hObject);

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

        private void AddPdfFiles_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new OpenFileDialog
            {
                Filter = "PDF 文档 (*.pdf)|*.pdf",
                Multiselect = true,
                Title = "选择一个或多个待加密 PDF 文件"
            };
            if (dlg.ShowDialog() == true)
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
            var dlg = new SaveFileDialog { Title = "选择输出目录（直接点击保存即可选定当前目录）", FileName = "选择此目录" };
            if (dlg.ShowDialog() == true) OutputDirBox.Text = Path.GetDirectoryName(dlg.FileName) ?? string.Empty;
        }

        private async void GenerateBtn_Click(object sender, RoutedEventArgs e)
        {
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

            DateTimeOffset? expiresAtUtc = null;
            string expiryLabel = "永久有效";
            if (ExpiryCombo.SelectedItem is ComboBoxItem expItem &&
                double.TryParse(expItem.Tag?.ToString(), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out double hours))
            {
                if (hours < 0)
                {
                    string rawHours = CustomHoursBox.Text.Trim();
                    bool parsed = double.TryParse(rawHours, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out hours) ||
                                  double.TryParse(rawHours, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.CurrentCulture, out hours);
                    if (!parsed || double.IsNaN(hours) || double.IsInfinity(hours) || hours <= 0 || hours > 87600)
                    {
                        MessageBox.Show(this, "请输入有效的自定义小时数（大于 0，最多 87600 小时，支持小数如 0.5）。", "参数错误", MessageBoxButton.OK, MessageBoxImage.Warning);
                        return;
                    }
                }
                if (hours > 0)
                {
                    expiresAtUtc = DateTimeOffset.UtcNow.AddHours(hours);
                    expiryLabel = expiresAtUtc.Value.ToLocalTime().ToString("yyyy-MM-dd HH:mm");
                }
            }

            MoireStrength moire = MoireStrength.Medium;
            if (MoireCombo.SelectedItem is ComboBoxItem mItem && int.TryParse(mItem.Tag?.ToString(), out int mVal)) moire = (MoireStrength)mVal;

            SpdfSecurityFlags flags = SpdfSecurityFlags.None;
            if (HiddenWatermarkChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableHiddenWatermark;
            if (BlurOnUnfocusChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableBlurOnUnfocus;
            if (VisibleWatermarkChk.IsChecked == true) flags |= SpdfSecurityFlags.EnableVisibleWatermark;
            if (AllowAdjustMoireChk.IsChecked == true) flags |= SpdfSecurityFlags.AllowUserAdjustMoire;

            string customOutDir = OutputDirBox.Text.Trim();
            string note = WatermarkNoteBox.Text.Trim();
            var files = _queuedPdfPaths.ToList();

            GenerateBtn.IsEnabled = false;
            StatusText.Text = $"正在加密处理 0 / {files.Count}...";

            try
            {
                for (int i = 0; i < files.Count; i++)
                {
                    string srcPath = files[i];
                    StatusText.Text = $"正在加密生成验证码 ({i + 1} / {files.Count}): {Path.GetFileName(srcPath)}";

                    GeneratedPdfRecord rec = await Task.Run(() =>
                    {
                        string fileName = Path.GetFileName(srcPath);
                        string targetDir = string.IsNullOrEmpty(customOutDir)
                            ? (Path.GetDirectoryName(srcPath) ?? Environment.CurrentDirectory)
                            : customOutDir;
                        string outPath = Path.Combine(targetDir, Path.GetFileNameWithoutExtension(srcPath) + ".spdf");

                        try
                        {
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
                            return new GeneratedPdfRecord
                            {
                                SourceFileName = fileName,
                                VerificationCode = code,
                                MaxPrintsText = maxPrints == 0 ? "禁止打印" : $"{maxPrints} 次",
                                ExpiryText = expiryLabel,
                                OutputPath = outPath,
                                Status = "成功"
                            };
                        }
                        catch (Exception ex)
                        {
                            return new GeneratedPdfRecord
                            {
                                SourceFileName = fileName,
                                VerificationCode = "--",
                                MaxPrintsText = maxPrints.ToString(),
                                ExpiryText = expiryLabel,
                                OutputPath = outPath,
                                Status = $"失败: {ex.Message}"
                            };
                        }
                    });
                    _records.Add(rec);
                }

                StatusText.Text = $"已完成 {files.Count} 份文档加密，已自动生成对应独立验证码。";
            }
            finally
            {
                GenerateBtn.IsEnabled = true;
            }
        }

        private void CopySelectedCode_Click(object sender, RoutedEventArgs e)
        {
            if (ResultsGrid.SelectedItem is GeneratedPdfRecord rec && rec.Status == "成功")
            {
                Clipboard.SetText(rec.VerificationCode);
                StatusText.Text = $"已复制 {rec.SourceFileName} 的验证码: {rec.VerificationCode}";
            }
        }

        private void CopyAllCodes_Click(object sender, RoutedEventArgs e)
        {
            var valid = _records.Where(r => r.Status == "成功").ToList();
            if (valid.Count == 0) return;

            var sb = new StringBuilder();
            foreach (var r in valid)
            {
                sb.AppendLine($"文件: {r.SourceFileName} | 验证码: {r.VerificationCode} | 打印上限: {r.MaxPrintsText} | 有效期: {r.ExpiryText} | 路径: {r.OutputPath}");
            }
            Clipboard.SetText(sb.ToString());
            StatusText.Text = $"已复制 {valid.Count} 条加密记录到剪贴板。";
        }

        private void ExportCsv_Click(object sender, RoutedEventArgs e)
        {
            var valid = _records.Where(r => r.Status == "成功").ToList();
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
            if (dlg.ShowDialog() == true)
            {
                var sb = new StringBuilder();
                sb.AppendLine("原文件名,验证码,打印限制,有效期,输出路径");
                foreach (var r in valid)
                {
                    sb.AppendLine($"\"{r.SourceFileName}\",\"{r.VerificationCode}\",\"{r.MaxPrintsText}\",\"{r.ExpiryText}\",\"{r.OutputPath}\"");
                }
                File.WriteAllText(dlg.FileName, sb.ToString(), new UTF8Encoding(encoderShouldEmitUTF8Identifier: true));
                StatusText.Text = $"已导出清单: {dlg.FileName}";
            }
        }

        private void LoadForensicImage_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new OpenFileDialog
            {
                Filter = "图片文件 (*.png;*.jpg;*.jpeg;*.bmp)|*.png;*.jpg;*.jpeg;*.bmp",
                Title = "选择待取证分析的截图或照片"
            };
            if (dlg.ShowDialog() == true)
            {
                _forensicSourceBitmap?.Dispose();
                _forensicSourceBitmap = new Bitmap(dlg.FileName);
                ForensicSourceImage.Source = ToBitmapSource(_forensicSourceBitmap);
                RunForensicEnhance();
            }
        }

        private void ForensicModeCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_forensicSourceBitmap != null) RunForensicEnhance();
        }

        private void RunForensicEnhance()
        {
            if (_forensicSourceBitmap == null) return;

            ForensicFilterMode mode = ForensicFilterMode.ChromaStegoExtract;
            if (ForensicModeCombo.SelectedItem is ComboBoxItem item && int.TryParse(item.Tag?.ToString(), out int val))
            {
                mode = (ForensicFilterMode)val;
            }

            _forensicOutputBitmap?.Dispose();
            _forensicOutputBitmap = WatermarkEnhancer.Enhance(_forensicSourceBitmap, mode);
            ForensicOutputImage.Source = ToBitmapSource(_forensicOutputBitmap);
        }

        private void SaveForensicImage_Click(object sender, RoutedEventArgs e)
        {
            if (_forensicOutputBitmap == null)
            {
                MessageBox.Show(this, "请先加载并增强图片。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            var dlg = new SaveFileDialog
            {
                Filter = "PNG 图片 (*.png)|*.png",
                FileName = $"水印取证_{DateTime.Now:yyyyMMdd_HHmmss}.png"
            };
            if (dlg.ShowDialog() == true) _forensicOutputBitmap.Save(dlg.FileName, ImageFormat.Png);
        }

        private static BitmapSource ToBitmapSource(Bitmap bmp)
        {
            IntPtr hBitmap = bmp.GetHbitmap();
            try
            {
                BitmapSource src = Imaging.CreateBitmapSourceFromHBitmap(hBitmap, IntPtr.Zero, Int32Rect.Empty, BitmapSizeOptions.FromEmptyOptions());
                src.Freeze();
                return src;
            }
            finally
            {
                DeleteObject(hBitmap);
            }
        }
    }
}
```


---

[返回总览 README](../README.md)
