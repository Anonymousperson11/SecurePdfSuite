# SecurePdfSuite 2.0.0（Rust 版）整改单

> 出具时间：2026-11-15　审核方式：全程只读（未运行构建、未执行任何 exe）
> 审核结论：**不通过，暂不能发布**
> 对照基准：`github_upload/docs/01`~`05-*.md` 中归档的 v1.1.0 C# 源码（下称“旧版”）

## 使用说明

- 按 P0 → P1 → P2 → P3 顺序处理；P0 全部完成并通过真机验收之前，不要动 P2/P3。
- 每条有编号、位置、问题、整改要求、验收标准。完成后在本文件该条标题后标注 `[已完成]` 并写明验证方式，不要删除条目。
- 可信度：**已核实** = 已在代码或二进制上确认；**待实机** = 代码推导成立，但运行期后果必须在真机上确认。
- “编译通过 + clippy 通过 + 单元测试通过”不算验收。凡涉及界面、安装、打印的条目，必须有真机操作记录。
- 不得通过放宽 lint、删除测试、改弱断言来“通过”。

---

## P0 阻断项

### P0-1 运行库引导的处理器架构常量写错　（常量已核实；后果待实机）
- 位置：`crates/winui3/src/bootstrap.rs:83-87`
- 问题：x64 分支传 `8` 并注释为 X64。Windows SDK `appmodel.h` 中 `PackageDependencyProcessorArchitectures_X64 = 0x4`，`0x8` 是 Arm，`0x10` 是 Arm64。x64 机器上四个候选包都会按 ARM 解析而失败，三个程序都报“未安装运行库”。
- 整改：改为 `4`，或传 `0`（None，使用当前进程架构）。先查 SDK 头文件或官方文档确认数值，不要凭记忆。顺带删除 `bootstrap.rs:202-203` 的无意义占位常量。
- 验收：在装有 Windows App Runtime 1.6+ 的 x64 Win11 上，Reader、Creator、安装向导三个窗口都能正常显示。

### P0-2 三个 exe 依赖 VC++ 运行库且未随包分发　（已核实，PE 导入表）
- 位置：`.cargo/config.toml`；`target/x86_64-pc-windows-msvc/release/*.exe` 导入 `VCRUNTIME140.dll`、`api-ms-win-crt-*`
- 问题：干净系统上双击安装器即报缺 DLL。
- 整改：对 `x86_64-pc-windows-msvc` 目标启用 `-C target-feature=+crt-static`（写入 `.cargo/config.toml` 的 `[target.x86_64-pc-windows-msvc] rustflags`），CI 同步。
- 验收：三个 release exe 的导入表中不再出现 `VCRUNTIME140.dll` 与 `api-ms-win-crt-*`；在未装 VC++ 运行库的干净虚拟机上能启动。

### P0-3 Windows 10 上进程无法加载　（静态导入已核实；Win10 表现待实机）
- 位置：`crates/winui3/src/bootstrap.rs:129-153`；`crates/winui3/src/bindings.rs:204362, 204614`
- 问题：`TryCreatePackageDependency` / `AddPackageDependency` 经 `link!` 静态导入 `api-ms-win-appmodel-runtime-l1-1-5.dll`，这组 API 是 Windows 11 (22000+) 才有。Win10 上加载器阶段即失败，缺运行库提示和安装器的 Win32 回退都执行不到。清单与文档均声明支持 Win10。
- 整改（二选一，须在文档中写明选择）：
  - A. 支持 Win10：这两个 API 改为 `LoadLibraryW` + `GetProcAddress` 动态解析；不可用时走 `Microsoft.WindowsAppRuntime.Bootstrap.dll` 的 `MddBootstrapInitialize2`，再不行给出中文提示。
  - B. 仅支持 Win11：改清单、README、安装器前置检查，Win10 上安装器明确拒绝并说明原因（此时安装器自身不得静态依赖该 API 集）。
- 验收：Win10 22H2 x64 实机上，要么程序正常运行，要么出现明确的中文提示，不得出现系统级“找不到 DLL”弹窗。

