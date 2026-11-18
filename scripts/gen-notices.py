#!/usr/bin/env python3
"""Regenerates assets/THIRD-PARTY-NOTICES.txt from the dependency graph.

Usage:  python3 scripts/gen-notices.py           rewrite the file
        python3 scripts/gen-notices.py --check   exit 1 if the file is out of date
Run it after any change to Cargo.lock; scripts/check.sh runs the --check form.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NOTICES = os.path.join(ROOT, "assets", "THIRD-PARTY-NOTICES.txt")
# Proc-macro / build-time crates: never linked into the shipped executables.
BUILD_ONLY = {"proc-macro2", "quote", "syn", "unicode-ident", "windows-implement",
              "zeroize_derive", "zerocopy-derive"}
PDFIUM_BEGIN = "PDFium  (pdfium.dll"
PDFIUM_END = "OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."


def shipped_crates():
    out = subprocess.run(
        ["cargo", "tree", "--workspace", "--target", "x86_64-pc-windows-msvc", "-e", "normal",
         "--prefix", "none", "--format", "{p}|{l}|{r}"],
        cwd=ROOT, check=True, capture_output=True, text=True).stdout
    crates = {}
    for line in out.splitlines():
        line = line.replace(" (*)", "").replace(" (proc-macro)", "").strip()
        if line.count("|") != 2:
            continue
        pkg, lic, repo = line.split("|")
        if " v" not in pkg or "(" in pkg:      # workspace members carry a "(path)" suffix
            continue
        name, version = pkg.split(" v", 1)
        if name in BUILD_ONLY:
            continue
        lic = lic.replace("/", " OR ")
        if "MIT" not in lic and "Apache-2.0" not in lic:
            sys.exit(f"{name} {version} offers neither MIT nor Apache-2.0 ({lic}); "
                     "add its licence text to gen-notices.py before shipping")
        crates[(name, version)] = (lic, repo)
    if not crates:
        sys.exit("cargo tree returned no dependencies")
    return crates


def build(previous):
    start = previous.index(PDFIUM_BEGIN)
    end = previous.index(PDFIUM_END, start) + len(PDFIUM_END)
    pdfium = previous[start:end]
    with open(os.path.join(ROOT, "LICENSE-APACHE"), encoding="utf-8") as f:
        apache = f.read().rstrip("\n")
    rows = "\n".join(f"  {name} {version}    {lic}\n      {repo}"
                     for (name, version), (lic, repo) in sorted(shipped_crates().items()))
    return f"""SecurePdfSuite（阅读端 SecurePdfReader / 创建端 SecurePdfCreator / 安装程序）—— 第三方组件声明
==============================================================================================

本文件随阅读端与创建端的安装包一并分发，适用于 SecurePdfReader.exe、SecurePdfCreator.exe
以及安装程序本身。SecurePdfSuite 自身以 MIT 或 Apache-2.0 双许可发布。

目录
  一、随包分发的独立组件：PDFium、Windows App SDK 引导库
  二、运行所需但不随包分发的组件：Windows App Runtime
  三、静态链接进可执行文件的 Rust 组件
  四、许可全文：MIT、Apache License 2.0

==============================================================================================
一、随包分发的独立组件
==============================================================================================

{pdfium}

------------------------------------------------------------------------------
Windows App SDK 引导库  (Microsoft.WindowsAppRuntime.Bootstrap.dll，版本 2.6.5，
取自 NuGet 包 Microsoft.WindowsAppSDK.Foundation 2.6.5 的 runtimes/win-x64/native/，未做修改)
  项目主页:  https://github.com/microsoft/WindowsAppSDK
  版权:      (c) Microsoft Corporation
  许可:      Microsoft Software License Terms — Microsoft Windows App SDK
             https://www.nuget.org/packages/Microsoft.WindowsAppSDK.Foundation/2.6.5/License
  说明:      该文件属于上述许可第 3 节所述的“可分发代码”，按原样随应用分发。
             仅在 Windows 10 上用于定位已安装的 Windows App Runtime；Windows 11 上不会加载。

==============================================================================================
二、运行所需但不随包分发的组件
==============================================================================================

Windows App Runtime（Windows App SDK 运行库，1.6 或更高版本，x64）
  由用户自行从微软官方渠道安装：
             https://learn.microsoft.com/zh-cn/windows/apps/windows-app-sdk/downloads
  许可:      Microsoft Software License Terms — Microsoft Windows App SDK
  界面所用的 WinUI 3 控件由该运行库提供，本软件不包含其任何文件。

==============================================================================================
三、静态链接进可执行文件的 Rust 组件
==============================================================================================

Rust 标准库（std / core / alloc）    MIT OR Apache-2.0
      https://github.com/rust-lang/rust
      Copyright (c) The Rust Project Contributors

以下 crate 均提供 MIT 或 Apache-2.0 许可（部分另提供 Zlib / BSD / Unlicense / 0BSD 选项），
本软件按 MIT 或 Apache-2.0 使用；版权归各项目的作者与贡献者所有，详见各项目仓库。
其中 image、png、zune-jpeg、tiff、gif、weezl、fdeflate 等图像解码组件用于创建端的水印取证功能；
zeroize 与 windows-* 用于全部三个程序。

{rows}

==============================================================================================
四、许可全文
==============================================================================================

------------------------------------------------------------------------------
MIT License
------------------------------------------------------------------------------

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

------------------------------------------------------------------------------
Apache License 2.0
------------------------------------------------------------------------------

{apache}
"""


def main():
    with open(NOTICES, encoding="utf-8-sig", newline="") as f:
        previous = f.read().replace("\r\n", "\n")
    text = build(previous)
    if "--check" in sys.argv[1:]:
        if text != previous:
            sys.exit("assets/THIRD-PARTY-NOTICES.txt is out of date; run scripts/gen-notices.py")
        print("THIRD-PARTY-NOTICES.txt is up to date")
        return
    # UTF-8 with BOM and CRLF so Notepad on any Windows version shows it correctly.
    with open(NOTICES, "w", encoding="utf-8-sig", newline="\r\n") as f:
        f.write(text)
    print("wrote", NOTICES)


if __name__ == "__main__":
    main()
