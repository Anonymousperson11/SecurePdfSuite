//! Self-extracting `SPDFPKG1` LZMS payload packer, SHA-256 verifier, and archive codec.

use core::ptr::null_mut;
use std::fs;
use std::path::Path;

use windows_core::PCWSTR;

use crate::bindings;
use crate::cab::{MAX_DECOMPRESSED_PAYLOAD_BYTES, compress_lzms, decompress_lzms};

const ARCHIVE_MAGIC: &[u8; 8] = b"SPDFARC1";
const TRAILER_MAGIC: &[u8; 8] = b"SPDFPKG1";
const TRAILER_LEN: usize = 48; // magic(8) + payload_len(u64 LE = 8) + payload_sha256(32)
const MAX_ARCHIVE_FILES: usize = 4096;

/// Packaged application product flavor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppKind {
    /// Batch `.spdf` Creator & watermark forensics workbench.
    Creator = 1,
    /// Protected `.spdf` document Reader.
    Reader = 2,
}

impl AppKind {
    /// Parses flavor code from wire tag byte.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Creator),
            2 => Some(Self::Reader),
            _ => None,
        }
    }

    /// Friendly Chinese + English product display title.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Creator => "安全 PDF 加密与取证工具 (SecurePdfCreator)",
            Self::Reader => "受保护 PDF 阅读器 (SecurePdfReader)",
        }
    }

    /// Main installed EXE filename.
    #[must_use]
    pub const fn exe_name(self) -> &'static str {
        match self {
            Self::Creator => "SecurePdfCreator.exe",
            Self::Reader => "SecurePdfReader.exe",
        }
    }

    /// Default subdirectory under `%LOCALAPPDATA%\Programs\`.
    #[must_use]
    pub const fn install_folder_name(self) -> &'static str {
        match self {
            Self::Creator => "SecurePdfCreator",
            Self::Reader => "SecurePdfReader",
        }
    }

    /// Uninstall helper script filename inside installation directory.
    #[must_use]
    pub const fn uninstall_script_name(self) -> &'static str {
        match self {
            Self::Creator => "uninstall_creator.bat",
            Self::Reader => "uninstall_reader.bat",
        }
    }

    /// Shortcut file stem (without `.lnk`), identical to v1.x so upgrades replace it in place.
    #[must_use]
    pub const fn shortcut_name(self) -> &'static str {
        match self {
            Self::Creator => "SecurePdf 创建端",
            Self::Reader => "SecurePdf 阅读端",
        }
    }

    /// Shortcut tooltip text.
    #[must_use]
    pub const fn shortcut_description(self) -> &'static str {
        match self {
            Self::Creator => "创建加密 PDF 并自动生成验证码",
            Self::Reader => "受控阅读加密 .spdf 文档",
        }
    }

    /// Key name under `...\CurrentVersion\Uninstall`, identical to v1.x so the entry is replaced.
    #[must_use]
    pub const fn uninstall_key_name(self) -> &'static str {
        match self {
            Self::Creator => "SecurePdfCreator",
            Self::Reader => "SecurePdfReader",
        }
    }

    /// Name shown in "Apps & features".
    #[must_use]
    pub const fn uninstall_display_name(self) -> &'static str {
        match self {
            Self::Creator => "SecurePdf 创建端 (发送方)",
            Self::Reader => "SecurePdf 阅读端 (接收方)",
        }
    }

    /// The other product flavor (used to diagnose a `pack --app` pointed at the wrong folder).
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Creator => Self::Reader,
            Self::Reader => Self::Creator,
        }
    }
}

/// Single file entry stored inside a `SPDFPKG1` payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    /// Normalized relative forward-slash path (validated against traversal).
    pub rel_path: String,
    /// Raw uncompressed file contents.
    pub data: Vec<u8>,
}

struct AlgGuard(bindings::BCRYPT_ALG_HANDLE);
impl Drop for AlgGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid CNG algorithm handle opened by `BCryptOpenAlgorithmProvider`.
            let _ = unsafe { bindings::BCryptCloseAlgorithmProvider(self.0, 0) };
            self.0 = bindings::BCRYPT_ALG_HANDLE(null_mut());
        }
    }
}

struct HashGuard(bindings::BCRYPT_HASH_HANDLE);
impl Drop for HashGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid CNG hash handle created by `BCryptCreateHash`.
            let _ = unsafe { bindings::BCryptDestroyHash(self.0) };
            self.0 = bindings::BCRYPT_HASH_HANDLE(null_mut());
        }
    }
}