### P0-4 机器指纹与 v1.x 不兼容，升级后老用户文档被锁死　（已核实）
- 位置：`crates/secure_pdf_core/src/licensing.rs:44-49, 333-336`
- 问题：旧版为 `SHA-256("MG={guid}|BIOS={uuid}|VOL={serial}|OS={Environment.MachineName}")`，其中计算机名是 NetBIOS 名（大写、最长 15 字符），任一字段读取失败时用**空串**。新版优先用 `ComputerNameDnsHostname`，失败字段用 `UNKNOWN-MG` / `UNKNOWN-BIOS` / `00000000` / `UNKNOWN-HOST`。指纹是配额记录的 HMAC 密钥，一变则已有记录全部判为篡改。
- 整改：
  - 计算机名改用 `ComputerNameNetBIOS`（与 `Environment.MachineName` 一致）。
  - 各字段失败时回退为空串，与旧版逐字节一致。
  - 对照旧版 `ReadRegistryMachineGuid` / `ReadBiosUuid` / `ReadSystemVolumeSerial`（`01-core-crypto-and-quota.md:1164-1220`）逐项核对取值、是否 Trim、大小写、卷根路径取法。
  - 把拼接与哈希拆成纯函数，便于测试。
- 验收：新增单元测试，用固定四元组断言哈希与机器码等于旧版算法的已知答案；在同一台机器上，旧版 Reader 显示的机器码与新版一致；旧版产生的配额记录新版可读、无篡改告警。

### P0-5 文档与发布物仍是 .NET v1.1.0　（已核实）
- 位置：`README.md`、`github_upload/README.md`、`github_upload/docs/*`、根目录 `install_creator.exe` / `install_reader.exe`
- 问题：README 中的静默安装命令（`--desktop --start-menu --assoc`）在新安装器里是未知参数；体积、运行库要求、卸载方式等描述全部过时；新安装包只在 `target/dist/`。
- 整改：其余 P0/P1 完成后重写两份 README 与 docs（构建、打包、前置条件、CLI、卸载、已知限制、与 v1 的差异），替换根目录安装包。旧版 C# 源码归档如需保留，移到明确标注为 v1 的目录。
- 验收：按 README 从零操作可完成构建、打包、静默安装、卸载；文档里出现的每条命令都实际执行过。

---

## P1 严重项

### 核心库

#### P1-C1 打印机风控退化为纯黑名单　（已核实）
- 位置：`crates/secure_pdf_core/src/printing.rs:164-202, 249-311`
- 问题：旧版在黑名单之后还要求端口“看起来是物理的”，否则拒绝（`02-core-pdf-printing-forensics.md:428-436, 546-554`）。新版没命中关键字即放行，任何未知虚拟打印机都能通过。
- 整改：恢复白名单终判。端口须满足其一：LPT/COM、合法 UNC 共享、以 `USB` / `WSD` / `IP_` / `IPP` / `DOT4` / `CNBJNP_` / `CNMFNP_` 开头、字面 IPv4、或监视器名含 `TCP/IP` / `USB` / `WSD` / `IPP` / `BJNP` / `EpsonNet`；否则拒绝并给出“未识别为物理或标准网络端口”。同时保留新版已有的黑名单。
- 验收：新增测试——未知端口 + 未知监视器被拒；现有允许用例仍通过。

#### P1-C2 UNC 端口校验放宽　（已核实）
- 位置：`printing.rs:269-293`
- 问题：旧版只接受恰好 `\\服务器\共享名` 两段，且服务器不能是 `.` 或 `?`；新版接受更深路径（如 `\\srv\share\out.prn`，实为输出到网络文件），并使其绕过“本地端口”检查。另外 `IP_x.x.x.x` 形式指向本机的端口名未被识别（旧版会去掉 `IP_` 前缀再比对）。
- 整改：恢复两段限制与 `.` / `?` 拒绝；`IP_` 前缀剥离后再做本机地址比对。
- 验收：新增测试覆盖三段 UNC、`\\.\pipe\x`、`\\?\...`、`IP_<本机IP>`，均被拒。

