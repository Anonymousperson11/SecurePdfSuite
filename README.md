# SecurePdfSuite 加密 PDF 分发、受控阅读与水印取证套件（v2.0.0）

适用于 **Windows 10 / Windows 11 (x64)** 的原生桌面受控文档套件，基于 **Rust + WinUI 3** 构建。

---

## 一、运行环境与兼容性说明

- **操作系统**：Windows 10（1809+，推荐 22H2）或 Windows 11（64 位 `x64`）。
- **无需 VC++ 运行库**：所有可执行文件均静态链接 MSVC C 运行时（`+crt-static`），干净系统上不会缺少 `VCRUNTIME140.dll` 或 `api-ms-win-crt-*.dll`。
- **需要 Windows App Runtime 1.6 或更高版本（x64）**：
  - 官方下载页：<https://learn.microsoft.com/zh-cn/windows/apps/windows-app-sdk/downloads>
  - **Win10 / Win11 双路径引导（P0-3 方案 A）**：在 Windows 11 上运行期动态解析系统 `TryCreatePackageDependency` / `AddPackageDependency` 接口（无静态 DLL 导入，确保进程在 Windows 10 上可正常加载）；在 Windows 10 上自动回退使用随包分发的 `Microsoft.WindowsAppRuntime.Bootstrap.dll`（`MddBootstrapInitialize2`）。
  - 若系统尚未安装运行库，创建端与阅读端会弹出中文提示并可一键打开官方下载页；交互安装器在缺少运行库时会自动切换为原生 Win32 对话框完成文件部署，随后引导安装运行库。

---

## 二、核心组件与使用流程

### 1. 创建端（`SecurePdfCreator.exe`）

1. **批量加密生成 `.spdf`**
   - 点击 **添加 PDF 文件...** / **添加文件夹...** 或直接将文件/文件夹拖入列表（文件夹在后台线程递归扫描并按规范化路径去重）。
   - 配置文档受控策略：
     - **允许打印次数**：`0`～`1000` 之间的整数（`0` 表示禁止打印）。
     - **有效期**：提供“永久有效 / 1 小时 / 6 小时 / 24 小时 / 72 小时 / 自定义小时数”（支持小数，如 `0.5` 表示 30 分钟，上限 `87600` 小时）。
     - **摩尔纹强度**：关闭 / 低 / 中（推荐）/ 高。
     - **保护选项**：半透明可见水印、隐形溯源水印（时间、IP、设备码）、阅读窗口失焦自动遮挡、允许接收方调节摩尔纹强度（默认不勾选）。
     - **水印备注**：可选中文或英文备注（如 `张三专用`），原样嵌入可见与打印追溯水印。
     - **输出文件夹**：可手输或浏览完整路径；**留空则默认输出到每个源 PDF 所在的文件夹**。同名文件自动递增 `_1`、`_2` 后缀并以临时文件原子落盘，绝不覆盖已有 `.spdf`。
   - 点击 **开始加密并生成验证码**：**每个文件独立由系统密码学随机数生成 12 位验证码（`XXXX-XXXX-XXXX`）**。
   - **多批累加结果表与导出**：结果表跨批次保留（含原文件名、验证码、打印上限、本地时区有效期、状态及输出路径/失败原因），支持 **复制选中验证码**、**复制全部清单** 与 **导出清单 (.csv)**（自动转义表格公式注入字符）。加密进行中禁止关闭窗口；存在未导出/未复制的验证码时关窗会弹出确认框（默认按钮为“返回”）。

2. **水印取证工作台**
   - 导入疑似泄密的屏幕截图或手机翻拍照片（自动识别 EXIF 拍摄方向转正；超过 5000 万像素的超高清照片自动按比例缩小后处理）。
   - **微点矩阵自动解码**：对未缩放的原始比例截图，直接从画面 8 字节微点矩阵中还原出**显示设备 IPv4 地址与画面精确时间**。
   - **隐形水印视觉增强**：内置“色度隐形水印提取（适合手机翻拍）”、“自适应背景消除与对比增强”、“全局直方图均衡与反相”三种算法，加载后自动计算并在切换算法时自动重算，支持将完整分辨率增强结果另存为 PNG。

