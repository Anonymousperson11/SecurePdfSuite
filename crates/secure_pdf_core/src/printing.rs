//! Physical printer security guard (whitelist/blacklist inspection) and GDI raster printing.

use std::collections::HashMap;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_core::{PCWSTR, PWSTR};

use crate::bindings;
use crate::error::{CoreError, CoreResult};
use crate::format::SpdfPolicy;
use crate::licensing::{enumerate_ipv4_addresses, netbios_computer_name};
use crate::pdf::MemoryPdfDocument;
use crate::storage::{LocalQuotaStore, QuotaBackend, QuotaProtector, QuotaStatus};
use crate::win32_util::{HKeyGuard, to_wide_null, wide_ptr_to_string, wide_slice_to_string};

/// Maximum physical print DPI permitted per axis.
pub const MAX_PRINT_DPI: f64 = 300.0;
/// Maximum raster print bitmap pixels per page (36M px).
pub const MAX_PRINT_PIXELS: f64 = 36_000_000.0;

const BLOCKED_DRIVER_OR_NAME_KEYWORDS: &[&str] = &[
    "pdf",
    "xps document writer",
    "microsoft xps",
    "foxit",
    "kingsoft",
    "cutepdf",
    "bullzip",
    "bluebeam",
    "onenote",
    "fax",
    "snagit",
    "virtual",
    "imageprinter",
    "image writer",
    "document converter",
    "caj",
    "remote desktop easy print",
    "thinprint",
    "tp output gateway",
    "evernote",
    "虚拟",
    "传真",
];

const BLOCKED_PORT_OR_MONITOR_KEYWORDS: &[&str] = &[
    "portprompt",
    "nul",
    "file:",
    "xpsport",
    "pdf",
    "onenote",
    "fax",
    "virtual",
    "redirected",
    "tsport",
    "ts0",
    "thinprint",
    "adobe pdf",
    "microsoft print",
    "send to",
    "\\pipe\\",
];

/// Port name prefixes owned by genuine USB / WSD / TCP-IP / IPP / IEEE-1284.4 / vendor network monitors.
const PHYSICAL_PORT_PREFIXES: &[&str] = &["USB", "WSD", "IP_", "IPP", "DOT4", "CNBJNP_", "CNMFNP_"];

/// Port monitor names that only ever drive physical or standard network devices.
const PHYSICAL_MONITOR_KEYWORDS: &[&str] = &["tcp/ip", "usb", "wsd", "ipp", "bjnp", "epsonnet"];

/// Port metadata queried from `EnumPortsW` and Win32 Print Monitor registry subkeys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrinterPortMetadata {
    /// Port name (e.g. `USB001`, `192.168.1.50`, `WSD-...`).
    pub port_name: String,
    /// Monitor name (e.g. `Standard TCP/IP Port`, `USB Monitor`, `Local Monitor`).
    pub monitor_name: String,
    /// Human-readable description from `PORT_INFO_2W`.
    pub description: String,
    /// Optional `HostName` value under `Control\Print\Monitors\{monitor}\Ports\{port}`.
    pub registry_host: String,
    /// Optional `IPAddress` value under `Control\Print\Monitors\{monitor}\Ports\{port}`.
    pub registry_ip: String,
}

/// Complete candidate description evaluated by [`evaluate_printer_candidate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrinterCandidate {
    /// Installed Windows printer name.
    pub printer_name: String,
    /// Printer driver name.
    pub driver_name: String,
    /// Comma-separated port name(s).
    pub port_name: String,
    /// Optional print server name (for network shared printers).
    pub server_name: String,
    /// Raw Win32 `PRINTER_INFO_2W.Attributes` bitmask.
    pub attributes: u32,
    /// Raw Win32 `PRINTER_INFO_2W.Status` bitmask.
    pub status: u32,
    /// Whether this printer is currently the user's Windows default printer.
    pub is_default: bool,
    /// Per-port resolved metadata map keyed by lowercased port name.
    pub ports: Vec<PrinterPortMetadata>,
}

/// Policy verdict describing whether a printer is physically trustworthy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterPolicyEvaluation {
    /// Printer display/selection name.
    pub printer_name: String,
    /// Whether physical raster printing is permitted.
    pub allowed: bool,
    /// Concise summary shown in UI (`"物理端口: USB001"` or rejection summary).
    pub summary: String,
    /// Detailed explanation when blocked (`allowed == false`).
    pub reason: String,
    /// Whether this printer is the Windows default printer.
    pub is_default: bool,
    /// Driver name.
    pub driver_name: String,
    /// Port name(s).
    pub port_name: String,
    /// Whether Windows reports the printer as offline (work-offline attribute or offline status).
    pub is_offline: bool,
}

impl PrinterCandidate {
    /// `PRINTER_ATTRIBUTE_WORK_OFFLINE` set or `PRINTER_STATUS_OFFLINE` reported (as in v1.x).
    #[must_use]
    pub const fn is_offline(&self) -> bool {
        (self.attributes & 0x0000_0400) != 0 || (self.status & 0x0000_0080) != 0
    }
}

/// Parameters required to execute a guarded GDI physical print job.
#[derive(Debug, Clone)]
pub struct PrintJobContext<'a> {
    /// Decrypted `.spdf` document policy.
    pub policy: &'a SpdfPolicy,
    /// Installed Windows printer name selected by user.
    pub printer_name: &'a str,
    /// Current machine SHA-256 fingerprint for quota verification.
    pub machine_hash: &'a [u8; 32],
    /// Zero-based page indices to print, in output order (must be non-empty and in range).
    pub page_indices: &'a [i32],
    /// Trace watermark burned onto every printed page regardless of policy flags
    /// (see [`build_print_watermark_text`]); must not be blank.
    pub watermark_text: &'a str,
}

/// Builds the trace watermark burned onto printed pages: optional note, local time, IP and
/// machine code (`"{note} | {time} | IP:{ip} | {code}"`, identical to v1.x).
#[must_use]
pub fn build_print_watermark_text(
    watermark_note: &str,
    local_time: &str,
    ipv4: &str,
    machine_code: &str,
) -> String {
    let stamp = format!("{local_time} | IP:{ipv4} | {machine_code}");
    let note = watermark_note.trim();
    if note.is_empty() {
        stamp
    } else {
        format!("{note} | {stamp}")
    }
}

