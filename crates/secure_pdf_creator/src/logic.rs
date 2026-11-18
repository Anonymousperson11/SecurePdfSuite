//! Pure Rust business logic for `SecurePdfCreator` (queue, policy validation, unique paths, CSV export).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use secure_pdf_core::{
    ForensicsImage, MoireLevel, SecurityFlags, SpdfPolicy,
    crypto::generate_verification_code,
    forensics::rgba_to_premultiplied_bgra,
    format::{CreateSpdfOptions, DocId, MAX_PDF_BYTES, utc_now_dotnet_ticks, write_spdf_file},
    localtime::format_dotnet_ticks_local,
};

/// Longest side cap (`3000px`) for forensics preview `WriteableBitmap` allocation.
pub const MAX_PREVIEW_SIDE: u32 = 3000;

/// Ticks per hour (`3600 * 10_000_000`) in .NET `DateTime.Ticks`.
const TICKS_PER_HOUR: f64 = 36_000_000_000.0;

/// Longest validity accepted, in hours (ten years, identical to v1.x).
pub const MAX_EXPIRY_HOURS: f64 = 87_600.0;

/// Largest print allowance accepted.
pub const MAX_PRINTS: i32 = 1000;

/// Validity presets offered in the UI, in combo-box order: label and hours (`None` = custom).
pub const EXPIRY_PRESETS: [(&str, Option<f64>); 6] = [
    ("永久有效", Some(0.0)),
    ("1 小时内有效", Some(1.0)),
    ("6 小时内有效", Some(6.0)),
    ("24 小时内有效（1 天）", Some(24.0)),
    ("72 小时内有效（3 天）", Some(72.0)),
    ("自定义小时数", None),
];

/// Index of the "custom hours" entry in [`EXPIRY_PRESETS`].
pub const EXPIRY_CUSTOM_INDEX: i32 = 5;

/// Case-insensitive identity of a path for de-duplication.
///
/// The path is made absolute (resolving `.` / `..` lexically, without touching the disk), a
/// `\\?\` prefix is dropped, separators are unified and the result is lower-cased with full
/// Unicode case mapping, so `C:\文档\Ä.PDF`, `c:/文档/sub/../ä.pdf` and `\\?\C:\文档\ä.pdf` are
/// one and the same file.
#[must_use]
pub fn normalized_path_key(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy().replace('/', "\\");
    let trimmed = text
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| text.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or(text);
    trimmed.to_lowercase()
}

fn has_pdf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Deduplicated encryption queue maintaining insertion order.
#[derive(Debug, Default)]
pub struct CreatorQueue {
    items: Vec<PathBuf>,
    seen_keys: HashSet<String>,
}

impl CreatorQueue {
    /// Adds a single file if it ends with `.pdf` (case-insensitive) and is not already queued.
    pub fn push_file(&mut self, path: PathBuf) -> bool {
        if !has_pdf_extension(&path) {
            return false;
        }
        if self.seen_keys.insert(normalized_path_key(&path)) {
            self.items.push(path);
            return true;
        }
        false
    }

    /// Adds every not-yet-queued PDF from `paths`, returning the ones that were actually added.
    pub fn push_many<I: IntoIterator<Item = PathBuf>>(&mut self, paths: I) -> Vec<PathBuf> {
        let mut added = Vec::new();
        for path in paths {
            if self.push_file(path.clone()) {
                added.push(path);
            }
        }
        added
    }

    /// Removes item at `index` if in bounds.
    pub fn remove_at(&mut self, index: usize) -> Option<PathBuf> {
        if index >= self.items.len() {
            return None;
        }
        let removed = self.items.remove(index);
        self.seen_keys.remove(&normalized_path_key(&removed));
        Some(removed)
    }

    /// Empties all queued PDFs.
    pub fn clear(&mut self) {
        self.items.clear();
        self.seen_keys.clear();
    }

    /// Returns slice of queued PDF paths.
    #[must_use]
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }
}

/// Expands dropped / picked paths into PDF files: folders are walked recursively (sorted),
/// plain files are passed through. Touches the disk, so call it off the UI thread.
#[must_use]
pub fn expand_to_pdf_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut discovered = Vec::new();
            collect_pdfs_recursive(path, &mut discovered, 0);
            discovered.sort();
            out.append(&mut discovered);
        } else if has_pdf_extension(path) {
            out.push(path.clone());
        }
    }
    out
}

fn collect_pdfs_recursive(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_pdfs_recursive(&path, out, depth + 1);
        } else if file_type.is_file() && has_pdf_extension(&path) {
            out.push(path);
        }
    }
}

/// One optional protection the operator can switch on for a batch (one check box each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyOption {
    /// Embed the hidden trace watermark (time, IP, machine code).
    HiddenWatermark,
    /// Cover the page with a privacy curtain when the reader loses focus.
    BlurOnUnfocus,
    /// Let the recipient adjust the moire strength in the reader.
    AllowAdjustMoire,
    /// Render the visible watermark overlay across pages.
    VisibleWatermark,
}

impl PolicyOption {
    /// The `.spdf` security flag bit this option sets.
    #[must_use]
    pub const fn flag_bit(self) -> u32 {
        match self {
            Self::HiddenWatermark => SecurityFlags::HIDDEN_WATERMARK,
            Self::BlurOnUnfocus => SecurityFlags::BLUR_ON_UNFOCUS,
            Self::AllowAdjustMoire => SecurityFlags::ALLOW_USER_ADJUST_MOIRE,
            Self::VisibleWatermark => SecurityFlags::VISIBLE_WATERMARK,
        }
    }
}

/// UI policy parameters validated before launching batch encryption.
#[derive(Clone, Debug)]
pub struct BatchPolicyInput {
    /// `0` disallows printing; otherwise `1..=1000`.
    pub max_prints: i32,
    /// Validity in hours (fractions allowed, e.g. `0.5`); `None` for permanent access.
    pub expiry_hours: Option<f64>,
    /// Anti-camera moire strength.
    pub moire_level: MoireLevel,
    /// Protections that are switched on; anything not listed stays off.
    pub options: Vec<PolicyOption>,
    /// Custom watermark note (empty = runtime trace info only).
    pub watermark_label: String,
}

/// Parses the print allowance text box.
///
/// # Errors
///
/// Returns a user-facing message when the text is not an integer in `0..=1000`.
pub fn parse_max_prints(text: &str) -> Result<i32, String> {
    let message = || format!("允许打印次数必须是 0 到 {MAX_PRINTS} 之间的整数（0 表示禁止打印）。");
    let value = text.trim().parse::<i32>().map_err(|_| message())?;
    if (0..=MAX_PRINTS).contains(&value) {
        Ok(value)
    } else {
        Err(message())
    }
}