### 2. 阅读端（`SecurePdfReader.exe`）

1. **验证与内存解密**
   - 双击 `.spdf` 文件（或启动阅读器后浏览/拖入文件），窗口显示本机 12 位设备码并支持一键复制。
   - 输入发送方提供的验证码（不区分大小写，自动忽略空格与连字符；连续输错按次数退避冷却）。通过校验后仅在内存中解密并通过随包分发的无 V8/XFA 精简版 `pdfium.dll` 按当前显示器 DPI 实时渲染，磁盘不留任何明文临时文件。

2. **受控阅读与防截屏锁定（`fail-closed`）**
   - **防截屏保护**：窗口显示前后及后台每 30 秒巡检均调用并核验 `SetWindowDisplayAffinity`，工具栏如实显示当前生效级别（从截图/录屏中排除或显示为黑色）。
   - **失焦幕布与到期清屏**：开启失焦遮挡策略时，窗口失去焦点立即覆盖全不透明（`alpha=255`）幕布；文档到期（同时受系统 UTC 时钟与单调时钟约束，7 天内到期额外启用毫秒级精准定时器）、系统时间异常回拨超限或防截屏保护失效时，立即停止计时器与渲染工作线程、清空页面与水印图像源、释放内存明文，弹出说明后关闭窗口。关闭窗口或发生未预期异常（panic hook）时同样立即清零释放明文。
   - **追溯叠加层与阅读交互**：使用系统字体渲染支持中文备注的深色半透明可见水印、低对比度蓝通道隐形水印与每秒相位跳变的动态摩尔纹；支持输入页码回车跳页、`Ctrl + 滚轮/+/-/0` 保持视口中心缩放、滚动到页首/页尾停顿后继续滚轮翻页及跨显示器 DPI 自动重绘。

3. **受控物理打印**
   - 屏蔽复制、剪切、全选、另存与截屏键，仅可通过 `Ctrl+P` 或 **安全打印** 按钮发起（准备或打印期间自动防重入并阻止关闭窗口）。
   - 支持 **全部页面**、**仅当前页**、**自定义页码范围（如 `1-3,5`）** 三种范围模式，自动标注默认与脱机打印机。
   - **双重打印机风控**：拦截传真、启用“保留打印的文档”队列、虚拟导出驱动关键字（PDF / XPS / OneNote / WPS 等）、指向本机或回环地址的共享/TCP 端口（含剥离 `IP_` 前缀比对）及非双段式 `\\服务器\共享名` 的文件/管道 UNC 路径，并要求端口或监视器匹配已知物理/标准网络打印特征。
   - **强制烧录追溯水印与配额保护**：打印页面无条件烧录含备注、本地时间、IP、设备码的旋转水印；打印前原子预扣本地双写配额（DPAPI 文件 + HKCU 注册表），若 0 页送出则自动回滚，并在失败提示中明确告知本次是否扣减了打印次数。

---

## 三、安装、静默部署、退出码与卸载

### 1. 交互安装

直接双击 `SecurePdfCreatorSetup.exe` 或 `SecurePdfReaderSetup.exe`（仅写入当前用户目录与 `HKCU` 注册表，无需管理员权限）：
- 全新安装默认路径：`%LOCALAPPDATA%\Programs\SecurePdfCreator` / `%LOCALAPPDATA%\Programs\SecurePdfReader`。
- **从 1.x 或旧版升级**：自动检测已有卸载项或 1.x 默认目录（`%LOCALAPPDATA%\SecurePdfCreator` / `SecurePdfReader`），默认原位覆盖升级并自动清理 `SecurePdf.Core.dll`、`BouncyCastle.Cryptography.dll`、`*.exe.config` 等旧版遗留文件，不会产生两套并存程序。

### 2. 命令行静默安装与工具子命令

静默安装参数不区分大小写，默认值与 1.x 保持一致（静默模式下默认不创建快捷方式、不修改文件关联，按需开启）：