/// Parses a user page selection such as `1-3,5` into sorted, de-duplicated zero-based indices.
///
/// Accepts `,` `，` `;` `；` and spaces as separators and `-` `－` `—` `~` as range dashes.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] with a user-facing message when the specification is
/// empty, malformed, reversed or outside `1..=page_count`.
pub fn parse_page_ranges(spec: &str, page_count: i32) -> CoreResult<Vec<i32>> {
    let invalid = |msg: String| CoreError::InvalidArgument(msg);
    if spec.trim().is_empty() {
        return Err(invalid("请输入自定义页码范围，例如 1-3,5".to_owned()));
    }
    let parse_page = |text: &str| -> Option<i32> {
        let t = text.trim();
        if t.is_empty() || t.len() > 9 || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse::<i32>().ok()
    };

    let mut pages = std::collections::BTreeSet::new();
    for token in spec
        .split([',', '，', ';', '；', ' ', '\t'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let normalized: String = token
            .chars()
            .map(|c| {
                if matches!(c, '－' | '—' | '~' | '～') {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        if let Some((left, right)) = normalized.split_once('-') {
            match (parse_page(left), parse_page(right)) {
                (Some(start), Some(end)) if start >= 1 && end >= start && end <= page_count => {
                    pages.extend((start..=end).map(|p| p - 1));
                }
                _ => {
                    return Err(invalid(format!(
                        "无效页码区间: {token}（文档共 {page_count} 页）"
                    )));
                }
            }
        } else {
            match parse_page(&normalized) {
                Some(single) if single >= 1 && single <= page_count => {
                    pages.insert(single - 1);
                }
                _ => {
                    return Err(invalid(format!(
                        "无效页码: {token}（文档共 {page_count} 页）"
                    )));
                }
            }
        }
    }
    if pages.is_empty() {
        return Err(invalid("未解析到有效页码。".to_owned()));
    }
    Ok(pages.into_iter().collect())
}

/// Lists all installed printers annotated with security policy evaluation.
#[must_use]
pub fn list_evaluated_printers() -> Vec<PrinterPolicyEvaluation> {
    let (_primary, local_ips) = enumerate_ipv4_addresses();
    let local_hosts = local_host_aliases();
    let port_map = enumerate_ports_map();
    let default_name = get_default_printer_name().unwrap_or_default();

    enumerate_printers_raw(&default_name, &port_map)
        .into_iter()
        .map(|cand| evaluate_printer_candidate(&cand, &local_ips, &local_hosts))
        .collect()
}

/// Evaluates the security policy for a single named Windows printer.
///
/// # Errors
///
/// Returns [`CoreError::PrinterSecurity`] if no installed printer matches `printer_name`.
pub fn evaluate_printer_by_name(printer_name: &str) -> CoreResult<PrinterPolicyEvaluation> {
    let evaluated = list_evaluated_printers()
        .into_iter()
        .find(|p| p.printer_name.eq_ignore_ascii_case(printer_name.trim()));
    evaluated.ok_or_else(|| {
        CoreError::PrinterSecurity(format!("未在系统中找到名为“{printer_name}”的打印机。"))
    })
}

/// Pure policy engine deciding whether `candidate` is a genuine local/LAN physical printer.
#[must_use]
pub fn evaluate_printer_candidate(
    candidate: &PrinterCandidate,
    local_ips: &[String],
    local_hosts: &[String],
) -> PrinterPolicyEvaluation {
    if let Some(reason) = check_printer_attributes_and_names(candidate, local_ips, local_hosts) {
        return blocked_eval(candidate, &reason);
    }

    let port_tokens: Vec<&str> = candidate
        .port_name
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if port_tokens.is_empty() {
        return blocked_eval(candidate, "打印机端口配置为空，无法验证物理输出路径。");
    }

    for token in &port_tokens {
        let meta = candidate
            .ports
            .iter()
            .find(|p| p.port_name.eq_ignore_ascii_case(token));
        if let Some(reason) = check_single_port(token, meta, local_ips, local_hosts) {
            return blocked_eval(candidate, &reason);
        }
    }

    PrinterPolicyEvaluation {
        printer_name: candidate.printer_name.clone(),
        allowed: true,
        summary: format!("物理输出允许（端口：{}）", candidate.port_name),
        reason: String::new(),
        is_default: candidate.is_default,
        driver_name: candidate.driver_name.clone(),
        port_name: candidate.port_name.clone(),
        is_offline: candidate.is_offline(),
    }
}

fn blocked_eval(candidate: &PrinterCandidate, reason: &str) -> PrinterPolicyEvaluation {
    PrinterPolicyEvaluation {
        printer_name: candidate.printer_name.clone(),
        allowed: false,
        summary: format!("已禁止：{reason}"),
        reason: reason.to_owned(),
        is_default: candidate.is_default,
        driver_name: candidate.driver_name.clone(),
        port_name: candidate.port_name.clone(),
        is_offline: candidate.is_offline(),
    }
}

fn check_printer_attributes_and_names(
    candidate: &PrinterCandidate,
    local_ips: &[String],
    local_hosts: &[String],
) -> Option<String> {
    let fax_flag = u32::try_from(bindings::PRINTER_ATTRIBUTE_FAX).unwrap_or(0x4000);
    if (candidate.attributes & fax_flag) != 0 {
        return Some("传真设备不允许接收受保护文档。".to_owned());
    }
    let keep_jobs_flag =
        u32::try_from(bindings::PRINTER_ATTRIBUTE_KEEPPRINTEDJOBS).unwrap_or(0x100);
    if (candidate.attributes & keep_jobs_flag) != 0 {
        return Some("打印机启用了“保留打印后台文档”，存在假脱机泄露风险。".to_owned());
    }

    let name_l = candidate.printer_name.to_lowercase();
    let driver_l = candidate.driver_name.to_lowercase();
    for kw in BLOCKED_DRIVER_OR_NAME_KEYWORDS {
        if name_l.contains(kw) || driver_l.contains(kw) {
            return Some(format!("检测到受限虚拟/文档导出打印驱动关键字“{kw}”。"));
        }
    }

    let srv = candidate.server_name.trim();
    if !srv.is_empty()
        && is_local_or_loopback_target(srv.trim_start_matches('\\'), local_ips, local_hosts)
    {
        return Some(format!("网络共享打印服务器“{srv}”指向本机回环地址。"));
    }

    None
}

fn check_single_port(
    port_token: &str,
    meta: Option<&PrinterPortMetadata>,
    local_ips: &[String],
    local_hosts: &[String],
) -> Option<String> {
    let port_l = port_token.to_lowercase();
    let monitor_l = meta.map_or_else(String::new, |m| m.monitor_name.to_lowercase());
    let desc_l = meta.map_or_else(String::new, |m| m.description.to_lowercase());

    for kw in BLOCKED_PORT_OR_MONITOR_KEYWORDS {
        if port_l.contains(kw) || monitor_l.contains(kw) || desc_l.contains(kw) {
            return Some(format!("端口或监视器包含高风险虚拟重定向特征“{kw}”。"));
        }
    }

    if is_drive_file_path(port_token) {
        return Some(format!("端口“{port_token}”指向本地磁盘文件路径。"));
    }

    let is_unc = port_token.starts_with(r"\\");
    if is_unc {
        // Only `\\server\share` is a shared printer; anything deeper is "print to network file",
        // and `\\.\` / `\\?\` are device / pipe namespaces.
        let segments: Vec<&str> = port_token[2..].split('\\').collect();
        let well_formed = segments.len() == 2
            && !segments[0].is_empty()
            && !segments[1].is_empty()
            && segments[0] != "."
            && segments[0] != "?";
        if !well_formed {
            return Some(format!(
                "端口“{port_token}”不是 \\\\服务器\\共享名 形式的共享打印机，属于文件或管道输出。"
            ));
        }
        if is_local_or_loopback_target(segments[0], local_ips, local_hosts) {
            return Some(format!("UNC 打印路径指向本机环回目标“{}”。", segments[0]));
        }
    }

    let is_parallel_or_serial = is_lpt_or_com_port(port_token);
    if (monitor_l.contains("local monitor") || monitor_l.contains("local port"))
        && !is_parallel_or_serial
        && !is_unc
    {
        return Some(format!(
            "Local Port 端口“{port_token}”非物理 LPT/COM 或远程共享，可能写入本地文件。"
        ));
    }

    if let Some(m) = meta {
        for target in [&m.registry_host, &m.registry_ip] {
            let trimmed = target.trim();
            if !trimmed.is_empty() && is_local_or_loopback_target(trimmed, local_ips, local_hosts) {
                return Some(format!("网络打印端口目标地址“{trimmed}”指向本机。"));
            }
        }
    }

    // Standard TCP/IP ports are conventionally named `IP_<address>`; compare the bare address.
    let literal_host = strip_prefix_ignore_ascii_case(port_token.trim(), "IP_")
        .unwrap_or_else(|| port_token.trim());
    if (looks_like_ipv4(literal_host) || looks_like_bare_ip_or_host(port_token))
        && is_local_or_loopback_target(literal_host, local_ips, local_hosts)
    {
        return Some(format!("打印端口“{port_token}”指向本机地址。"));
    }

    // Whitelist verdict: anything that is not recognisably physical / standard network is refused.
    let looks_physical = is_parallel_or_serial
        || is_unc
        || PHYSICAL_PORT_PREFIXES
            .iter()
            .any(|prefix| strip_prefix_ignore_ascii_case(port_token.trim(), prefix).is_some())
        || looks_like_ipv4(port_token.trim())
        || PHYSICAL_MONITOR_KEYWORDS
            .iter()
            .any(|kw| monitor_l.contains(kw));
    if !looks_physical {
        return Some(format!("未识别为物理或标准网络端口（{port_token}）。"));
    }

    None
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// `X:`, `X:\...` or `X:/...` — a local drive (file) output target.
const fn is_drive_file_path(port: &str) -> bool {
    let bytes = port.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'\\' || bytes[2] == b'/')
}

/// Genuine parallel / serial ports such as `LPT1:` or `COM3` (one to three digits).
fn is_lpt_or_com_port(port: &str) -> bool {
    let upper = port.trim().to_ascii_uppercase();
    let Some(rest) = upper
        .strip_prefix("LPT")
        .or_else(|| upper.strip_prefix("COM"))
    else {
        return false;
    };
    let digits = rest.trim_end_matches(':');
    !digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Strict dotted-quad IPv4 literal (four decimal groups of 1-3 digits, each <= 255).
fn looks_like_ipv4(text: &str) -> bool {
    let mut groups = 0_usize;
    for part in text.split('.') {
        groups += 1;
        if part.is_empty()
            || part.len() > 3
            || !part.bytes().all(|b| b.is_ascii_digit())
            || part.parse::<u16>().map_or(true, |v| v > 255)
        {
            return false;
        }
    }
    groups == 4
}

fn looks_like_bare_ip_or_host(port: &str) -> bool {
    let lower = port.trim().to_ascii_lowercase();
    lower == "localhost"
        || lower == "::1"
        || lower.parse::<std::net::Ipv4Addr>().is_ok()
        || lower.starts_with("127.")
}

fn is_local_or_loopback_target(target: &str, local_ips: &[String], local_hosts: &[String]) -> bool {
    let trimmed = target
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    if trimmed == "::1" {
        return true;
    }
    let cleaned = trimmed.split(':').next().unwrap_or("").to_ascii_lowercase();
    if cleaned.is_empty() {
        return false;
    }
    if cleaned == "localhost"
        || cleaned == "."
        || cleaned == "0.0.0.0"
        || cleaned.starts_with("127.")
    {
        return true;
    }
    if local_ips.iter().any(|ip| ip.eq_ignore_ascii_case(&cleaned)) {
        return true;
    }
    local_hosts.iter().any(|h| {
        let host = h.to_ascii_lowercase();
        !host.is_empty() && (cleaned == host || cleaned.starts_with(&format!("{host}.")))
    })
}

fn local_host_aliases() -> Vec<String> {
    let base = netbios_computer_name().trim().to_ascii_lowercase();
    if base.is_empty() {
        Vec::new()
    } else {
        vec![base]
    }
}

/// Prints the selected pages of `pdf` to physical `ctx.printer_name` with the trace watermark
/// burned onto every page, enforcing `LocalQuotaStore` and rolling back the slot if zero physical
/// pages were sent.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`], [`CoreError::PrinterSecurity`],
/// [`CoreError::QuotaExhausted`] or [`CoreError::DocumentExpired`] before any quota is consumed,
/// and [`CoreError::PrintFailed`] (stating whether the print slot was consumed) once the print
/// pipeline has started.
pub fn print_document_guarded<B: QuotaBackend, P: QuotaProtector, F: FnMut(i32, i32)>(
    pdf: &MemoryPdfDocument,
    quota_store: &LocalQuotaStore<B, P>,
    ctx: &PrintJobContext<'_>,
    mut on_page_progress: F,
) -> CoreResult<QuotaStatus> {
    validate_print_request(ctx, pdf.page_count())?;
    if ctx.policy.max_prints <= 0 {
        return Err(CoreError::QuotaExhausted);
    }

    // Re-scan right before printing so a port silently switched to file output is caught.
    let eval = evaluate_printer_by_name(ctx.printer_name)?;
    if !eval.allowed {
        return Err(CoreError::PrinterSecurity(eval.reason));
    }

    let status_after_consume = quota_store.consume_print_slot(ctx.policy, ctx.machine_hash)?;

    match execute_gdi_print_job(pdf, ctx, &mut on_page_progress) {
        Ok(pages_printed) if pages_printed > 0 => Ok(status_after_consume),
        Ok(_) => Err(finish_failed_print(
            quota_store,
            ctx,
            &CoreError::PrintCancelledOrEmpty,
            0,
        )),
        Err((err, pages_printed)) => {
            Err(finish_failed_print(quota_store, ctx, &err, pages_printed))
        }
    }
}

/// Validates page selection and watermark before any quota is touched.
fn validate_print_request(ctx: &PrintJobContext<'_>, page_count: i32) -> CoreResult<()> {
    if ctx.page_indices.is_empty() {
        return Err(CoreError::InvalidArgument(
            "至少需要选择一页进行打印。".to_owned(),
        ));
    }
    if ctx
        .page_indices
        .iter()
        .any(|&idx| idx < 0 || idx >= page_count)
    {
        return Err(CoreError::InvalidArgument("页码超出文档范围。".to_owned()));
    }
    if ctx.watermark_text.trim().is_empty() {
        return Err(CoreError::InvalidArgument(
            "打印追溯水印内容为空，已拒绝打印。".to_owned(),
        ));
    }
    Ok(())
}

/// Rolls the reserved slot back when nothing reached the printer and reports the outcome.
fn finish_failed_print<B: QuotaBackend, P: QuotaProtector>(
    quota_store: &LocalQuotaStore<B, P>,
    ctx: &PrintJobContext<'_>,
    cause: &CoreError,
    pages_sent: i32,
) -> CoreError {
    let quota_consumed = pages_sent > 0
        || quota_store
            .rollback_print_slot(ctx.policy, ctx.machine_hash)
            .is_err();
    CoreError::PrintFailed {
        message: describe_print_failure(&cause.to_string(), pages_sent, quota_consumed),
        quota_consumed,
    }
}

/// User-facing failure text that always states whether the print count was deducted.
fn describe_print_failure(cause: &str, pages_sent: i32, quota_consumed: bool) -> String {
    let suffix = if !quota_consumed {
        "尚未送出任何页面，本次打印次数未扣减。".to_owned()
    } else if pages_sent > 0 {
        format!("已有 {pages_sent} 页送往打印机，本次打印次数已扣减。")
    } else {
        "打印次数归还失败，本次打印次数已扣减。".to_owned()
    };
    format!("{cause}\n{suffix}")
}

struct PrinterDcGuard(bindings::HDC);

impl Drop for PrinterDcGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Releases Win32 printer DC owned by this guard.
            unsafe {
                let _ = bindings::DeleteDC(self.0);
            }
        }
    }
}

struct GdiObjectGuard(bindings::HGDIOBJ);

impl Drop for GdiObjectGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Deletes GDI font object created for this print job.
            unsafe {
                let _ = bindings::DeleteObject(self.0);
            }
        }
    }
}

