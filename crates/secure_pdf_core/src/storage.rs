//! Tamper-evident dual-store quota engine protected by DPAPI, HMAC-SHA256, and a named Win32 mutex.

use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use windows_core::PCWSTR;
use zeroize::Zeroizing;

use crate::bindings;
use crate::crypto::{constant_time_equals, hmac_sha256};
use crate::error::{CoreError, CoreResult};
use crate::format::{DocId, SpdfPolicy, TICKS_PER_SECOND, utc_now_dotnet_ticks};
use crate::win32_util::{HKeyGuard, LocalAllocGuard, NamedMutexLock, to_wide_null};

/// Version byte for serialized quota payloads.
pub const QUOTA_RECORD_VERSION: u8 = 1;
/// Clock rollback grace window before flagging tampering (5 minutes).
pub const CLOCK_ROLLBACK_TOLERANCE_TICKS: i64 = 5 * 60 * TICKS_PER_SECOND;
/// 10-second timeout for cross-process quota lock acquisition.
pub const QUOTA_MUTEX_TIMEOUT_MS: u32 = 10_000;

const DPAPI_ENTROPY: &[u8] = b"SecurePdfSuite-Quota-v2";
const QUOTA_PAYLOAD_LEN: usize = 1 + 16 + 4 + 4 + 8 + 8; // 41 bytes
const HMAC_LEN: usize = 32;
const QUOTA_PLAINTEXT_LEN: usize = QUOTA_PAYLOAD_LEN + HMAC_LEN; // 73 bytes

/// Current validated quota state returned to callers after verification or print slot allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaStatus {
    /// Allowed total prints under policy.
    pub max_prints: i32,
    /// Prints consumed so far.
    pub used_prints: i32,
    /// Remaining prints (`max(0, max_prints - used_prints)`).
    pub remaining_prints: i32,
    /// Monotonically increasing update sequence counter.
    pub sequence_number: i64,
    /// High-watermark UTC .NET ticks observed for rollback detection.
    pub max_seen_utc_ticks: i64,
}

/// Consistency of the primary / secondary quota copies as found on disk and in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaStoreHealth {
    /// Neither copy exists yet (first time this document is opened on this machine).
    Fresh,
    /// Both copies are valid and agree.
    Consistent,
    /// A copy is missing, fails verification, or the two copies disagree.
    Inconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuotaRecord {
    doc_id: DocId,
    max_prints: i32,
    used_prints: i32,
    sequence_number: i64,
    max_seen_utc_ticks: i64,
}

impl QuotaRecord {
    const fn to_status(self) -> QuotaStatus {
        let diff = self.max_prints.saturating_sub(self.used_prints);
        let remaining = if diff < 0 { 0 } else { diff };
        QuotaStatus {
            max_prints: self.max_prints,
            used_prints: self.used_prints,
            remaining_prints: remaining,
            sequence_number: self.sequence_number,
            max_seen_utc_ticks: self.max_seen_utc_ticks,
        }
    }

    fn encode(self, machine_hash: &[u8; 32]) -> CoreResult<[u8; QUOTA_PLAINTEXT_LEN]> {
        let mut out = [0_u8; QUOTA_PLAINTEXT_LEN];
        out[0] = QUOTA_RECORD_VERSION;
        out[1..17].copy_from_slice(self.doc_id.as_wire_bytes());
        out[17..21].copy_from_slice(&self.max_prints.to_le_bytes());
        out[21..25].copy_from_slice(&self.used_prints.to_le_bytes());
        out[25..33].copy_from_slice(&self.sequence_number.to_le_bytes());
        out[33..41].copy_from_slice(&self.max_seen_utc_ticks.to_le_bytes());

        let tag = hmac_sha256(machine_hash, &out[..QUOTA_PAYLOAD_LEN])?;
        out[QUOTA_PAYLOAD_LEN..].copy_from_slice(&tag);
        Ok(out)
    }

