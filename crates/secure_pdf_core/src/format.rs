//! `.spdf` v2 encrypted PDF container reader, writer, and policy serialization.

use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zeroize::Zeroizing;

use crate::crypto::{
    DEFAULT_SCRYPT_N, DEFAULT_SCRYPT_P, DEFAULT_SCRYPT_R, GCM_NONCE_SIZE, GCM_TAG_SIZE, KEY_SIZE,
    MIN_UNLOCK_SCRYPT_N, SALT_SIZE, aes_256_gcm_decrypt, aes_256_gcm_encrypt, derive_key_scrypt,
    is_acceptable_scrypt_cost, random_bytes,
};
use crate::error::{CoreError, CoreResult};

/// Magic 4-byte prefix identifying `SecurePdf` containers.
pub const SPDF_MAGIC: [u8; 4] = *b"SPDF";
/// Current `.spdf` container version.
pub const SPDF_VERSION: u16 = 2;
/// Fixed authenticated header length (bytes 0..50).
pub const AAD_HEADER_LEN: usize = 50;
/// Maximum allowed embedded PDF payload size (1 GiB).
pub const MAX_PDF_BYTES: usize = 1024 * 1024 * 1024;
/// Maximum serialized policy ciphertext size (1 MiB).
pub const MAX_POLICY_CIPHERTEXT_BYTES: usize = 1024 * 1024;

const POLICY_SCHEMA_VERSION: u8 = 1;
const MIN_CIPHERTEXT_BYTES: usize = GCM_TAG_SIZE + 1;
const MAX_POLICY_STRING_BYTES: usize = 64 * 1024;

/// Number of 100-nanosecond ticks per second in .NET `DateTime`.
pub const TICKS_PER_SECOND: i64 = 10_000_000;
/// Difference between .NET epoch (`0001-01-01T00:00:00Z`) and Unix epoch (`1970-01-01T00:00:00Z`).
pub const DOTNET_UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;

/// 16-byte document identifier compatible with .NET `System.Guid` wire endianness.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocId([u8; 16]);

impl DocId {
    /// Generates a cryptographically random RFC 4122 v4 identifier in .NET `Guid` byte order.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Crypto`] if system RNG fails.
    pub fn new_random() -> CoreResult<Self> {
        let mut raw = [0_u8; 16];
        random_bytes(&mut raw)?;
        // In .NET `Guid`, Data3 high byte is index 7 and Data4 variant byte is index 8.
        raw[7] = (raw[7] & 0x0F) | 0x40;
        raw[8] = (raw[8] & 0x3F) | 0x80;
        Ok(Self(raw))
    }

    /// Constructs a `DocId` from raw 16-byte `.spdf` / .NET `Guid` wire representation.
    #[must_use]
    pub const fn from_wire_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 16-byte `.spdf` / .NET `Guid` wire representation.
    #[must_use]
    pub const fn as_wire_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Formats the identifier matching .NET `Guid.ToString("N")` (32 lowercase hex chars).
    #[must_use]
    pub fn to_hex_n(&self) -> String {
        let b = &self.0;
        // .NET mixed-endian layout: Data1 LE [0..4], Data2 LE [4..6], Data3 LE [6..8], Data4 BE [8..16].
        let ordered = [
            b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9], b[10], b[11], b[12], b[13],
            b[14], b[15],
        ];
        let mut out = String::with_capacity(32);
        for byte in ordered {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
        out
    }
}

impl fmt::Debug for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocId({})", self.to_hex_n())
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex_n())
    }
}

/// Anti-camera moire lattice intensity stored in `.spdf` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MoireLevel {
    /// Disabled.
    Off = 0,
    /// Subtle grid pattern.
    Low = 1,
    /// Balanced grid & microdot lattice.
    #[default]
    Medium = 2,
    /// High-contrast anti-camera pattern.
    High = 3,
}

impl TryFrom<u8> for MoireLevel {
    type Error = CoreError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Off),
            1 => Ok(Self::Low),
            2 => Ok(Self::Medium),
            3 => Ok(Self::High),
            other => Err(CoreError::Format(format!("未知的抗屏摄纹理等级：{other}"))),
        }
    }
}

/// Bitflags governing reader runtime security enforcement (`SpdfSecurityFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SecurityFlags(u32);

impl SecurityFlags {
    /// Embed hidden blue-channel forensic trace watermark.
    pub const HIDDEN_WATERMARK: u32 = 1 << 0;
    /// Cover content with a privacy curtain when window loses focus.
    pub const BLUR_ON_UNFOCUS: u32 = 1 << 1;
    /// Allow reader user to cycle moire lattice intensity.
    pub const ALLOW_USER_ADJUST_MOIRE: u32 = 1 << 2;
    /// Render visible forensic watermark overlay across pages and prints.
    pub const VISIBLE_WATERMARK: u32 = 1 << 3;

    /// Constructs flags from raw bitmask.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns raw 32-bit flag representation.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns `true` if all bits in `flag` are set.
    #[must_use]
    pub const fn contains(self, flag: u32) -> bool {
        (self.0 & flag) == flag
    }
}