fn execute_gdi_print_job<F: FnMut(i32, i32)>(
    pdf: &MemoryPdfDocument,
    ctx: &PrintJobContext<'_>,
    on_page_progress: &mut F,
) -> Result<i32, (CoreError, i32)> {
    let driver = to_wide_null("WINSPOOL");
    let device = to_wide_null(ctx.printer_name);
    // SAFETY: Creates non-file physical printer GDI DC using `WINSPOOL` driver.
    let raw_dc = unsafe {
        bindings::CreateDCW(
            PCWSTR(driver.as_ptr()),
            PCWSTR(device.as_ptr()),
            PCWSTR::null(),
            None,
        )
    };
    if raw_dc.0.is_null() {
        return Err((
            CoreError::PrinterSecurity("无法创建打印机 GDI 设备上下文。".to_owned()),
            0,
        ));
    }
    let dc = PrinterDcGuard(raw_dc);

    // SAFETY: Queries printer technology flag to block metafile/capture virtual devices.
    let tech = unsafe { bindings::GetDeviceCaps(Some(dc.0), bindings::TECHNOLOGY) };
    if tech == bindings::DT_METAFILE {
        return Err((
            CoreError::PrinterSecurity("检测到图元文件(Metafile)虚拟打印输出，已阻止。".to_owned()),
            0,
        ));
    }

    // SAFETY: Queries horizontal/vertical printable device pixels & DPI.
    let dev_w = unsafe { bindings::GetDeviceCaps(Some(dc.0), bindings::HORZRES) }.max(1);
    // SAFETY: Queries vertical printable pixels.
    let dev_h = unsafe { bindings::GetDeviceCaps(Some(dc.0), bindings::VERTRES) }.max(1);
    // SAFETY: Queries horizontal DPI.
    let dpi_x = unsafe { bindings::GetDeviceCaps(Some(dc.0), bindings::LOGPIXELSX) }.max(72);
    // SAFETY: Queries vertical DPI.
    let dpi_y = unsafe { bindings::GetDeviceCaps(Some(dc.0), bindings::LOGPIXELSY) }.max(72);

    let doc_name_w = to_wide_null(&format!("SecurePdf - {}", ctx.policy.original_file_name));
    let di = bindings::DOCINFOW {
        cbSize: i32::try_from(size_of::<bindings::DOCINFOW>()).unwrap_or(40),
        lpszDocName: PCWSTR(doc_name_w.as_ptr()),
        lpszOutput: PCWSTR::null(),
        lpszDatatype: PCWSTR::null(),
        fwType: 0,
    };

    // SAFETY: Starts print job strictly without an output file path (`lpszOutput = NULL`).
    let job_id = unsafe { bindings::StartDocW(dc.0, &raw const di) };
    if job_id <= 0 {
        return Err((
            CoreError::PrinterSecurity("打印机拒绝了打印作业，或打印已被取消。".to_owned()),
            0,
        ));
    }

    let total = i32::try_from(ctx.page_indices.len()).unwrap_or(i32::MAX);
    let mut pages_sent = 0_i32;

    for &page_index in ctx.page_indices {
        on_page_progress(pages_sent + 1, total);
        let res = print_single_page(
            pdf,
            dc.0,
            page_index,
            dev_w,
            dev_h,
            dpi_x,
            dpi_y,
            ctx.watermark_text,
        );
        if let Err(err) = res {
            // SAFETY: Aborts incomplete GDI print job cleanly before returning error.
            unsafe {
                let _ = bindings::AbortDoc(dc.0);
            }
            return Err((err, pages_sent));
        }
        pages_sent += 1;
    }

    // SAFETY: Finalizes spooled physical GDI print job.
    let end_rc = unsafe { bindings::EndDoc(dc.0) };
    if end_rc <= 0 {
        return Err((
            CoreError::PrinterSecurity("提交物理打印作业失败。".to_owned()),
            pages_sent,
        ));
    }
    Ok(pages_sent)
}

