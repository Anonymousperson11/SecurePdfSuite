# SecurePdfSuite 2.0.0（Pure Rust + WinUI 3）整改与断点备份

> 初版生成时间：2026-11-15 17:20 PST  
> 上一轮备份归档：`.backups/PROGRESS_BACKUP_20261115.md`（2026-11-15 21:13 PST）  
> 本轮备份时间：2026-11-17 20:58 PST（备份 Claude 于 `2026-11-16 20:23`–`2026-11-17 02:29 PST` 的整改成果）  
> 源码无损快照包：`.backups/claude_snapshot_20261117.tar.gz`（共 118 项，不含 `target/` 编译缓存，附 `target/dist_probe/selftest.log`；SHA-256：`082e0a07e357ae37c4e69aa0449c8f7591aa8f68ebcbab4a083744e76a51d9e5`）  
> 注意：当前目录不是 Git 仓库，恢复或回滚任何源码文件可直接从 `.backups/claude_snapshot_20261117.tar.gz` 提取（注意正式发布前按 `REMEDIATION.md` `P3-3` 要求移除临时备份目录与本备份文档）。

---

## 1. 当前总体状态

对照 `REMEDIATION.md`（2026-11-15 21:34 出具的 2.0.0 审核整改单），Claude 在 `2026-11-16 20:23` 至 `2026-11-17 02:29 PST` 期间通过并行子任务（见 `target/fork-creator`、`target/fork-installer`、`target/fork-reader`）**完成了 P0～P3 中几乎全部源码重构、安全加固、单测补强与门禁脚本改造**，最后停在 `2026-11-17 02:29:01` 对 `secure_pdf_reader` 测试目标的构建阶段。

- **已完成部分**：`P0-1`～`P0-4`、`P1` 全部 18 项（核心库 3 项 + 阅读端 6 项 + 创建端 5 项 + 安装器 6 项，含阅读端打印 `P1-R3` 跨库联动）、`P2-1`～`P2-20` 全部 20 项、`P3-1`～`P3-3` 的门禁脚本/CI/许可证/第三方声明/资源图标/DLL 搜索路径加固。
- **尚未收尾部分（断点遗留）**：
  1. 并行修改合流后的全工作区 `cargo fmt --all` 与完整 `./scripts/check.sh`（`[1/7]`～`[7/7]`）总门禁复跑；
  2. `SecurePdfReader.exe` 的 Release 重编译及 `target/dist/` 下两份正式 `Setup.exe` 的重新打包与隔离自检（Creator 已在 `target/dist_probe/` 先行试跑通过）；
  3. `target/` 下临时并行目录（`fork-creator`、`fork-installer`、`fork-reader`、`dist_probe`）清理；
  4. `P0-5` 文档与根目录旧版发布物更新（重写 `README.md`、`github_upload/README.md`，归档 v1.1.0 文档，替换根目录安装包）；
  5. `REMEDIATION.md` 各条目 `[已完成]` 状态回填与末尾 A/B/C/D 四环境真机交互验收。

---

## 2. 对照 `REMEDIATION.md` 逐项完成情况

### 2.1 P0 阻断项