#### P1-C3 与 v1.x `.spdf` 的兼容没有任何测试保证　（已核实）
- 位置：`crates/secure_pdf_core/src/crypto.rs:707-720`
- 问题：名为 RFC 7914 测试向量的用例只验证了确定性和归一化，没有比对已知答案。
- 整改：
  - 加入 RFC 7914 §12 的标准向量。注意 `derive_key_scrypt` 会把口令归一化为大写，需要拆出一个接收原始字节的内部函数来测。
  - 加入至少两个旧版生成的 `.spdf` 样本作为测试夹具（不同策略），断言新版能解锁且策略字段、PDF 字节一致；再加一个新版生成、用旧版算法已知答案校验的反向用例。
- 验收：上述测试存在并通过。

### 阅读端

#### P1-R1 隐形溯源水印实质失效　（已核实）
- 位置：`crates/secure_pdf_reader/src/logic.rs:181-209, 265-292, 300-304`；`reader_window.rs:637-651`
- 问题：旧版是低 alpha 蓝色文字（IP、实时时间、设备码）加可解码的 8 字节微点（IPv4 + Unix 秒），Creator 取证页可提取。新版微点载荷是不可逆哈希，取证端没有解码函数；只开隐形水印时画的是固定字符串 `PROTECTED`；微点绘制与 `HIDDEN_WATERMARK` 标志无关。
- 整改：按旧版 `AntiCameraOverlay`（`04-reader-app.md`）恢复载荷与编码；绘制受 `HIDDEN_WATERMARK` 控制；在 core 的 forensics 中提供对应解码函数，Creator 取证页可显示解码结果。
- 验收：单元测试做“编码 → 渲染 → 解码”往返并还原出 IP 与时间；实机截取一帧（临时关闭防截屏的调试构建或对屏拍照）经 Creator 取证页能读出设备码/IP/时间。

#### P1-R2 可见水印几乎不可见且内容残缺　（已核实）
- 位置：`reader/src/logic.rs:298-337`；`reader/src/session.rs:62-85`
- 问题：约 8.6% 不透明的浅蓝字叠在白底上基本看不见（旧版为深色 `ARGB(34,15,23,42)`）；文本截断到 48 字节，时间和备注永远不出现；5×7 点阵只支持 ASCII，中文备注被整段丢弃；时间固定为解锁时刻的 UTC；字号不随 DPI/缩放变化。
- 整改：用系统字体渲染（DirectWrite/GDI 或 XAML TextBlock 平铺），支持中文；去掉 48 字节截断；颜色对比度对齐旧版；时间为本地时间并定时刷新；字号按 DPI 缩放。
- 验收：白底页面上肉眼可辨；“张三专用”这样的中文备注出现在水印中；测试断言水印文本包含 IP、时间、备注。

#### P1-R3 打印只能整本，且界面承诺与实际不符　（已核实）
- 位置：`reader/src/print_dialog.rs:100-126, 198`；`core/src/printing.rs:506, 611, 658-669`
- 问题：没有页码范围（旧版有 全部 / 当前页 / 自定义 `1-3,5`）；对话框写“强制烧录追溯水印”，实际只在 `VISIBLE_WATERMARK` 置位时才画，且不含时间戳（旧版无条件烧录并带时间）。
- 整改：恢复页码范围解析与三种模式；打印水印无条件烧录，内容含设备码、IP、时间；失败提示说明是否已扣次数。
- 验收：范围解析有单元测试（含非法输入）；实机打印第 3 页只出 1 张且带水印。

#### P1-R4 锁定后未清除页面内容　（已核实）
- 位置：`reader/src/reader_window.rs:220, 505-511, 573-598`
- 问题：到期或防截屏失效后，页面位图仍在可视树里，上面只盖了 alpha 252 的遮罩；在途渲染完成后还会继续 `SetSource`；计时器不停，每 30 秒重复锁定并重弹提示。
- 整改：锁定时清空页面与叠加层图像源、停止计时器、遮罩全不透明、渲染回调先检查锁定状态；提示后关闭窗口（对齐旧版）。失焦遮罩同样改为全不透明。
- 验收：实机设置 1 分钟有效期，到期后窗口内无任何页面内容残留。