/// Computes SHA-256 via Windows CNG (`bcrypt.dll`) cleanly across all Windows versions.
///
/// # Errors
///
/// Returns an error if CNG algorithm provider or hash computation fails.
pub fn sha256_bytes(data: &[u8]) -> Result<[u8; 32], String> {
    let mut alg = bindings::BCRYPT_ALG_HANDLE(null_mut());
    // SAFETY: Opens stateless CNG SHA-256 provider.
    let st = unsafe {
        bindings::BCryptOpenAlgorithmProvider(
            &raw mut alg,
            bindings::BCRYPT_SHA256_ALGORITHM,
            PCWSTR::null(),
            0,
        )
    };
    if st.0 < 0 || alg.0.is_null() {
        return Err(format!(
            "BCryptOpenAlgorithmProvider(SHA256) 失败：0x{:08X}",
            st.0
        ));
    }
    let alg_guard = AlgGuard(alg);

    let mut hash = bindings::BCRYPT_HASH_HANDLE(null_mut());
    // SAFETY: Creates reusable CNG hash object with provider-managed heap buffer.
    let st_create =
        unsafe { bindings::BCryptCreateHash(alg_guard.0, &raw mut hash, None, 0, None, 0) };
    if st_create.0 < 0 || hash.0.is_null() {
        return Err(format!("BCryptCreateHash 失败：0x{:08X}", st_create.0));
    }
    let hash_guard = HashGuard(hash);

    if !data.is_empty() {
        // SAFETY: Hashes caller slice into active CNG hash handle.
        let st_data = unsafe { bindings::BCryptHashData(hash_guard.0, data, 0) };
        if st_data.0 < 0 {
            return Err(format!("BCryptHashData 失败：0x{:08X}", st_data.0));
        }
    }

    let mut digest = [0_u8; 32];
    // SAFETY: Writes 32-byte SHA-256 digest into fixed stack array.
    let st_fin = unsafe { bindings::BCryptFinishHash(hash_guard.0, digest.as_mut_ptr(), 32, 0) };
    if st_fin.0 < 0 {
        return Err(format!("BCryptFinishHash 失败：0x{:08X}", st_fin.0));
    }
    Ok(digest)
}

/// Maximum characters allowed in one path component (v1.x `IsSafeFileName` limit).
pub const MAX_COMPONENT_CHARS: usize = 200;

/// Returns `true` for Windows reserved device names (`CON`, `NUL`, `COM1`, `LPT¹` ...), with or
/// without an extension.
#[must_use]
pub fn is_reserved_device_name(component: &str) -> bool {
    let base = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ');
    let upper = base.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let (Some(digit), None) = (chars.next(), chars.next()) else {
        return false;
    };
    (prefix == "COM" || prefix == "LPT")
        && (matches!(digit, '1'..='9') || matches!(digit, '¹' | '²' | '³'))
}

/// Checks one file or directory name against the Win32 naming rules that matter for safety.
///
/// Rejects empty / `.` / `..` names, a trailing dot or space (silently stripped by Win32, so
/// two entries could collide), reserved or control characters, and device names.
#[must_use]
pub fn is_safe_file_component(component: &str) -> bool {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.chars().count() > MAX_COMPONENT_CHARS
        || component.ends_with('.')
        || component.ends_with(' ')
    {
        return false;
    }
    if component.chars().any(|c| {
        c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return false;
    }
    !is_reserved_device_name(component)
}

/// Validates and normalizes an archive relative path, blocking `ZipSlip` / drive / root escapes.
///
/// # Errors
///
/// Returns an error if `raw` contains traversal (`..`), drive prefixes, root slashes, control
/// characters, reserved device names, or components ending in a dot or space.
pub fn sanitize_relative_path(raw: &str) -> Result<String, String> {
    let unified = raw.replace('\\', "/");
    if unified.is_empty() || unified.contains('\0') || unified.starts_with('/') {
        return Err(format!("检测到非法安装包路径：{raw}"));
    }
    let mut clean_parts = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("检测到路径越权企图：{raw}")),
            _ => {
                if !is_safe_file_component(part) {
                    return Err(format!("检测到非法文件名：{raw}"));
                }
                clean_parts.push(part);
            }
        }
    }
    if clean_parts.is_empty() {
        return Err("安装包路径不能为空。".to_owned());
    }
    Ok(clean_parts.join("/"))
}

/// Rejects entry lists in which two paths differ only by letter case (one would overwrite the
/// other on a case-insensitive file system).
///
/// # Errors
///
/// Returns an error naming the first duplicated path.
pub fn ensure_no_case_insensitive_duplicates(entries: &[ArchiveEntry]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::with_capacity(entries.len());
    for entry in entries {
        let key = entry.rel_path.replace('\\', "/").to_lowercase();
        if !seen.insert(key) {
            return Err(format!(
                "安装包内存在仅大小写不同的重复文件：{}",
                entry.rel_path
            ));
        }
    }
    Ok(())
}