| 编号 | 状态 | 涉及文件 | 已落地的实现要点 |
| :--- | :--- | :--- | :--- |
| **P0-1** | 代码与单测已完成 | `crates/winui3/src/bootstrap.rs` | 修正 `PACKAGE_DEPENDENCY_ARCH_X64 = 0x4`、`PACKAGE_DEPENDENCY_ARCH_ARM64 = 0x10`，删除旧占位常量，新增与 `appmodel.h` 对照的单测 |
| **P0-2** | 代码与门禁已完成 | `.cargo/config.toml`、`scripts/build-env.sh`、`scripts/package.sh` | 启用 `-C target-feature=+crt-static` 与 `/ignore:4099`；`package.sh` 用 `llvm-readobj --coff-imports` 阻断 `VCRUNTIME*` / `msvcp*` / `api-ms-win-crt*` 导入 |
| **P0-3** | 代码与资产已完成（选方案 A：支持 Win10） | `crates/winui3/src/bootstrap.rs`、`assets/Microsoft.WindowsAppRuntime.Bootstrap.dll`、`scripts/package.sh` | `TryCreatePackageDependency` / `AddPackageDependency` 改为运行期 `GetModuleHandleW` / `LoadLibraryExW(LOAD_LIBRARY_SEARCH_SYSTEM32)` + `GetProcAddress` 动态解析；Win10 回退随包分发的 `Microsoft.WindowsAppRuntime.Bootstrap.dll`（`MddBootstrapInitialize2`）；`package.sh` 校验 PE 不静态导入 `api-ms-win-appmodel-runtime-l1-1-[3-9]` |
| **P0-4** | 代码与单测已完成 | `crates/secure_pdf_core/src/licensing.rs` | 计算机名改用 `ComputerNameNetBIOS`，任一硬件字段读取失败回退空串（无 `UNKNOWN-*`），不额外 `trim` 注册表/BIOS 值，修复 WMI 代理接口 `CoSetProxyBlanket`；拆出纯函数 `machine_fingerprint_source` / `compute_machine_hash` 并补齐已知答案单测 |
| **P0-5** | **待收尾** | `README.md`、`github_upload/*`、`install_*.exe` | 按整改单要求需待其余 P0/P1 验证完毕后重写文档并替换根目录旧版 `.NET` 安装包 |

### 2.2 P1 严重项

- **核心库（`crates/secure_pdf_core`）**
  - **P1-C1 / P1-C2**（`src/printing.rs`）：恢复物理/标准网络端口白名单终判；UNC 路径严格限制为 `\\服务器\共享名` 两段且拒绝 `.` / `?`；剥离 `IP_` 前缀后再做本机/回环地址比对；补齐未知端口、三段 UNC、管道与 `IP_<本机IP>` 拦截单测。
  - **P1-C3**（`src/crypto.rs`、`tests/fixtures/*`）：拆出 `scrypt_raw` 加入 RFC 7914 §12 标准测试向量及 v1.1.0 `SecurePdf.Core.dll` 已知答案；新增 `tests/fixtures/{v1_default_policy.spdf, v1_expiring_noprint.spdf, v1_plain.pdf, v2_generated.spdf}` 双向兼容测试夹具。
- **阅读端（`crates/secure_pdf_reader` + `secure_pdf_core`）**
  - **P1-R1**（`core/src/watermark.rs`、`reader/src/logic.rs`、`creator/src/forensics_tab.rs`）：恢复低 alpha 蓝色溯源文字 + 可解码 8 字节微点矩阵（IPv4 + Unix 秒），严格受 `HIDDEN_WATERMARK` 标志位控制，Creator 取证页可直接解码还原 IP 与时间；新增往返单测与调试演示模块 `reader/src/debug_demo.rs`。
  - **P1-R2**（`core/src/watermark.rs`、`core/src/localtime.rs`、`reader/src/session.rs`）：改用 GDI `Microsoft YaHei UI` 系统粗体渲染抗锯齿遮罩（支持中文备注且无 48 字节截断），可见水印恢复深色 `ARGB(34, 15, 23, 42)` + 浅色阴影，按显示 DPI 缩放，每秒刷新本地时间。
  - **P1-R3**（`core/src/printing.rs`、`reader/src/print_dialog.rs`、`reader/src/logic.rs`）：支持“全部页面 / 仅当前页 / 自定义页码范围（如 `1-3,5`）”三种模式；打印页面无条件烧录含备注、本地时间、IP、设备码的追溯水印；失败提示明确告知是否扣减打印次数。
  - **P1-R4**（`reader/src/reader_window.rs`）：`lock_and_close` 立即停止所有计时器、关闭渲染线程、将页面与叠加层 `ImageSource` 置空、显示 `alpha=255` 全不透明遮罩、释放明文后弹窗并关闭窗口；失焦幕布同样改为 `alpha=255` 全不透明，渲染回调优先检查 `locked` 与 `generation`。
  - **P1-R5**（`reader/src/reader_window.rs`、`reader/src/print_dialog.rs`）：引入 `PrintPhase::{Idle, Preparing, Printing}`，按钮与 `Ctrl+P` 共用防重入拦截，且 `Printing` 期间拒绝关闭窗口。
  - **P1-R6**（`reader/src/session.rs`、`reader/src/unlock_window.rs`、`reader/src/reader_window.rs`、`reader/src/main.rs`）：`MemoryPdfDocument` 置于 `RwLock<Option<...>>` 中支持确定性释放；UI 控件置于 `thread_local!` 避免跨线程泄漏；注册窗口 `Closed` 处理与全局 panic hook（先清明文再弹窗退出）；`open_reader_window` 失败路径显式关窗并清明文；验证码使用 `Zeroizing<String>` 且解锁后立即清空 `PasswordBox`。