#[allow(clippy::too_many_arguments)]
fn print_single_page(
    pdf: &MemoryPdfDocument,
    dc: bindings::HDC,
    page_index: i32,
    dev_w: i32,
    dev_h: i32,
    dpi_x: i32,
    dpi_y: i32,
    watermark_text: &str,
) -> CoreResult<()> {
    let (width_pt, height_pt) = pdf.page_size_points(page_index)?;
    let (raster_w, raster_h, dest_x, dest_y, dest_w, dest_h) =
        compute_print_geometry(width_pt, height_pt, dev_w, dev_h, dpi_x, dpi_y);

    let (bgr_bytes, _stride) = pdf.render_page_bgr24_for_print(page_index, raster_w, raster_h)?;

    // SAFETY: Begins a new physical printer page on active GDI DC.
    if unsafe { bindings::StartPage(dc) } <= 0 {
        return Err(CoreError::PrinterSecurity(
            "打印机 StartPage 失败。".to_owned(),
        ));
    }

    // SAFETY: Enables high-quality halftone scaling when stretching PDF bitmap to printer page.
    unsafe {
        let _ = bindings::SetStretchBltMode(dc, bindings::HALFTONE);
    }

    let bmi = bindings::BITMAPINFO {
        bmiHeader: bindings::BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<bindings::BITMAPINFOHEADER>()).unwrap_or(40),
            biWidth: raster_w,
            biHeight: -raster_h, // Top-down DIB orientation
            biPlanes: 1,
            biBitCount: 24,
            biCompression: u32::try_from(bindings::BI_RGB).unwrap_or(0),
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        ..Default::default()
    };

    // SAFETY: Transfers top-down 24-bpp `bgr_bytes` raster into physical printer `dc`.
    let lines = unsafe {
        bindings::StretchDIBits(
            dc,
            dest_x,
            dest_y,
            dest_w,
            dest_h,
            0,
            0,
            raster_w,
            raster_h,
            Some(bgr_bytes.as_ptr().cast()),
            &raw const bmi,
            u32::try_from(bindings::DIB_RGB_COLORS).unwrap_or(0),
            bindings::SRCCOPY,
        )
    };
    if lines <= 0 {
        return Err(CoreError::PrinterSecurity(
            "打印光栅位图输出失败。".to_owned(),
        ));
    }

    // The trace watermark is burned unconditionally: printed copies must always be attributable.
    draw_gdi_rotated_watermark(dc, dev_w, dev_h, dpi_y, watermark_text);

    // SAFETY: Ends the current physical page and flushes strokes to the printer driver.
    if unsafe { bindings::EndPage(dc) } <= 0 {
        return Err(CoreError::PrinterSecurity(
            "打印机 EndPage 失败。".to_owned(),
        ));
    }
    Ok(())
}