/// Parses a custom validity in hours; accepts `.` or `,` / `，` as the decimal separator.
///
/// # Errors
///
/// Returns a user-facing message unless the value is a finite number in `(0, 87600]`.
pub fn parse_expiry_hours(text: &str) -> Result<f64, String> {
    let message = || {
        "请输入有效的自定义小时数（大于 0，最多 87600 小时，支持小数，如 0.5 表示 30 分钟）。"
            .to_owned()
    };
    let trimmed = text.trim();
    let plain_decimal = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '，'));
    if !plain_decimal {
        return Err(message());
    }
    let normalized = trimmed.replace([',', '，'], ".");
    let hours = normalized.parse::<f64>().map_err(|_| message())?;
    if hours.is_finite() && hours > 0.0 && hours <= MAX_EXPIRY_HOURS {
        Ok(hours)
    } else {
        Err(message())
    }
}

/// Resolves the validity selection (preset index + custom text) into hours.
///
/// # Errors
///
/// Returns a user-facing message when the custom value is invalid.
pub fn expiry_hours_for_choice(
    preset_index: i32,
    custom_text: &str,
) -> Result<Option<f64>, String> {
    let preset = usize::try_from(preset_index)
        .ok()
        .and_then(|i| EXPIRY_PRESETS.get(i))
        .map_or(Some(0.0), |(_, hours)| *hours);
    match preset {
        Some(hours) if hours > 0.0 => Ok(Some(hours)),
        Some(_) => Ok(None),
        None => parse_expiry_hours(custom_text).map(Some),
    }
}

/// Validates operator inputs and materializes a [`SpdfPolicy`] template anchored to `now_ticks`.
///
/// # Errors
///
/// Returns a descriptive user-facing validation error if the print limit or validity is invalid.
pub fn validate_policy_input(
    input: &BatchPolicyInput,
    now_ticks: i64,
) -> Result<SpdfPolicy, String> {
    if input.max_prints < 0 || input.max_prints > MAX_PRINTS {
        return Err(format!(
            "允许打印次数必须在 0（禁止打印）到 {MAX_PRINTS} 之间。"
        ));
    }
    let expires_at_utc_ticks = match input.expiry_hours {
        None => None,
        Some(hours) if !hours.is_finite() || hours <= 0.0 => {
            return Err("有效期必须大于 0 小时，或选择永久有效。".to_owned());
        }
        Some(hours) if hours > MAX_EXPIRY_HOURS => {
            return Err("有效期不能超过 87600 小时（约 10 年）。".to_owned());
        }
        Some(hours) => {
            // At most 87600 h * 3.6e10 ticks = 3.2e15, far inside the i64 range.
            #[allow(clippy::cast_possible_truncation)]
            let offset_ticks = (hours * TICKS_PER_HOUR).round() as i64;
            Some(now_ticks.saturating_add(offset_ticks.max(1)))
        }
    };

    let flag_bits = input
        .options
        .iter()
        .fold(0_u32, |bits, option| bits | option.flag_bit());

    Ok(SpdfPolicy {
        doc_id: DocId::from_wire_bytes([0_u8; 16]),
        max_prints: input.max_prints,
        expires_at_utc_ticks,
        moire_level: input.moire_level,
        flags: SecurityFlags::from_bits(flag_bits),
        original_file_name: String::new(),
        watermark_note: input.watermark_label.trim().to_owned(),
    })
}

/// Convenience helper computing expiry ticks relative to the current Windows UTC clock.
#[must_use]
pub fn now_policy_ticks() -> i64 {
    utc_now_dotnet_ticks()
}

/// Interprets the output-folder text box: empty means "next to each source file".
///
/// Surrounding quotes are dropped; a non-empty value must be a full path (a relative path would
/// land in the program's current directory, which the user cannot see) and is normalized.
///
/// # Errors
///
/// Returns a user-facing message for relative or malformed paths.
pub fn normalize_output_dir(text: &str) -> Result<Option<PathBuf>, String> {
    let trimmed = text.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let candidate = Path::new(trimmed);
    if !candidate.is_absolute() {
        return Err("输出文件夹请填写完整路径，例如 D:\\加密输出。".to_owned());
    }
    std::path::absolute(candidate)
        .map(Some)
        .map_err(|e| format!("输出文件夹路径无效：{e}"))
}

/// Folder that receives the `.spdf` for `source_pdf`: the chosen folder, or the source's own.
///
/// # Errors
///
/// Returns a user-facing message when the source file's folder cannot be determined.
pub fn target_dir_for(source_pdf: &Path, custom_out_dir: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(dir) = custom_out_dir {
        return Ok(dir.to_path_buf());
    }
    std::path::absolute(source_pdf)
        .ok()
        .and_then(|abs| abs.parent().map(Path::to_path_buf))
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| "无法确定源文件所在目录。".to_owned())
}

/// Reserves a collision-free `<stem>.spdf` path inside `output_dir`.
///
/// The name is claimed by atomically creating an empty placeholder (`create_new`), so two
/// writers can never be handed the same path and an existing `.spdf` is never overwritten;
/// taken names fall through to ` (2)`, ` (3)`, ... The caller replaces the placeholder with the
/// finished file, or removes it with [`release_reserved_output`] on failure.
///
/// # Errors
///
/// Returns an error if the folder is not writable or the collision search bound is exceeded.
pub fn reserve_unique_output_path(input_pdf: &Path, output_dir: &Path) -> Result<PathBuf, String> {
    let stem = input_pdf
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("protected");

    for n in 1_u32..=100_000 {
        let name = if n == 1 {
            format!("{stem}.spdf")
        } else {
            format!("{stem} ({n}).spdf")
        };
        let candidate = output_dir.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            // A directory with that name reports "access denied" on Windows: try the next name.
            Err(_) if candidate.is_dir() => {}
            Err(err) => return Err(format!("无法在输出目录中创建文件：{err}")),
        }
    }
    Err(format!(
        "输出目录中同名文件过多，无法为 {} 分配唯一文件名。",
        input_pdf.display()
    ))
}

/// Removes a placeholder left by [`reserve_unique_output_path`] if it is still empty.
pub fn release_reserved_output(path: &Path) {
    if fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == 0) {
        let _ = fs::remove_file(path);
    }
}