#### P1-R5 打印中可用 Ctrl+P 重复提交　（已核实）
- 位置：`reader/src/reader_window.rs:328-340, 394-397`；`print_dialog.rs:178`
- 整改：增加“打印对话框已开 / 正在打印”标志，快捷键与按钮共用。
- 验收：打印进行中按 Ctrl+P 无反应。

#### P1-R6 关窗与异常路径不释放明文　（已核实 / 部分待实机）
- 位置：`reader/src/session.rs:12`；`unlock_window.rs:364, 425-437`；`reader_window.rs:85-122`
- 问题：事件闭包全部 `.forget()` 并持有会话，`MemoryPdfDocument` 在进程生命周期内不析构；没有 Closed 处理和 panic hook；`open_reader_window` 中途失败不关窗；验证码 `String` 不清零，`PasswordBox` 不清空。
- 整改：注册窗口 Closed 处理，释放会话并清零；`open_reader_window` 失败路径显式 Close；验证码用 `Zeroizing<String>`，解锁后清空输入框；加 panic hook 做同样清理后退出。
- 验收：关闭阅读窗口后进程退出；代码审阅确认 Drop 路径可达。

### 创建端

#### P1-K1 整批共用一个验证码　（已核实）
- 位置：`crates/secure_pdf_creator/src/encrypt_tab.rs:112-115, 631, 746`；`logic.rs:142`
- 问题：旧版每个文件独立生成验证码。新版整批共用输入框里的码，批次间不更换，且允许手输任意 8 位以上字符（如 `11111111`）。
- 整改：恢复每文件独立自动生成；去掉手输或至少限定字符集与长度。删除 `encrypt_tab.rs:113` 的硬编码回退码 `ABCD-EFGH-JKLM-NPQR`，随机数失败应报错并中止。
- 验收：一批 3 个文件得到 3 个不同验证码，互相不能解锁。

#### P1-K2 结果表缺失、失败原因不可见、新批次清空旧结果　（已核实）
- 位置：`encrypt_tab.rs:621, 672-698`
- 问题：没有逐文件结果表；只显示“成功 x/总数”；全部失败时导出按钮禁用，原因无处可看；开始新批次时 `results.clear()` 静默丢弃上一批未导出的验证码。
- 整改：恢复结果表（文件、验证码、状态、失败原因），多批累加；恢复“复制选中验证码 / 复制全部清单”；有失败行时也允许导出；清空结果必须经用户确认。
- 验收：实机跑两批（含一个非 PDF 文件），两批结果都在表里，失败原因可见。

#### P1-K3 “允许接收方调节摩尔纹”恒为开　（已核实）
- 位置：`creator/src/logic.rs:169`
- 整改：恢复复选框，默认不勾选，标志随之写入。
- 验收：`validate_policy_input` 的单元测试覆盖四个标志位的映射；默认生成的文件在 Reader 中不能调节摩尔纹。

#### P1-K4 加密中可直接关窗；关窗确认默认按钮是“退出”　（已核实）
- 位置：`creator/src/main.rs:118-135`；`winui3/src/dialog.rs:137`
- 整改：`running` 时拒绝关闭并提示；确认对话框默认按钮改为“返回”；已有其他对话框打开时不得静默吞掉关窗请求。
- 验收：实机验证两种情形。

#### P1-K5 有效期与输出目录功能缩水　（已核实）
- 位置：`encrypt_tab.rs:120, 128, 772-782, 872-880`；`logic.rs:150-157`
- 问题：有效期只接受整数天（旧版为小时，含 1/6/24/72 预设与小数）；输出框只读，丢了“留空则输出到源文件目录”；默认目录取 `%USERPROFILE%\Desktop`，不存在时落到当前目录。CSV 里到期时间是 UTC（旧版为本地时间）；`format_dotnet_ticks_utc` 的文档注释与实现不符。
- 整改：恢复按小时设置与预设；恢复留空语义与手输路径；桌面路径用 `SHGetKnownFolderPath`；到期时间显示本地时间并标注时区。
- 验收：0.5 小时有效期可设置并在 Reader 中按时到期。