- **创建端（`crates/secure_pdf_creator`）**
  - **P1-K1**（`src/logic.rs`、`src/encrypt_tab.rs`）：每文件独立调用 CSPRNG 生成验证码，移除手输框与硬编码回退码，随机数失败直接报错中止。
  - **P1-K2**（`src/encrypt_tab.rs`、`src/logic.rs`）：恢复 6 列逐文件结果表（原文件、验证码、允许打印、有效期至、状态、输出路径/失败原因），多批次累加保留，支持“复制选中验证码 / 复制全部清单 / 含失败行导出 CSV”，清空结果需二次确认。
  - **P1-K3**（`src/encrypt_tab.rs`、`src/logic.rs`）：恢复“允许接收方调节摩尔纹强度”复选框且默认不勾选，单测覆盖全部 4 个 `SecurityFlags` 映射。
  - **P1-K4**（`src/main.rs`、`crates/winui3/src/dialog.rs`）：加密运行中拒绝关窗并提示；未保存验证码确认框默认焦点设为“返回”（`DialogDefault::Close`）；已有对话框打开时回退原生 `MessageBox` 避免静默吞掉关窗请求。
  - **P1-K5**（`src/encrypt_tab.rs`、`src/logic.rs`、`src/shell.rs`、`core/src/localtime.rs`）：有效期恢复按小时设置（含永久/1h/6h/24h/72h/自定义小数如 `0.5`）；输出目录可手输且留空表示输出到各源文件所在目录；桌面路径通过 `SHGetKnownFolderPath(FOLDERID_Desktop)` 解析；界面与 CSV 均显示带 UTC 偏移的本地时间。
- **安装器（`crates/secure_pdf_installer`）**
  - **P1-I1**（`src/uninstall.rs`）：重写卸载脚本生成器，补齐 `chcp 65001 >nul`、`cd /d "%SystemRoot%"`（置于 `setlocal` 之前）、`setlocal DisableDelayedExpansion`、`%%` 转义、`ping` 等待、开始菜单文件夹清理、检查 `.spdf` 关联归属后再删（含清理旧 `ProgID`）、末尾单行 `(goto) 2>nul & cd /d "%SystemRoot%" & del & rmdir & ver >nul` 自删。
  - **P1-I2**（`src/registry.rs`、`src/win32_install.rs`）：写入并在卸载时清理 `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<AppId>`（含 `DisplayName`、`DisplayVersion`、`Publisher`、`InstallLocation`、`UninstallString`、`EstimatedSize`、`NoModify`、`NoRepair`）。
  - **P1-I3**（`src/paths.rs`、`src/wizard.rs`、`src/main.rs`）：实现 `normalize_install_dir`（去引号、拒绝相对路径/盘符根目录/系统目录/共享容器根目录）与向导浏览后自动追加应用子目录 `with_app_folder`。
  - **P1-I4**（`src/win32_install.rs`、`src/registry.rs`）：检测 v1.x 旧卸载项与旧默认目录 `%LOCALAPPDATA%\<App>`，升级时默认复用原目录、清理 `SecurePdf.Core.dll` / `BouncyCastle.Cryptography.dll` / `*.exe.config` 等遗留文件与旧快捷方式，快捷方式统一使用中文名。
  - **P1-I5**（`src/main.rs`、`src/wizard.rs`、`src/win32_install.rs`、`src/logging.rs`、`src/package.rs`）：`find_installed_runtime()` 前置/后置检测运行库（静默模式缺运行库返回专用退出码 `3`）；无人值守失败写入 `install_error.log`；GUI 下覆盖运行中进程前先确认、安装进行中禁止关窗；写文件自动清只读并重试；卸载脚本先于快捷方式/注册表生成，后者失败降级为警告；打包与安装强制校验主程序存在且匹配 `AppKind`。
  - **P1-I6**（`src/main.rs`）：同时兼容 `--desktop` / `--start-menu` / `--assoc` 与 `--no-*`（大小写不敏感，静默默认值与 v1.x 一致），拒绝以 `--` 开头的 `--dir` 参数值，`help` 仅作为首个参数时触发。

