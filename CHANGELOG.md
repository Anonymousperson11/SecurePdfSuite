# 更新日志（Changelog）

本文件记录 **SecurePdfSuite** 各公开发布版本的主要架构变更、新增功能、体验优化与安全修复。

---

## [v2.0.0] - 2026-11-20

接续 `v0.0.1` 预告的全面架构重制与首次完整源码公开版本。全套件由 `.NET Framework 4.8 + WPF` 迁移至原生 **Rust 1.95（2024 Edition）+ WinUI 3**，并以 `MIT OR Apache-2.0` 双重开源许可公开全部源码、构建脚本与 CI 质量门禁。

### 重大变更与架构升级

- **原生 Rust + WinUI 3 全面重制**：
  - 拆分构建 `secure_pdf_core`、`winui3`、`secure_pdf_creator`、`secure_pdf_reader`、`secure_pdf_installer` 五个工作区 Crate。
  - 所有可执行文件静态链接 MSVC C 运行时（`+crt-static`），在未安装 VC++ Redistributable 的干净系统上可直接运行。
  - **Win10 / Win11 双路径运行库引导**：Windows 11 运行期动态解析 `TryCreatePackageDependency` / `AddPackageDependency`（无静态导入，确保 Windows 10 可正常装载进程）；Windows 10 自动回退至随包分发的 `Microsoft.WindowsAppRuntime.Bootstrap.dll`（`MddBootstrapInitialize2`），需系统安装 `Windows App Runtime 1.6+ (x64)`。
- **自研 LZMS 自解压安装器与安装包瘦身**：
  - 采用 Windows 原生 `cabinet.dll` `LZMS` 分块压缩与 `SPDFPKG1` `SHA-256` 完整性校验。
  - 安装包体积对比 `v0.0.1` 大幅缩小：`SecurePdfCreatorSetup.exe` 约 **1.88 MiB**（`1,967,767` 字节），`SecurePdfReaderSetup.exe` 约 **4.16 MiB**（`4,359,573` 字节）。
  - 支持从 1.x / 旧版原位覆盖升级并自动清理 `SecurePdf.Core.dll`、`BouncyCastle.Cryptography.dll`、`*.exe.config` 等旧版遗留文件；内置 `INSTALLER_SELFTEST` 隔离自检流程。
- **二进制安全加固**：
  - 链接期启用 `/DEPENDENTLOADFLAG:0x800` 与进程入口 `SetDefaultDllDirectories`，防止在“下载”等公共目录运行安装包或主程序时发生同目录系统 DLL 劫持。
  - Release 构建通过 `--remap-path-prefix` 脱敏，校验二进制不嵌入本机源码路径。

### 新增功能与体验优化

- **创建端（`SecurePdfCreator.exe`）**：
  - **原生内置 HEIC / HEIF 解码**：集成纯 Rust `heic-rs` 解码器，可直接导入苹果 iPhone `.HEIC` / `.HEIF` 翻拍照片进行取证分析（无需购买微软商店 HEVC 扩展），自动识别 ISO `irot`/`imir` 与 EXIF 拍摄方向并支持 >50 MP 超高清照片按比例缩放。
  - **取证增强与微点解码升级**：优化高反差翻拍场景及仅启用隐形水印时的色度提取清晰度，兼容微信压缩图片；微点矩阵解码自动忽略深色窗口边框与彩色 UI 横幅。
  - **文件拖拽与响应性优化**：在 UI STA 线程完成 `DragOperationDeferral`，修复拖拽文件/文件夹偶发卡顿或失效问题；优化跨 DPI 自适应布局。
- **阅读端（`SecurePdfReader.exe`）**：
  - **安全内存页面缓存与预取**：新增自动清零（`zeroize`）的页面位图 LRU 缓存（最多 8 页）与邻页预取，在关闭窗口、失焦锁屏、文档到期或发生异常时立即清零销毁内存明文。
  - **底层自定义打印水印**：打印时若发送方配置了自定义水印文字，将以 **80% 透明度（20% 不透明度）渲染于页面底层**（标准 PDF 空白底透出、不遮挡正文与图片）；若未配置则按原稿页面输出，不再向纸面强制打印时间/IP/设备码。
  - **防泄密与叠加层打磨**：优化蓝通道隐形水印在白纸页面上的无感隐蔽性，同时保证手机翻拍后的取证可还原度；保持 30 秒周期性 `SetWindowDisplayAffinity` 巡检、失焦全遮蔽幕布、单调+UTC 双时钟过期控制及物理打印机双重风控。

### 向后兼容性

- **`.spdf` 容器格式**：魔数、50 字节头部 AAD、策略序列化、`scrypt (N=16384, r=8, p=1)` 与 CNG `AES-256-GCM` 与 v1.x 完全一致，双向兼容加解密。
- **机器码与本地配额**：保持 `SHA-256("MG=...|BIOS=...|VOL=...|OS=<NetBIOS名>")` 指纹规则、DPAPI 熵与 73 字节双写配额结构，升级后老文档无需重新授权，已用打印次数无缝延续。

### 质量门禁验证

- 通过 `./scripts/check.sh` 全部 7 项自动化质量门禁：`cargo fmt`、`THIRD-PARTY-NOTICES.txt` 同步检查、`cargo deny` 依赖与漏洞审计、`clippy -D warnings`、全工作区 200 项单元测试、Release PE 合规检查，以及双安装包隔离“安装 → 覆盖升级 → 卸载”自检。

---

## [v0.0.1] - 2026-11-15

- 初版（`.NET Framework 4.8 + WPF`）二进制预览发布，包含基础 `.spdf` 批量加密、受控阅读、防截屏、动态摩尔纹、物理打印拦截与早期取证功能及架构设计文档。