    fn decode(raw: &[u8], expected_doc_id: DocId, machine_hash: &[u8; 32]) -> CoreResult<Self> {
        if raw.len() != QUOTA_PLAINTEXT_LEN {
            return Err(CoreError::QuotaTampered("配额记录长度异常。".to_owned()));
        }
        let expected_tag = hmac_sha256(machine_hash, &raw[..QUOTA_PAYLOAD_LEN])?;
        if !constant_time_equals(&expected_tag, &raw[QUOTA_PAYLOAD_LEN..]) {
            return Err(CoreError::QuotaTampered(
                "配额记录 HMAC 完整性校验失败。".to_owned(),
            ));
        }
        if raw[0] != QUOTA_RECORD_VERSION {
            return Err(CoreError::QuotaTampered(
                "配额记录版本号不匹配。".to_owned(),
            ));
        }
        let mut doc_bytes = [0_u8; 16];
        doc_bytes.copy_from_slice(&raw[1..17]);
        let doc_id = DocId::from_wire_bytes(doc_bytes);
        if doc_id != expected_doc_id {
            return Err(CoreError::QuotaTampered(
                "配额记录文档编号不匹配。".to_owned(),
            ));
        }
        let max_prints = i32::from_le_bytes([raw[17], raw[18], raw[19], raw[20]]);
        let used_prints = i32::from_le_bytes([raw[21], raw[22], raw[23], raw[24]]);
        let sequence_number = i64::from_le_bytes([
            raw[25], raw[26], raw[27], raw[28], raw[29], raw[30], raw[31], raw[32],
        ]);
        let max_seen_utc_ticks = i64::from_le_bytes([
            raw[33], raw[34], raw[35], raw[36], raw[37], raw[38], raw[39], raw[40],
        ]);
        if max_prints < 0 || used_prints < 0 || sequence_number < 0 || max_seen_utc_ticks <= 0 {
            return Err(CoreError::QuotaTampered(
                "配额记录字段取值越界。".to_owned(),
            ));
        }
        Ok(Self {
            doc_id,
            max_prints,
            used_prints,
            sequence_number,
            max_seen_utc_ticks,
        })
    }
}

/// Abstract dual-store persistence backend (primary disk + secondary HKCU store).
pub trait QuotaBackend: Send + Sync {
    /// Reads the primary protected quota blob, returning `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] on non-missing read failure.
    fn read_primary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>>;

    /// Persists the primary protected quota blob.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if writing fails.
    fn write_primary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()>;

    /// Reads the secondary protected quota blob, returning `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] on non-missing read failure.
    fn read_secondary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>>;

    /// Persists the secondary protected quota blob.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if writing fails.
    fn write_secondary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()>;
}

/// Abstracts DPAPI sealing/unsealing for production and deterministic unit tests.
pub trait QuotaProtector: Send + Sync {
    /// Seals `plaintext` with optional `entropy`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Crypto`] on sealing error.
    fn protect(&self, plaintext: &[u8], entropy: &[u8]) -> CoreResult<Vec<u8>>;

    /// Unseals `blob` with optional `entropy`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Crypto`] / [`CoreError::QuotaTampered`] on unsealing failure.
    fn unprotect(&self, blob: &[u8], entropy: &[u8]) -> CoreResult<Zeroizing<Vec<u8>>>;
}

/// Production Windows DPAPI (`CryptProtectData` / `CryptUnprotectData`) protector.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsDpapiProtector;

impl QuotaProtector for WindowsDpapiProtector {
    fn protect(&self, plaintext: &[u8], entropy: &[u8]) -> CoreResult<Vec<u8>> {
        let in_len = u32::try_from(plaintext.len())
            .map_err(|_| CoreError::Crypto("DPAPI 输入长度溢出。".to_owned()))?;
        let ent_len = u32::try_from(entropy.len())
            .map_err(|_| CoreError::Crypto("DPAPI entropy 长度溢出。".to_owned()))?;
        let in_blob = bindings::DATA_BLOB {
            cbData: in_len,
            pbData: plaintext.as_ptr().cast_mut(),
        };
        let ent_blob = bindings::DATA_BLOB {
            cbData: ent_len,
            pbData: entropy.as_ptr().cast_mut(),
        };
        let mut out_blob = bindings::DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };

        let dpapi_flags = u32::try_from(bindings::CRYPTPROTECT_UI_FORBIDDEN).unwrap_or(1);
        // SAFETY: `in_blob` and `ent_blob` point to valid read-only slices for the synchronous call.
        let ok = unsafe {
            bindings::CryptProtectData(
                &raw const in_blob,
                PCWSTR::null(),
                Some(&raw const ent_blob),
                None,
                None,
                dpapi_flags,
                &raw mut out_blob,
            )
        };
        if !ok.as_bool() || out_blob.pbData.is_null() {
            return Err(CoreError::Crypto(
                "DPAPI CryptProtectData 失败。".to_owned(),
            ));
        }
        let _alloc = LocalAllocGuard(out_blob.pbData);
        let out_len = usize::try_from(out_blob.cbData)
            .map_err(|_| CoreError::Crypto("DPAPI 输出长度溢出。".to_owned()))?;
        // SAFETY: `out_blob.pbData` holds `out_len` initialized bytes returned by `CryptProtectData`.
        let slice = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_len) };
        Ok(slice.to_vec())
    }

    fn unprotect(&self, blob: &[u8], entropy: &[u8]) -> CoreResult<Zeroizing<Vec<u8>>> {
        let in_len = u32::try_from(blob.len())
            .map_err(|_| CoreError::QuotaTampered("DPAPI 密文长度非法。".to_owned()))?;
        let ent_len = u32::try_from(entropy.len())
            .map_err(|_| CoreError::Crypto("DPAPI entropy 长度溢出。".to_owned()))?;
        let in_blob = bindings::DATA_BLOB {
            cbData: in_len,
            pbData: blob.as_ptr().cast_mut(),
        };
        let ent_blob = bindings::DATA_BLOB {
            cbData: ent_len,
            pbData: entropy.as_ptr().cast_mut(),
        };
        let mut out_blob = bindings::DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };

        let dpapi_flags = u32::try_from(bindings::CRYPTPROTECT_UI_FORBIDDEN).unwrap_or(1);
        // SAFETY: Synchronously unseals DPAPI protected bytes into newly LocalAlloc'd `out_blob`.
        let ok = unsafe {
            bindings::CryptUnprotectData(
                &raw const in_blob,
                null_mut(),
                Some(&raw const ent_blob),
                None,
                None,
                dpapi_flags,
                &raw mut out_blob,
            )
        };
        if !ok.as_bool() || out_blob.pbData.is_null() {
            return Err(CoreError::QuotaTampered(
                "DPAPI 解封失败，配额记录可能已损坏或跨账户复制。".to_owned(),
            ));
        }
        let _alloc = LocalAllocGuard(out_blob.pbData);
        let out_len = usize::try_from(out_blob.cbData)
            .map_err(|_| CoreError::QuotaTampered("DPAPI 解封长度溢出。".to_owned()))?;
        // SAFETY: `out_blob.pbData` holds `out_len` decrypted plaintext bytes.
        let slice = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_len) };
        let mut out = Zeroizing::new(vec![0_u8; out_len]);
        out.copy_from_slice(slice);
        Ok(out)
    }
}