/// Reads a source PDF after checking its size on disk first, so an oversized file is refused
/// without being pulled into memory.
///
/// # Errors
///
/// Returns a user-facing message for unreadable, empty or oversized (> 1 GiB) files.
pub fn read_source_pdf(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::metadata(path).map_err(|e| format!("读取失败：{e}"))?;
    if !meta.is_file() {
        return Err("源路径不是文件。".to_owned());
    }
    check_source_len(meta.len())?;
    let bytes = fs::read(path).map_err(|e| format!("读取失败：{e}"))?;
    // The file may have grown between the size check and the read.
    check_source_len(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    Ok(bytes)
}

/// Size gate shared by the pre-read check and the post-read re-check.
fn check_source_len(len: u64) -> Result<(), String> {
    if len == 0 {
        return Err("文件为空。".to_owned());
    }
    if len > u64::try_from(MAX_PDF_BYTES).unwrap_or(u64::MAX) {
        return Err("文件超过 1 GB 大小上限，未读取。".to_owned());
    }
    Ok(())
}

/// Single completed or failed batch record exported to CSV manifest.
#[derive(Clone, Debug)]
pub struct BatchResultRow {
    /// Original PDF path.
    pub source_pdf: String,
    /// Written `.spdf` path (empty when failed).
    pub output_spdf: String,
    /// Encrypted document GUID in 32-char hex.
    pub doc_id: String,
    /// Verification code used to protect this document.
    pub verification_code: String,
    /// Print quota summary.
    pub max_prints: String,
    /// Expiry summary.
    pub expires_at: String,
    /// Status string (`成功` / `失败`).
    pub status: String,
    /// Diagnostic detail or empty string.
    pub note: String,
}

impl BatchResultRow {
    /// `true` for a successfully encrypted document (the only rows that carry a usable code).
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.status == STATUS_OK
    }

    /// File name part of the source path, for compact display.
    #[must_use]
    pub fn source_file_name(&self) -> &str {
        self.source_pdf
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&self.source_pdf)
    }
}

/// Status text of a successfully encrypted row.
pub const STATUS_OK: &str = "成功";
/// Status text of a failed row (the reason is in [`BatchResultRow::note`]).
pub const STATUS_FAILED: &str = "失败";
/// Placeholder shown instead of a verification code on failed rows.
pub const NO_CODE: &str = "--";

/// Plain-text list of all successful rows for the clipboard (one line per document).
#[must_use]
pub fn clipboard_listing(rows: &[BatchResultRow]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for r in rows.iter().filter(|r| r.succeeded()) {
        let _ = write!(
            out,
            "文件: {} | 验证码: {} | 打印上限: {} | 有效期: {} | 路径: {}\r\n",
            r.source_file_name(),
            r.verification_code,
            r.max_prints,
            r.expires_at,
            r.output_spdf
        );
    }
    out
}

/// Marks a long-running job as active and clears the mark when dropped.
///
/// Because the flag is cleared in `Drop`, it is reset even when the worker thread panics, so the
/// window can never be left permanently "busy".
#[derive(Debug)]
pub struct BusyGuard {
    flag: Arc<AtomicBool>,
}

impl BusyGuard {
    /// Claims `flag`; returns `None` when another job already holds it.
    #[must_use]
    pub fn try_acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self { flag: flag.clone() })
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

/// Outcome of one batch run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchSummary {
    /// Number of files in the batch.
    pub total: usize,
    /// Files encrypted successfully.
    pub succeeded: usize,
    /// Files that failed (each has a row with the reason).
    pub failed: usize,
    /// Set when the batch stopped early; the remaining files were not processed.
    pub aborted: Option<String>,
}

impl BatchSummary {
    /// One-line, user-facing description of the batch outcome.
    #[must_use]
    pub fn describe(&self) -> String {
        let skipped = self.total.saturating_sub(self.succeeded + self.failed);
        if let Some(reason) = &self.aborted {
            return format!(
                "批次已中止：{reason}（成功 {} 份，失败 {} 份，未处理 {skipped} 份）。",
                self.succeeded, self.failed
            );
        }
        if self.failed == 0 {
            format!(
                "已完成 {} 份文档加密，每份文档各有独立验证码。请及时导出或复制验证码。",
                self.succeeded
            )
        } else {
            format!(
                "完成：成功 {} 份，失败 {} 份（失败原因见结果表）。",
                self.succeeded, self.failed
            )
        }
    }
}

fn prints_text(max_prints: i32) -> String {
    if max_prints <= 0 {
        "禁止打印".to_owned()
    } else {
        format!("{max_prints} 次")
    }
}

fn expiry_text(policy: &SpdfPolicy) -> String {
    policy
        .expires_at_utc_ticks
        .map_or_else(|| "永久有效".to_owned(), format_dotnet_ticks_local)
}

fn failed_row(src: &Path, policy: &SpdfPolicy, reason: String) -> BatchResultRow {
    BatchResultRow {
        source_pdf: src.to_string_lossy().into_owned(),
        output_spdf: String::new(),
        doc_id: String::new(),
        verification_code: NO_CODE.to_owned(),
        max_prints: prints_text(policy.max_prints),
        expires_at: expiry_text(policy),
        status: STATUS_FAILED.to_owned(),
        note: reason,
    }
}

/// Encrypts one PDF with its own verification code `code`.
///
/// The output goes to `custom_out_dir`, or next to the source file when that is `None`. A
/// failure (unreadable, not a PDF, disk full, ...) is reported as a failed row and never leaves
/// a placeholder file behind.
#[must_use]
pub fn encrypt_one(
    src: &Path,
    custom_out_dir: Option<&Path>,
    policy: &SpdfPolicy,
    code: &str,
) -> BatchResultRow {
    let orig_name = src
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.pdf");

    let res = (|| -> Result<(PathBuf, String), String> {
        let target_dir = target_dir_for(src, custom_out_dir)?;
        let raw_pdf = read_source_pdf(src)?;
        let out_path = reserve_unique_output_path(src, &target_dir)?;
        let opts = CreateSpdfOptions {
            verification_code: code,
            max_prints: policy.max_prints,
            expires_at_utc_ticks: policy.expires_at_utc_ticks,
            moire_level: policy.moire_level,
            flags: policy.flags,
            original_file_name: orig_name,
            watermark_note: &policy.watermark_note,
            doc_id: None,
            scrypt_n: None,
            scrypt_r: None,
            scrypt_p: None,
        };
        match write_spdf_file(&raw_pdf, &out_path, &opts) {
            Ok(doc_id) => Ok((out_path, doc_id.to_hex_n())),
            Err(err) => {
                release_reserved_output(&out_path);
                Err(err.to_string())
            }
        }
    })();

    match res {
        Ok((out_path, doc_id)) => BatchResultRow {
            source_pdf: src.to_string_lossy().into_owned(),
            output_spdf: out_path.to_string_lossy().into_owned(),
            doc_id,
            verification_code: code.to_owned(),
            max_prints: prints_text(policy.max_prints),
            expires_at: expiry_text(policy),
            status: STATUS_OK.to_owned(),
            note: String::new(),
        },
        Err(reason) => failed_row(src, policy, reason),
    }
}