fn compute_print_geometry(
    width_pt: f64,
    height_pt: f64,
    dev_w: i32,
    dev_h: i32,
    dpi_x: i32,
    dpi_y: i32,
) -> (i32, i32, i32, i32, i32, i32) {
    let dev_width_px = f64::from(dev_w);
    let dev_height_px = f64::from(dev_h);
    let scale = (dev_width_px / width_pt).min(dev_height_px / height_pt);
    let out_width_px = (width_pt * scale).max(1.0);
    let out_height_px = (height_pt * scale).max(1.0);
    #[allow(clippy::cast_possible_truncation)]
    let dest_w = (out_width_px.round() as i32).clamp(1, dev_w);
    #[allow(clippy::cast_possible_truncation)]
    let dest_h = (out_height_px.round() as i32).clamp(1, dev_h);
    let dest_x = (dev_w - dest_w) / 2;
    let dest_y = (dev_h - dest_h) / 2;

    let cap_x = (MAX_PRINT_DPI / f64::from(dpi_x)).min(1.0);
    let cap_y = (MAX_PRINT_DPI / f64::from(dpi_y)).min(1.0);
    let mut raster_scale = cap_x.min(cap_y);
    let cand_pixels = (out_width_px * raster_scale) * (out_height_px * raster_scale);
    if cand_pixels > MAX_PRINT_PIXELS {
        raster_scale *= (MAX_PRINT_PIXELS / cand_pixels).sqrt();
    }
    #[allow(clippy::cast_possible_truncation)]
    let raster_w = ((out_width_px * raster_scale).round() as i32).max(16);
    #[allow(clippy::cast_possible_truncation)]
    let raster_h = ((out_height_px * raster_scale).round() as i32).max(16);
    (raster_w, raster_h, dest_x, dest_y, dest_w, dest_h)
}

fn draw_gdi_rotated_watermark(dc: bindings::HDC, dev_w: i32, dev_h: i32, dpi_y: i32, text: &str) {
    #[allow(clippy::cast_possible_truncation)]
    let font_px = ((f64::from(dpi_y) * 14.0 / 72.0).round() as i32).clamp(14, 160);
    let face = to_wide_null("Microsoft YaHei");
    // SAFETY: Creates standard TrueType bold GDI font sized to physical printer DPI.
    let font = unsafe {
        bindings::CreateFontW(
            -font_px,
            0,
            0,
            0,
            bindings::FW_BOLD,
            0,
            0,
            0,
            1, // DEFAULT_CHARSET
            u32::try_from(bindings::OUT_TT_PRECIS).unwrap_or(0),
            u32::try_from(bindings::CLIP_DEFAULT_PRECIS).unwrap_or(0),
            u32::try_from(bindings::CLEARTYPE_QUALITY).unwrap_or(0),
            u32::try_from(bindings::DEFAULT_PITCH).unwrap_or(0),
            PCWSTR(face.as_ptr()),
        )
    };
    if font.0.is_null() {
        return;
    }
    let _font_guard = GdiObjectGuard(bindings::HGDIOBJ(font.0));
    // SAFETY: Selects watermark font into `dc` and preserves previous font to restore cleanly.
    let prev_obj = unsafe { bindings::SelectObject(dc, bindings::HGDIOBJ(font.0)) };

    // SAFETY: Enables advanced GDI world transform for smooth -30 degree watermark stamping.
    unsafe {
        let _ = bindings::SetGraphicsMode(dc, bindings::GM_ADVANCED);
    }
    // SAFETY: Draws watermark glyphs with transparent background fill.
    unsafe {
        let _ = bindings::SetBkMode(dc, bindings::TRANSPARENT);
    }
    // SAFETY: Subtle cool-gray trace color (`RGB(150, 158, 170)` -> COLORREF `0x00AA9E96`).
    unsafe {
        let _ = bindings::SetTextColor(dc, bindings::COLORREF(0x00AA_9E96));
    }
    // SAFETY: Centers drawn string horizontally around transform origin.
    unsafe {
        let _ = bindings::SetTextAlign(
            dc,
            u32::try_from(bindings::TA_CENTER | bindings::TA_BASELINE).unwrap_or(0),
        );
    }

    let utf16: Vec<u16> = text.encode_utf16().collect();
    #[allow(clippy::cast_precision_loss)]
    let est_w = (utf16.len() as f32) * (font_px as f32) * 0.65;
    #[allow(clippy::cast_precision_loss)]
    let est_h = font_px as f32;
    #[allow(clippy::cast_possible_truncation)]
    let step_x = ((est_w * 1.35).max(220.0) as i32).max(220);
    #[allow(clippy::cast_possible_truncation)]
    let step_y = ((est_h * 3.8).max(160.0) as i32).max(160);

    let angle_rad = (-30.0_f32).to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let mut y = -dev_h / 2;
    while y < dev_h * 3 / 2 {
        let mut x = -dev_w / 2;
        while x < dev_w * 3 / 2 {
            #[allow(clippy::cast_precision_loss)]
            let xf = bindings::XFORM {
                eM11: cos_a,
                eM12: sin_a,
                eM21: -sin_a,
                eM22: cos_a,
                eDx: x as f32,
                eDy: y as f32,
            };
            // SAFETY: Applies translation + rotation world transform for current grid cell.
            unsafe {
                let _ = bindings::SetWorldTransform(dc, &raw const xf);
            }
            // SAFETY: Renders watermark text slice at transformed `(0, 0)` origin.
            unsafe {
                let _ = bindings::TextOutW(dc, 0, 0, &utf16);
            }
            x += step_x;
        }
        y += step_y;
    }

    // SAFETY: Restores identity transform and previous GDI font before returning.
    unsafe {
        let _ = bindings::ModifyWorldTransform(
            dc,
            None,
            u32::try_from(bindings::MWT_IDENTITY).unwrap_or(1),
        );
    }
    if !prev_obj.0.is_null() {
        // SAFETY: Restores original GDI font handle into `dc`.
        unsafe {
            let _ = bindings::SelectObject(dc, prev_obj);
        }
    }
}