/// Default Windows dual backend (`%LOCALAPPDATA%\SecurePdfSuite\Quotas\{docId:N}.dat` + `HKCU\Software\SecurePdfSuite\Quotas`).
#[derive(Debug, Clone)]
pub struct WindowsDualQuotaBackend {
    quota_dir: PathBuf,
    registry_subkey: String,
}

impl WindowsDualQuotaBackend {
    /// Creates the default production backend using `%LOCALAPPDATA%\SecurePdfSuite\Quotas`.
    #[must_use]
    pub fn new_default() -> Self {
        let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
        Self {
            quota_dir: base.join("SecurePdfSuite").join("Quotas"),
            registry_subkey: r"Software\SecurePdfSuite\Quotas".to_owned(),
        }
    }

    /// Creates a backend with custom directory and HKCU subkey (primarily for isolated tests).
    #[must_use]
    pub const fn with_paths(quota_dir: PathBuf, registry_subkey: String) -> Self {
        Self {
            quota_dir,
            registry_subkey,
        }
    }

    fn file_path(&self, doc_id: DocId) -> PathBuf {
        self.quota_dir.join(format!("{}.dat", doc_id.to_hex_n()))
    }
}

impl QuotaBackend for WindowsDualQuotaBackend {
    fn read_primary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>> {
        let path = self.file_path(doc_id);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CoreError::Io(e)),
        }
    }

    fn write_primary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()> {
        fs::create_dir_all(&self.quota_dir)?;
        let path = self.file_path(doc_id);
        let tmp = self
            .quota_dir
            .join(format!("{}.{}.tmp", doc_id.to_hex_n(), std::process::id()));
        fs::write(&tmp, blob)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn read_secondary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>> {
        let subkey_w = to_wide_null(&self.registry_subkey);
        let val_w = to_wide_null(&doc_id.to_hex_n());
        let mut raw_key = bindings::HKEY(null_mut());
        let read_sam = u32::try_from(bindings::KEY_READ).unwrap_or(0x20019);
        // SAFETY: Opens HKCU subkey for read if it exists.
        let rc = unsafe {
            bindings::RegOpenKeyExW(
                bindings::HKEY_CURRENT_USER,
                PCWSTR(subkey_w.as_ptr()),
                None,
                bindings::ACCESS_MASK(read_sam),
                &raw mut raw_key,
            )
        };
        if rc.0 != 0 {
            return Ok(None);
        }
        let key = HKeyGuard(raw_key);

        let mut val_type = 0_u32;
        let mut len = 0_u32;
        // SAFETY: Probes size of `REG_BINARY` value `{docId:N}`.
        let rc_size = unsafe {
            bindings::RegQueryValueExW(
                key.0,
                PCWSTR(val_w.as_ptr()),
                None,
                Some(&raw mut val_type),
                None,
                &raw mut len,
            )
        };
        if rc_size.0 != 0 || val_type != bindings::REG_BINARY || len == 0 {
            return Ok(None);
        }

        let mut out = vec![0_u8; usize::try_from(len).unwrap_or(0)];
        // SAFETY: Reads binary bytes of `{docId:N}` into `out`.
        let rc_read = unsafe {
            bindings::RegQueryValueExW(
                key.0,
                PCWSTR(val_w.as_ptr()),
                None,
                Some(&raw mut val_type),
                Some(out.as_mut_ptr()),
                &raw mut len,
            )
        };
        if rc_read.0 != 0 || val_type != bindings::REG_BINARY {
            return Ok(None);
        }
        out.truncate(usize::try_from(len).unwrap_or(out.len()));
        Ok(Some(out))
    }

    fn write_secondary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()> {
        let subkey_w = to_wide_null(&self.registry_subkey);
        let val_w = to_wide_null(&doc_id.to_hex_n());
        let mut raw_key = bindings::HKEY(null_mut());
        let write_sam = u32::try_from(bindings::KEY_WRITE).unwrap_or(0x20006);
        let non_volatile = u32::try_from(bindings::REG_OPTION_NON_VOLATILE).unwrap_or(0);
        // SAFETY: Creates or opens HKCU quota backup subkey with write permission.
        let rc = unsafe {
            bindings::RegCreateKeyExW(
                bindings::HKEY_CURRENT_USER,
                PCWSTR(subkey_w.as_ptr()),
                None,
                PCWSTR::null(),
                non_volatile,
                bindings::ACCESS_MASK(write_sam),
                None,
                &raw mut raw_key,
                None,
            )
        };
        if rc.0 != 0 {
            return Err(CoreError::Io(std::io::Error::from_raw_os_error(rc.0)));
        }
        let key = HKeyGuard(raw_key);

        // SAFETY: Writes `blob` as `REG_BINARY` under `{docId:N}`.
        let rc_set = unsafe {
            bindings::RegSetValueExW(
                key.0,
                PCWSTR(val_w.as_ptr()),
                None,
                bindings::REG_BINARY,
                Some(blob),
            )
        };
        if rc_set.0 != 0 {
            return Err(CoreError::Io(std::io::Error::from_raw_os_error(rc_set.0)));
        }
        Ok(())
    }
}