/// Authenticated document policy decrypted from `.spdf` wrapper block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpdfPolicy {
    /// Unique document GUID.
    pub doc_id: DocId,
    /// Maximum allowed physical prints (`0` disallows printing).
    pub max_prints: i32,
    /// Optional expiration instant in UTC .NET 100ns ticks (`None` = perpetual).
    pub expires_at_utc_ticks: Option<i64>,
    /// Default anti-camera moire level.
    pub moire_level: MoireLevel,
    /// Security feature bitflags.
    pub flags: SecurityFlags,
    /// Original unencrypted PDF filename shown in reader title/print job.
    pub original_file_name: String,
    /// Custom watermark annotation supplied by document creator.
    pub watermark_note: String,
}

impl SpdfPolicy {
    /// Returns `true` if the policy has an expiration timestamp that is at or before `now_ticks`.
    #[must_use]
    pub const fn is_expired_at(&self, now_ticks: i64) -> bool {
        match self.expires_at_utc_ticks {
            Some(exp) => now_ticks >= exp,
            None => false,
        }
    }
}

/// Unencrypted metadata parsed from `.spdf` header (available prior to verification code entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdfContainerInfo {
    /// Container format version (`2`).
    pub version: u16,
    /// Document identifier.
    pub doc_id: DocId,
    /// `scrypt` work factor `N`.
    pub scrypt_n: i32,
    /// `scrypt` block parameter `r`.
    pub scrypt_r: i32,
    /// `scrypt` parallelization parameter `p`.
    pub scrypt_p: i32,
}

/// Options used when packaging a PDF into a `.spdf` container.
#[derive(Debug, Clone)]
pub struct CreateSpdfOptions<'a> {
    /// Raw plaintext verification code (normalized before scrypt derivation).
    pub verification_code: &'a str,
    /// Maximum print count (`0` forbids printing).
    pub max_prints: i32,
    /// Optional expiration time in .NET UTC ticks.
    pub expires_at_utc_ticks: Option<i64>,
    /// Anti-camera moire level.
    pub moire_level: MoireLevel,
    /// Runtime security policy flags.
    pub flags: SecurityFlags,
    /// Original PDF filename.
    pub original_file_name: &'a str,
    /// Optional creator watermark note.
    pub watermark_note: &'a str,
    /// Optional document identifier (`None` generates random v4 GUID).
    pub doc_id: Option<DocId>,
    /// Scrypt `N` (`None` uses [`DEFAULT_SCRYPT_N`]).
    pub scrypt_n: Option<i32>,
    /// Scrypt `r` (`None` uses [`DEFAULT_SCRYPT_R`]).
    pub scrypt_r: Option<i32>,
    /// Scrypt `p` (`None` uses [`DEFAULT_SCRYPT_P`]).
    pub scrypt_p: Option<i32>,
}

/// Fully decrypted `.spdf` document held strictly in memory.
pub struct DecryptedSpdf {
    /// Authenticated policy extracted from container.
    pub policy: SpdfPolicy,
    /// Zeroized raw PDF bytes (`%PDF-...`).
    pub pdf_bytes: Zeroizing<Vec<u8>>,
}

/// Converts a `SystemTime` to .NET `DateTime` UTC ticks (100ns intervals since 0001-01-01).
#[must_use]
pub fn system_time_to_dotnet_ticks(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => {
            let secs = i64::try_from(d.as_secs()).unwrap_or(i64::MAX / TICKS_PER_SECOND);
            let sub_ticks = i64::from(d.subsec_nanos() / 100);
            DOTNET_UNIX_EPOCH_TICKS
                .saturating_add(secs.saturating_mul(TICKS_PER_SECOND))
                .saturating_add(sub_ticks)
        }
        Err(err) => {
            let d = err.duration();
            let secs = i64::try_from(d.as_secs()).unwrap_or(0);
            let sub_ticks = i64::from(d.subsec_nanos() / 100);
            DOTNET_UNIX_EPOCH_TICKS
                .saturating_sub(secs.saturating_mul(TICKS_PER_SECOND))
                .saturating_sub(sub_ticks)
        }
    }
}

/// Returns the current system UTC time in .NET 100ns ticks.
#[must_use]
pub fn utc_now_dotnet_ticks() -> i64 {
    system_time_to_dotnet_ticks(SystemTime::now())
}

/// Converts positive .NET UTC ticks (`> DOTNET_UNIX_EPOCH_TICKS`) to `SystemTime`, if representable.
#[must_use]
pub fn dotnet_ticks_to_system_time(ticks: i64) -> Option<SystemTime> {
    if ticks < DOTNET_UNIX_EPOCH_TICKS {
        return None;
    }
    let since_unix = ticks - DOTNET_UNIX_EPOCH_TICKS;
    let secs = u64::try_from(since_unix / TICKS_PER_SECOND).ok()?;
    let rem_ticks = u32::try_from(since_unix % TICKS_PER_SECOND).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(secs, rem_ticks * 100))
}