fn get_default_printer_name() -> Option<String> {
    let mut len = 0_u32;
    // SAFETY: Probes buffer size for default printer name.
    let _ = unsafe { bindings::GetDefaultPrinterW(None, &raw mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0_u16; usize::try_from(len).ok()?];
    // SAFETY: Reads default printer name into `buf`.
    let ok = unsafe { bindings::GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &raw mut len) };
    if ok.as_bool() {
        Some(wide_slice_to_string(&buf).trim().to_owned())
    } else {
        None
    }
}

fn enumerate_ports_map() -> HashMap<String, PrinterPortMetadata> {
    let mut needed = 0_u32;
    let mut returned = 0_u32;
    // SAFETY: Probes required byte buffer size for `EnumPortsW(level = 2)`.
    let _ = unsafe {
        bindings::EnumPortsW(
            PCWSTR::null(),
            2,
            None,
            0,
            &raw mut needed,
            &raw mut returned,
        )
    };
    let mut map = HashMap::new();
    if needed == 0 {
        return map;
    }
    let mut aligned = vec![0_u64; usize::try_from(needed).unwrap_or(0).div_ceil(8)];
    let ok = unsafe {
        // SAFETY: `aligned` provides 8-byte aligned storage of at least `needed` bytes.
        bindings::EnumPortsW(
            PCWSTR::null(),
            2,
            Some(aligned.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
            &raw mut returned,
        )
    };
    if !ok.as_bool() || returned == 0 {
        return map;
    }

    let info_ptr = aligned.as_ptr().cast::<bindings::PORT_INFO_2W>();
    let count = usize::try_from(returned).unwrap_or(0);
    // SAFETY: `EnumPortsW` populated `count` contiguous `PORT_INFO_2W` entries inside `aligned`.
    let infos = unsafe { std::slice::from_raw_parts(info_ptr, count) };
    for info in infos {
        // SAFETY: String pointers inside `PORT_INFO_2W` remain valid within `aligned`.
        let port_name = unsafe { wide_ptr_to_string(info.pPortName.0) }
            .trim()
            .to_owned();
        if port_name.is_empty() {
            continue;
        }
        // SAFETY: Same lifetime guarantee within `aligned`.
        let monitor_name = unsafe { wide_ptr_to_string(info.pMonitorName.0) }
            .trim()
            .to_owned();
        // SAFETY: Same lifetime guarantee within `aligned`.
        let description = unsafe { wide_ptr_to_string(info.pDescription.0) }
            .trim()
            .to_owned();
        let (registry_host, registry_ip) = read_port_registry_targets(&monitor_name, &port_name);
        map.insert(
            port_name.to_ascii_lowercase(),
            PrinterPortMetadata {
                port_name,
                monitor_name,
                description,
                registry_host,
                registry_ip,
            },
        );
    }
    map
}

fn read_port_registry_targets(monitor_name: &str, port_name: &str) -> (String, String) {
    if monitor_name.is_empty() || port_name.is_empty() {
        return (String::new(), String::new());
    }
    let subkey = format!(
        r"SYSTEM\CurrentControlSet\Control\Print\Monitors\{monitor_name}\Ports\{port_name}"
    );
    let subkey_w = to_wide_null(&subkey);
    let mut raw_key = bindings::HKEY(null_mut());
    let access =
        u32::try_from(bindings::KEY_READ | bindings::KEY_WOW64_64KEY).unwrap_or(0x0002_0119);
    // SAFETY: Opens HKLM print monitor port subkey read-only if it exists.
    let rc = unsafe {
        bindings::RegOpenKeyExW(
            bindings::HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_w.as_ptr()),
            None,
            bindings::ACCESS_MASK(access),
            &raw mut raw_key,
        )
    };
    if rc.0 != 0 {
        return (String::new(), String::new());
    }
    let key = HKeyGuard(raw_key);
    (
        read_reg_sz(key.0, "HostName").unwrap_or_default(),
        read_reg_sz(key.0, "IPAddress").unwrap_or_default(),
    )
}

fn read_reg_sz(hkey: bindings::HKEY, value: &str) -> Option<String> {
    let val_w = to_wide_null(value);
    let mut ty = 0_u32;
    let mut len = 0_u32;
    // SAFETY: Probes registry string byte size.
    let rc = unsafe {
        bindings::RegQueryValueExW(
            hkey,
            PCWSTR(val_w.as_ptr()),
            None,
            Some(&raw mut ty),
            None,
            &raw mut len,
        )
    };
    if rc.0 != 0 || ty != bindings::REG_SZ || len < 2 {
        return None;
    }
    let mut buf = vec![0_u8; usize::try_from(len).ok()?];
    // SAFETY: Reads UTF-16 registry string bytes into `buf`.
    let rc2 = unsafe {
        bindings::RegQueryValueExW(
            hkey,
            PCWSTR(val_w.as_ptr()),
            None,
            Some(&raw mut ty),
            Some(buf.as_mut_ptr()),
            &raw mut len,
        )
    };
    if rc2.0 != 0 || ty != bindings::REG_SZ {
        return None;
    }
    let actual = usize::try_from(len).ok()?.min(buf.len());
    let wide: Vec<u16> = buf[..actual]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    Some(wide_slice_to_string(&wide).trim().to_owned())
}