### 2.3 P2 一般项（20/20 已完成）

- **阅读端与 WinUI 封装（`P2-1`～`P2-6`、`P2-19`）**：打印失败分支重读配额、巡检写回配额（`P2-1`）；独立 `render_worker.rs` 单工作线程最新请求覆盖 + 像素封顶后保持 DIP 尺寸拉伸显示（`P2-2`）；打印机枚举移至后台线程（`P2-3`）；启动失败弹窗提示、通过无保护探测窗区分“防截屏被拒”与“UI 初始化失败”、缺运行库提示写明 `Windows App Runtime 1.6+` 且不夹带英文系统错误（`P2-4`、`protection.rs`）；补齐页码框回车跳页、边缘滚轮停顿翻页、缩放保持视口中心、跨显示器 DPI 变化重绘、`剩余/总次数` 与 `有效期至`、配额不一致自动修复提示、每秒动态摩尔纹相位、本机 IP 显示、打印机“（脱机）/（默认）”标注（`P2-5`）；后台线程仅传 `Arc<ReaderSession>` 与 `DispatcherQueue`，WinUI 对象留在 UI 线程（`P2-6`）；剪贴板传入窗口 `HWND` 且 `shell_item_path` 用 `CoTaskMemGuard` 防泄漏（`P2-19`）。
- **创建端与核心库（`P2-7`～`P2-13`、`P2-20`）**：读取前先查 `fs::metadata().len()` 判断 1 GiB 上限（`P2-7`）；`BusyGuard` + `catch_unwind` 防工作线程 panic 卡死界面（`P2-8`）；取证页修正状态机并恢复加载后自动增强、切换算法自动重算（`P2-9`）；超 50MP 大图按比例自动缩小后再取证（`P2-10`）；修正 `rgba_to_premultiplied_bgra` 预乘与白底合成（`P2-11`）；目录递归扫描移至后台线程（`P2-12`）；Unicode 全小写规范化路径去重 + `OpenOptions::create_new(true)` 原子占位防并发竞争（`P2-13`）；修正 `format_dotnet_ticks_utc` 注释并新增 `format_dotnet_ticks_local`（`P2-20`）。
- **安装器与二进制加固（`P2-14`～`P2-18`）**：`is_safe_file_component` 拦截保留设备名（含 `COM¹` 等上标变体）、尾部点/空格及大小写重名（`P2-14`）；通过 `pe_image_end` 解析 PE 节表与签名表末尾，精确区分“裸底包无载荷”与“安装包被截断”，并将提示措辞修正为防损坏表述（`P2-15`）；新增 `selftest.rs` 在临时目录与临时注册表树下完整跑通“全新安装→卸载”及“覆盖 v1.x→改关联→卸载”隔离自检（`P2-16`）；检查 `SetCompressorInformation` 返回值（`P2-17`）；三个程序入口调用 `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_USER_DIRS)` 并在 `build.rs` 加链接参数 `/DEPENDENTLOADFLAG:0x800`（`P2-18`）。

### 2.4 P3 门禁、测试与发布卫生