/// Formats .NET UTC ticks as a UTC timestamp (`YYYY-MM-DD HH:MM:SS UTC`).
///
/// For user-facing local time use [`crate::localtime::format_dotnet_ticks_local`].
#[must_use]
pub fn format_dotnet_ticks_utc(ticks: i64) -> String {
    let Some(since_unix) = ticks.checked_sub(DOTNET_UNIX_EPOCH_TICKS) else {
        return format!("ticks:{ticks}");
    };
    let secs = since_unix.div_euclid(TICKS_PER_SECOND);
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400);
    let hour = day_secs / 3_600;
    let min = (day_secs % 3_600) / 60;
    let sec = day_secs % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02} UTC")
}

/// Howard Hinnant's Euclidean civil-date conversion from days since `1970-01-01`.
#[allow(clippy::cast_possible_truncation)]
const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}

/// Checks whether `bytes` contains `%PDF-` within the first 1024 bytes.
#[must_use]
pub fn looks_like_pdf(bytes: &[u8]) -> bool {
    let scan_len = bytes.len().min(1024);
    bytes[..scan_len].windows(5).any(|w| w == b"%PDF-")
}

/// Writes an encrypted `.spdf` v2 container to `output_path` atomically using a `.tmp` sibling file.
///
/// # Errors
///
/// Returns an error for invalid options, oversized/non-PDF inputs, cryptographic failure, or I/O errors.
pub fn write_spdf_file(
    pdf_bytes: &[u8],
    output_path: &Path,
    opts: &CreateSpdfOptions<'_>,
) -> CoreResult<DocId> {
    if pdf_bytes.is_empty() || pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(CoreError::InvalidArgument(
            "PDF 文件为空或超出 1 GiB 上限。".to_owned(),
        ));
    }
    if !looks_like_pdf(pdf_bytes) {
        return Err(CoreError::InvalidArgument(
            "输入文件缺少合法的 %PDF- 文件头标识。".to_owned(),
        ));
    }
    if opts.max_prints < 0 {
        return Err(CoreError::InvalidArgument(
            "允许打印次数不能为负数。".to_owned(),
        ));
    }
    if let Some(ticks) = opts.expires_at_utc_ticks
        && ticks <= 0
    {
        return Err(CoreError::InvalidArgument(
            "文档到期时间戳必须大于 0。".to_owned(),
        ));
    }

    let n = opts.scrypt_n.unwrap_or(DEFAULT_SCRYPT_N);
    let r = opts.scrypt_r.unwrap_or(DEFAULT_SCRYPT_R);
    let p = opts.scrypt_p.unwrap_or(DEFAULT_SCRYPT_P);
    if !is_acceptable_scrypt_cost(n, r, p) {
        return Err(CoreError::InvalidArgument(
            "scrypt 密钥派生参数超出安全范围。".to_owned(),
        ));
    }

    let doc_id = match opts.doc_id {
        Some(id) => id,
        None => DocId::new_random()?,
    };
    let mut salt = [0_u8; SALT_SIZE];
    random_bytes(&mut salt)?;
    let mut wrap_nonce = [0_u8; GCM_NONCE_SIZE];
    random_bytes(&mut wrap_nonce)?;
    let mut payload_nonce = [0_u8; GCM_NONCE_SIZE];
    random_bytes(&mut payload_nonce)?;
    let mut doc_key = Zeroizing::new([0_u8; KEY_SIZE]);
    random_bytes(&mut *doc_key)?;

    let mut aad_header = [0_u8; AAD_HEADER_LEN];
    aad_header[0..4].copy_from_slice(&SPDF_MAGIC);
    aad_header[4..6].copy_from_slice(&SPDF_VERSION.to_le_bytes());
    aad_header[6..22].copy_from_slice(doc_id.as_wire_bytes());
    aad_header[22..38].copy_from_slice(&salt);
    aad_header[38..42].copy_from_slice(&n.to_le_bytes());
    aad_header[42..46].copy_from_slice(&r.to_le_bytes());
    aad_header[46..50].copy_from_slice(&p.to_le_bytes());

    let policy_plain = serialize_policy(&doc_key, opts);
    let kek_vec = derive_key_scrypt(opts.verification_code, &salt, n, r, p, KEY_SIZE)?;
    let mut kek = Zeroizing::new([0_u8; KEY_SIZE]);
    kek.copy_from_slice(&kek_vec);

    let wrapped_policy = aes_256_gcm_encrypt(&kek, &wrap_nonce, &policy_plain, &aad_header)?;
    let wrapped_len = i32::try_from(wrapped_policy.len())
        .map_err(|_| CoreError::Format("策略密文长度超出 i32 范围。".to_owned()))?;

    let payload_cipher = aes_256_gcm_encrypt(&doc_key, &payload_nonce, pdf_bytes, &aad_header)?;
    let payload_len = i64::try_from(payload_cipher.len())
        .map_err(|_| CoreError::Format("正文密文长度溢出。".to_owned()))?;

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let tmp_name = format!(
        "{}.{}.tmp",
        output_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output.spdf"),
        std::process::id()
    );
    let tmp_path = output_path.with_file_name(tmp_name);

    let write_res: CoreResult<()> = (|| {
        let file = File::create(&tmp_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&aad_header)?;
        writer.write_all(&wrap_nonce)?;
        writer.write_all(&wrapped_len.to_le_bytes())?;
        writer.write_all(&wrapped_policy)?;
        writer.write_all(&payload_nonce)?;
        writer.write_all(&payload_len.to_le_bytes())?;
        writer.write_all(&payload_cipher)?;
        writer.flush()?;
        let inner = writer
            .into_inner()
            .map_err(|e| CoreError::Io(e.into_error()))?;
        inner.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    if let Err(err) = replace_file_atomically(&tmp_path, output_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    Ok(doc_id)
}

fn replace_file_atomically(src: &Path, dst: &Path) -> CoreResult<()> {
    // `fs::rename` replaces an existing destination in one step on Windows
    // (`MOVEFILE_REPLACE_EXISTING`). Deleting it first would open a window in which another
    // process could claim the name a caller reserved with a placeholder file.
    fs::rename(src, dst)?;
    Ok(())
}

fn serialize_policy(doc_key: &[u8; KEY_SIZE], opts: &CreateSpdfOptions<'_>) -> Zeroizing<Vec<u8>> {
    let name_bytes = opts.original_file_name.as_bytes();
    let note_bytes = opts.watermark_note.as_bytes();
    let cap = 1 + KEY_SIZE + 4 + 8 + 1 + 4 + 10 + name_bytes.len() + note_bytes.len();
    let mut out = Zeroizing::new(Vec::with_capacity(cap));
    out.push(POLICY_SCHEMA_VERSION);
    out.extend_from_slice(doc_key);
    out.extend_from_slice(&opts.max_prints.to_le_bytes());
    out.extend_from_slice(&opts.expires_at_utc_ticks.unwrap_or(0).to_le_bytes());
    out.push(opts.moire_level as u8);
    out.extend_from_slice(&opts.flags.bits().to_le_bytes());
    write_leb128_str(&mut out, opts.original_file_name);
    write_leb128_str(&mut out, opts.watermark_note);
    out
}

fn write_leb128_str(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let mut len = bytes.len();
    loop {
        let low7 = u8::try_from(len & 0x7F).unwrap_or(0);
        len >>= 7;
        if len == 0 {
            buf.push(low7);
            break;
        }
        buf.push(low7 | 0x80);
    }
    buf.extend_from_slice(bytes);
}

/// Reads the unencrypted header metadata from a `.spdf` container without decrypting.
///
/// # Errors
///
/// Returns [`CoreError::Format`] if magic, version, or scrypt parameters are invalid, or
/// [`CoreError::Io`] on filesystem failure.
pub fn read_spdf_info(spdf_path: &Path) -> CoreResult<SpdfContainerInfo> {
    let mut file = File::open(spdf_path)?;
    let (info, _, _) = read_aad_header(&mut file)?;
    Ok(info)
}

fn read_aad_header(
    file: &mut File,
) -> CoreResult<(SpdfContainerInfo, [u8; AAD_HEADER_LEN], [u8; SALT_SIZE])> {
    let mut aad = [0_u8; AAD_HEADER_LEN];
    file.read_exact(&mut aad).map_err(|_| {
        CoreError::Format("文件长度不足 50 字节，不是合法的 .spdf 容器。".to_owned())
    })?;

    if aad[0..4] != SPDF_MAGIC {
        return Err(CoreError::Format(
            "文件头不是 SPDF，无法识别此受保护文档。".to_owned(),
        ));
    }
    let version = u16::from_le_bytes([aad[4], aad[5]]);
    if version != SPDF_VERSION {
        return Err(CoreError::Format(format!(
            "不支持的 .spdf 容器版本 v{version}（当前仅支持 v{SPDF_VERSION}）。"
        )));
    }
    let mut doc_id_bytes = [0_u8; 16];
    doc_id_bytes.copy_from_slice(&aad[6..22]);
    let mut salt = [0_u8; SALT_SIZE];
    salt.copy_from_slice(&aad[22..38]);
    let n = i32::from_le_bytes([aad[38], aad[39], aad[40], aad[41]]);
    let r = i32::from_le_bytes([aad[42], aad[43], aad[44], aad[45]]);
    let p = i32::from_le_bytes([aad[46], aad[47], aad[48], aad[49]]);

    if !is_acceptable_scrypt_cost(n, r, p) || n < MIN_UNLOCK_SCRYPT_N {
        return Err(CoreError::Format(
            "容器中的 scrypt 密钥派生参数超出安全策略允许范围。".to_owned(),
        ));
    }

    Ok((
        SpdfContainerInfo {
            version,
            doc_id: DocId::from_wire_bytes(doc_id_bytes),
            scrypt_n: n,
            scrypt_r: r,
            scrypt_p: p,
        },
        aad,
        salt,
    ))
}

/// Opens and decrypts a `.spdf` v2 file in memory using `verification_code`.
///
/// # Errors
///
/// - [`CoreError::InvalidArgument`] if `verification_code` is empty after normalization
/// - [`CoreError::WrongVerificationCode`] if the verification code fails key-wrap AEAD verification
/// - [`CoreError::Format`] if the header, policy, payload length, payload AEAD, or `%PDF-` signature is invalid
pub fn open_spdf_file(spdf_path: &Path, verification_code: &str) -> CoreResult<DecryptedSpdf> {
    if verification_code.trim().is_empty() {
        return Err(CoreError::InvalidArgument("请输入验证码。".to_owned()));
    }
    let mut file = File::open(spdf_path)?;
    let total_len = file.metadata()?.len();
    let (info, aad_header, salt) = read_aad_header(&mut file)?;

    let mut wrap_nonce = [0_u8; GCM_NONCE_SIZE];
    file.read_exact(&mut wrap_nonce)
        .map_err(|_| CoreError::Format("容器读取策略 nonce 时意外截断。".to_owned()))?;

    let mut wrapped_len_bytes = [0_u8; 4];
    file.read_exact(&mut wrapped_len_bytes)
        .map_err(|_| CoreError::Format("容器读取策略长度时意外截断。".to_owned()))?;
    let wrapped_len_i32 = i32::from_le_bytes(wrapped_len_bytes);
    let wrapped_len = usize::try_from(wrapped_len_i32)
        .map_err(|_| CoreError::Format(format!("非法的策略密文长度：{wrapped_len_i32}")))?;
    if !(MIN_CIPHERTEXT_BYTES..=MAX_POLICY_CIPHERTEXT_BYTES).contains(&wrapped_len) {
        return Err(CoreError::Format(format!(
            "非法的策略密文长度：{wrapped_len}"
        )));
    }

    let current_pos = file.stream_position()?;
    let remaining_after_wrap_len = total_len.saturating_sub(current_pos);
    if u64::try_from(wrapped_len).unwrap_or(u64::MAX) > remaining_after_wrap_len {
        return Err(CoreError::Format(
            "策略密文长度超出文件剩余字节数。".to_owned(),
        ));
    }

    let mut wrapped_policy = vec![0_u8; wrapped_len];
    file.read_exact(&mut wrapped_policy)
        .map_err(|_| CoreError::Format("策略密文读取截断。".to_owned()))?;

    let mut payload_nonce = [0_u8; GCM_NONCE_SIZE];
    file.read_exact(&mut payload_nonce)
        .map_err(|_| CoreError::Format("容器读取正文 nonce 时意外截断。".to_owned()))?;

    let mut payload_len_bytes = [0_u8; 8];
    file.read_exact(&mut payload_len_bytes)
        .map_err(|_| CoreError::Format("容器读取正文长度时意外截断。".to_owned()))?;
    let payload_len_i64 = i64::from_le_bytes(payload_len_bytes);
    let max_cipher_len = i64::try_from(MAX_PDF_BYTES + GCM_TAG_SIZE).unwrap_or(i64::MAX);
    if payload_len_i64 < i64::try_from(MIN_CIPHERTEXT_BYTES).unwrap_or(17)
        || payload_len_i64 > max_cipher_len
    {
        return Err(CoreError::Format(format!(
            "非法的正文密文长度：{payload_len_i64}"
        )));
    }
    let payload_len = usize::try_from(payload_len_i64)
        .map_err(|_| CoreError::Format("正文密文长度超出内存可寻址范围。".to_owned()))?;

    let payload_pos = file.stream_position()?;
    let remaining_bytes = total_len.saturating_sub(payload_pos);
    if u64::try_from(payload_len).unwrap_or(u64::MAX) != remaining_bytes {
        return Err(CoreError::Format(format!(
            "容器正文密文长度（{payload_len}）与文件实际剩余字节（{remaining_bytes}）不匹配。"
        )));
    }

    let kek_vec = derive_key_scrypt(
        verification_code,
        &salt,
        info.scrypt_n,
        info.scrypt_r,
        info.scrypt_p,
        KEY_SIZE,
    )?;
    let mut kek = Zeroizing::new([0_u8; KEY_SIZE]);
    kek.copy_from_slice(&kek_vec);

    let policy_plain = aes_256_gcm_decrypt(&kek, &wrap_nonce, &wrapped_policy, &aad_header)?;
    let (doc_key, policy) = deserialize_policy(&policy_plain, info.doc_id)?;

    let mut payload_cipher = vec![0_u8; payload_len];
    file.read_exact(&mut payload_cipher)
        .map_err(|_| CoreError::Format("容器正文读取截断。".to_owned()))?;

    let pdf_bytes =
        match aes_256_gcm_decrypt(&doc_key, &payload_nonce, &payload_cipher, &aad_header) {
            Ok(bytes) => bytes,
            Err(CoreError::WrongVerificationCode) => {
                return Err(CoreError::Format(
                    "文档正文完整性校验失败，容器文件可能已损坏。".to_owned(),
                ));
            }
            Err(other) => return Err(other),
        };

    if !looks_like_pdf(&pdf_bytes) {
        return Err(CoreError::Format(
            "解密后的正文缺少 %PDF- 文件头标识。".to_owned(),
        ));
    }

    Ok(DecryptedSpdf { policy, pdf_bytes })
}

fn deserialize_policy(
    plain: &[u8],
    doc_id: DocId,
) -> CoreResult<(Zeroizing<[u8; KEY_SIZE]>, SpdfPolicy)> {
    // Minimum size: 1 (schema) + 32 (doc_key) + 4 (max_prints) + 8 (ticks) + 1 (moire) + 4 (flags) + 2 (empty strings) = 52
    if plain.len() < 52 {
        return Err(CoreError::Format("策略明文长度异常过短。".to_owned()));
    }
    let schema_ver = plain[0];
    if schema_ver != POLICY_SCHEMA_VERSION {
        return Err(CoreError::Format(format!(
            "不支持的策略块版本：{schema_ver}"
        )));
    }

    let mut doc_key = Zeroizing::new([0_u8; KEY_SIZE]);
    doc_key.copy_from_slice(&plain[1..33]);
    let max_prints = i32::from_le_bytes([plain[33], plain[34], plain[35], plain[36]]);
    if max_prints < 0 {
        return Err(CoreError::Format(format!(
            "策略中允许打印次数非法：{max_prints}"
        )));
    }
    let expires_raw = i64::from_le_bytes([
        plain[37], plain[38], plain[39], plain[40], plain[41], plain[42], plain[43], plain[44],
    ]);
    let expires_at_utc_ticks = match expires_raw.cmp(&0) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(expires_raw),
        std::cmp::Ordering::Less => {
            return Err(CoreError::Format(format!(
                "策略中过期时间戳非法：{expires_raw}"
            )));
        }
    };
    let moire_level = MoireLevel::try_from(plain[45])?;
    let flags = SecurityFlags::from_bits(u32::from_le_bytes([
        plain[46], plain[47], plain[48], plain[49],
    ]));

    let mut cursor = 50_usize;
    let original_file_name = read_leb128_str(plain, &mut cursor)?;
    let watermark_note = read_leb128_str(plain, &mut cursor)?;
    if cursor != plain.len() {
        return Err(CoreError::Format(
            "策略明文尾部包含多余未解析数据。".to_owned(),
        ));
    }

    Ok((
        doc_key,
        SpdfPolicy {
            doc_id,
            max_prints,
            expires_at_utc_ticks,
            moire_level,
            flags,
            original_file_name,
            watermark_note,
        },
    ))
}

fn read_leb128_str(buf: &[u8], cursor: &mut usize) -> CoreResult<String> {
    let mut byte_len = 0_usize;
    let mut shift = 0_u32;
    loop {
        let byte = *buf
            .get(*cursor)
            .ok_or_else(|| CoreError::Format("读取策略字符串长度前缀时截断。".to_owned()))?;
        *cursor += 1;
        let low = usize::from(byte & 0x7F);
        byte_len |= low
            .checked_shl(shift)
            .ok_or_else(|| CoreError::Format("策略字符串长度 varint 溢出。".to_owned()))?;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
        if shift >= 28 {
            return Err(CoreError::Format("策略字符串长度 varint 过长。".to_owned()));
        }
    }
    if byte_len > MAX_POLICY_STRING_BYTES {
        return Err(CoreError::Format("策略字符串长度超出安全限制。".to_owned()));
    }
    let end = cursor
        .checked_add(byte_len)
        .filter(|&end| end <= buf.len())
        .ok_or_else(|| CoreError::Format("策略字符串内容截断。".to_owned()))?;
    let s = std::str::from_utf8(&buf[*cursor..end])
        .map_err(|_| CoreError::Format("策略字符串包含非法 UTF-8 编码。".to_owned()))?;
    *cursor = end;
    Ok(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_pdf() -> Vec<u8> {
        b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n".to_vec()
    }

    #[test]
    fn doc_id_dotnet_guid_hex_n_ordering_matches_microsoft_wire_layout() {
        // C# Guid("11223344-5566-7788-99aa-bbccddeeff00").ToByteArray()
        let wire = [
            0x44, 0x33, 0x22, 0x11, // Data1 LE
            0x66, 0x55, // Data2 LE
            0x88, 0x77, // Data3 LE
            0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, // Data4 BE
        ];
        let id = DocId::from_wire_bytes(wire);
        assert_eq!(id.to_hex_n(), "112233445566778899aabbccddeeff00");
    }

    #[test]
    fn dotnet_ticks_roundtrip_and_formatting() {
        let ticks = DOTNET_UNIX_EPOCH_TICKS + 1_700_000_000 * TICKS_PER_SECOND;
        let st = dotnet_ticks_to_system_time(ticks).unwrap();
        assert_eq!(system_time_to_dotnet_ticks(st), ticks);
        assert_eq!(format_dotnet_ticks_utc(ticks), "2023-11-14 22:13:20 UTC");
    }

    #[test]
    fn spdf_create_read_info_and_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let spdf_path = dir.path().join("sample.spdf");
        let pdf = minimal_pdf();
        let exp = utc_now_dotnet_ticks() + 3_600 * TICKS_PER_SECOND;

        let doc_id = write_spdf_file(
            &pdf,
            &spdf_path,
            &CreateSpdfOptions {
                verification_code: "AB23-CD45-EF67",
                max_prints: 5,
                expires_at_utc_ticks: Some(exp),
                moire_level: MoireLevel::High,
                flags: SecurityFlags::from_bits(
                    SecurityFlags::HIDDEN_WATERMARK
                        | SecurityFlags::BLUR_ON_UNFOCUS
                        | SecurityFlags::VISIBLE_WATERMARK,
                ),
                original_file_name: "合同文本.pdf",
                watermark_note: "仅限内审",
                doc_id: None,
                scrypt_n: Some(1_024),
                scrypt_r: Some(8),
                scrypt_p: Some(1),
            },
        )
        .unwrap();

        let info = read_spdf_info(&spdf_path).unwrap();
        assert_eq!(info.version, 2);
        assert_eq!(info.doc_id, doc_id);
        assert_eq!(info.scrypt_n, 1_024);

        // Open with fullwidth lower-case dashes normalized form
        let opened = open_spdf_file(&spdf_path, " ａｂ２３—ｃｄ４５＿ｅｆ６７ ").unwrap();
        assert_eq!(opened.policy.doc_id, doc_id);
        assert_eq!(opened.policy.max_prints, 5);
        assert_eq!(opened.policy.expires_at_utc_ticks, Some(exp));
        assert_eq!(opened.policy.moire_level, MoireLevel::High);
        assert!(
            opened
                .policy
                .flags
                .contains(SecurityFlags::VISIBLE_WATERMARK)
        );
        assert_eq!(opened.policy.original_file_name, "合同文本.pdf");
        assert_eq!(opened.policy.watermark_note, "仅限内审");
        assert_eq!(opened.pdf_bytes.as_slice(), pdf.as_slice());

        // Wrong code fails cleanly with WrongVerificationCode
        assert!(matches!(
            open_spdf_file(&spdf_path, "XXXX-YYYY-ZZZZ"),
            Err(CoreError::WrongVerificationCode)
        ));
    }

    #[test]
    fn spdf_open_rejects_tampered_payload_header_and_trailing_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let spdf_path = dir.path().join("tamper.spdf");
        write_spdf_file(
            &minimal_pdf(),
            &spdf_path,
            &CreateSpdfOptions {
                verification_code: "TEST-1234",
                max_prints: 1,
                expires_at_utc_ticks: None,
                moire_level: MoireLevel::Off,
                flags: SecurityFlags::default(),
                original_file_name: "doc.pdf",
                watermark_note: "",
                doc_id: None,
                scrypt_n: Some(1_024),
                scrypt_r: Some(8),
                scrypt_p: Some(1),
            },
        )
        .unwrap();

        // Append 1 trailing byte -> payloadLen vs remaining_bytes mismatch
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&spdf_path)
            .unwrap();
        f.write_all(&[0xAB]).unwrap();
        drop(f);
        assert!(matches!(
            open_spdf_file(&spdf_path, "TEST-1234"),
            Err(CoreError::Format(_))
        ));

        // Corrupt magic header
        let mut raw = fs::read(&spdf_path).unwrap();
        raw.pop(); // remove trailing byte
        raw[0] = b'X';
        fs::write(&spdf_path, &raw).unwrap();
        assert!(matches!(
            open_spdf_file(&spdf_path, "TEST-1234"),
            Err(CoreError::Format(_))
        ));
    }

    const V1_PLAIN_PDF: &[u8] = include_bytes!("../tests/fixtures/v1_plain.pdf");

    fn open_fixture(bytes: &[u8], code: &str) -> CoreResult<DecryptedSpdf> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.spdf");
        fs::write(&path, bytes).unwrap();
        open_spdf_file(&path, code)
    }

    // Fixtures were written by the v1.1.0 `SecurePdf.Core.dll` (`SpdfContainer.Create`).
    #[test]
    fn opens_v1_generated_spdf_with_default_policy() {
        let bytes = include_bytes!("../tests/fixtures/v1_default_policy.spdf");
        let doc = open_fixture(bytes, "abcd efgh-jklm").unwrap();
        assert_eq!(doc.pdf_bytes.as_slice(), V1_PLAIN_PDF);
        assert_eq!(
            doc.policy.doc_id.to_hex_n(),
            "75f1d8efa4054c07984d8e7fa2060d7a"
        );
        assert_eq!(doc.policy.max_prints, 3);
        assert_eq!(doc.policy.expires_at_utc_ticks, None);
        assert_eq!(doc.policy.moire_level, MoireLevel::Medium);
        assert_eq!(
            doc.policy.flags.bits(),
            SecurityFlags::HIDDEN_WATERMARK | SecurityFlags::BLUR_ON_UNFOCUS
        );
        assert_eq!(doc.policy.original_file_name, "合同-A4.pdf");
        assert_eq!(doc.policy.watermark_note, "");

        assert!(matches!(
            open_fixture(bytes, "WXYZ-2345-6789"),
            Err(CoreError::WrongVerificationCode)
        ));
    }

    #[test]
    fn opens_v1_generated_spdf_with_expiry_all_flags_and_custom_kdf_cost() {
        let bytes = include_bytes!("../tests/fixtures/v1_expiring_noprint.spdf");
        let doc = open_fixture(bytes, "WXYZ-2345-6789").unwrap();
        assert_eq!(doc.pdf_bytes.as_slice(), V1_PLAIN_PDF);
        assert_eq!(
            doc.policy.doc_id.to_hex_n(),
            "34d801a871f345a69e63e5aa349c8e5f"
        );
        assert_eq!(doc.policy.max_prints, 0);
        // 2099-12-31T23:59:59Z in .NET ticks.
        assert_eq!(
            doc.policy.expires_at_utc_ticks,
            Some(662_380_415_990_000_000)
        );
        assert_eq!(doc.policy.moire_level, MoireLevel::High);
        assert_eq!(doc.policy.flags.bits(), 0b1111);
        assert_eq!(doc.policy.original_file_name, "report final.pdf");
        assert_eq!(doc.policy.watermark_note, "张三专用");
    }

    // `v2_generated.spdf` was written by this crate (see the ignored generator below) and
    // unlocked with the v1.1.0 `SecurePdf.Core.dll`, which returned identical PDF bytes and
    // policy fields. This test pins the file so the two implementations stay interchangeable.
    #[test]
    fn opens_v2_generated_spdf_that_v1_was_verified_to_unlock() {
        let bytes = include_bytes!("../tests/fixtures/v2_generated.spdf");
        let doc = open_fixture(bytes, "QRST-UVWX-YZ23").unwrap();
        assert_eq!(doc.pdf_bytes.as_slice(), V1_PLAIN_PDF);
        assert_eq!(doc.policy.max_prints, 2);
        assert_eq!(
            doc.policy.expires_at_utc_ticks,
            Some(662_380_415_990_000_000)
        );
        assert_eq!(doc.policy.moire_level, MoireLevel::Low);
        assert_eq!(
            doc.policy.flags.bits(),
            SecurityFlags::HIDDEN_WATERMARK | SecurityFlags::VISIBLE_WATERMARK
        );
        assert_eq!(doc.policy.original_file_name, "新版-生成.pdf");
        assert_eq!(doc.policy.watermark_note, "李四 / 仅供内部");
    }

    /// Regenerates `v2_generated.spdf` into the directory named by `SECUREPDF_FIXTURE_OUT`.
    #[test]
    #[ignore = "fixture generator; run manually with SECUREPDF_FIXTURE_OUT set"]
    fn generate_v2_fixture_for_v1_cross_check() {
        let out_dir = std::env::var_os("SECUREPDF_FIXTURE_OUT").unwrap();
        let out = Path::new(&out_dir).join("v2_generated.spdf");
        let _ = fs::remove_file(&out);
        write_spdf_file(
            V1_PLAIN_PDF,
            &out,
            &CreateSpdfOptions {
                verification_code: "QRST-UVWX-YZ23",
                max_prints: 2,
                expires_at_utc_ticks: Some(662_380_415_990_000_000),
                moire_level: MoireLevel::Low,
                flags: SecurityFlags::from_bits(
                    SecurityFlags::HIDDEN_WATERMARK | SecurityFlags::VISIBLE_WATERMARK,
                ),
                original_file_name: "新版-生成.pdf",
                watermark_note: "李四 / 仅供内部",
                doc_id: None,
                scrypt_n: None,
                scrypt_r: None,
                scrypt_p: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn writing_over_an_existing_placeholder_replaces_it_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("reserved.spdf");
        fs::write(&out, b"").unwrap(); // name reserved by the caller with an empty placeholder
        let opts = CreateSpdfOptions {
            verification_code: "ABCD-EFGH-JKLM",
            max_prints: 1,
            expires_at_utc_ticks: None,
            moire_level: MoireLevel::Off,
            flags: SecurityFlags::from_bits(0),
            original_file_name: "a.pdf",
            watermark_note: "",
            doc_id: None,
            scrypt_n: Some(1_024),
            scrypt_r: None,
            scrypt_p: None,
        };
        write_spdf_file(&minimal_pdf(), &out, &opts).unwrap();
        let first = fs::read(&out).unwrap();
        assert!(first.starts_with(&SPDF_MAGIC));
        // A second write over the finished file also succeeds and replaces it.
        write_spdf_file(&minimal_pdf(), &out, &opts).unwrap();
        assert_ne!(fs::read(&out).unwrap(), first);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("reserved.spdf")]);
        assert!(open_spdf_file(&out, "ABCD-EFGH-JKLM").is_ok());
    }

    #[test]
    fn spdf_create_rejects_non_pdf_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let err = write_spdf_file(
            b"NOT A PDF",
            &dir.path().join("bad.spdf"),
            &CreateSpdfOptions {
                verification_code: "ABCD-EFGH",
                max_prints: 0,
                expires_at_utc_ticks: None,
                moire_level: MoireLevel::Low,
                flags: SecurityFlags::default(),
                original_file_name: "bad.pdf",
                watermark_note: "",
                doc_id: None,
                scrypt_n: Some(1_024),
                scrypt_r: Some(8),
                scrypt_p: Some(1),
            },
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::InvalidArgument(_)));
    }
}