fn enumerate_printers_raw(
    default_printer: &str,
    port_map: &HashMap<String, PrinterPortMetadata>,
) -> Vec<PrinterCandidate> {
    let flags = u32::try_from(bindings::PRINTER_ENUM_LOCAL | bindings::PRINTER_ENUM_CONNECTIONS)
        .unwrap_or(6);
    let mut needed = 0_u32;
    let mut returned = 0_u32;
    // SAFETY: Probes byte size for `EnumPrintersW(level = 2)`.
    let _ = unsafe {
        bindings::EnumPrintersW(
            flags,
            PCWSTR::null(),
            2,
            None,
            0,
            &raw mut needed,
            &raw mut returned,
        )
    };
    if needed == 0 {
        return Vec::new();
    }

    let mut aligned = vec![0_u64; usize::try_from(needed).unwrap_or(0).div_ceil(8)];
    // SAFETY: Populates 8-byte aligned buffer with `PRINTER_INFO_2W` structures.
    let ok = unsafe {
        bindings::EnumPrintersW(
            flags,
            PCWSTR::null(),
            2,
            Some(aligned.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
            &raw mut returned,
        )
    };
    if !ok.as_bool() || returned == 0 {
        return Vec::new();
    }

    let count = usize::try_from(returned).unwrap_or(0);
    // SAFETY: `EnumPrintersW` wrote `count` contiguous `PRINTER_INFO_2W` records inside `aligned`.
    let infos = unsafe {
        std::slice::from_raw_parts(aligned.as_ptr().cast::<bindings::PRINTER_INFO_2W>(), count)
    };
    let mut out = Vec::with_capacity(count);
    for info in infos {
        // SAFETY: `pPrinterName` points into `aligned`.
        let printer_name = unsafe { wide_ptr_to_string(info.pPrinterName.0) }
            .trim()
            .to_owned();
        if printer_name.is_empty() {
            continue;
        }
        // SAFETY: `pDriverName` points into `aligned`.
        let driver_name = unsafe { wide_ptr_to_string(info.pDriverName.0) }
            .trim()
            .to_owned();
        // SAFETY: `pPortName` points into `aligned`.
        let port_name = unsafe { wide_ptr_to_string(info.pPortName.0) }
            .trim()
            .to_owned();
        // SAFETY: `pServerName` points into `aligned`.
        let server_name = unsafe { wide_ptr_to_string(info.pServerName.0) }
            .trim()
            .to_owned();

        let ports = port_name
            .split(',')
            .map(str::trim)
            .filter_map(|t| port_map.get(&t.to_ascii_lowercase()).cloned())
            .collect();

        out.push(PrinterCandidate {
            is_default: !default_printer.is_empty()
                && printer_name.eq_ignore_ascii_case(default_printer),
            printer_name,
            driver_name,
            port_name,
            server_name,
            attributes: info.Attributes,
            status: info.Status,
            ports,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cand: &PrinterCandidate) -> PrinterPolicyEvaluation {
        evaluate_printer_candidate(
            cand,
            &["192.168.1.100".to_owned(), "10.0.0.5".to_owned()],
            &["workstation-a".to_owned(), "workstation-a.local".to_owned()],
        )
    }

    #[test]
    fn allows_usb_wsd_and_remote_network_physical_printers() {
        let usb = eval(&PrinterCandidate {
            printer_name: "HP LaserJet Pro M404".to_owned(),
            driver_name: "HP LaserJet Pro M404-M405 PCL-6".to_owned(),
            port_name: "USB001".to_owned(),
            ports: vec![PrinterPortMetadata {
                port_name: "USB001".to_owned(),
                monitor_name: "USB Monitor".to_owned(),
                ..PrinterPortMetadata::default()
            }],
            ..PrinterCandidate::default()
        });
        assert!(usb.allowed);

        let lan = eval(&PrinterCandidate {
            printer_name: "Office Copier 3F".to_owned(),
            driver_name: "Canon Generic Plus UFR II".to_owned(),
            port_name: "IP_192.168.1.250".to_owned(),
            ports: vec![PrinterPortMetadata {
                port_name: "IP_192.168.1.250".to_owned(),
                monitor_name: "Standard TCP/IP Port".to_owned(),
                registry_ip: "192.168.1.250".to_owned(),
                ..PrinterPortMetadata::default()
            }],
            ..PrinterCandidate::default()
        });
        assert!(lan.allowed);
    }

    #[test]
    fn blocks_virtual_pdf_xps_fax_file_keepjobs_and_rdp_redirect() {
        let pdf_printer = eval(&PrinterCandidate {
            printer_name: "Microsoft Print to PDF".to_owned(),
            driver_name: "Microsoft Print To PDF".to_owned(),
            port_name: "PORTPROMPT:".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!pdf_printer.allowed);

        let fax = eval(&PrinterCandidate {
            printer_name: "Workstation Fax".to_owned(),
            driver_name: "Generic Driver".to_owned(),
            port_name: "USB001".to_owned(),
            attributes: 0x4000,
            ..PrinterCandidate::default()
        });
        assert!(!fax.allowed);

        let keep_jobs = eval(&PrinterCandidate {
            printer_name: "HP LaserJet".to_owned(),
            driver_name: "HP PCL6".to_owned(),
            port_name: "USB001".to_owned(),
            attributes: 0x0100,
            ..PrinterCandidate::default()
        });
        assert!(!keep_jobs.allowed);

        let rdp = eval(&PrinterCandidate {
            printer_name: "Printer (redirected 2)".to_owned(),
            driver_name: "Remote Desktop Easy Print".to_owned(),
            port_name: "TS001".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!rdp.allowed);
    }

    #[test]
    fn blocks_loopback_self_ip_drive_paths_local_monitor_file_and_tainted_pools() {
        let file_port = eval(&PrinterCandidate {
            printer_name: "Canon Safe".to_owned(),
            driver_name: "Canon Driver".to_owned(),
            port_name: r"C:\Temp\out.prn".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!file_port.allowed);

        let self_ip = eval(&PrinterCandidate {
            printer_name: "Loopback TCP Printer".to_owned(),
            driver_name: "HP Driver".to_owned(),
            port_name: "CustomPort".to_owned(),
            ports: vec![PrinterPortMetadata {
                port_name: "CustomPort".to_owned(),
                monitor_name: "Standard TCP/IP Port".to_owned(),
                registry_ip: "192.168.1.100".to_owned(),
                ..PrinterPortMetadata::default()
            }],
            ..PrinterCandidate::default()
        });
        assert!(!self_ip.allowed);

        let pool_mixed = eval(&PrinterCandidate {
            printer_name: "Mixed Pool".to_owned(),
            driver_name: "HP Driver".to_owned(),
            port_name: "USB001, FILE:".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!pool_mixed.allowed);

        let loopback_unc = eval(&PrinterCandidate {
            printer_name: "Local UNC Share".to_owned(),
            driver_name: "HP Driver".to_owned(),
            port_name: r"\\127.0.0.1\SharedPrinter".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!loopback_unc.allowed);
    }

    fn candidate(port: &str, monitor: &str) -> PrinterCandidate {
        PrinterCandidate {
            printer_name: "Office Device".to_owned(),
            driver_name: "Vendor Class Driver".to_owned(),
            port_name: port.to_owned(),
            ports: vec![PrinterPortMetadata {
                port_name: port.to_owned(),
                monitor_name: monitor.to_owned(),
                ..PrinterPortMetadata::default()
            }],
            ..PrinterCandidate::default()
        }
    }

    #[test]
    fn whitelist_rejects_unknown_port_with_unknown_monitor() {
        let unknown = eval(&candidate("CustomPort7", "Acme Spool Monitor"));
        assert!(!unknown.allowed);
        assert!(unknown.reason.contains("未识别为物理或标准网络端口"));

        // No port metadata at all (monitor unknown) and a non-physical port name.
        let bare = eval(&PrinterCandidate {
            printer_name: "Mystery Device".to_owned(),
            driver_name: "Generic Driver".to_owned(),
            port_name: "SPOOLX".to_owned(),
            ..PrinterCandidate::default()
        });
        assert!(!bare.allowed);
    }

    #[test]
    fn whitelist_accepts_each_recognised_physical_port_family() {
        for (port, monitor) in [
            ("USB001", "USB Monitor"),
            ("USB002", "Dynamic Print Monitor"),
            ("WSD-5f3a2c1b-0001", "WSD Port Monitor"),
            ("IP_192.168.1.250", "Standard TCP/IP Port"),
            ("IPP-office", "Microsoft IPP Port Monitor"),
            ("DOT4_001", "DOT4 Port Monitor"),
            ("CNBJNP_0123456789AB", "Canon BJNP Port"),
            ("CNMFNP_0123456789AB", "Canon MF Network Port Monitor"),
            ("192.168.1.77", ""),
            ("LPT1:", "Local Monitor"),
            ("COM3", ""),
            ("PrinterA", "EpsonNet Print Port"),
            (r"\\printsrv\LaserJet4F", ""),
        ] {
            let verdict = eval(&candidate(port, monitor));
            assert!(verdict.allowed, "{port} / {monitor}: {}", verdict.reason);
        }
    }

    #[test]
    fn unc_ports_must_be_exactly_server_share_and_not_device_namespaces() {
        for port in [
            r"\\srv\share\out.prn",
            r"\\.\pipe\x",
            r"\\?\C:\Temp\out.prn",
            r"\\?\GLOBALROOT\Device\Null",
            r"\\srv",
            r"\\srv\",
            r"\\\share",
        ] {
            let verdict = eval(&candidate(port, ""));
            assert!(!verdict.allowed, "{port} must be rejected");
        }
        // Shares that resolve to this machine by IP, NetBIOS name or FQDN are loopbacks.
        for port in [
            r"\\192.168.1.100\Shared",
            r"\\WORKSTATION-A\Shared",
            r"\\workstation-a.corp.example\Shared",
            r"\\localhost\Shared",
        ] {
            let verdict = eval(&candidate(port, ""));
            assert!(!verdict.allowed, "{port} must be rejected");
        }
    }

    #[test]
    fn ip_prefixed_port_names_pointing_at_this_machine_are_rejected() {
        for port in [
            "IP_192.168.1.100",
            "ip_10.0.0.5",
            "IP_127.0.0.1",
            "10.0.0.5",
        ] {
            let verdict = eval(&candidate(port, "Standard TCP/IP Port"));
            assert!(!verdict.allowed, "{port} must be rejected");
            assert!(verdict.reason.contains("本机"), "{}", verdict.reason);
        }
        assert!(eval(&candidate("IP_192.168.1.101", "Standard TCP/IP Port")).allowed);
    }

    #[test]
    fn local_monitor_ports_disguised_as_physical_names_are_rejected() {
        for port in ["USB999", "IP_1.2.3.4", "WSD-fake", "MyOutput"] {
            let verdict = eval(&candidate(port, "Local Monitor"));
            assert!(!verdict.allowed, "{port} must be rejected");
        }
        assert!(!eval(&candidate("C:", "")).allowed);
        assert!(!eval(&candidate("LPT1234:", "")).allowed);
    }

    #[test]
    fn page_ranges_parse_sorted_unique_zero_based_indices() {
        assert_eq!(parse_page_ranges("1-3,5", 10).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(parse_page_ranges(" 3 ", 3).unwrap(), vec![2]);
        assert_eq!(
            parse_page_ranges("5；2－3，2;10", 10).unwrap(),
            vec![1, 2, 4, 9]
        );
        assert_eq!(parse_page_ranges("2~4 4—5", 5).unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn page_ranges_reject_invalid_input_with_user_facing_messages() {
        for spec in [
            "",
            "   ",
            "0",
            "11",
            "3-2",
            "0-2",
            "2-11",
            "a",
            "1-b",
            "-3",
            "3-",
            "1--2",
            "1.5",
            "+2",
            "99999999999",
            ",",
        ] {
            let err = parse_page_ranges(spec, 10).unwrap_err();
            assert!(
                matches!(err, CoreError::InvalidArgument(_)),
                "{spec:?} must be rejected"
            );
        }
        let msg = parse_page_ranges("12", 10).unwrap_err().to_string();
        assert!(msg.contains("12") && msg.contains("10"));
        assert!(parse_page_ranges("1", 0).is_err());
    }

    #[test]
    fn print_watermark_text_always_carries_time_ip_and_machine_code() {
        let with_note = build_print_watermark_text(
            " 张三专用 ",
            "2026-11-16 09:30:00",
            "192.168.1.20",
            "ABCD-EFGH-JKLM",
        );
        assert_eq!(
            with_note,
            "张三专用 | 2026-11-16 09:30:00 | IP:192.168.1.20 | ABCD-EFGH-JKLM"
        );
        let bare =
            build_print_watermark_text("  ", "2026-11-16 09:30:00", "10.0.0.8", "WXYZ-2345-6789");
        assert_eq!(bare, "2026-11-16 09:30:00 | IP:10.0.0.8 | WXYZ-2345-6789");
    }

    #[test]
    fn print_request_validation_rejects_bad_pages_and_blank_watermark_before_quota() {
        let policy = SpdfPolicy {
            doc_id: crate::format::DocId::from_wire_bytes([7; 16]),
            max_prints: 1,
            expires_at_utc_ticks: None,
            flags: crate::format::SecurityFlags::from_bits(0),
            moire_level: crate::format::MoireLevel::Off,
            original_file_name: "a.pdf".to_owned(),
            watermark_note: String::new(),
        };
        let hash = [0_u8; 32];
        let ctx = |pages: &'static [i32], text: &'static str| PrintJobContext {
            policy: &policy,
            printer_name: "X",
            machine_hash: &hash,
            page_indices: pages,
            watermark_text: text,
        };
        assert!(validate_print_request(&ctx(&[0, 2], "t"), 3).is_ok());
        assert!(validate_print_request(&ctx(&[], "t"), 3).is_err());
        assert!(validate_print_request(&ctx(&[3], "t"), 3).is_err());
        assert!(validate_print_request(&ctx(&[-1], "t"), 3).is_err());
        assert!(validate_print_request(&ctx(&[0], "   "), 3).is_err());
    }

    #[test]
    fn print_failure_text_states_whether_the_print_count_was_deducted() {
        assert!(describe_print_failure("x", 0, false).contains("未扣减"));
        let partial = describe_print_failure("x", 2, true);
        assert!(partial.contains("已有 2 页") && partial.contains("已扣减"));
        assert!(describe_print_failure("x", 0, true).contains("已扣减"));
    }

    #[test]
    fn offline_printers_are_flagged_from_attribute_or_status_bits() {
        let mut cand = candidate("USB001", "USB Monitor");
        assert!(!eval(&cand).is_offline);
        cand.attributes = 0x0000_0400;
        assert!(eval(&cand).is_offline);
        cand.attributes = 0;
        cand.status = 0x0000_0080;
        let verdict = eval(&cand);
        assert!(verdict.is_offline);
        assert!(
            verdict.allowed,
            "offline only labels the printer, it does not block it"
        );
    }

    #[test]
    fn compute_print_geometry_caps_300dpi_and_36m_pixels_and_keeps_aspect() {
        // 600 DPI 8.5x11 page -> 5100x6600 dev px -> capped to 300 DPI (2550x3300 = 8.4M px)
        let (rw, rh, dx, dy, dw, dh) = compute_print_geometry(612.0, 792.0, 5100, 6600, 600, 600);
        assert_eq!(dw, 5100);
        assert_eq!(dh, 6600);
        assert_eq!(dx, 0);
        assert_eq!(dy, 0);
        assert!((rw - 2550).abs() <= 2);
        assert!((rh - 3300).abs() <= 2);

        // Huge poster 300 DPI 10000x10000 = 100M px -> capped <= 36M px
        let (pw, ph, _, _, _, _) = compute_print_geometry(1000.0, 1000.0, 10_000, 10_000, 300, 300);
        assert!(i64::from(pw) * i64::from(ph) <= 36_000_000);
    }
}
