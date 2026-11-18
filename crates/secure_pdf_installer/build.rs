//! Build script for `SecurePdfInstaller`: embeds the application manifest, the icon and version
//! resources, and hardens how statically imported DLLs are resolved.

use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_FILE: &str = "installer.ico";
const FILE_DESCRIPTION: &str = "SecurePdfSuite 安装程序";
const INTERNAL_NAME: &str = "SecurePdfInstaller";

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let manifest_path = manifest_dir.join("app.manifest");
    let icon_path = manifest_dir.join("../../assets/icons").join(ICON_FILE);
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", icon_path.display());
    println!("cargo:rerun-if-env-changed=SECUREPDF_RC");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
    // Resolve statically imported DLLs from System32 only (LOAD_LIBRARY_SEARCH_SYSTEM32), so
    // a DLL planted next to the executable cannot be loaded in place of a system library.
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    match compile_resources(&out_dir, &icon_path) {
        Ok(res_path) => println!("cargo:rustc-link-arg-bins={}", res_path.display()),
        Err(message) => {
            println!("cargo:warning={message}");
            std::process::exit(1);
        }
    }
}

/// Writes the resource script (icon + `VERSIONINFO`) and compiles it to a `.res` file.
fn compile_resources(out_dir: &Path, icon_path: &Path) -> Result<PathBuf, String> {
    let env = |key: &str| std::env::var(key).unwrap_or_default();
    let (major, minor, patch) = (
        env("CARGO_PKG_VERSION_MAJOR"),
        env("CARGO_PKG_VERSION_MINOR"),
        env("CARGO_PKG_VERSION_PATCH"),
    );
    let version_text = env("CARGO_PKG_VERSION");
    let authors = env("CARGO_PKG_AUTHORS").replace(':', ", ");
    let icon = icon_path.display().to_string().replace('\\', "/");

    let script = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "080404B0"
    BEGIN
      VALUE "CompanyName", "{authors}"
      VALUE "FileDescription", "{FILE_DESCRIPTION}"
      VALUE "FileVersion", "{version_text}.0"
      VALUE "InternalName", "{INTERNAL_NAME}"
      VALUE "LegalCopyright", "Copyright (C) 2026 {authors}"
      VALUE "OriginalFilename", "{INTERNAL_NAME}.exe"
      VALUE "ProductName", "SecurePdfSuite"
      VALUE "ProductVersion", "{version_text}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x804, 1200
  END
END
"#
    );
    let rc_path = out_dir.join("app.rc");
    let res_path = out_dir.join("app.res");
    std::fs::write(&rc_path, script)
        .map_err(|e| format!("cannot write {}: {e}", rc_path.display()))?;

    let mut attempts = Vec::new();
    for compiler in resource_compilers() {
        let is_llvm = compiler
            .file_stem()
            .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case("llvm-rc"));
        let mut cmd = Command::new(&compiler);
        if is_llvm {
            cmd.args(["/no-preprocess", "/C", "65001", "/FO"]);
        } else {
            cmd.args(["/nologo", "/c65001", "/fo"]);
        }
        cmd.arg(&res_path).arg(&rc_path);
        match cmd.output() {
            Ok(output) if output.status.success() && res_path.is_file() => return Ok(res_path),
            Ok(output) => attempts.push(format!(
                "{}: {}",
                compiler.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(err) => attempts.push(format!("{}: {err}", compiler.display())),
        }
    }
    Err(format!(
        "no working Windows resource compiler (tried {}); install LLVM (`llvm-rc`) or the Windows \
         SDK (`rc.exe`), or point the SECUREPDF_RC environment variable at one",
        attempts.join("; ")
    ))
}

/// Candidate resource compilers: explicit override, `llvm-rc`, then the Windows SDK `rc.exe`.
fn resource_compilers() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(explicit) = std::env::var_os("SECUREPDF_RC") {
        list.push(PathBuf::from(explicit));
    }
    list.push(PathBuf::from("llvm-rc"));
    list.push(PathBuf::from("rc.exe"));
    // rc.exe is not on PATH outside a Visual Studio developer prompt; probe the SDK directly.
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits) {
        let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        list.extend(
            versions
                .into_iter()
                .rev()
                .map(|dir| dir.join("x64").join("rc.exe"))
                .filter(|path| path.is_file()),
        );
    }
    list
}