- **P3-1 门禁与 CI**：新增 `scripts/build-env.sh`、`scripts/package.sh`、`scripts/gen-notices.py`，升级 `scripts/check.sh` 为 7 步全链路门禁（`fmt` → `gen-notices --check` → `cargo deny` → `clippy` → `test` → `package.sh` 含 PE 导入/资源/路径检查 + 双包打包 + 隔离自检 → `sccache`）；`deny.toml` 开启 `multiple-versions-include-dev = true` 并显式登记测试依赖 `windows-link@0.2.1`；`.github/workflows/ci.yml` 拆分为 `ubuntu-latest`（`cargo-deny-action` + 声明同步检查）与 `windows-latest`（完整执行 `./scripts/check.sh` 并上传产物）。
- **P3-2 测试补强**：5 个 crate 均已大幅扩充单元测试（覆盖机器指纹、端口白名单/UNC/`IP_`、RFC 7914 与 v1 `.spdf` 夹具、微点往返、中英文水印像素断言、巡检边界、按键过滤、策略校验、卸载脚本 `%`/中文转义、PE 截断、架构常量等）。
- **P3-3 发布卫生**：删除 `Cargo.toml` 占位 `repository`；根目录补齐 `LICENSE-MIT`、`LICENSE-APACHE`、`.gitignore`；`gen-notices.py` 自动从依赖树生成涵盖阅读端/创建端/安装器、PDFium、Windows App SDK Bootstrap 与全部静态链接 crate 的 `assets/THIRD-PARTY-NOTICES.txt`；`build-env.sh` 配置 `--remap-path-prefix` 消除本机路径；三个二进制的 `build.rs` 编译嵌入 `assets/icons/{reader,creator,installer}.ico` 与中文 `VERSIONINFO` 资源；界面文案已去除“Rust / WinUI / O(W) / 硬件级防截屏”等开发术语与失实表述（并有单测守卫）。

---

## 3. 当前门禁与构建产物实测状态（2026-11-17 核查）

1. **第三方声明同步检查**：`python3 scripts/gen-notices.py --check` → **通过**（`THIRD-PARTY-NOTICES.txt is up to date`）。
2. **依赖许可证与安全门禁**：`cargo deny --offline check` → **通过**（`advisories ok, bans ok, licenses ok, sources ok`）。
3. **代码格式检查**：`cargo fmt --all -- --check` → **未格式化（退出码 1）**，仅 `crates/secure_pdf_core/src/{lib.rs, printing.rs, storage.rs}` 有 3 处换行差异，运行一次 `cargo fmt --all` 即可消除。
4. **Release 编译与安装包状态**：
   - `target/x86_64-pc-windows-msvc/release/SecurePdfInstaller.exe`（`943 KB`，`2026-11-17 02:14:39`）：**已更新**。
   - `target/x86_64-pc-windows-msvc/release/SecurePdfCreator.exe`（`2.22 MB`，`2026-11-17 02:14:40`）：**已更新**，且已在 `target/dist_probe/` 完成试打包（`SecurePdfCreatorSetup.exe`，`1.82 MB`）与隔离自检（`target/dist_probe/selftest.log` 全部检查项 `ok`，末尾 `SecurePdfInstaller selftest: OK`）。
   - `target/x86_64-pc-windows-msvc/release/SecurePdfReader.exe`（`2026-11-15 20:29:51`）：**尚未重编 Release 版**（`02:16`–`02:29` 刚在 `target/fork-reader` 完成 Debug/Test 编译）。
   - `target/dist/` 下的 `SecurePdfCreatorSetup.exe` / `SecurePdfReaderSetup.exe`：**仍是 `2026-11-15` 旧包**，待跑 `scripts/package.sh` 刷新。

---

## 4. 下一步断点恢复清单（按序执行）

1. **统一格式化并跑通全量质量门禁（含测试、Release 构建、正式打包与双端隔离自检）**
   - 执行 `cargo fmt --all`
   - 在非沙箱 WSL2 终端（需允许调用 Windows `.exe` interop）运行：
     ```bash
     DENY_OFFLINE=1 ./scripts/check.sh
     ```
     确认 `[1/7]` `fmt`、`[2/7]` `gen-notices`、`[3/7]` `cargo deny`、`[4/7]` `clippy -D warnings`、`[5/7]` 全工作区 Windows 单元测试、`[6/7]` `package.sh`（Release 构建 + PE 导入/标志/版本资源/路径脱敏检查 + 生成 `target/dist/SecurePdf{Reader,Creator}Setup.exe` + 两份安装包 `INSTALLER_SELFTEST` 隔离自检）全部通过。
2. **清理临时构建目录**
   - 删除并行子任务遗留的 `target/fork-creator`、`target/fork-installer`、`target/fork-reader`、`target/dist_probe`。