/// Verifies that the payload contains the main executable of `app_kind` at its top level.
///
/// # Errors
///
/// Returns an error if the main executable is missing or empty.
pub fn ensure_main_executable(app_kind: AppKind, entries: &[ArchiveEntry]) -> Result<(), String> {
    let has = |name: &str| {
        entries
            .iter()
            .any(|e| e.rel_path.eq_ignore_ascii_case(name) && !e.data.is_empty())
    };
    if has(app_kind.exe_name()) {
        return Ok(());
    }
    if has(app_kind.other().exe_name()) {
        return Err(format!(
            "安装内容中没有主程序 {}，却包含 {}：应用类型与文件目录不匹配。",
            app_kind.exe_name(),
            app_kind.other().exe_name()
        ));
    }
    Err(format!("安装内容中缺少主程序 {}。", app_kind.exe_name()))
}

/// Collects all regular files recursively under `source_dir` with sanitized relative paths.
///
/// # Errors
///
/// Returns an error if `source_dir` is unreadable or exceeds the entry limit.
pub fn collect_directory_entries(source_dir: &Path) -> Result<Vec<ArchiveEntry>, String> {
    if !source_dir.is_dir() {
        return Err(format!("打包源目录不存在：{}", source_dir.display()));
    }
    let mut entries = Vec::new();
    collect_dir_inner(source_dir, source_dir, &mut entries, 0)?;
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    if entries.is_empty() {
        return Err("打包源目录为空，未找到任何文件。".to_owned());
    }
    Ok(entries)
}

fn collect_dir_inner(
    root: &Path,
    cur: &Path,
    out: &mut Vec<ArchiveEntry>,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("目录嵌套深度超过 16 层限制。".to_owned());
    }
    let rd = fs::read_dir(cur).map_err(|e| format!("读取目录 {} 失败：{e}", cur.display()))?;
    for item in rd {
        let entry = item.map_err(|e| format!("遍历目录项失败：{e}"))?;
        let ftype = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败：{e}"))?;
        if ftype.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ftype.is_dir() {
            collect_dir_inner(root, &path, out, depth + 1)?;
        } else if ftype.is_file() {
            if out.len() >= MAX_ARCHIVE_FILES {
                return Err("待打包文件数量超出安全上限。".to_owned());
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|_| "相对路径提取失败。".to_owned())?;
            let clean = sanitize_relative_path(&rel.to_string_lossy())?;
            let data =
                fs::read(&path).map_err(|e| format!("读取文件 {} 失败：{e}", path.display()))?;
            out.push(ArchiveEntry {
                rel_path: clean,
                data,
            });
        }
    }
    Ok(())
}

/// Strips any trailing `SPDFPKG1` overlay from `exe_bytes` so an installer can also re-pack cleanly.
#[must_use]
pub fn strip_existing_overlay(exe_bytes: &[u8]) -> &[u8] {
    if exe_bytes.len() < TRAILER_LEN {
        return exe_bytes;
    }
    let trailer_start = exe_bytes.len() - TRAILER_LEN;
    let trailer = &exe_bytes[trailer_start..];
    if &trailer[..8] != TRAILER_MAGIC {
        return exe_bytes;
    }
    let payload_len = usize::try_from(u64::from_le_bytes(
        trailer[8..16].try_into().unwrap_or([0; 8]),
    ))
    .unwrap_or(usize::MAX);
    if payload_len <= trailer_start {
        &exe_bytes[..trailer_start - payload_len]
    } else {
        exe_bytes
    }
}

/// Appends an LZMS-compressed `SPDFPKG1` overlay payload + SHA-256 trailer onto `base_exe`.
///
/// # Errors
///
/// Returns an error if compression, hashing, or path validation fails.
pub fn pack_installer_executable(
    base_exe: &[u8],
    app_kind: AppKind,
    entries: &[ArchiveEntry],
) -> Result<Vec<u8>, String> {
    ensure_main_executable(app_kind, entries)?;
    let stub = strip_existing_overlay(base_exe);
    let raw_stream = serialize_entries(entries)?;
    let raw_len =
        u64::try_from(raw_stream.len()).map_err(|_| "安装包原始数据长度超出范围。".to_owned())?;
    let raw_sha = sha256_bytes(&raw_stream)?;
    let compressed = compress_lzms(&raw_stream)?;

    let mut payload = Vec::with_capacity(8 + 1 + 8 + 32 + compressed.len());
    payload.extend_from_slice(ARCHIVE_MAGIC);
    payload.push(app_kind as u8);
    payload.extend_from_slice(&raw_len.to_le_bytes());
    payload.extend_from_slice(&raw_sha);
    payload.extend_from_slice(&compressed);

    let payload_len =
        u64::try_from(payload.len()).map_err(|_| "安装包载荷长度超出范围。".to_owned())?;
    let payload_sha = sha256_bytes(&payload)?;

    let mut out = Vec::with_capacity(stub.len() + payload.len() + TRAILER_LEN);
    out.extend_from_slice(stub);
    out.extend_from_slice(&payload);
    out.extend_from_slice(TRAILER_MAGIC);
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&payload_sha);
    Ok(out)
}