### 安装器

#### P1-I1 卸载脚本缺陷　（已核实；自删行为待实机）
- 位置：`crates/secure_pdf_installer/src/win32_install.rs:372-409`
- 问题：
  - 以 UTF-8 写出但没有 `chcp 65001`，中文系统下含中文的路径（如载荷里的 `关联spdf文件格式.bat`、中文用户名）乱码，卸载残留。
  - `%` 未转义为 `%%`。
  - 脚本先删自身，后面的 `rmdir` 读不到；从资源管理器双击时当前目录就是安装目录，`rmdir` 也会失败。
  - `taskkill` 后没有等待；开始菜单的 `SecurePdfSuite` 文件夹不删。
  - 无条件删除 `.spdf` 关联，即使安装时用了 `--no-assoc` 或关联已指向别的程序；旧 ProgID 的清理也丢了。
- 整改：对照旧版卸载脚本生成逻辑（`05-installer-and-tests.md`）逐项恢复：`chcp 65001`、`setlocal DisableDelayedExpansion`、`%%` 转义、`cd /d %SystemRoot%`、`(goto) 2>nul & del & rmdir` 单行自删、等待、条件删关联、删开始菜单文件夹。
- 验收：在中文用户名账户下安装到含中文和 `%` 的路径，卸载后安装目录、快捷方式、注册表项全部消失；关联被改给其他程序时卸载不动它。