3. **完成 `P0-5` 文档重写与发布物整理**
   - 重写 `README.md` 与 `github_upload/README.md`（更新 2.0.0 架构、Windows App Runtime 1.6+ 前置要求、构建与打包命令、新旧兼容的 `--silent` 参数与退出码 `0`～`6` 表、卸载机制、与 v1.1.0 差异）；将 `github_upload/docs/` 旧版 C# 文档归档至明确标注 `v1` 的目录。
   - 用 `target/dist/` 中通过自检的新版安装包更新根目录安装包。
4. **回填 `REMEDIATION.md` 与真机交互验收**
   - 在 `REMEDIATION.md` 已验证通过的条目后标注 `[已完成]` 与验证方式；
   - 按 `REMEDIATION.md` 末尾表格在环境 A/B/C/D 完成真机 UI、防截屏、物理打印、到期清屏与翻拍取证验收，并在正式发布前清理 `.backups/` 与过期的过程备份文件。

---

## 5. 当前工作区文件结构索引

```text
SecurePdfSuite/
├── .backups/
│   ├── claude_snapshot_20261117.tar.gz         <- Claude 进度无损源码快照（118 项）
│   ├── claude_snapshot_20261117.tar.gz.sha256  <- 快照 SHA-256 校验值
│   └── PROGRESS_BACKUP_20261115.md             <- 2026-11-15 上一版进度备份
├── .cargo/config.toml                          <- sccache + x86_64-pc-windows-msvc 静态 CRT 配置
├── .github/workflows/ci.yml                    <- Ubuntu 供应链检查 + Windows 全门禁 CI
├── .gitignore
├── Cargo.toml / Cargo.lock
├── LICENSE-APACHE / LICENSE-MIT
├── clippy.toml / deny.toml / rust-toolchain.toml / rustfmt.toml
├── PROGRESS_BACKUP.md                          <- 本进度与断点备份文档
├── REMEDIATION.md                              <- 2.0.0 审核整改单（待回填状态与真机记录）
├── README.md                                   <- 仍为 v1.1.0 文档（待 P0-5 重写）
├── scripts/
│   ├── build-env.sh                            <- 统一 PATH、cargo-xwin、静态 CRT 与路径映射
│   ├── check.sh                                <- [1/7]~[7/7] 全量质量门禁脚本
│   ├── gen-notices.py                          <- 自动生成/校验第三方组件声明
│   └── package.sh                              <- Release 构建、PE 卫生检查、打包与隔离自检
├── assets/
│   ├── Microsoft.WindowsAppRuntime.Bootstrap.dll
│   ├── THIRD-PARTY-NOTICES.txt
│   ├── icons/{creator.ico, installer.ico, reader.ico}
│   ├── pdfium.dll
│   └── 关联spdf文件格式.bat
└── crates/
    ├── secure_pdf_core/
    │   ├── src/{bindings.rs, crypto.rs, error.rs, forensics.rs, format.rs, lib.rs,
    │   │        licensing.rs, localtime.rs, pdf.rs, printing.rs, storage.rs,
    │   │        watermark.rs, win32_util.rs}
    │   └── tests/fixtures/{v1_default_policy.spdf, v1_expiring_noprint.spdf,
    │                       v1_plain.pdf, v2_generated.spdf}
    ├── winui3/
    │   └── src/{app.rs, bindings.rs, bootstrap.rs, controls.rs, dialog.rs,
    │            interop.rs, lib.rs, window.rs}
    ├── secure_pdf_creator/
    │   ├── build.rs, app.manifest
    │   └── src/{encrypt_tab.rs, forensics_tab.rs, logic.rs, main.rs, shell.rs}
    ├── secure_pdf_reader/
    │   ├── build.rs, app.manifest
    │   └── src/{debug_demo.rs, logic.rs, main.rs, print_dialog.rs, protection.rs,
    │            reader_window.rs, render_worker.rs, session.rs, unlock_window.rs}
    └── secure_pdf_installer/
        ├── build.rs, app.manifest
        └── src/{bindings.rs, cab.rs, lib.rs, logging.rs, main.rs, package.rs,
                 paths.rs, registry.rs, selftest.rs, uninstall.rs, win32_install.rs,
                 wizard.rs}
```