/// Message shown when the installer file was cut short (for example an interrupted download).
pub const INCOMPLETE_PACKAGE_MESSAGE: &str = "安装包不完整（可能下载未完成），请重新获取安装包。";
/// Message shown when the embedded payload fails its integrity check.
pub const CORRUPT_PACKAGE_MESSAGE: &str = "安装包校验失败（文件已损坏），请重新获取安装包。";

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<usize> {
    let raw = bytes.get(offset..offset.checked_add(2)?)?;
    Some(usize::from(u16::from_le_bytes(raw.try_into().ok()?)))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<usize> {
    let raw = bytes.get(offset..offset.checked_add(4)?)?;
    usize::try_from(u32::from_le_bytes(raw.try_into().ok()?)).ok()
}

/// File offset at which the PE image proper ends (end of the last section's raw data or of the
/// certificate table). Returns `None` when `bytes` does not start with a well-formed PE header.
#[must_use]
pub fn pe_image_end(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..2)? != b"MZ" {
        return None;
    }
    let pe_offset = read_u32_le(bytes, 0x3C)?;
    if bytes.get(pe_offset..pe_offset.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let coff = pe_offset + 4;
    let section_count = read_u16_le(bytes, coff + 2)?;
    let optional_size = read_u16_le(bytes, coff + 16)?;
    let optional = coff + 20;
    let section_table = optional.checked_add(optional_size)?;
    let mut end = section_table.checked_add(section_count.checked_mul(40)?)?;
    for index in 0..section_count {
        let header = section_table + index * 40;
        let raw_size = read_u32_le(bytes, header + 16)?;
        let raw_pointer = read_u32_le(bytes, header + 20)?;
        if raw_size > 0 {
            end = end.max(raw_pointer.checked_add(raw_size)?);
        }
    }
    // Attribute certificate table (data directory 4) is addressed by file offset.
    let directories = match read_u16_le(bytes, optional)? {
        0x20B => optional + 112,
        0x10B => optional + 96,
        _ => return Some(end),
    };
    let cert_entry = directories + 4 * 8;
    if cert_entry + 8 <= section_table
        && let (Some(cert_offset), Some(cert_size)) = (
            read_u32_le(bytes, cert_entry),
            read_u32_le(bytes, cert_entry + 4),
        )
        && cert_offset > 0
        && cert_size > 0
    {
        end = end.max(cert_offset.checked_add(cert_size)?);
    }
    Some(end)
}

/// Reads and verifies an embedded `SPDFPKG1` overlay from `exe_bytes`, if present.
///
/// `Ok(None)` means the executable genuinely carries no payload (a bare installer stub). An
/// executable whose payload was cut short is reported as an incomplete package, not as "no
/// payload". The embedded hashes detect accidental corruption only: they are stored in the same
/// file as the payload and therefore offer no protection against deliberate modification.
///
/// # Errors
///
/// Returns [`INCOMPLETE_PACKAGE_MESSAGE`] for truncated files, [`CORRUPT_PACKAGE_MESSAGE`] for
/// integrity failures, or a specific message for structurally unsafe content.
pub fn read_embedded_package(
    exe_bytes: &[u8],
) -> Result<Option<(AppKind, Vec<ArchiveEntry>)>, String> {
    let image_end = pe_image_end(exe_bytes);
    let trailer_start = exe_bytes.len().checked_sub(TRAILER_LEN);
    let trailer = trailer_start
        .map(|start| &exe_bytes[start..])
        .filter(|t| &t[..8] == TRAILER_MAGIC);
    let (Some(trailer_start), Some(trailer)) = (trailer_start, trailer) else {
        // No trailer: either a bare stub, or a package whose tail is missing.
        return match image_end {
            Some(end) if exe_bytes.len() != end => Err(INCOMPLETE_PACKAGE_MESSAGE.to_owned()),
            _ => Ok(None),
        };
    };

    let payload_len = usize::try_from(u64::from_le_bytes(
        trailer[8..16].try_into().unwrap_or([0; 8]),
    ))
    .map_err(|_| CORRUPT_PACKAGE_MESSAGE.to_owned())?;
    if payload_len < 49 || payload_len > trailer_start {
        return Err(CORRUPT_PACKAGE_MESSAGE.to_owned());
    }
    let payload_start = trailer_start - payload_len;
    if image_end.is_some_and(|end| payload_start < end) {
        return Err(CORRUPT_PACKAGE_MESSAGE.to_owned());
    }
    let payload = &exe_bytes[payload_start..trailer_start];

    let expected_payload_sha: [u8; 32] = trailer[16..48].try_into().unwrap_or([0; 32]);
    let actual_payload_sha = sha256_bytes(payload)?;
    if !ct_eq_32(&expected_payload_sha, &actual_payload_sha) {
        return Err(CORRUPT_PACKAGE_MESSAGE.to_owned());
    }

    if &payload[..8] != ARCHIVE_MAGIC {
        return Err("安装包内部格式无效。".to_owned());
    }
    let app_kind =
        AppKind::from_tag(payload[8]).ok_or_else(|| "未知的安装包目标应用类型。".to_owned())?;
    let raw_len = usize::try_from(u64::from_le_bytes(
        payload[9..17].try_into().unwrap_or([0; 8]),
    ))
    .map_err(|_| "解压目标大小超出地址空间。".to_owned())?;
    if raw_len > MAX_DECOMPRESSED_PAYLOAD_BYTES {
        return Err("安装包解压大小超出 512 MiB 安全限制。".to_owned());
    }
    let expected_raw_sha: [u8; 32] = payload[17..49].try_into().unwrap_or([0; 32]);
    let compressed = &payload[49..];

    let raw = decompress_lzms(compressed, raw_len)?;
    let actual_raw_sha = sha256_bytes(&raw)?;
    if !ct_eq_32(&expected_raw_sha, &actual_raw_sha) {
        return Err(CORRUPT_PACKAGE_MESSAGE.to_owned());
    }

    let entries = deserialize_entries(&raw)?;
    ensure_main_executable(app_kind, &entries)?;
    Ok(Some((app_kind, entries)))
}

fn ct_eq_32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0_u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

fn serialize_entries(entries: &[ArchiveEntry]) -> Result<Vec<u8>, String> {
    if entries.len() > MAX_ARCHIVE_FILES {
        return Err("打包文件总数超出限制。".to_owned());
    }
    ensure_no_case_insensitive_duplicates(entries)?;
    let count = u32::try_from(entries.len()).map_err(|_| "文件数量溢出。".to_owned())?;
    let mut raw = Vec::new();
    raw.extend_from_slice(&count.to_le_bytes());

    for entry in entries {
        let clean = sanitize_relative_path(&entry.rel_path)?;
        let path_bytes = clean.as_bytes();
        let path_len = u16::try_from(path_bytes.len()).map_err(|_| format!("路径过长：{clean}"))?;
        let data_len =
            u64::try_from(entry.data.len()).map_err(|_| format!("文件体积过大：{clean}"))?;
        raw.extend_from_slice(&path_len.to_le_bytes());
        raw.extend_from_slice(path_bytes);
        raw.extend_from_slice(&data_len.to_le_bytes());
        raw.extend_from_slice(&entry.data);
    }
    Ok(raw)
}

fn deserialize_entries(raw: &[u8]) -> Result<Vec<ArchiveEntry>, String> {
    if raw.len() < 4 {
        return Err("归档流长度非法。".to_owned());
    }
    let count = usize::try_from(u32::from_le_bytes(raw[..4].try_into().unwrap_or([0; 4])))
        .map_err(|_| "归档文件计数非法。".to_owned())?;
    if count > MAX_ARCHIVE_FILES {
        return Err("归档文件数超出安全限制。".to_owned());
    }

    let mut pos = 4_usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let path_len_slice = raw
            .get(pos..pos + 2)
            .ok_or_else(|| "归档流提前结束（路径长度）。".to_owned())?;
        let path_len = usize::from(u16::from_le_bytes(
            path_len_slice.try_into().unwrap_or([0; 2]),
        ));
        pos += 2;

        let path_bytes = raw
            .get(pos..pos + path_len)
            .ok_or_else(|| "归档流提前结束（路径内容）。".to_owned())?;
        let raw_path =
            std::str::from_utf8(path_bytes).map_err(|_| "归档内路径包含无效 UTF-8。".to_owned())?;
        let clean_path = sanitize_relative_path(raw_path)?;
        pos += path_len;

        let data_len_slice = raw
            .get(pos..pos + 8)
            .ok_or_else(|| "归档流提前结束（文件大小）。".to_owned())?;
        let data_len = usize::try_from(u64::from_le_bytes(
            data_len_slice.try_into().unwrap_or([0; 8]),
        ))
        .map_err(|_| "文件长度溢出。".to_owned())?;
        pos += 8;

        let data_slice = raw
            .get(pos..pos + data_len)
            .ok_or_else(|| "归档流提前结束（文件内容）。".to_owned())?;
        pos += data_len;

        entries.push(ArchiveEntry {
            rel_path: clean_path,
            data: data_slice.to_vec(),
        });
    }
    if pos != raw.len() {
        return Err("归档尾部存在未解析的多余字节。".to_owned());
    }
    ensure_no_case_insensitive_duplicates(&entries)?;
    Ok(entries)
}