/// Dual-store quota coordinator parameterized over backend and protector.
pub struct LocalQuotaStore<B = WindowsDualQuotaBackend, P = WindowsDpapiProtector> {
    backend: B,
    protector: P,
    mutex_prefix: String,
}

impl Default for LocalQuotaStore<WindowsDualQuotaBackend, WindowsDpapiProtector> {
    fn default() -> Self {
        Self::new(
            WindowsDualQuotaBackend::new_default(),
            WindowsDpapiProtector,
        )
    }
}

impl<B: QuotaBackend, P: QuotaProtector> LocalQuotaStore<B, P> {
    /// Creates a store instance backed by custom `backend` and `protector`.
    #[must_use]
    pub fn new(backend: B, protector: P) -> Self {
        Self {
            backend,
            protector,
            mutex_prefix: r"Local\SecurePdfSuite.Quota".to_owned(),
        }
    }

    /// Overrides the Win32 mutex name prefix (for parallel isolated unit tests).
    #[must_use]
    pub fn with_mutex_prefix(mut self, prefix: String) -> Self {
        self.mutex_prefix = prefix;
        self
    }

    fn lock_doc(&self, doc_id: DocId) -> CoreResult<NamedMutexLock> {
        let name = format!("{}.{}", self.mutex_prefix, doc_id.to_hex_n());
        NamedMutexLock::acquire(&name, QUOTA_MUTEX_TIMEOUT_MS)
    }

    /// Verifies quota integrity, detects tampering or clock rollback, merges stores, and checks expiry.
    ///
    /// # Errors
    ///
    /// - [`CoreError::QuotaTampered`] if both blobs are corrupted or system clock rolled back > 5 minutes
    /// - [`CoreError::DocumentExpired`] if the policy expiration timestamp has passed
    pub fn initialize_or_verify(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
    ) -> CoreResult<QuotaStatus> {
        self.initialize_or_verify_at(policy, machine_hash, utc_now_dotnet_ticks())
    }

    /// Deterministic clock-injected variant of [`Self::initialize_or_verify`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::initialize_or_verify`].
    pub fn initialize_or_verify_at(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
        now_ticks: i64,
    ) -> CoreResult<QuotaStatus> {
        let _lock = self.lock_doc(policy.doc_id)?;
        let record = self.load_and_verify_locked(policy, machine_hash, now_ticks)?;
        Ok(record.to_status())
    }

    /// Atomically checks quota & expiry and consumes one print slot across both stores.
    ///
    /// # Errors
    ///
    /// - [`CoreError::QuotaExhausted`] if no print slots remain (`used_prints >= max_prints`)
    /// - [`CoreError::DocumentExpired`] or [`CoreError::QuotaTampered`] on policy violation
    pub fn consume_print_slot(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
    ) -> CoreResult<QuotaStatus> {
        self.consume_print_slot_at(policy, machine_hash, utc_now_dotnet_ticks())
    }

    /// Deterministic clock-injected variant of [`Self::consume_print_slot`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::consume_print_slot`].
    pub fn consume_print_slot_at(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
        now_ticks: i64,
    ) -> CoreResult<QuotaStatus> {
        let _lock = self.lock_doc(policy.doc_id)?;
        let mut record = self.load_and_verify_locked(policy, machine_hash, now_ticks)?;
        if record.used_prints >= record.max_prints {
            return Err(CoreError::QuotaExhausted);
        }
        record.used_prints = record.used_prints.saturating_add(1);
        record.sequence_number = record.sequence_number.saturating_add(1);
        record.max_seen_utc_ticks = record.max_seen_utc_ticks.max(now_ticks);
        self.persist_both(policy.doc_id, machine_hash, record)?;
        Ok(record.to_status())
    }