```bat
:: 静默安装创建端并创建桌面与“开始”菜单快捷方式
SecurePdfCreatorSetup.exe --silent --dir "D:\Apps\SecurePdfCreator" --desktop --start-menu

:: 静默安装阅读端并创建快捷方式与 .spdf 文件关联
SecurePdfReaderSetup.exe --silent --dir "D:\Apps\SecurePdfReader" --desktop --start-menu --assoc

:: 显式关闭选项（与静默默认值一致，兼容脚本显式传参）
SecurePdfReaderSetup.exe --silent --no-desktop --no-start-menu --no-assoc
```

安装器底包（`SecurePdfInstaller.exe`）与成品安装包还支持以下命令行模式：

```bat
:: 将暂存目录打包为自解压安装包（自动做回读校验与主程序匹配检查）
SecurePdfInstaller.exe pack --app <creator|reader> --source-dir <源目录> --output <输出Setup.exe> [--stub <底包.exe>]

:: 在临时目录与临时 HKCU 子键中执行“安装 → 覆盖 1.x → 卸载”隔离自检（不影响本机真实安装）
SecurePdfReaderSetup.exe INSTALLER_SELFTEST [--report <报告路径>]

:: 查看命令行帮助说明（须作为第一个参数）
SecurePdfInstaller.exe --help
```

### 3. 进程退出码说明

| 程序 | 退出码 | 含义 |
| :--- | :---: | :--- |
| 安装程序（`*Setup.exe` / `SecurePdfInstaller.exe`） | `0` | 操作成功完成（安装、打包、隔离自检通过或显示帮助） |
| 安装程序 | `1` | 安装或打包过程失败（静默/无人值守模式下详情写入安装包同目录或 `%TEMP%` 的 `install_error.log`） |
| 安装程序 | `2` | 命令行参数无效，或安装目录不符合安全规范（如相对路径、磁盘根目录、Windows 系统目录、共享容器根目录），未修改系统 |
| 安装程序 | `3` | 程序文件已安装（或缺少运行库时取消了回退安装），但本机未检测到 `Windows App Runtime 1.6+ (x64)`，需安装运行库后方可启动 |
| 安装程序 | `4` | 安装包文件不完整（下载截断）或压缩数据损坏，未修改系统 |
| 安装程序 | `5` | 当前运行的是不含安装载荷的裸安装器底包，未修改系统 |
| 安装程序 | `6` | `INSTALLER_SELFTEST` 隔离自检发现校验不一致 |
| 阅读端（`SecurePdfReader.exe`） | `0` | 正常退出 |
| 阅读端 | `1` | 窗口初始化失败或遇到未预期错误退出（已先行清除内存中的解密文档） |
| 阅读端 | `2` | 本机未安装 `Windows App Runtime 1.6+ (x64)` |

### 4. 卸载方式

可通过 **Windows 设置 → 应用 → 已安装的应用** 点击卸载，或直接双击安装目录中的 `uninstall_creator.bat` / `uninstall_reader.bat`：
- 卸载脚本按安装清单逐个删除程序文件与空子目录（非递归删除，用户存放在该目录下的自有文件会保留）。
- 仅在 `.spdf` 关联仍指向当前安装目录下的 `SecurePdfReader.exe` 时才移除注册表关联（同时清理旧版 `ProgID`）；若关联已被用户改给其他程序则保持不动。
- 本机打印配额记录（`%LOCALAPPDATA%\SecurePdfSuite\Quotas` 与对应注册表项）在卸载时故意保留，防止通过卸载重装重置文档剩余打印次数。

---

## 四、从源码构建、门禁校验与打包

### 1. 前置工具链

- Rust 工具链：`1.95`（仓库根目录 [`rust-toolchain.toml`](rust-toolchain.toml) 自动指定，`rustfmt` + `clippy` 组件，`x86_64-pc-windows-msvc` 目标）。
- Windows 资源编译器：LLVM `llvm-rc` 或 Windows SDK `rc.exe`（也可通过环境变量 `SECUREPDF_RC` 指定路径），用于编译嵌入图标与 `VERSIONINFO` 版本资源。
- 依赖审计：`cargo-deny`（`cargo install cargo-deny --locked`）与 `python3`（校验第三方声明同步）。
- **Linux / WSL2 交叉编译环境**：额外安装 `cargo-xwin`（及可选 `sccache`、`llvm` 的 `llvm-readobj`），并保持 WSL2 Windows interop 开启以运行测试可执行文件与安装器隔离自检。