/// Encrypts `pdfs` one by one, drawing a fresh verification code from `next_code` for every
/// file and reporting each finished row through `on_row(row, done, total)`.
///
/// A single file failing does not affect the others. If `next_code` fails (the system random
/// number generator is unavailable) the batch is aborted on the spot: no document is ever
/// written with a predictable or shared code.
pub fn encrypt_batch_with<C, R>(
    pdfs: &[PathBuf],
    custom_out_dir: Option<&Path>,
    policy: &SpdfPolicy,
    mut next_code: C,
    mut on_row: R,
) -> BatchSummary
where
    C: FnMut() -> Result<String, String>,
    R: FnMut(BatchResultRow, usize, usize),
{
    let mut summary = BatchSummary {
        total: pdfs.len(),
        ..BatchSummary::default()
    };
    for (idx, src) in pdfs.iter().enumerate() {
        let row = match next_code() {
            Ok(code) => encrypt_one(src, custom_out_dir, policy, &code),
            Err(err) => {
                let reason = format!("无法生成随机验证码：{err}");
                summary.failed += 1;
                summary.aborted = Some(reason.clone());
                on_row(failed_row(src, policy, reason), idx + 1, summary.total);
                break;
            }
        };
        if row.succeeded() {
            summary.succeeded += 1;
        } else {
            summary.failed += 1;
        }
        on_row(row, idx + 1, summary.total);
    }
    summary
}

/// [`encrypt_batch_with`] using the system random number generator for the per-file codes.
pub fn encrypt_batch<R>(
    pdfs: &[PathBuf],
    custom_out_dir: Option<&Path>,
    policy: &SpdfPolicy,
    on_row: R,
) -> BatchSummary
where
    R: FnMut(BatchResultRow, usize, usize),
{
    encrypt_batch_with(
        pdfs,
        custom_out_dir,
        policy,
        || generate_verification_code(4, 4).map_err(|e| e.to_string()),
        on_row,
    )
}

/// Escapes a CSV field cleanly with RFC 4180 quoting and neutralizes Excel/LibreOffice formula injection.
#[must_use]
pub fn csv_escape_field(raw: &str) -> String {
    let formula_prefixed = if raw.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{raw}")
    } else {
        raw.to_owned()
    };
    let escaped = formula_prefixed.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Writes batch encryption records to `path` with UTF-8 BOM and CRLF line endings.
///
/// # Errors
///
/// Returns an I/O error description if creating or writing the CSV fails.
pub fn export_batch_csv(path: &Path, rows: &[BatchResultRow]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| format!("无法创建 CSV 文件：{e}"))?;
    file.write_all(&[0xEF, 0xBB, 0xBF])
        .and_then(|()| {
            file.write_all(
                "\"源文件\",\"输出文件\",\"文档ID\",\"验证码\",\"打印次数\",\"有效期\",\"状态\",\"失败原因\"\r\n"
                    .as_bytes(),
            )
        })
        .map_err(|e| format!("写入 CSV 表头失败：{e}"))?;

    for r in rows {
        let line = [
            csv_escape_field(&r.source_pdf),
            csv_escape_field(&r.output_spdf),
            csv_escape_field(&r.doc_id),
            csv_escape_field(&r.verification_code),
            csv_escape_field(&r.max_prints),
            csv_escape_field(&r.expires_at),
            csv_escape_field(&r.status),
            csv_escape_field(&r.note),
        ]
        .join(",");
        file.write_all(line.as_bytes())
            .and_then(|()| file.write_all(b"\r\n"))
            .map_err(|e| format!("写入 CSV 行失败：{e}"))?;
    }
    Ok(())
}