    /// Rolls back one previously consumed print slot when a physical print fails with 0 pages printed.
    ///
    /// # Errors
    ///
    /// Propagates lock, integrity, or persistence failures.
    pub fn rollback_print_slot(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
    ) -> CoreResult<QuotaStatus> {
        self.rollback_print_slot_at(policy, machine_hash, utc_now_dotnet_ticks())
    }

    /// Deterministic clock-injected variant of [`Self::rollback_print_slot`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::rollback_print_slot`].
    pub fn rollback_print_slot_at(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
        now_ticks: i64,
    ) -> CoreResult<QuotaStatus> {
        let _lock = self.lock_doc(policy.doc_id)?;
        let mut record = self.load_and_verify_locked(policy, machine_hash, now_ticks)?;
        if record.used_prints > 0 {
            record.used_prints -= 1;
            record.sequence_number = record.sequence_number.saturating_add(1);
            record.max_seen_utc_ticks = record.max_seen_utc_ticks.max(now_ticks);
            self.persist_both(policy.doc_id, machine_hash, record)?;
        }
        Ok(record.to_status())
    }

    fn load_and_verify_locked(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
        now_ticks: i64,
    ) -> CoreResult<QuotaRecord> {
        let primary_raw = self.backend.read_primary(policy.doc_id)?;
        let secondary_raw = self.backend.read_secondary(policy.doc_id)?;

        let primary_rec = primary_raw
            .as_deref()
            .map(|b| self.unseal_record(b, policy.doc_id, machine_hash));
        let secondary_rec = secondary_raw
            .as_deref()
            .map(|b| self.unseal_record(b, policy.doc_id, machine_hash));

        let mut merged = match (primary_rec, secondary_rec) {
            (None, None) => QuotaRecord {
                doc_id: policy.doc_id,
                max_prints: policy.max_prints.max(0),
                used_prints: 0,
                sequence_number: 1,
                max_seen_utc_ticks: now_ticks,
            },
            (Some(Ok(r)), None | Some(Err(_))) | (None | Some(Err(_)), Some(Ok(r))) => r,
            (Some(Ok(a)), Some(Ok(b))) => QuotaRecord {
                doc_id: policy.doc_id,
                max_prints: a.max_prints.min(b.max_prints).min(policy.max_prints.max(0)),
                used_prints: a.used_prints.max(b.used_prints),
                sequence_number: a.sequence_number.max(b.sequence_number),
                max_seen_utc_ticks: a.max_seen_utc_ticks.max(b.max_seen_utc_ticks),
            },
            (Some(Err(e)), Some(Err(_)) | None) | (None, Some(Err(e))) => {
                return Err(e);
            }
        };

        // Respect policy ceiling without ever relaxing an already-tightened limit.
        merged.max_prints = merged.max_prints.min(policy.max_prints.max(0));

        if policy.expires_at_utc_ticks.is_some()
            && now_ticks.saturating_add(CLOCK_ROLLBACK_TOLERANCE_TICKS) < merged.max_seen_utc_ticks
        {
            return Err(CoreError::QuotaTampered(
                "检测到系统时间大幅回拨，基于安全策略已锁定此限时文档。".to_owned(),
            ));
        }

        if policy.is_expired_at(now_ticks) {
            return Err(CoreError::DocumentExpired);
        }

        merged.max_seen_utc_ticks = merged.max_seen_utc_ticks.max(now_ticks);
        merged.sequence_number = merged.sequence_number.saturating_add(1);
        self.persist_both(policy.doc_id, machine_hash, merged)?;
        Ok(merged)
    }

    /// Read-only consistency check of the two stores, without repairing or rewriting anything.
    ///
    /// Call before [`Self::initialize_or_verify`] to learn whether that call is about to repair
    /// a missing, corrupted or diverging copy (v1.x `TamperWarning`).
    ///
    /// # Errors
    ///
    /// Propagates backend read failures and [`CoreError::QuotaLockTimeout`].
    pub fn inspect_store_health(
        &self,
        policy: &SpdfPolicy,
        machine_hash: &[u8; 32],
    ) -> CoreResult<QuotaStoreHealth> {
        let _lock = self.lock_doc(policy.doc_id)?;
        let primary = self
            .backend
            .read_primary(policy.doc_id)?
            .map(|b| self.unseal_record(&b, policy.doc_id, machine_hash));
        let secondary = self
            .backend
            .read_secondary(policy.doc_id)?
            .map(|b| self.unseal_record(&b, policy.doc_id, machine_hash));
        Ok(match (primary, secondary) {
            (None, None) => QuotaStoreHealth::Fresh,
            (Some(Ok(a)), Some(Ok(b)))
                if a.used_prints == b.used_prints && a.sequence_number == b.sequence_number =>
            {
                QuotaStoreHealth::Consistent
            }
            _ => QuotaStoreHealth::Inconsistent,
        })
    }