### 2. 构建与打包命令

```bash
# 完整执行 7 步质量门禁：
# [1/7] cargo fmt 检查 -> [2/7] THIRD-PARTY-NOTICES 同步检查 -> [3/7] cargo deny ->
# [4/7] clippy (-D warnings) -> [5/7] 单元与集成测试 ->
# [6/7] Release 构建 + PE 导入/加载标志/版本资源/路径脱敏检查 + 双安装包打包 + 隔离自检 -> [7/7] sccache 统计
./scripts/check.sh

# 离线环境（使用本地 advisory-db 执行 cargo deny）：
DENY_OFFLINE=1 ./scripts/check.sh

# 仅执行 Release 构建、打包与隔离自检（产物输出至 target/dist/）：
./scripts/package.sh

# 若 Cargo.lock 依赖发生变更，重新生成第三方组件声明：
python3 scripts/gen-notices.py
```

打包完成后，成品安装包与自检日志位于：
- `target/dist/SecurePdfCreatorSetup.exe`（附 `SecurePdfCreatorSetup.selftest.log`）
- `target/dist/SecurePdfReaderSetup.exe`（附 `SecurePdfReaderSetup.selftest.log`）

---

## 五、与 v1.1.0（.NET 版）的兼容性及差异

| 维度 | 说明 |
| :--- | :--- |
| `.spdf` 文件兼容 | **完全兼容**：容器魔数、50 字节头部 AAD、策略块编码、`scrypt (N=16384, r=8, p=1)` 与 CNG `AES-256-GCM` 与 v1.x 完全一致，双向可加解密。 |
| 机器码与打印配额兼容 | **完全兼容**：硬件指纹保持 `SHA-256("MG=...|BIOS=...|VOL=...|OS=<NetBIOS名>")` 且读取失败字段回退空串，DPAPI 熵与 73 字节双写记录结构不变；升级后旧文档机器码不变、已用打印次数无缝延续。 |
| 运行时与体积 | 由 `.NET Framework 4.8 + WPF` 迁移至原生 Rust + `WinUI 3`（静态链接 CRT，依赖系统安装的 `Windows App Runtime 1.6+ x64`）。 |
| 新装默认路径 | 全新安装默认位于 `%LOCALAPPDATA%\Programs\SecurePdf{Creator,Reader}`；检测到已有 v1.x 安装时仍沿用原目录就地升级并清理旧 `.dll` / `.config`。 |
| 二进制加载安全 | 新增链接器 `/DEPENDENTLOADFLAG:0x800` 与入口 `SetDefaultDllDirectories` 限制，防止安装包在“下载”等公共目录运行时遭受同目录系统 DLL 劫持。 |

---

## 六、技术规格与安全边界

| 模块 | 实现规格 |
| :--- | :--- |
| 密钥派生与容器加密 | 纯 Rust RFC 7914 `scrypt`（含成本上限防 DoS 检查）+ Windows CNG `bcrypt.dll` `AES-256-GCM` + 头部策略 AAD |
| 本地配额防篡改存储 | Windows DPAPI（当前用户）+ `HMAC-SHA256` 文件与 `HKCU` 注册表双写 + `Local\` 命名互斥锁 + 5 分钟时钟回拨容差 |
| 自解压打包引擎 | Windows 原生 `cabinet.dll` `LZMS` 分块压缩 + `SPDFPKG1` `SHA-256` 完整性校验（用于检测传输截断或意外损坏） |
| PDF 渲染内核 | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) `159.0.8152` 无 V8/XFA 精简版 `pdfium.dll` |
| 安全边界说明 | 本套件在标准 Windows 用户态下提供纵深防护：可阻断常规桌面截屏/录屏与虚拟打印导出，并为外摄翻拍画面提供设备/IP/时间溯源能力；但无法从物理上阻止外部相机拍摄屏幕，亦不防范具备管理员或内核驱动权限的本机内存转储。 |