#### P1-I2 没有“应用和功能”卸载项　（已核实）
- 位置：`win32_install.rs:419-470`
- 整改：写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<AppId>`（DisplayName、DisplayVersion、Publisher、InstallLocation、UninstallString、EstimatedSize、NoModify、NoRepair），卸载时删除。
- 验收：“设置 → 应用”中可见并可卸载。

#### P1-I3 安装目录不校验　（已核实）
- 位置：`installer/src/main.rs:150`；`wizard.rs:293-296, 332-338`
- 问题：接受相对路径、盘符根目录、带引号路径；向导“更改目录”不再自动追加应用子目录，文件会平铺进用户选的目录。
- 整改：恢复旧版 `NormalizeInstallDir`（去引号、转绝对路径、拒绝根目录与系统目录）；浏览后自动追加 `SecurePdfReader` / `SecurePdfCreator`。
- 验收：`--dir C:\`、`--dir .` 被拒并返回非零；单元测试覆盖。

#### P1-I4 与 v1 的升级路径　（已核实）
- 位置：`win32_install.rs:90-94`；`package.rs:497`
- 问题：默认目录从 `%LOCALAPPDATA%\<App>` 改为 `%LOCALAPPDATA%\Programs\<App>`，快捷方式名由中文改为英文 exe 名，于是两套并存；卸载 v1 会连带删掉 v2 的 `.spdf` 关联；装到旧目录时旧的 `SecurePdf.Core.dll`、`*.exe.config` 不清理，旧卸载项仍显示 1.1.0。
- 整改：安装时检测 v1（旧卸载项 / 旧默认目录），默认装到同一目录并清理 v1 遗留文件、旧快捷方式、旧卸载项；快捷方式沿用中文名。
- 验收：在装有 v1.1.0 的机器上运行新安装器，结束后只剩一套程序、一组快捷方式、一个卸载项，关联可用。

#### P1-I5 前置条件、静默模式与失败处理　（已核实）
- 位置：`installer/src/main.rs:3-6, 59, 67, 139-166`；`wizard.rs:80-113, 353-364`；`win32_install.rs:424-427, 440-453`；`package.rs:503-516`
- 问题：
  - `--silent` 不检查 Windows App Runtime，装完起不来仍返回 0；GUI 回退路径装完也不提示。
  - release 是 windows 子系统，`eprintln!` 不可见，静默失败只有退出码 1（旧版写 `install_error.log`）。
  - 覆盖安装不经确认直接杀进程（Creator 未导出的验证码会丢）。
  - 安装中可点 X 关窗，留下半装状态。
  - 写文件无重试、不清只读属性、无回滚；快捷方式或注册表失败会中止，此时文件已落盘但卸载脚本未生成。
  - 不校验载荷里有没有主程序（`pack --app reader` 指到 creator 目录也能打包）。
- 整改：检测运行库并给出明确提示与专用退出码；写安装日志；GUI 下杀进程前确认；安装中禁止关窗；写入重试并清只读；卸载脚本先于快捷方式/注册表生成，后者失败降级为警告；打包与安装时校验主程序存在。
- 验收：各情形实机验证；退出码在 README 中列表说明。

#### P1-I6 静默参数与旧版不兼容　（已核实）
- 位置：`installer/src/main.rs:270-290`
- 问题：旧版为 `--desktop` / `--start-menu` / `--assoc`（选择加入、不区分大小写），新版为 `--no-*`（选择退出、区分大小写）。已有部署脚本会直接失败。另外 `--silent --dir --no-desktop` 会把 `--no-desktop` 当作目录；任意位置出现 `help` 即进入帮助。
- 整改：同时接受新旧两套参数并在帮助里写清默认值；大小写不敏感；`--dir` 的值不得以 `--` 开头；`help` 只在作为第一个参数时生效。
- 验收：`parse_cli_args` 测试覆盖上述情形。

---

## P2 一般项

| 编号 | 位置 | 问题 | 整改 |
|---|---|---|---|
| P2-1 | `reader/src/print_dialog.rs:238-252`；`reader_window.rs:484-491` | 部分打印失败后界面剩余次数不更新；巡检结果不回写 | 失败分支重新读取配额；巡检 `Ok(status)` 回写界面 |
| P2-2 | `reader/src/reader_window.rs:549-609` | 每次翻页/缩放新起线程，无节流无取消；像素封顶后显示尺寸缩小而缩放标签不变 | 单工作线程 + 最新请求覆盖；封顶后保持 DIP 尺寸拉伸显示 |
| P2-3 | `reader/src/print_dialog.rs:65` | 打印机枚举（含 WMI）在 UI 线程 | 移到后台线程 |
| P2-4 | `reader/src/main.rs:28, 34`；`winui3/src/app.rs:109`；`bootstrap.rs:192-200` | 启动失败无提示；任何窗口创建失败都报“防截屏被拒”；运行库提示夹带英文原始错误、不说明要装哪个版本 | 启动失败用 MessageBox；按实际原因区分文案；写明所需运行库版本 |
| P2-5 | `reader/src/reader_window.rs` | 相对旧版缺：页码输入跳页、滚到边缘后滚轮翻页、缩放保持视口中心、DPI 变化重绘、“剩余/总次数”与“有效期至”显示、篡改告警、动态摩尔纹、本机 IP 显示、打印机脱机标注 | 对照 `04-reader-app.md` 逐项恢复 |
| P2-6 | `reader/src/unlock_window.rs:386-391`；`reader_window.rs:472-502, 569-608` | WinUI 对象被整体 move 进后台线程，在非 UI 线程上克隆和析构 | 后台线程只带会话与 `DispatcherQueue`，UI 句柄留在 UI 线程 |
| P2-7 | `creator/src/encrypt_tab.rs:723` | 先把整个文件读进内存再查 1 GiB 上限 | 先用 `metadata().len()` 判断 |
| P2-8 | `creator/src/encrypt_tab.rs:630-632` | 工作线程 panic 后 `running` 永远为真，界面卡死 | `catch_unwind` 或守卫对象复位状态 |
| P2-9 | `creator/src/forensics_tab.rs:181-268, 306` | 加载/增强失败后按钮与状态文字不恢复；加载新图不清上一张的增强结果；不再自动增强；增强期间选图被静默忽略 | 修正状态机；恢复加载后自动增强与切换算法自动重算 |
| P2-10 | `core/src/forensics.rs:80` | 超过 50M 像素的照片直接拒绝（旧版自动缩小） | 恢复降采样 |
| P2-11 | `creator/src/logic.rs:301`；`forensics.rs:37-48` | 注释称预乘 BGRA，实际未预乘；增强算法忽略 alpha | 预乘或先在白底上合成 |
| P2-12 | `creator/src/encrypt_tab.rs:257-262, 499-501, 849-869` | 目录递归扫描与队列全量重建在 UI 线程 | 扫描移到后台；增量刷新 |
| P2-13 | `creator/src/logic.rs:37, 201` | 去重只对 ASCII 不区分大小写；输出名“检查后写入”存在竞争 | 用规范化路径去重；用 `create_new` 语义占位 |
| P2-14 | `installer/src/package.rs:160-186` | 路径清洗不拒保留设备名、尾部点/空格、大小写重名 | 对齐旧版 `IsSafeFileName` |
| P2-15 | `installer/src/package.rs:310-361` | 截断的安装包被当成“无载荷”，弹出用法说明；“可能已被篡改”措辞误导（哈希与载荷同处一文件，只能防损坏） | 区分“安装包不完整”；修改措辞 |
| P2-16 | `installer/src/package.rs:447-482` | `INSTALLER_SELFTEST` 只在内存里打包解包，不落盘 | 恢复隔离的“安装 → 卸载”自检，或改名并更正注释 |
| P2-17 | `installer/src/cab.rs:60-67` | `SetCompressorInformation` 返回值被忽略 | 失败即报错 |
| P2-18 | 三个 exe 的导入表 | `cabinet.dll`、`bcrypt.dll` 等静态导入且不在 KnownDLLs，安装器放在“下载”目录时存在同目录 DLL 劫持面 | 入口处尽早调用 `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`，相关 DLL 改延迟加载 |
| P2-19 | `winui3/src/interop.rs:249, 519` | `OpenClipboard(None)` 后 `EmptyClipboard`；失败路径泄漏 `pwstr` | 传窗口句柄；用守卫释放 |
| P2-20 | `core/src/format.rs:282-296` | `format_dotnet_ticks_utc` 文档注释写 local，实际是 UTC | 改注释或改实现 |

---

## P3 门禁、测试与发布卫生

### P3-1 门禁
- `scripts/check.sh` 止于 release 构建。追加：准备 stage → `pack` 两个安装包 → 自检 → 在临时目录静默安装并校验文件清单 → 卸载并校验无残留。
- `cargo deny` 从未跑通（见 `PROGRESS_BACKUP.md`）。`deny.toml` 设了 `multiple-versions = "deny"`，而 `Cargo.lock` 中 `windows-link` 有两个版本且未在 skip 中。修到 `cargo deny check` 实际通过，不得以 `|| true` 之类绕过。
- `.github/workflows/ci.yml`：`EmbarkStudios/cargo-deny-action@v2` 是 Docker action，不能在 `windows-latest` 上运行，改为独立的 ubuntu job 或直接 `cargo install cargo-deny` 后调用；与 `check.sh` 保持步骤一致。

### P3-2 测试补强
- core：P0-4、P1-C1、P1-C2、P1-C3 中要求的测试；EXIF 测试补 JPEG APP1 路径与方向 8 的实际旋转结果；50MP 上限；滑窗结果与暴力算法对拍。
- reader：水印文本内容、微点往返、仅隐形/仅摩尔纹时的叠加层、巡检边界（无有效期、恰好到期）、快捷键过滤的更多键位、常规 DPI 下的渲染尺寸；现有“存在非透明像素”的断言要换成有约束力的断言。
- creator：`validate_policy_input` 的有效期、标志位、边界值。
- installer：卸载脚本的非 ASCII 与 `%` 路径、注册表写后回读、截断与 trailer 篡改、`--dir` 缺值。
- winui3：引导用的架构常量。

### P3-3 发布卫生
- `Cargo.toml:17` 的 `repository` 是占位地址，改为真实地址或删除。
- 仓库根目录补 LICENSE 文件（清单声明了 `MIT OR Apache-2.0`）。
- `assets/THIRD-PARTY-NOTICES.txt`：标题只写“阅读端”但 Creator 也分发；补充静态链接的 Rust 依赖（`image`、`png`、`zune-jpeg`、`tiff`、`gif`、`weezl`、`fdeflate`、`zeroize`、`windows-*` 等）及 Windows App SDK。
- release 二进制里嵌有 `/home/llc/...` 路径，加 `--remap-path-prefix`。
- 三个 exe 与 Setup 补版本信息资源和图标。
- 加 `.gitignore`（至少 `target/`）。
- `PROGRESS_BACKUP.md` 内容已过期且含本机路径，发布前删除或更新。
- 用户可见文案去掉开发术语与中英混排，例如 `creator/src/main.rs:72`（“纯原生 Rust + WinUI 3 Fluent · … O(W) 水印取证”）、`forensics_tab.rs:91-93, 268`、`encrypt_tab.rs:114, 124`；`reader_window.rs:208` 的“硬件级防截屏已激活”应区分实际生效的保护级别。

---

## 核对后确认无问题的部分（不要改动）

- `.spdf` v2 容器布局、50 字节 AAD、策略块字段顺序与 7-bit 长度前缀字符串，与旧版一致。
- 配额记录 41+32 字节布局、DPAPI 熵 `SecurePdfSuite-Quota-v2`、主副存储路径、互斥锁名 `Local\SecurePdfSuite.Quota.{docId:N}`、5 分钟回拨容差，与旧版一致。
- scrypt / Salsa20/8 / BlockMix / ROMix、PBKDF2、AES-256-GCM、HMAC-SHA256 的实现；成本参数上限检查。
- 打印几何（300 DPI 与 36M 像素上限）、24bpp 行对齐、先扣配额再打印、零页才回滚。
- 防截屏的设置时序（激活前设置、激活前后复核、巡检重设、失败即锁定）；阅读端无明文落盘；剪贴板只写设备码。
- 安装包结构、完整性校验先于任何落盘、`..` / 绝对路径 / 盘符相对 / UNC / ADS 被拒、注册表只写 HKCU、卸载不递归删除。
- CSV 公式注入中和、输出文件名不含路径分隔符、同批重名自动加后缀、`.tmp` 原子写入。

---

## 最终验收（全部整改完成后）

在以下四种环境各执行一遍，并把结果记录附在本文件末尾：

| 环境 | 说明 |
|---|---|
| A | 干净的 Win11 x64，未装 VC++ 运行库，已装 Windows App Runtime |
| B | 干净的 Win11 x64，未装 Windows App Runtime |
| C | Win10 22H2 x64（按 P0-3 的选择，验证正常运行或明确拒绝） |
| D | 已安装 v1.1.0 并有已解锁、已消耗过打印次数的文档的机器 |

流程：
1. 运行 Creator 安装包 → 加密 3 个 PDF（其中一个设置 1 次打印、短有效期、中文水印备注）→ 导出 CSV。
2. 运行 Reader 安装包 → 双击 `.spdf` → 输入验证码解锁 → 翻页、缩放 → 确认可见水印可辨且含中文备注。
3. 尝试截屏（应为黑屏或被排除）；尝试打印到 Microsoft Print to PDF（应被拒）；打印到物理打印机的指定页（应成功、带水印、扣 1 次）。
4. 等待有效期到期，确认窗口锁定且无内容残留。
5. 环境 D：升级后打开旧文档，确认机器码不变、剩余打印次数与升级前一致、无篡改告警。
6. 对屏拍照，经 Creator 取证页还原出设备码/IP/时间。
7. 从“设置 → 应用”卸载两个程序，确认目录、快捷方式、注册表、关联均无残留。
8. `./scripts/check.sh` 全流程通过（含 `cargo deny` 与打包冒烟）。
