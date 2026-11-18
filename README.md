# SecurePdfSuite 加密 PDF 分发、受控阅读与防翻拍套件（v1.1.0）

适用于 **Windows 10 / Windows 11 (x64)**，基于 **.NET Framework 4.8 + WPF**，无需额外安装运行库（Windows 10 1903+ / Windows 11 默认自带 .NET Framework 4.8）。

---

## 一、目录文件说明

| 文件 / 目录 | 大小 / 用途 |
| :--- | :--- |
| `install_creator.exe` | **发送端安装包**（`72,704` 字节 ≈ `71.0 KB`；解包约 `0.10 MB`，不含 `pdfium.dll`） |
| `install_reader.exe` | **阅读端安装包**（`3,024,384` 字节 ≈ `2.88 MB`；解包约 `7.58 MB`，内置无 V8/XFA 精简版 `pdfium.dll`） |
| `SecurePdfCreator/` | 本机已部署/升级的发送端目录（`SecurePdfCreator.exe`、`SecurePdfCreator.exe.config`、`SecurePdf.Core.dll`、`uninstall_creator.bat`） |
| `SecurePdfReader/` | 本机已部署/升级的阅读端目录（`SecurePdfReader.exe`、`SecurePdfReader.exe.config`、`SecurePdf.Core.dll`、`pdfium.dll`、`THIRD-PARTY-NOTICES.txt`、`关联spdf文件格式.bat`、`uninstall_reader.bat`） |
| `github_upload/` | `v1.1.0` 全量源码注解文档与从源码编译打包说明（`README.md` + `docs/01`~`05.md`） |

> 相比 v1.0.1：发送端安装包从 `4.43 MB` 降至 `71.0 KB`（解包由 `6.62 MB` 降至 `0.10 MB`），阅读端安装包从 `10.98 MB` 降至 `2.88 MB`（解包由 `21.71 MB` 降至 `7.58 MB`），完整清除 `BouncyCastle.Cryptography.dll` 与旧版 V8/XFA 原生依赖，且完全兼容 v1.0.x 已生成的 `.spdf` 文件。

---

## 二、快速使用流程

### 1. 发送方（`SecurePdfCreator`）

1. 运行安装程序 `install_creator.exe` 完成安装（如从 v1.0.x 升级，安装器会自动清理残留的 `BouncyCastle.Cryptography.dll` 并更新安全卸载脚本）。
2. 打开 **SecurePdf 创建端**，点击“添加 PDF”或将 `.pdf` 文件拖拽至列表，使用现代目录选择对话框指定输出文件夹。
3. 配置受控策略参数：
   - **最大打印次数**（`0` 表示完全禁止打印）。
   - **解锁后有效小时数**（`0` 表示永久有效；支持小数如 `0.5` 小时 = 30 分钟，`24` 小时 = 1 天）。
   - **防翻拍保护强度**（启用高频摩尔纹干扰线，并可附加蓝通道微偏移隐形取证水印）。
4. 点击 **开始批量加密生成 .spdf**：
   - 自动避让重名文件（`_1`、`_2` 递增后缀，绝不覆盖既有 `.spdf`）。
   - 支持导出 CSV 授权清单（已对单元格内容做表格公式注入转义），若存在未保存的验证码，关闭窗口前会二次提醒。
5. **翻拍照片盲取证工作台**：右侧可导入疑似泄露的手机翻拍照片（支持按 EXIF 方向自动扶正与大图降采样保护），通过 `O(W)` 低内存局部背景减除与 `ChromaStegoExtract` 蓝/绿通道提取算法，还原隐藏在画面中的**设备指纹、局域网 IP 与精确翻拍时间**。

### 2. 接收方（`SecurePdfReader`）

1. 发送安装程序 `install_reader.exe` 及对应的 `.spdf` 文档和授权码给接收方；安装时默认勾选生成桌面快捷方式与关联 `.spdf` 文件格式（如后续关联失效，可运行目录内的 `关联spdf文件格式.bat` 修复当前用户 HKCU 关联）。
2. 双击 `.spdf` 文件（或先启动 **SecurePdf 阅读端** 选择文档）输入发送方提供的授权码即可解锁阅读。
3. 受控阅读与防护机制：
   - **纯内存加密解码与渲染**：仅将解密后的 PDF 保留在内存流，通过原生 `pdfium.dll` 与 `WriteableBitmap` 按屏幕 DPI 实时栅格化渲染（上限 32M 像素预算），异常退出或关闭即刻释放明文缓冲区。
   - **系统级防截屏保护（`fail-closed`）**：调用 `SetWindowDisplayAffinity`（`WDA_EXCLUDEFROMCAPTURE`）屏蔽绝大多数桌面截图与录屏工具；若保护无法生效或在阅读中失效，程序将立即终止阅读防止泄露。
   - **动态全屏水印 & 失焦幕布**：实时叠加设备指纹、活跃本机 IPv4、北京时间与动态摩尔纹，窗口失去焦点立即覆盖隐私幕布；后台每 30 秒巡检一次系统时钟、本地配额及活跃 IP（并在检测到过期或系统时间异常回拨后锁定文档）。
   - **受控物理打印**：禁止普通打印与另存为，仅 `Ctrl+P` / “安全打印”可调用物理打印机，强制拦截 Microsoft Print to PDF、XPS、WPS PDF、Adobe PDF、传真、保留历史打印队列 (`KeepPrintedJobs`)、未认证 Local Monitor 本地端口及指向本机共享的伪物理打印机；打印超时未发送任何页面时自动回滚预扣配额。

---

## 三、命令行静默安装与卸载

两个安装器均支持无界面静默安装/覆盖升级（适合批量部署或自动化脚本）：

```bat
:: 静默升级发送端并同步桌面与开始菜单快捷方式
install_creator.exe --silent --dir "D:\SecurePdfSuite\SecurePdfCreator" --desktop --start-menu

:: 静默升级阅读端并同步快捷方式与 .spdf 关联
install_reader.exe --silent --dir "D:\SecurePdfSuite\SecurePdfReader" --desktop --start-menu --assoc
```

卸载可通过 Windows“设置 → 应用”或安装目录下的 `uninstall_creator.bat` / `uninstall_reader.bat` 完成；v1.1.0 卸载脚本仅移除程序显式文件与对应快捷方式/注册表项，非递归清理目录，安全不误删其他用户数据。

---

## 四、技术规格与安全边界说明

| 模块 | 实现规格（v1.1.0） |
| :--- | :--- |
| 密钥派生与容器加密 | 纯托管 RFC 7914 `scrypt`（`N=16384, r=8, p=1`）+ Windows CNG `bcrypt.dll` AES-256-GCM + 头部策略 AAD |
| 本地配额防篡改状态机 | Windows DPAPI (`CurrentUser`) + HMAC-SHA256 文件（`%LOCALAPPDATA%\SecurePdfSuite\Quotas`）与 HKCU 注册表双写 + `Local\` 互斥锁 |
| 自解压打包引擎 | Windows 原生 `cabinet.dll` LZMS 16 MB 分块压缩 + `SPDFPKG1` SHA-256 校验 |
| PDF 渲染内核 | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) `159.0.8152` 无 V8/XFA 精简版 `pdfium.dll` |
| 安全边界说明 | 本工具在 Windows 用户态提供纵深防护：不防物理相机拍摄全图绕过、亦无法阻止具备管理员/内核驱动权限的深度内存转储；离线计时与配额基于本地受保护存储与时钟异常回拨监测。 |