/// Computes `(width, height)` scaled so `max(w, h) <= max_side`, keeping aspect ratio >= 1px.
#[must_use]
pub fn cap_preview_dimensions(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max_side || max_side == 0 || width == 0 || height == 0 {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(max_side) / f64::from(longest);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    (w, h)
}

/// Downsamples a [`ForensicsImage`] for safe XAML preview allocation and converts it to
/// premultiplied `BGRA8` (colour channels scaled by alpha, as `WriteableBitmap` expects).
#[must_use]
pub fn forensics_preview_bgra8(img: &ForensicsImage) -> (u32, u32, Vec<u8>) {
    let (tw, th) = cap_preview_dimensions(img.width, img.height, MAX_PREVIEW_SIDE);
    if tw == img.width && th == img.height {
        return (tw, th, img.to_bgra8());
    }
    let sw = img.width as usize;
    let sh = img.height as usize;
    let dw = tw as usize;
    let dh = th as usize;
    let mut bgra = Vec::with_capacity(dw * dh * 4);
    for dy in 0..dh {
        let sy = (dy * sh / dh).min(sh.saturating_sub(1));
        for dx in 0..dw {
            let sx = (dx * sw / dw).min(sw.saturating_sub(1));
            let s_idx = (sy * sw + sx) * 4;
            let px = [
                img.rgba[s_idx],
                img.rgba[s_idx + 1],
                img.rgba[s_idx + 2],
                img.rgba[s_idx + 3],
            ];
            bgra.extend_from_slice(&rgba_to_premultiplied_bgra(px));
        }
    }
    (tw, th, bgra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const NOW: i64 = 639_000_000_000_000_000;

    /// Every option, in flag-bit order.
    const ALL_OPTIONS: [PolicyOption; 4] = [
        PolicyOption::HiddenWatermark,
        PolicyOption::BlurOnUnfocus,
        PolicyOption::AllowAdjustMoire,
        PolicyOption::VisibleWatermark,
    ];

    fn input() -> BatchPolicyInput {
        BatchPolicyInput {
            max_prints: 1,
            expiry_hours: None,
            moire_level: MoireLevel::Medium,
            options: Vec::new(),
            watermark_label: String::new(),
        }
    }

    #[test]
    fn queue_deduplicates_case_insensitively_and_ignores_non_pdf() {
        let mut q = CreatorQueue::default();
        assert!(!q.push_file(PathBuf::from(r"C:\docs\readme.txt")));
        assert!(q.push_file(PathBuf::from(r"C:\Docs\Contract.PDF")));
        assert!(!q.push_file(PathBuf::from(r"c:\docs\contract.pdf")));
        assert_eq!(q.items().len(), 1);
        q.remove_at(0);
        assert!(q.push_file(PathBuf::from(r"c:\docs\contract.pdf")));
    }

    #[test]
    fn queue_deduplicates_by_normalized_path_including_non_ascii_case() {
        let mut q = CreatorQueue::default();
        assert!(q.push_file(PathBuf::from(r"C:\文档\ÄÖ报告.pdf")));
        // Same file: non-ASCII letters in the other case.
        assert!(!q.push_file(PathBuf::from(r"c:\文档\äö报告.PDF")));
        // Same file: forward slashes and a `..` detour.
        assert!(!q.push_file(PathBuf::from("C:/文档/sub/../ÄÖ报告.pdf")));
        // Same file: `.` segment and extended-length prefix.
        assert!(!q.push_file(PathBuf::from(r"C:\文档\.\ÄÖ报告.pdf")));
        assert!(!q.push_file(PathBuf::from(r"\\?\C:\文档\ÄÖ报告.pdf")));
        assert_eq!(q.items().len(), 1);
        // A genuinely different file is still accepted.
        assert!(q.push_file(PathBuf::from(r"C:\文档\ÄÖ报告2.pdf")));

        let added = q.push_many([
            PathBuf::from(r"C:\文档\äö报告.pdf"),
            PathBuf::from(r"C:\文档\新.pdf"),
            PathBuf::from(r"C:\文档\新.txt"),
        ]);
        assert_eq!(added, vec![PathBuf::from(r"C:\文档\新.pdf")]);

        q.clear();
        assert_eq!(q.items(), Vec::<PathBuf>::new());
        assert!(q.push_file(PathBuf::from(r"c:\文档\äö报告.pdf")));
    }

    #[test]
    fn folder_expansion_finds_nested_pdfs_sorted_and_skips_other_files() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("b").join("deep")).unwrap();
        for rel in ["z.pdf", "a.PDF", "note.txt", "b/m.pdf", "b/deep/c.pdf"] {
            fs::write(dir.path().join(rel), b"x").unwrap();
        }
        let single = dir.path().join("z.pdf");
        let found = expand_to_pdf_files(&[
            dir.path().to_path_buf(),
            single,
            dir.path().join("note.txt"),
        ]);
        let names: Vec<String> = found
            .iter()
            .map(|p| {
                p.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            ["a.PDF", "b/deep/c.pdf", "b/m.pdf", "z.pdf", "z.pdf"]
        );

        let mut q = CreatorQueue::default();
        assert_eq!(q.push_many(found).len(), 4);
    }

    #[test]
    fn policy_flags_map_each_checkbox_to_its_own_bit() {
        let bits = |options: &[PolicyOption]| {
            validate_policy_input(
                &BatchPolicyInput {
                    options: options.to_vec(),
                    ..input()
                },
                NOW,
            )
            .unwrap()
            .flags
            .bits()
        };
        // Nothing ticked -> no flag at all (in particular: moire adjustment is NOT implied).
        assert_eq!(bits(&[]), 0);

        assert_eq!(
            bits(&[PolicyOption::HiddenWatermark]),
            SecurityFlags::HIDDEN_WATERMARK
        );
        assert_eq!(
            bits(&[PolicyOption::BlurOnUnfocus]),
            SecurityFlags::BLUR_ON_UNFOCUS
        );
        assert_eq!(
            bits(&[PolicyOption::AllowAdjustMoire]),
            SecurityFlags::ALLOW_USER_ADJUST_MOIRE
        );
        assert_eq!(
            bits(&[PolicyOption::VisibleWatermark]),
            SecurityFlags::VISIBLE_WATERMARK
        );
        // Wire values are fixed by the v1.x container format.
        assert_eq!(ALL_OPTIONS.map(PolicyOption::flag_bit), [1, 2, 4, 8]);

        assert_eq!(bits(&ALL_OPTIONS), 0b1111);
        assert_eq!(
            bits(&[
                PolicyOption::HiddenWatermark,
                PolicyOption::BlurOnUnfocus,
                PolicyOption::VisibleWatermark,
            ]),
            0b1011
        );
        // Listing an option twice changes nothing.
        assert_eq!(
            bits(&[PolicyOption::BlurOnUnfocus, PolicyOption::BlurOnUnfocus]),
            0b0010
        );
    }

    #[test]
    fn policy_validity_is_measured_in_hours_and_accepts_fractions() {
        let ticks = |hours: f64| {
            validate_policy_input(
                &BatchPolicyInput {
                    expiry_hours: Some(hours),
                    ..input()
                },
                NOW,
            )
            .unwrap()
            .expires_at_utc_ticks
            .unwrap()
        };
        assert_eq!(
            validate_policy_input(&input(), NOW)
                .unwrap()
                .expires_at_utc_ticks,
            None
        );
        // 0.5 h = 30 minutes = 18_000_000_000 ticks.
        assert_eq!(ticks(0.5) - NOW, 18_000_000_000);
        assert_eq!(ticks(1.0) - NOW, 36_000_000_000);
        assert_eq!(ticks(6.0) - NOW, 216_000_000_000);
        assert_eq!(ticks(24.0) - NOW, 864_000_000_000);
        assert_eq!(ticks(72.0) - NOW, 2_592_000_000_000);
        assert_eq!(ticks(0.25) - NOW, 9_000_000_000);
        // Upper boundary: exactly ten years is accepted.
        assert_eq!(ticks(MAX_EXPIRY_HOURS) - NOW, 3_153_600_000_000_000);
        // A tiny but positive validity still expires strictly after "now".
        assert!(ticks(1e-12) > NOW);
    }

    #[test]
    fn policy_rejects_out_of_range_validity_and_print_counts() {
        for hours in [
            0.0,
            -1.0,
            MAX_EXPIRY_HOURS + 0.01,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let res = validate_policy_input(
                &BatchPolicyInput {
                    expiry_hours: Some(hours),
                    ..input()
                },
                NOW,
            );
            assert!(res.is_err(), "{hours} h must be rejected");
        }
        for (prints, ok) in [
            (-1, false),
            (0, true),
            (1, true),
            (1000, true),
            (1001, false),
        ] {
            let res = validate_policy_input(
                &BatchPolicyInput {
                    max_prints: prints,
                    ..input()
                },
                NOW,
            );
            assert_eq!(res.is_ok(), ok, "max_prints {prints}");
            if let Ok(policy) = res {
                assert_eq!(policy.max_prints, prints);
            }
        }
    }

    #[test]
    fn policy_carries_moire_level_and_trimmed_note() {
        let policy = validate_policy_input(
            &BatchPolicyInput {
                moire_level: MoireLevel::High,
                watermark_label: "  张三专用  ".to_owned(),
                ..input()
            },
            NOW,
        )
        .unwrap();
        assert_eq!(policy.moire_level, MoireLevel::High);
        assert_eq!(policy.watermark_note, "张三专用");
    }

    #[test]
    fn print_count_and_custom_hours_text_parsing() {
        assert_eq!(parse_max_prints(" 0 "), Ok(0));
        assert_eq!(parse_max_prints("1000"), Ok(1000));
        for bad in ["", "abc", "-1", "1001", "1.5", "１"] {
            assert!(parse_max_prints(bad).is_err(), "{bad:?}");
        }

        assert_eq!(parse_expiry_hours("0.5"), Ok(0.5));
        assert_eq!(parse_expiry_hours(" 12 "), Ok(12.0));
        assert_eq!(parse_expiry_hours("0,5"), Ok(0.5));
        assert_eq!(parse_expiry_hours("1，25"), Ok(1.25));
        assert_eq!(parse_expiry_hours("87600"), Ok(87_600.0));
        for bad in [
            "", " ", "0", "0.0", "-1", "87600.5", "abc", "1e3", "inf", "NaN", "1.2.3", "+2", "１２",
        ] {
            assert!(parse_expiry_hours(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn expiry_presets_resolve_to_1_6_24_72_hours_permanent_or_custom() {
        assert_eq!(EXPIRY_PRESETS.len(), 6);
        assert_eq!(expiry_hours_for_choice(0, "ignored"), Ok(None));
        assert_eq!(expiry_hours_for_choice(1, ""), Ok(Some(1.0)));
        assert_eq!(expiry_hours_for_choice(2, ""), Ok(Some(6.0)));
        assert_eq!(expiry_hours_for_choice(3, ""), Ok(Some(24.0)));
        assert_eq!(expiry_hours_for_choice(4, ""), Ok(Some(72.0)));
        assert_eq!(
            EXPIRY_PRESETS[usize::try_from(EXPIRY_CUSTOM_INDEX).unwrap()].1,
            None
        );
        assert_eq!(
            expiry_hours_for_choice(EXPIRY_CUSTOM_INDEX, "0.5"),
            Ok(Some(0.5))
        );
        assert!(expiry_hours_for_choice(EXPIRY_CUSTOM_INDEX, "").is_err());
        assert!(expiry_hours_for_choice(EXPIRY_CUSTOM_INDEX, "0").is_err());
        // No selection / out-of-range index falls back to "permanent", never to a short expiry.
        assert_eq!(expiry_hours_for_choice(-1, "3"), Ok(None));
        assert_eq!(expiry_hours_for_choice(99, "3"), Ok(None));
    }

    #[test]
    fn output_dir_text_empty_means_next_to_source_and_relative_paths_are_refused() {
        assert_eq!(normalize_output_dir(""), Ok(None));
        assert_eq!(normalize_output_dir("   "), Ok(None));
        assert_eq!(normalize_output_dir("\"\""), Ok(None));
        assert_eq!(
            normalize_output_dir(r#" "D:\加密 输出\sub\.." "#),
            Ok(Some(PathBuf::from(r"D:\加密 输出")))
        );
        assert_eq!(
            normalize_output_dir("D:/out/x"),
            Ok(Some(PathBuf::from(r"D:\out\x")))
        );
        for relative in ["out", r".\out", r"..\out", r"D:out", r"\out"] {
            assert!(normalize_output_dir(relative).is_err(), "{relative:?}");
        }

        let src = Path::new(r"C:\资料\合同\a.pdf");
        assert_eq!(
            target_dir_for(src, None),
            Ok(PathBuf::from(r"C:\资料\合同"))
        );
        assert_eq!(
            target_dir_for(src, Some(Path::new(r"D:\out"))),
            Ok(PathBuf::from(r"D:\out"))
        );
    }

    #[test]
    fn reserved_output_path_never_overwrites_and_never_hands_out_a_name_twice() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("report.pdf");

        let first = reserve_unique_output_path(&src, dir.path()).unwrap();
        assert_eq!(first, dir.path().join("report.spdf"));
        // The name is claimed on disk immediately, before any content is written.
        assert_eq!(fs::metadata(&first).unwrap().len(), 0);

        let second = reserve_unique_output_path(&src, dir.path()).unwrap();
        assert_eq!(second, dir.path().join("report (2).spdf"));

        // A folder squatting on the next name is skipped as well.
        fs::create_dir(dir.path().join("report (3).spdf")).unwrap();
        let fourth = reserve_unique_output_path(&src, dir.path()).unwrap();
        assert_eq!(fourth, dir.path().join("report (4).spdf"));

        // Releasing removes an untouched placeholder but never a file with content.
        release_reserved_output(&second);
        assert!(!second.exists());
        fs::write(&first, b"real spdf content").unwrap();
        release_reserved_output(&first);
        assert_eq!(fs::read(&first).unwrap(), b"real spdf content");

        let missing_dir = dir.path().join("does-not-exist");
        assert!(reserve_unique_output_path(&src, &missing_dir).is_err());
    }

    #[test]
    fn concurrent_reservations_for_the_same_name_all_get_distinct_paths() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().to_path_buf();
        let handles: Vec<std::thread::JoinHandle<Vec<PathBuf>>> = (0..8)
            .map(|_| {
                let out = out_dir.clone();
                std::thread::spawn(move || {
                    (0..10)
                        .map(|_| reserve_unique_output_path(Path::new("same.pdf"), &out).unwrap())
                        .collect()
                })
            })
            .collect::<Vec<_>>();
        let mut all = Vec::new();
        for handle in handles {
            all.extend(handle.join().unwrap());
        }
        assert_eq!(all.len(), 80);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 80, "a name was handed out twice");
    }

    #[test]
    fn source_size_is_checked_before_the_file_is_read() {
        let limit = u64::try_from(MAX_PDF_BYTES).unwrap();
        assert!(check_source_len(1).is_ok());
        assert!(check_source_len(limit).is_ok());
        assert!(check_source_len(0).unwrap_err().contains("为空"));
        assert!(check_source_len(limit + 1).unwrap_err().contains("上限"));

        let dir = tempdir().unwrap();
        // A sparse file just over 1 GiB: only its length exists on disk. If the size were
        // checked after reading, this test would have to allocate more than 1 GiB.
        let huge = dir.path().join("huge.pdf");
        File::create(&huge).unwrap().set_len(limit + 1).unwrap();
        assert!(read_source_pdf(&huge).unwrap_err().contains("上限"));

        let empty = dir.path().join("empty.pdf");
        fs::write(&empty, b"").unwrap();
        assert!(read_source_pdf(&empty).unwrap_err().contains("为空"));

        let ok = dir.path().join("ok.pdf");
        fs::write(&ok, b"%PDF-1.4").unwrap();
        assert_eq!(read_source_pdf(&ok).unwrap(), b"%PDF-1.4");
        assert!(read_source_pdf(&dir.path().join("missing.pdf")).is_err());
        assert!(read_source_pdf(dir.path()).is_err());
    }

    const MINI_PDF: &[u8] = b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n";

    #[test]
    fn batch_gives_every_file_its_own_code_and_codes_do_not_unlock_other_files() {
        use secure_pdf_core::format::open_spdf_file;

        let dir = tempdir().unwrap();
        let out = dir.path().join("out");
        fs::create_dir(&out).unwrap();
        let pdfs: Vec<PathBuf> = ["a.pdf", "b.pdf", "c.pdf"]
            .iter()
            .map(|name| {
                let p = dir.path().join(name);
                fs::write(&p, MINI_PDF).unwrap();
                p
            })
            .collect();
        let policy = validate_policy_input(
            &BatchPolicyInput {
                max_prints: 2,
                expiry_hours: Some(0.5),
                options: vec![PolicyOption::HiddenWatermark],
                watermark_label: "张三专用".to_owned(),
                ..input()
            },
            now_policy_ticks(),
        )
        .unwrap();

        let mut progress = Vec::new();
        let mut rows = Vec::new();
        let summary = encrypt_batch(&pdfs, Some(&out), &policy, |row, done, total| {
            progress.push((done, total));
            rows.push(row);
        });
        assert_eq!(
            summary,
            BatchSummary {
                total: 3,
                succeeded: 3,
                failed: 0,
                aborted: None
            }
        );
        assert_eq!(progress, [(1, 3), (2, 3), (3, 3)]);
        assert!(rows.iter().all(BatchResultRow::succeeded));

        // Three files -> three different, well-formed codes.
        let codes: HashSet<&str> = rows.iter().map(|r| r.verification_code.as_str()).collect();
        assert_eq!(codes.len(), 3);
        for code in &codes {
            assert_eq!(code.len(), 19, "{code}");
            assert_eq!(code.split('-').count(), 4);
        }

        for (i, row) in rows.iter().enumerate() {
            let path = Path::new(&row.output_spdf);
            assert_eq!(path.parent(), Some(out.as_path()));
            // Its own code opens it and the policy round-trips ...
            let doc = open_spdf_file(path, &row.verification_code).unwrap();
            assert_eq!(doc.pdf_bytes.as_slice(), MINI_PDF);
            assert_eq!(doc.policy.max_prints, 2);
            assert_eq!(doc.policy.expires_at_utc_ticks, policy.expires_at_utc_ticks);
            assert_eq!(doc.policy.flags.bits(), SecurityFlags::HIDDEN_WATERMARK);
            assert_eq!(doc.policy.watermark_note, "张三专用");
            assert_eq!(doc.policy.doc_id.to_hex_n(), row.doc_id);
            // ... and the code of another file in the same batch does not.
            let other = &rows[(i + 1) % rows.len()].verification_code;
            assert!(open_spdf_file(path, other).is_err());
            // Expiry is shown as local time with its zone label.
            assert!(row.expires_at.contains("(UTC"), "{}", row.expires_at);
            assert_eq!(row.max_prints, "2 次");
        }
    }

    #[test]
    fn batch_reports_per_file_failures_with_reasons_and_leaves_no_placeholder() {
        let dir = tempdir().unwrap();
        let good = dir.path().join("good.pdf");
        let fake = dir.path().join("fake.pdf");
        let missing = dir.path().join("missing.pdf");
        fs::write(&good, MINI_PDF).unwrap();
        fs::write(&fake, b"this is plain text, not a PDF").unwrap();
        let policy = validate_policy_input(&input(), NOW).unwrap();

        let mut rows = Vec::new();
        // No output folder chosen: results go next to each source file.
        let summary = encrypt_batch(
            &[fake, missing, good.clone()],
            None,
            &policy,
            |row, _, _| rows.push(row),
        );
        assert_eq!((summary.succeeded, summary.failed), (1, 2));
        assert_eq!(summary.aborted, None);
        assert!(summary.describe().contains("失败 2 份"));

        assert_eq!(rows[0].status, STATUS_FAILED);
        assert_eq!(rows[0].verification_code, NO_CODE);
        assert!(rows[0].note.contains("%PDF"), "{}", rows[0].note);
        assert_eq!(rows[1].status, STATUS_FAILED);
        assert!(rows[1].note.contains("读取失败"), "{}", rows[1].note);
        assert!(rows[2].succeeded());
        assert_eq!(
            rows[2].output_spdf,
            dir.path().join("good.spdf").to_string_lossy()
        );
        assert_eq!(rows[2].expires_at, "永久有效");
        assert_eq!(rows[2].max_prints, "1 次");

        // The failed non-PDF must not leave an empty `fake.spdf` behind.
        assert!(!dir.path().join("fake.spdf").exists());
        assert!(!dir.path().join("missing.spdf").exists());
        // No temporary files either.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                Path::new(n)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
            })
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // Running the same file again never overwrites the first output.
        let again = encrypt_one(&good, None, &policy, "AAAA-BBBB-CCCC-DDDD");
        assert_eq!(
            again.output_spdf,
            dir.path().join("good (2).spdf").to_string_lossy()
        );
    }

    #[test]
    fn batch_aborts_when_the_random_code_generator_fails_instead_of_using_a_fallback() {
        let dir = tempdir().unwrap();
        let pdfs: Vec<PathBuf> = ["1.pdf", "2.pdf", "3.pdf"]
            .iter()
            .map(|name| {
                let p = dir.path().join(name);
                fs::write(&p, MINI_PDF).unwrap();
                p
            })
            .collect();
        let policy = validate_policy_input(&input(), NOW).unwrap();

        let mut calls = 0;
        let mut rows = Vec::new();
        let summary = encrypt_batch_with(
            &pdfs,
            None,
            &policy,
            || {
                calls += 1;
                if calls == 1 {
                    Ok("QRST-UVWX-YZ23-4567".to_owned())
                } else {
                    Err("随机数发生器不可用".to_owned())
                }
            },
            |row, _, _| rows.push(row),
        );
        assert_eq!(calls, 2, "the third file must not be attempted");
        assert_eq!(
            (summary.total, summary.succeeded, summary.failed),
            (3, 1, 1)
        );
        assert!(summary.aborted.as_deref().unwrap().contains("随机验证码"));
        assert!(summary.describe().contains("已中止"));
        assert!(summary.describe().contains("未处理 1 份"));

        assert_eq!(rows.len(), 2);
        assert!(rows[0].succeeded());
        assert_eq!(rows[1].status, STATUS_FAILED);
        assert_eq!(rows[1].verification_code, NO_CODE);
        // Nothing was written for the files after the failure.
        assert!(dir.path().join("1.spdf").exists());
        assert!(!dir.path().join("2.spdf").exists());
        assert!(!dir.path().join("3.spdf").exists());
    }

    #[test]
    fn busy_guard_is_exclusive_and_clears_the_flag_even_if_the_worker_panics() {
        let flag = Arc::new(AtomicBool::new(false));
        let guard = BusyGuard::try_acquire(&flag).unwrap();
        assert!(flag.load(Ordering::Acquire));
        assert!(BusyGuard::try_acquire(&flag).is_none());
        drop(guard);
        assert!(!flag.load(Ordering::Acquire));

        let guard = BusyGuard::try_acquire(&flag).unwrap();
        let worker = std::thread::spawn(move || {
            let _held = guard;
            panic!("simulated worker crash");
        });
        assert!(worker.join().is_err());
        assert!(
            !flag.load(Ordering::Acquire),
            "flag stayed set after a panic"
        );
        assert!(BusyGuard::try_acquire(&flag).is_some());
    }

    fn row(name: &str, code: &str, status: &str, note: &str) -> BatchResultRow {
        BatchResultRow {
            source_pdf: format!(r"C:\docs\{name}"),
            output_spdf: if status == STATUS_OK {
                format!(r"C:\out\{name}.spdf")
            } else {
                String::new()
            },
            doc_id: String::new(),
            verification_code: code.to_owned(),
            max_prints: "1 次".to_owned(),
            expires_at: "永久有效".to_owned(),
            status: status.to_owned(),
            note: note.to_owned(),
        }
    }

    #[test]
    fn clipboard_listing_contains_only_successful_rows_with_their_own_codes() {
        let rows = [
            row("a.pdf", "AAAA-BBBB-CCCC-DDDD", STATUS_OK, ""),
            row("bad.txt", NO_CODE, STATUS_FAILED, "不是 PDF"),
            row("c.pdf", "EEEE-FFFF-GGGG-HHHH", STATUS_OK, ""),
        ];
        assert!(rows[0].succeeded() && !rows[1].succeeded());
        assert_eq!(rows[0].source_file_name(), "a.pdf");
        let text = clipboard_listing(&rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("a.pdf") && lines[0].contains("AAAA-BBBB-CCCC-DDDD"));
        assert!(lines[1].contains("c.pdf") && lines[1].contains("EEEE-FFFF-GGGG-HHHH"));
        assert!(!text.contains("bad.txt"));
        assert_eq!(clipboard_listing(&rows[1..2]), "");
    }

    #[test]
    fn csv_escaping_neutralizes_formula_injection_and_writes_bom_crlf() {
        assert_eq!(
            csv_escape_field("=cmd|' /C calc'!A0"),
            "\"'=cmd|' /C calc'!A0\""
        );
        assert_eq!(csv_escape_field("+1+2"), "\"'+1+2\"");
        assert_eq!(csv_escape_field("-10"), "\"'-10\"");
        assert_eq!(csv_escape_field("@SUM(A1)"), "\"'@SUM(A1)\"");
        assert_eq!(csv_escape_field("say \"hi\""), "\"say \"\"hi\"\"\"");

        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("batch.csv");
        export_batch_csv(
            &csv_path,
            &[
                BatchResultRow {
                    source_pdf: "=evil.pdf".to_owned(),
                    output_spdf: "safe.spdf".to_owned(),
                    doc_id: "0123456789abcdef0123456789abcdef".to_owned(),
                    verification_code: "ABCD-EFGH-JKLM-NPQR".to_owned(),
                    max_prints: "5".to_owned(),
                    expires_at: "永久".to_owned(),
                    status: STATUS_OK.to_owned(),
                    note: String::new(),
                },
                row(
                    "bad.txt",
                    NO_CODE,
                    STATUS_FAILED,
                    "输入文件缺少合法的 %PDF- 文件头标识。",
                ),
            ],
        )
        .unwrap();

        let raw = fs::read(&csv_path).unwrap();
        assert!(raw.starts_with(&[0xEF, 0xBB, 0xBF]));
        let body = std::str::from_utf8(&raw[3..]).unwrap();
        assert!(body.contains("\"'=evil.pdf\""));
        assert!(body.contains("\r\n"));
        // Failed rows are exported too, with their reason; `--` is neutralized like a formula.
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("\"验证码\"") && lines[0].contains("\"失败原因\""));
        assert!(lines[2].contains("\"失败\""));
        assert!(lines[2].contains("缺少合法的 %PDF- 文件头标识"));
        assert!(lines[2].contains("\"'--\""));
    }

    #[test]
    fn preview_dimension_capper_respects_3000px_limit() {
        assert_eq!(
            cap_preview_dimensions(1920, 1080, MAX_PREVIEW_SIDE),
            (1920, 1080)
        );
        assert_eq!(
            cap_preview_dimensions(6000, 3000, MAX_PREVIEW_SIDE),
            (3000, 1500)
        );
    }

    #[test]
    fn preview_pixels_are_really_premultiplied_bgra() {
        // Small image: straight conversion path.
        let small = ForensicsImage {
            width: 2,
            height: 1,
            rgba: vec![200, 100, 50, 128, 10, 20, 30, 255],
        };
        let (w, h, bgra) = forensics_preview_bgra8(&small);
        assert_eq!((w, h), (2, 1));
        assert_eq!(bgra, vec![25, 50, 100, 128, 30, 20, 10, 255]);

        // Oversized image: down-sampling path must premultiply as well.
        let wide = MAX_PREVIEW_SIDE * 2;
        let big = ForensicsImage {
            width: wide,
            height: 1,
            rgba: [200_u8, 100, 50, 128].repeat(wide as usize),
        };
        let (w, h, bgra) = forensics_preview_bgra8(&big);
        assert_eq!((w, h), (MAX_PREVIEW_SIDE, 1));
        assert_eq!(bgra.len(), MAX_PREVIEW_SIDE as usize * 4);
        assert!(
            bgra.as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [25, 50, 100, 128])
        );
    }
}