/// In-memory `LZMS` pack → unpack → corruption-rejection round trip (no disk access).
///
/// This is only the codec half of the self-test; the on-disk "install → uninstall" half lives
/// in [`crate::selftest`].
///
/// # Errors
///
/// Returns an error if packing, verification, or corruption detection misbehaves.
pub fn run_pack_roundtrip_check() -> Result<(), String> {
    let stub = b"MZ_FAKE_STUB_PE_BINARY_CONTENT";
    let entries = vec![
        ArchiveEntry {
            rel_path: "SecurePdfReader.exe".to_owned(),
            data: vec![0x4D_u8; 4096],
        },
        ArchiveEntry {
            rel_path: "assets/notice.txt".to_owned(),
            data: b"LZMS 16MiB roundtrip selftest payload".to_vec(),
        },
    ];
    let packed = pack_installer_executable(stub, AppKind::Reader, &entries)?;
    if packed.is_empty() {
        return Err("打包产物为空字节流。".to_owned());
    }
    let unpacked = read_embedded_package(&packed)?
        .ok_or_else(|| "未检测到打包后的 SPDFPKG1 载荷。".to_owned())?;
    if unpacked.0 != AppKind::Reader || unpacked.1 != entries {
        return Err("LZMS 自检解包数据与原始内容不一致。".to_owned());
    }

    // Flip one payload byte: the integrity check must fail closed.
    let mut damaged = packed;
    let mid = stub.len() + 20;
    damaged[mid] ^= 0xFF;
    if read_embedded_package(&damaged).is_ok() {
        return Err("完整性校验未能拦截损坏的 SPDFPKG1 载荷！".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, data: &[u8]) -> ArchiveEntry {
        ArchiveEntry {
            rel_path: path.to_owned(),
            data: data.to_vec(),
        }
    }

    fn reader_entries() -> Vec<ArchiveEntry> {
        vec![
            entry("SecurePdfReader.exe", &[0x4D; 3000]),
            entry("docs/说明.txt", "说明文字".as_bytes()),
        ]
    }

    /// Smallest well-formed PE32+ image: headers plus one 512-byte section, 1024 bytes total.
    fn minimal_pe_stub() -> Vec<u8> {
        let mut pe = vec![0_u8; 1024];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        pe[coff..coff + 2].copy_from_slice(&0x8664_u16.to_le_bytes());
        pe[coff + 2..coff + 4].copy_from_slice(&1_u16.to_le_bytes()); // one section
        pe[coff + 16..coff + 18].copy_from_slice(&240_u16.to_le_bytes()); // optional header size
        let optional = coff + 20;
        pe[optional..optional + 2].copy_from_slice(&0x20B_u16.to_le_bytes());
        let section = optional + 240;
        pe[section..section + 5].copy_from_slice(b".text");
        pe[section + 16..section + 20].copy_from_slice(&512_u32.to_le_bytes()); // raw size
        pe[section + 20..section + 24].copy_from_slice(&512_u32.to_le_bytes()); // raw pointer
        pe
    }

    #[test]
    fn sanitize_relative_path_blocks_traversal_and_drive_escapes() {
        assert_eq!(
            sanitize_relative_path(r"sub\app.exe").unwrap(),
            "sub/app.exe"
        );
        assert!(sanitize_relative_path("../evil.dll").is_err());
        assert!(sanitize_relative_path(r"a\..\..\b").is_err());
        assert!(sanitize_relative_path(r"C:\Windows\a.dll").is_err());
        assert!(sanitize_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn sanitize_rejects_unc_ads_device_names_and_trailing_dot_or_space() {
        for bad in [
            r"\\server\share\a.dll",
            "file.txt:stream",
            "C:evil.dll",
            "NUL",
            "nul.txt",
            "sub/CON",
            "sub/con.log.txt",
            "COM1",
            "com9.dat",
            "LPT1.txt",
            "Aux",
            "PRN.",
            "CONIN$",
            "name.",
            "name ",
            "dir./file.txt",
            "dir /file.txt",
            "a<b.txt",
            "a>b.txt",
            "a|b.txt",
            "a?b.txt",
            "a*b.txt",
            "a\"b.txt",
            "tab\tname.txt",
            "",
            "   ",
            ".",
            "./.",
            "a\0b",
        ] {
            assert!(
                sanitize_relative_path(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        // Names that merely resemble device names, and ordinary Unicode names, are fine.
        for good in [
            "COM0.txt",
            "COM10.txt",
            "console.txt",
            "nullable.dll",
            "LPT.txt",
            "关联spdf文件格式.bat",
            "THIRD-PARTY-NOTICES.txt",
            "a b/c d.txt",
            ".hidden",
        ] {
            assert!(
                sanitize_relative_path(good).is_ok(),
                "{good:?} must be accepted"
            );
        }
        assert_eq!(sanitize_relative_path("./a//b.txt").unwrap(), "a/b.txt");
        let long = "x".repeat(MAX_COMPONENT_CHARS + 1);
        assert!(sanitize_relative_path(&long).is_err());
        assert!(sanitize_relative_path(&"x".repeat(MAX_COMPONENT_CHARS)).is_ok());
    }

    #[test]
    fn packing_rejects_case_insensitive_duplicates_and_missing_or_wrong_main_exe() {
        let stub = minimal_pe_stub();
        let dup = vec![
            entry("SecurePdfReader.exe", b"MZ"),
            entry("Readme.txt", b"a"),
            entry("README.TXT", b"b"),
        ];
        let err = pack_installer_executable(&stub, AppKind::Reader, &dup).unwrap_err();
        assert!(err.contains("大小写"), "{err}");
        let dup_dir = vec![
            entry("SecurePdfReader.exe", b"MZ"),
            entry("Docs/a.txt", b"a"),
            entry("docs\\A.TXT", b"b"),
        ];
        assert!(pack_installer_executable(&stub, AppKind::Reader, &dup_dir).is_err());

        // `pack --app reader` pointed at the creator's folder must not produce a package.
        let creator_files = vec![
            entry("SecurePdfCreator.exe", b"MZ"),
            entry("pdfium.dll", b"x"),
        ];
        let err = pack_installer_executable(&stub, AppKind::Reader, &creator_files).unwrap_err();
        assert!(
            err.contains("SecurePdfReader.exe") && err.contains("不匹配"),
            "{err}"
        );
        assert!(pack_installer_executable(&stub, AppKind::Creator, &creator_files).is_ok());

        let no_exe = vec![entry("pdfium.dll", b"x")];
        assert!(
            pack_installer_executable(&stub, AppKind::Creator, &no_exe)
                .unwrap_err()
                .contains("缺少主程序")
        );
        // The main exe must be at the top level and non-empty.
        let nested = vec![entry("bin/SecurePdfReader.exe", b"MZ")];
        assert!(pack_installer_executable(&stub, AppKind::Reader, &nested).is_err());
        let empty = vec![entry("SecurePdfReader.exe", b"")];
        assert!(pack_installer_executable(&stub, AppKind::Reader, &empty).is_err());
    }

    #[test]
    fn pe_image_end_matches_the_stub_and_ignores_non_pe_data() {
        let stub = minimal_pe_stub();
        assert_eq!(pe_image_end(&stub), Some(1024));
        let mut with_overlay = stub.clone();
        with_overlay.extend_from_slice(&[0xAB; 300]);
        assert_eq!(pe_image_end(&with_overlay), Some(1024));
        assert_eq!(pe_image_end(b"MZ_FAKE_STUB"), None);
        assert_eq!(pe_image_end(b""), None);
        assert_eq!(pe_image_end(&stub[..0x90]), None);
    }

    #[test]
    fn bare_stub_has_no_payload_and_packed_stub_roundtrips() {
        let stub = minimal_pe_stub();
        assert_eq!(read_embedded_package(&stub), Ok(None));

        let entries = reader_entries();
        let packed = pack_installer_executable(&stub, AppKind::Reader, &entries).unwrap();
        assert_eq!(&packed[..stub.len()], stub.as_slice());
        assert_eq!(
            read_embedded_package(&packed),
            Ok(Some((AppKind::Reader, entries.clone())))
        );
        assert_eq!(strip_existing_overlay(&packed), stub.as_slice());
        // Re-packing an already packed installer replaces the payload instead of stacking it.
        let repacked = pack_installer_executable(&packed, AppKind::Reader, &entries).unwrap();
        assert_eq!(repacked, packed);
    }

    #[test]
    fn truncated_packages_are_reported_as_incomplete_never_as_no_payload() {
        let stub = minimal_pe_stub();
        let packed = pack_installer_executable(&stub, AppKind::Reader, &reader_entries()).unwrap();
        let payload_len = packed.len() - stub.len();
        // Cut anywhere inside the appended payload / trailer: 1 byte short, mid-trailer,
        // mid-payload, and with only a single payload byte left.
        for cut in [
            1,
            10,
            TRAILER_LEN,
            TRAILER_LEN + 1,
            payload_len / 2,
            payload_len - 1,
        ] {
            let truncated = &packed[..packed.len() - cut];
            assert_eq!(
                read_embedded_package(truncated),
                Err(INCOMPLETE_PACKAGE_MESSAGE.to_owned()),
                "cut {cut} bytes"
            );
        }
        assert!(INCOMPLETE_PACKAGE_MESSAGE.contains("安装包不完整"));
        // Cutting exactly at the image end yields the bare stub again.
        assert_eq!(read_embedded_package(&packed[..stub.len()]), Ok(None));
    }

    #[test]
    fn trailer_and_payload_corruption_is_rejected_with_a_damage_message() {
        let stub = minimal_pe_stub();
        let packed = pack_installer_executable(&stub, AppKind::Reader, &reader_entries()).unwrap();
        let trailer_start = packed.len() - TRAILER_LEN;

        // Every byte of the trailer's length and hash fields, and bytes across the payload.
        let mut positions: Vec<usize> = (trailer_start + 8..packed.len()).collect();
        positions.extend([stub.len(), stub.len() + 8, stub.len() + 9, stub.len() + 20]);
        positions.extend([stub.len() + 49, trailer_start - 1]);
        for pos in positions {
            let mut damaged = packed.clone();
            damaged[pos] ^= 0x01;
            let err = read_embedded_package(&damaged).unwrap_err();
            assert_eq!(err, CORRUPT_PACKAGE_MESSAGE, "byte {pos}");
        }
        // Damaging the trailer magic makes the tail unrecognisable: reported as incomplete.
        let mut bad_magic = packed.clone();
        bad_magic[trailer_start] ^= 0x01;
        assert_eq!(
            read_embedded_package(&bad_magic),
            Err(INCOMPLETE_PACKAGE_MESSAGE.to_owned())
        );
        // A declared payload length reaching back into the PE image is rejected.
        let mut overlong = packed;
        let fake_len = u64::try_from(trailer_start - 100).unwrap();
        overlong[trailer_start + 8..trailer_start + 16].copy_from_slice(&fake_len.to_le_bytes());
        assert_eq!(
            read_embedded_package(&overlong),
            Err(CORRUPT_PACKAGE_MESSAGE.to_owned())
        );
        // The wording must not claim tamper protection the format cannot provide.
        for message in [CORRUPT_PACKAGE_MESSAGE, INCOMPLETE_PACKAGE_MESSAGE] {
            assert!(!message.contains("篡改"));
        }
    }

    #[test]
    fn reading_rejects_internally_consistent_but_unsafe_archives() {
        // Build a package whose hashes are valid but whose entry list is unsafe, by bypassing
        // the packer's own validation.
        fn package_raw(app: AppKind, raw_stream: &[u8]) -> Vec<u8> {
            let mut payload = Vec::new();
            payload.extend_from_slice(ARCHIVE_MAGIC);
            payload.push(app as u8);
            payload.extend_from_slice(&u64::try_from(raw_stream.len()).unwrap().to_le_bytes());
            payload.extend_from_slice(&sha256_bytes(raw_stream).unwrap());
            payload.extend_from_slice(&compress_lzms(raw_stream).unwrap());
            let mut out = minimal_pe_stub();
            out.extend_from_slice(&payload);
            out.extend_from_slice(TRAILER_MAGIC);
            out.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&sha256_bytes(&payload).unwrap());
            out
        }
        fn raw_stream(entries: &[(&str, &[u8])]) -> Vec<u8> {
            let mut raw = u32::try_from(entries.len()).unwrap().to_le_bytes().to_vec();
            for (path, data) in entries {
                raw.extend_from_slice(&u16::try_from(path.len()).unwrap().to_le_bytes());
                raw.extend_from_slice(path.as_bytes());
                raw.extend_from_slice(&u64::try_from(data.len()).unwrap().to_le_bytes());
                raw.extend_from_slice(data);
            }
            raw
        }

        let good = package_raw(
            AppKind::Reader,
            &raw_stream(&[("SecurePdfReader.exe", b"MZ"), ("a.txt", b"1")]),
        );
        assert!(matches!(read_embedded_package(&good), Ok(Some(_))));

        for unsafe_entries in [
            &[
                ("SecurePdfReader.exe", &b"MZ"[..]),
                ("../evil.dll", &b"x"[..]),
            ][..],
            &[("SecurePdfReader.exe", &b"MZ"[..]), ("NUL", &b"x"[..])][..],
            &[("SecurePdfReader.exe", &b"MZ"[..]), ("a.txt.", &b"x"[..])][..],
            &[
                ("SecurePdfReader.exe", &b"MZ"[..]),
                ("A.TXT", &b"x"[..]),
                ("a.txt", &b"y"[..]),
            ][..],
            &[("SecurePdfCreator.exe", &b"MZ"[..])][..],
            &[("readme.txt", &b"x"[..])][..],
        ] {
            let package = package_raw(AppKind::Reader, &raw_stream(unsafe_entries));
            assert!(
                read_embedded_package(&package).is_err(),
                "{:?} must be rejected",
                unsafe_entries.iter().map(|e| e.0).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn lzms_pack_read_and_tamper_selftest_passes() {
        run_pack_roundtrip_check().unwrap();
    }
}