    fn unseal_record(
        &self,
        blob: &[u8],
        doc_id: DocId,
        machine_hash: &[u8; 32],
    ) -> CoreResult<QuotaRecord> {
        let plain = self.protector.unprotect(blob, DPAPI_ENTROPY)?;
        QuotaRecord::decode(&plain, doc_id, machine_hash)
    }

    fn persist_both(
        &self,
        doc_id: DocId,
        machine_hash: &[u8; 32],
        record: QuotaRecord,
    ) -> CoreResult<()> {
        let encoded = Zeroizing::new(record.encode(machine_hash)?);
        let blob = self.protector.protect(&*encoded, DPAPI_ENTROPY)?;
        self.backend.write_primary(doc_id, &blob)?;
        self.backend.write_secondary(doc_id, &blob)?;
        Ok(())
    }
}

/// Removes an arbitrary path safely when cleaning up temporary files.
pub fn remove_temp_file_quietly(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{DOTNET_UNIX_EPOCH_TICKS, MoireLevel, SecurityFlags};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryDualBackend {
        primary: Mutex<HashMap<DocId, Vec<u8>>>,
        secondary: Mutex<HashMap<DocId, Vec<u8>>>,
    }

    impl MemoryDualBackend {
        fn clear_primary(&self, doc_id: DocId) {
            self.primary.lock().unwrap().remove(&doc_id);
        }
        fn corrupt_primary(&self, doc_id: DocId) {
            if let Some(v) = self.primary.lock().unwrap().get_mut(&doc_id)
                && let Some(first) = v.first_mut()
            {
                *first ^= 0xFF;
            }
        }
        fn corrupt_secondary(&self, doc_id: DocId) {
            if let Some(v) = self.secondary.lock().unwrap().get_mut(&doc_id)
                && let Some(first) = v.first_mut()
            {
                *first ^= 0xFF;
            }
        }
        fn overwrite_secondary(&self, doc_id: DocId, blob: Vec<u8>) {
            self.secondary.lock().unwrap().insert(doc_id, blob);
        }
        fn snapshot_primary(&self, doc_id: DocId) -> Vec<u8> {
            self.primary.lock().unwrap().get(&doc_id).unwrap().clone()
        }
    }

    impl QuotaBackend for MemoryDualBackend {
        fn read_primary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>> {
            Ok(self.primary.lock().unwrap().get(&doc_id).cloned())
        }
        fn write_primary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()> {
            self.primary.lock().unwrap().insert(doc_id, blob.to_vec());
            Ok(())
        }
        fn read_secondary(&self, doc_id: DocId) -> CoreResult<Option<Vec<u8>>> {
            Ok(self.secondary.lock().unwrap().get(&doc_id).cloned())
        }
        fn write_secondary(&self, doc_id: DocId, blob: &[u8]) -> CoreResult<()> {
            self.secondary.lock().unwrap().insert(doc_id, blob.to_vec());
            Ok(())
        }
    }

    fn sample_policy(max_prints: i32, expires: Option<i64>) -> SpdfPolicy {
        SpdfPolicy {
            doc_id: DocId::new_random().unwrap(),
            max_prints,
            expires_at_utc_ticks: expires,
            moire_level: MoireLevel::Medium,
            flags: SecurityFlags::from_bits(SecurityFlags::HIDDEN_WATERMARK),
            original_file_name: "quota_test.pdf".to_owned(),
            watermark_note: String::new(),
        }
    }

    #[test]
    fn quota_consume_rollback_and_exhaust_with_real_dpapi() {
        let backend = MemoryDualBackend::default();
        let store = LocalQuotaStore::new(backend, WindowsDpapiProtector)
            .with_mutex_prefix(format!("Local\\SecurePdfSuite.Test.{}", std::process::id()));
        let policy = sample_policy(2, None);
        let machine_hash = [0x42_u8; 32];
        let t0 = DOTNET_UNIX_EPOCH_TICKS + 1_700_000_000 * TICKS_PER_SECOND;

        let init = store
            .initialize_or_verify_at(&policy, &machine_hash, t0)
            .unwrap();
        assert_eq!(init.max_prints, 2);
        assert_eq!(init.used_prints, 0);
        assert_eq!(init.remaining_prints, 2);

        let after_first = store
            .consume_print_slot_at(&policy, &machine_hash, t0 + 100)
            .unwrap();
        assert_eq!(after_first.used_prints, 1);
        assert_eq!(after_first.remaining_prints, 1);

        // Rollback when GDI print cancels on 0 pages
        let after_rollback = store
            .rollback_print_slot_at(&policy, &machine_hash, t0 + 200)
            .unwrap();
        assert_eq!(after_rollback.used_prints, 0);
        assert_eq!(after_rollback.remaining_prints, 2);

        assert!(
            store
                .consume_print_slot_at(&policy, &machine_hash, t0 + 300)
                .is_ok()
        );
        assert!(
            store
                .consume_print_slot_at(&policy, &machine_hash, t0 + 400)
                .is_ok()
        );
        assert!(matches!(
            store.consume_print_slot_at(&policy, &machine_hash, t0 + 500),
            Err(CoreError::QuotaExhausted)
        ));
    }

    #[test]
    fn quota_recovers_single_corrupt_store_and_takes_conservative_max_used() {
        let backend = MemoryDualBackend::default();
        let store = LocalQuotaStore::new(backend, WindowsDpapiProtector).with_mutex_prefix(
            format!("Local\\SecurePdfSuite.Merge.{}", std::process::id()),
        );
        let policy = sample_policy(3, None);
        let machine_hash = [0x55_u8; 32];
        let t0 = DOTNET_UNIX_EPOCH_TICKS + 1_700_000_000 * TICKS_PER_SECOND;

        store
            .initialize_or_verify_at(&policy, &machine_hash, t0)
            .unwrap();
        // Save stale snapshot (0 used) into secondary
        let stale_blob = store.backend.snapshot_primary(policy.doc_id);
        store
            .consume_print_slot_at(&policy, &machine_hash, t0 + 100)
            .unwrap();
        store.backend.overwrite_secondary(policy.doc_id, stale_blob);

        // Deleting/resetting one store cannot regain consumed prints (conservative merge takes max used)
        let merged = store
            .initialize_or_verify_at(&policy, &machine_hash, t0 + 200)
            .unwrap();
        assert_eq!(merged.used_prints, 1);
        assert_eq!(merged.remaining_prints, 2);

        // Deleting primary still recovers from valid secondary
        store.backend.clear_primary(policy.doc_id);
        let recovered = store
            .initialize_or_verify_at(&policy, &machine_hash, t0 + 300)
            .unwrap();
        assert_eq!(recovered.used_prints, 1);

        // Single corrupt primary still recovers cleanly
        store.backend.corrupt_primary(policy.doc_id);
        let healed = store
            .initialize_or_verify_at(&policy, &machine_hash, t0 + 400)
            .unwrap();
        assert_eq!(healed.used_prints, 1);

        // Both stores corrupt fails closed!
        store.backend.corrupt_primary(policy.doc_id);
        store.backend.corrupt_secondary(policy.doc_id);
        assert!(matches!(
            store.initialize_or_verify_at(&policy, &machine_hash, t0 + 500),
            Err(CoreError::QuotaTampered(_))
        ));
    }

    #[test]
    fn quota_rejects_expired_and_clock_rollback_and_machine_change() {
        let backend = MemoryDualBackend::default();
        let store = LocalQuotaStore::new(backend, WindowsDpapiProtector)
            .with_mutex_prefix(format!("Local\\SecurePdfSuite.Time.{}", std::process::id()));
        let t0 = DOTNET_UNIX_EPOCH_TICKS + 1_700_000_000 * TICKS_PER_SECOND;
        let expires = t0 + 3_600 * TICKS_PER_SECOND;
        let policy = sample_policy(2, Some(expires));
        let machine_hash = [0x77_u8; 32];

        store
            .initialize_or_verify_at(&policy, &machine_hash, t0)
            .unwrap();

        // Small backward drift (<= 5 min) allowed
        let drift_3min = t0 - 3 * 60 * TICKS_PER_SECOND;
        assert!(
            store
                .initialize_or_verify_at(&policy, &machine_hash, drift_3min)
                .is_ok()
        );

        // Large backward rollback (> 5 min) rejected!
        let rollback_10min = t0 - 10 * 60 * TICKS_PER_SECOND;
        assert!(matches!(
            store.initialize_or_verify_at(&policy, &machine_hash, rollback_10min),
            Err(CoreError::QuotaTampered(_))
        ));

        // Expired document rejected!
        assert!(matches!(
            store.initialize_or_verify_at(&policy, &machine_hash, expires),
            Err(CoreError::DocumentExpired)
        ));

        // Different machine hash fails HMAC check across stores
        let wrong_hash = [0x88_u8; 32];
        assert!(matches!(
            store.initialize_or_verify_at(&policy, &wrong_hash, t0 + 60),
            Err(CoreError::QuotaTampered(_))
        ));
    }

    #[test]
    fn store_health_reports_fresh_consistent_and_inconsistent_without_writing() {
        let store = LocalQuotaStore::new(MemoryDualBackend::default(), WindowsDpapiProtector)
            .with_mutex_prefix(format!("Local\\SecurePdfSuite.Test.Health.{}", std::process::id()));
        let policy = sample_policy(3, None);
        let hash = [0x42_u8; 32];

        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Fresh
        );
        // Inspecting must not create records.
        assert!(store.backend.read_primary(policy.doc_id).unwrap().is_none());

        store.initialize_or_verify(&policy, &hash).unwrap();
        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Consistent
        );

        // Diverging copies: secondary keeps an older record while primary advances.
        let stale = store.backend.snapshot_primary(policy.doc_id);
        store.consume_print_slot(&policy, &hash).unwrap();
        store.backend.overwrite_secondary(policy.doc_id, stale);
        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Inconsistent
        );
        store.initialize_or_verify(&policy, &hash).unwrap();
        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Consistent
        );

        store.backend.corrupt_secondary(policy.doc_id);
        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Inconsistent
        );
        store.initialize_or_verify(&policy, &hash).unwrap();
        store.backend.clear_primary(policy.doc_id);
        assert_eq!(
            store.inspect_store_health(&policy, &hash).unwrap(),
            QuotaStoreHealth::Inconsistent
        );
        // A different machine fingerprint invalidates both copies.
        store.initialize_or_verify(&policy, &hash).unwrap();
        assert_eq!(
            store.inspect_store_health(&policy, &[0x43_u8; 32]).unwrap(),
            QuotaStoreHealth::Inconsistent
        );
    }

    /// Upgrade check against *copies* of real v1.x quota records: every record in the copied
    /// directory (`SECUREPDF_QUOTA_COPY_DIR`) and copied HKCU subkey (`SECUREPDF_QUOTA_COPY_KEY`)
    /// must unprotect and pass the HMAC check with this machine's fingerprint. Read-only.
    #[test]
    #[ignore = "diagnostic; needs copies of real quota records, see README"]
    fn verify_copied_v1_quota_records_are_readable() {
        let dir = PathBuf::from(std::env::var_os("SECUREPDF_QUOTA_COPY_DIR").unwrap());
        let subkey = std::env::var("SECUREPDF_QUOTA_COPY_KEY").unwrap();
        assert!(!subkey.eq_ignore_ascii_case(r"Software\SecurePdfSuite\Quotas"));
        let backend = WindowsDualQuotaBackend::with_paths(dir.clone(), subkey);
        let store = LocalQuotaStore::new(backend, WindowsDpapiProtector);
        let (_, machine_hash) = crate::licensing::current_machine_identity().unwrap();

        let mut checked = 0_u32;
        let mut failures = Vec::new();
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let hex: Vec<u8> = (0..stem.len() / 2)
                .filter_map(|i| u8::from_str_radix(stem.get(i * 2..i * 2 + 2)?, 16).ok())
                .collect();
            let Ok(guid) = <[u8; 16]>::try_from(hex.as_slice()) else {
                continue;
            };
            let mut wire = guid;
            wire[..4].reverse();
            wire[4..6].reverse();
            wire[6..8].reverse();
            let doc_id = DocId::from_wire_bytes(wire);
            assert_eq!(doc_id.to_hex_n(), stem.to_ascii_lowercase());

            let primary = store.backend.read_primary(doc_id).unwrap();
            let secondary = store.backend.read_secondary(doc_id).unwrap();
            for (label, blob) in [("file", primary), ("registry", secondary)] {
                let Some(blob) = blob else {
                    failures.push(format!("{stem}: {label} copy missing"));
                    continue;
                };
                match store.unseal_record(&blob, doc_id, &machine_hash) {
                    Ok(rec) => println!(
                        "OK {label:8} used={} max={} seq={}",
                        rec.used_prints, rec.max_prints, rec.sequence_number
                    ),
                    Err(err) => failures.push(format!("{stem}: {label}: {err}")),
                }
            }
            checked += 1;
        }
        println!("CHECKED_RECORDS={checked} FAILURES={}", failures.len());
        assert!(checked > 0, "no quota record copies found");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn windows_dual_quota_backend_roundtrips_temp_dir_and_test_registry_key() {
        struct RegCleanup(String);
        impl Drop for RegCleanup {
            fn drop(&mut self) {
                windows_core::link!("advapi32.dll" "system" fn RegDeleteTreeW(
                    hkey: bindings::HKEY,
                    lpsubkey: PCWSTR,
                ) -> bindings::LSTATUS);
                let subkey_w = to_wide_null(&self.0);
                // SAFETY: Removes isolated per-PID unit test subkey under HKCU cleanly.
                unsafe {
                    let _ = RegDeleteTreeW(bindings::HKEY_CURRENT_USER, PCWSTR(subkey_w.as_ptr()));
                }
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let subkey = format!(r"Software\SecurePdfSuite\TestQuotas_{}", std::process::id());
        let _cleanup = RegCleanup(subkey.clone());
        let backend = WindowsDualQuotaBackend::with_paths(dir.path().to_path_buf(), subkey);
        let store = LocalQuotaStore::new(backend, WindowsDpapiProtector).with_mutex_prefix(
            format!("Local\\SecurePdfSuite.WinDual.{}", std::process::id()),
        );
        let policy = sample_policy(1, None);
        let hash = [0x12_u8; 32];
        let st = store.initialize_or_verify(&policy, &hash).unwrap();
        assert_eq!(st.remaining_prints, 1);
    }
}
