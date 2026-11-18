//! Verification-code normalization, RFC 7914 `scrypt`, and Windows CNG primitives.

use std::mem::size_of;
use std::ptr::null_mut;

use zeroize::{Zeroize, Zeroizing};

use crate::bindings;
use crate::error::{CoreError, CoreResult};
use crate::win32_util::{
    BcryptAlgGuard, BcryptHashGuard, BcryptKeyGuard, check_ntstatus, to_wide_le_bytes,
};

/// 32-character unambiguous alphabet matching `SecurePdf.Core` (`ABCDEFGHJKLMNPQRSTUVWXYZ23456789`).
pub const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// AES-256 key size in bytes.
pub const KEY_SIZE: usize = 32;
/// AES-GCM nonce size in bytes.
pub const GCM_NONCE_SIZE: usize = 12;
/// AES-GCM authentication tag size in bytes.
pub const GCM_TAG_SIZE: usize = 16;
/// Scrypt salt size in bytes.
pub const SALT_SIZE: usize = 16;

/// Default RFC 7914 scrypt work factor `N` for new documents.
pub const DEFAULT_SCRYPT_N: i32 = 16_384;
/// Default RFC 7914 scrypt block size parameter `r`.
pub const DEFAULT_SCRYPT_R: i32 = 8;
/// Default RFC 7914 scrypt parallelization parameter `p`.
pub const DEFAULT_SCRYPT_P: i32 = 1;
/// Minimum allowed scrypt `N` accepted during document unlock.
pub const MIN_UNLOCK_SCRYPT_N: i32 = 1_024;

/// Maximum scrypt cost bounds matching `.spdf` v2 security caps.
const MAX_SCRYPT_N: i32 = 1_048_576;
const MAX_SCRYPT_R: i32 = 32;
const MAX_SCRYPT_P: i32 = 16;
const MAX_SCRYPT_NR: i64 = 1_048_576;
const MAX_SCRYPT_NRP: i64 = 4_194_304;

/// Fills `buf` with cryptographically secure random bytes via Windows CNG `BCryptGenRandom`.
///
/// # Errors
///
/// Returns [`CoreError::Crypto`] if CNG random generation fails or length overflows `u32`.
pub fn random_bytes(buf: &mut [u8]) -> CoreResult<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let len = u32::try_from(buf.len())
        .map_err(|_| CoreError::InvalidArgument("随机缓冲区长度超出 u32 范围。".to_owned()))?;
    let flags = u32::try_from(bindings::BCRYPT_USE_SYSTEM_PREFERRED_RNG)
        .map_err(|_| CoreError::Crypto("无效的 CNG RNG 标志。".to_owned()))?;
    // SAFETY: `buf` is valid for `len` writable bytes and preferred RNG uses null algorithm handle.
    let status = unsafe { bindings::BCryptGenRandom(None, buf.as_mut_ptr(), len, flags) };
    check_ntstatus("BCryptGenRandom", status)
}

/// Generates a grouped human-readable verification code (default: 3 groups x 4 chars `XXXX-XXXX-XXXX`).
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] if `groups == 0` or `group_len == 0`, or [`CoreError::Crypto`]
/// if CNG RNG fails.
pub fn generate_verification_code(groups: usize, group_len: usize) -> CoreResult<String> {
    if groups == 0 || group_len == 0 {
        return Err(CoreError::InvalidArgument(
            "验证码分组数与每组长度必须大于 0。".to_owned(),
        ));
    }
    let total = groups
        .checked_mul(group_len)
        .ok_or_else(|| CoreError::InvalidArgument("验证码总长度溢出。".to_owned()))?;
    let mut entropy = Zeroizing::new(vec![0_u8; total]);
    random_bytes(&mut entropy)?;

    let mut out = String::with_capacity(total + groups - 1);
    for (idx, byte) in entropy.iter().enumerate() {
        if idx > 0 && idx % group_len == 0 {
            out.push('-');
        }
        let ch = CODE_ALPHABET[usize::from(byte & 0x1F)];
        out.push(char::from(ch));
    }
    Ok(out)
}

/// Normalizes user-entered verification codes before KDF derivation.
///
/// Converts full-width ASCII (`\u{FF01}`..`\u{FF5E}`) to half-width, strips whitespace,
/// underscores, hyphens, and Unicode dash punctuation (`U+2010`..`U+2015`, `U+2212`),
/// and converts ASCII letters to uppercase.
#[must_use]
pub fn normalize_verification_code(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    for raw in code.chars() {
        let ch = if ('\u{FF01}'..='\u{FF5E}').contains(&raw) {
            char::from_u32((raw as u32) - 0xFEE0).unwrap_or(raw)
        } else {
            raw
        };
        if ch.is_whitespace()
            || matches!(
                ch,
                '-' | '\u{3000}'
                    | '\u{2010}'
                    | '\u{2011}'
                    | '\u{2012}'
                    | '\u{2013}'
                    | '\u{2014}'
                    | '\u{2015}'
                    | '\u{2212}'
                    | '_'
            )
        {
            continue;
        }
        if ch.is_ascii() {
            out.push(ch.to_ascii_uppercase());
        } else {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
        }
    }
    out
}

/// Validates RFC 7914 `scrypt` parameters against bounded `DoS` & arithmetic-overflow guardrails.
#[must_use]
pub const fn is_acceptable_scrypt_cost(n: i32, r: i32, p: i32) -> bool {
    if n < 2 || n > MAX_SCRYPT_N || (n & (n - 1)) != 0 {
        return false;
    }
    if r < 1 || r > MAX_SCRYPT_R || p < 1 || p > MAX_SCRYPT_P {
        return false;
    }
    let nr = (n as i64) * (r as i64);
    let nrp = nr * (p as i64);
    nr <= MAX_SCRYPT_NR && nrp <= MAX_SCRYPT_NRP
}

/// Computes SHA-256 digest of `data` via Windows CNG.
///
/// # Errors
///
/// Returns [`CoreError::Crypto`] if CNG hashing fails.
pub fn sha256(data: &[u8]) -> CoreResult<[u8; 32]> {
    cng_hash_impl(data, None)
}

/// Computes HMAC-SHA-256 of `data` under `key` via Windows CNG.
///
/// # Errors
///
/// Returns [`CoreError::Crypto`] if CNG HMAC fails.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> CoreResult<[u8; 32]> {
    cng_hash_impl(data, Some(key))
}

fn cng_hash_impl(data: &[u8], hmac_key: Option<&[u8]>) -> CoreResult<[u8; 32]> {
    let mut alg_handle = bindings::BCRYPT_ALG_HANDLE(null_mut());
    let flags = if hmac_key.is_some() {
        u32::try_from(bindings::BCRYPT_ALG_HANDLE_HMAC_FLAG).unwrap_or(8)
    } else {
        0_u32
    };
    // SAFETY: Standard CNG provider open for SHA256 (with optional HMAC flag).
    let status = unsafe {
        bindings::BCryptOpenAlgorithmProvider(
            &raw mut alg_handle,
            bindings::BCRYPT_SHA256_ALGORITHM,
            windows_core::PCWSTR::null(),
            flags,
        )
    };
    check_ntstatus("BCryptOpenAlgorithmProvider(SHA256)", status)?;
    let _alg = BcryptAlgGuard(alg_handle);

    let obj_len = get_cng_u32_property(alg_handle, bindings::BCRYPT_OBJECT_LENGTH)?;
    let mut hash_obj = Zeroizing::new(vec![0_u8; usize::try_from(obj_len).unwrap_or(0)]);

    let mut hash_handle = bindings::BCRYPT_HASH_HANDLE(null_mut());
    // SAFETY: `hash_obj` stays alive for the lifetime of `hash_handle` and matches `obj_len`.
    let create_status = unsafe {
        bindings::BCryptCreateHash(
            alg_handle,
            &raw mut hash_handle,
            Some(hash_obj.as_mut_ptr()),
            obj_len,
            hmac_key,
            0,
        )
    };
    check_ntstatus("BCryptCreateHash", create_status)?;
    let _hash = BcryptHashGuard(hash_handle);

    if !data.is_empty() {
        // SAFETY: Valid active CNG hash handle and readable `data` slice.
        let data_status = unsafe { bindings::BCryptHashData(hash_handle, data, 0) };
        check_ntstatus("BCryptHashData", data_status)?;
    }

    let mut digest = [0_u8; 32];
    // SAFETY: SHA-256 produces exactly 32 bytes into `digest`.
    let finish_status =
        unsafe { bindings::BCryptFinishHash(hash_handle, digest.as_mut_ptr(), 32, 0) };
    check_ntstatus("BCryptFinishHash", finish_status)?;
    Ok(digest)
}

fn get_cng_u32_property(
    alg: bindings::BCRYPT_ALG_HANDLE,
    prop: windows_core::PCWSTR,
) -> CoreResult<u32> {
    let mut bytes = [0_u8; 4];
    let mut written = 0_u32;
    // SAFETY: Queries a 4-byte `u32` CNG property into `bytes`.
    let status = unsafe {
        bindings::BCryptGetProperty(
            bindings::BCRYPT_HANDLE(alg.0),
            prop,
            Some(bytes.as_mut_ptr()),
            4,
            &raw mut written,
            0,
        )
    };
    check_ntstatus("BCryptGetProperty", status)?;
    if written != 4 {
        return Err(CoreError::Crypto(format!(
            "CNG 属性长度异常：预期 4 字节，实际 {written}"
        )));
    }
    Ok(u32::from_le_bytes(bytes))
}

fn pbkdf2_hmac_sha256(
    password: &[u8],
    salt: &[u8],
    iterations: u64,
    out: &mut [u8],
) -> CoreResult<()> {
    let mut prf_handle = bindings::BCRYPT_ALG_HANDLE(null_mut());
    let hmac_flag = u32::try_from(bindings::BCRYPT_ALG_HANDLE_HMAC_FLAG).unwrap_or(8);
    // SAFETY: Opens CNG SHA-256 provider with HMAC flag for PBKDF2-HMAC-SHA256.
    let status = unsafe {
        bindings::BCryptOpenAlgorithmProvider(
            &raw mut prf_handle,
            bindings::BCRYPT_SHA256_ALGORITHM,
            windows_core::PCWSTR::null(),
            hmac_flag,
        )
    };
    check_ntstatus("BCryptOpenAlgorithmProvider(PBKDF2)", status)?;
    let _alg = BcryptAlgGuard(prf_handle);

    let out_len = u32::try_from(out.len())
        .map_err(|_| CoreError::InvalidArgument("PBKDF2 输出长度溢出。".to_owned()))?;
    // SAFETY: Valid PRF algorithm handle, readable slices, and writable `out` slice of `out_len` bytes.
    let derive_status = unsafe {
        bindings::BCryptDeriveKeyPBKDF2(
            prf_handle,
            Some(password),
            Some(salt),
            iterations,
            out.as_mut_ptr(),
            out_len,
            0,
        )
    };
    check_ntstatus("BCryptDeriveKeyPBKDF2", derive_status)
}

/// Derives a key using RFC 7914 `scrypt` on the normalized `verification_code`.
///
/// # Errors
///
/// Returns [`CoreError::InvalidArgument`] for empty codes or invalid cost parameters, or
/// [`CoreError::Crypto`] if CNG PBKDF2 fails.
pub fn derive_key_scrypt(
    verification_code: &str,
    salt: &[u8],
    n: i32,
    r: i32,
    p: i32,
    dk_len: usize,
) -> CoreResult<Zeroizing<Vec<u8>>> {
    let normalized = Zeroizing::new(normalize_verification_code(verification_code));
    if normalized.is_empty() {
        return Err(CoreError::InvalidArgument("验证码不能为空。".to_owned()));
    }
    if salt.is_empty() {
        return Err(CoreError::InvalidArgument(
            "scrypt salt 不能为空。".to_owned(),
        ));
    }
    scrypt_raw(normalized.as_bytes(), salt, n, r, p, dk_len)
}

/// RFC 7914 `scrypt` over raw password bytes (no verification-code normalization).
fn scrypt_raw(
    password: &[u8],
    salt: &[u8],
    n: i32,
    r: i32,
    p: i32,
    dk_len: usize,
) -> CoreResult<Zeroizing<Vec<u8>>> {
    if !is_acceptable_scrypt_cost(n, r, p) {
        return Err(CoreError::InvalidArgument(
            "非法的 scrypt 开销参数。".to_owned(),
        ));
    }
    if dk_len == 0 || dk_len > 1024 {
        return Err(CoreError::InvalidArgument(
            "非法的派生密钥长度。".to_owned(),
        ));
    }

    let n_usize =
        usize::try_from(n).map_err(|_| CoreError::InvalidArgument("scrypt N 溢出。".to_owned()))?;
    let r_usize =
        usize::try_from(r).map_err(|_| CoreError::InvalidArgument("scrypt r 溢出。".to_owned()))?;
    let p_usize =
        usize::try_from(p).map_err(|_| CoreError::InvalidArgument("scrypt p 溢出。".to_owned()))?;

    let block_bytes = 128_usize
        .checked_mul(r_usize)
        .ok_or_else(|| CoreError::InvalidArgument("scrypt 块长度溢出。".to_owned()))?;
    let mf_len = block_bytes
        .checked_mul(p_usize)
        .ok_or_else(|| CoreError::InvalidArgument("scrypt 总缓冲区长度溢出。".to_owned()))?;

    let mut b_buf = Zeroizing::new(vec![0_u8; mf_len]);
    pbkdf2_hmac_sha256(password, salt, 1, &mut b_buf)?;

    let words_per_block = block_bytes / 4;
    let total_v_words = n_usize
        .checked_mul(words_per_block)
        .ok_or_else(|| CoreError::InvalidArgument("scrypt ROMix 工作集溢出。".to_owned()))?;
    let mut v_scratch = Zeroizing::new(vec![0_u32; total_v_words]);
    let mut block_words = Zeroizing::new(vec![0_u32; words_per_block]);
    let mut xy_scratch = Zeroizing::new(vec![0_u32; words_per_block * 2]);

    for chunk in b_buf.chunks_exact_mut(block_bytes) {
        for (dst, src) in block_words.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *dst = u32::from_le_bytes(*src);
        }
        scrypt_ro_mix(
            &mut block_words,
            r_usize,
            n_usize,
            &mut v_scratch,
            &mut xy_scratch,
        );
        for (dst, &src) in chunk
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(block_words.iter())
        {
            *dst = src.to_le_bytes();
        }
    }

    let mut derived = Zeroizing::new(vec![0_u8; dk_len]);
    pbkdf2_hmac_sha256(password, &b_buf, 1, &mut derived)?;
    Ok(derived)
}

fn scrypt_ro_mix(
    block: &mut [u32],
    r: usize,
    n: usize,
    v_scratch: &mut [u32],
    xy_scratch: &mut [u32],
) {
    let words = block.len();
    let (x_words, y_words) = xy_scratch.split_at_mut(words);
    x_words.copy_from_slice(block);

    for i in 0..n {
        let start = i * words;
        v_scratch[start..start + words].copy_from_slice(x_words);
        scrypt_block_mix(x_words, y_words, r);
    }

    let mask = n - 1;
    let last_half_offset = (2 * r - 1) * 16;
    for _ in 0..n {
        let j = usize::try_from(x_words[last_half_offset]).unwrap_or(0) & mask;
        let v_slice = &v_scratch[j * words..(j + 1) * words];
        for (dst, &src) in x_words.iter_mut().zip(v_slice.iter()) {
            *dst ^= src;
        }
        scrypt_block_mix(x_words, y_words, r);
    }

    block.copy_from_slice(x_words);
}

fn scrypt_block_mix(b: &mut [u32], y: &mut [u32], r: usize) {
    let mut x = [0_u32; 16];
    x.copy_from_slice(&b[(2 * r - 1) * 16..(2 * r) * 16]);

    let half_r_offset = r * 16;
    for i in 0..(2 * r) {
        let src = &b[i * 16..(i + 1) * 16];
        for (xw, &sw) in x.iter_mut().zip(src.iter()) {
            *xw ^= sw;
        }
        salsa20_8(&mut x);

        let dst_start = if (i & 1) == 0 {
            (i / 2) * 16
        } else {
            half_r_offset + (i / 2) * 16
        };
        y[dst_start..dst_start + 16].copy_from_slice(&x);
    }

    b.copy_from_slice(y);
    x.zeroize();
}

fn salsa20_8(state: &mut [u32; 16]) {
    let mut x = *state;
    for _ in 0..4 {
        // Column round
        quarter_round(&mut x, 0, 4, 8, 12);
        quarter_round(&mut x, 5, 9, 13, 1);
        quarter_round(&mut x, 10, 14, 2, 6);
        quarter_round(&mut x, 15, 3, 7, 11);
        // Row round
        quarter_round(&mut x, 0, 1, 2, 3);
        quarter_round(&mut x, 5, 6, 7, 4);
        quarter_round(&mut x, 10, 11, 8, 9);
        quarter_round(&mut x, 15, 12, 13, 14);
    }
    for (dst, &src) in state.iter_mut().zip(x.iter()) {
        *dst = dst.wrapping_add(src);
    }
    x.zeroize();
}

#[inline]
const fn quarter_round(x: &mut [u32; 16], i0: usize, i1: usize, i2: usize, i3: usize) {
    x[i1] ^= x[i0].wrapping_add(x[i3]).rotate_left(7);
    x[i2] ^= x[i1].wrapping_add(x[i0]).rotate_left(9);
    x[i3] ^= x[i2].wrapping_add(x[i1]).rotate_left(13);
    x[i0] ^= x[i3].wrapping_add(x[i2]).rotate_left(18);
}

fn open_aes_gcm_key(
    key: &[u8; KEY_SIZE],
) -> CoreResult<(BcryptAlgGuard, BcryptKeyGuard, Zeroizing<Vec<u8>>)> {
    let mut alg_handle = bindings::BCRYPT_ALG_HANDLE(null_mut());
    // SAFETY: Standard CNG AES provider handle acquisition.
    let status = unsafe {
        bindings::BCryptOpenAlgorithmProvider(
            &raw mut alg_handle,
            bindings::BCRYPT_AES_ALGORITHM,
            None,
            0,
        )
    };
    check_ntstatus("BCryptOpenAlgorithmProvider(AES)", status)?;
    let alg = BcryptAlgGuard(alg_handle);

    let gcm_mode = to_wide_le_bytes("ChainingModeGCM");
    // SAFETY: Sets chaining mode property to GCM on the opened AES algorithm provider.
    let prop_status = unsafe {
        bindings::BCryptSetProperty(
            bindings::BCRYPT_HANDLE(alg.0.0),
            bindings::BCRYPT_CHAINING_MODE,
            &gcm_mode,
            0,
        )
    };
    check_ntstatus("BCryptSetProperty(GCM)", prop_status)?;

    let obj_len = get_cng_u32_property(alg.0, bindings::BCRYPT_OBJECT_LENGTH)?;
    let mut key_obj = Zeroizing::new(vec![0_u8; usize::try_from(obj_len).unwrap_or(0)]);
    let mut key_handle = bindings::BCRYPT_KEY_HANDLE(null_mut());
    // SAFETY: `key_obj` stays alive alongside the returned `BcryptKeyGuard`.
    let key_status = unsafe {
        bindings::BCryptGenerateSymmetricKey(
            alg.0,
            &raw mut key_handle,
            Some(key_obj.as_mut_ptr()),
            obj_len,
            key,
            0,
        )
    };
    check_ntstatus("BCryptGenerateSymmetricKey", key_status)?;
    Ok((alg, BcryptKeyGuard(key_handle), key_obj))
}

fn build_auth_info(
    nonce: &[u8; GCM_NONCE_SIZE],
    aad: &[u8],
    tag: &mut [u8; GCM_TAG_SIZE],
) -> CoreResult<bindings::BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO> {
    let cb_size = u32::try_from(size_of::<bindings::BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>())
        .map_err(|_| CoreError::Crypto("GCM 结构体大小溢出。".to_owned()))?;
    let info_version = u32::try_from(bindings::BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION)
        .map_err(|_| CoreError::Crypto("GCM 结构体版本异常。".to_owned()))?;
    let cb_nonce = u32::try_from(nonce.len())
        .map_err(|_| CoreError::Crypto("GCM nonce 长度溢出。".to_owned()))?;
    let cb_auth = u32::try_from(aad.len())
        .map_err(|_| CoreError::InvalidArgument("AAD 长度超出 u32 范围。".to_owned()))?;
    let cb_tag =
        u32::try_from(tag.len()).map_err(|_| CoreError::Crypto("GCM tag 长度溢出。".to_owned()))?;

    Ok(bindings::BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: cb_size,
        dwInfoVersion: info_version,
        pbNonce: nonce.as_ptr().cast_mut(),
        cbNonce: cb_nonce,
        pbAuthData: if aad.is_empty() {
            null_mut()
        } else {
            aad.as_ptr().cast_mut()
        },
        cbAuthData: cb_auth,
        pbTag: tag.as_mut_ptr(),
        cbTag: cb_tag,
        pbMacContext: null_mut(),
        cbMacContext: 0,
        cbAAD: 0,
        cbData: 0,
        dwFlags: 0,
    })
}

/// Encrypts `plaintext` with AES-256-GCM returning `ciphertext || tag(16)`.
///
/// # Errors
///
/// Returns [`CoreError::Crypto`] or [`CoreError::InvalidArgument`] if CNG encryption fails or buffer
/// sizes exceed `u32`.
pub fn aes_256_gcm_encrypt(
    key: &[u8; KEY_SIZE],
    nonce: &[u8; GCM_NONCE_SIZE],
    plaintext: &[u8],
    aad: &[u8],
) -> CoreResult<Vec<u8>> {
    let pt_len_u32 = u32::try_from(plaintext.len())
        .map_err(|_| CoreError::InvalidArgument("加密数据超出单块 32 位限制。".to_owned()))?;
    let (_alg, hkey, _key_obj) = open_aes_gcm_key(key)?;
    let mut tag = [0_u8; GCM_TAG_SIZE];
    let mut auth_info = build_auth_info(nonce, aad, &mut tag)?;

    let mut out = vec![0_u8; plaintext.len() + GCM_TAG_SIZE];
    let mut written = 0_u32;
    let cipher_ptr = if plaintext.is_empty() {
        null_mut()
    } else {
        out.as_mut_ptr()
    };

    // SAFETY: Valid AES-GCM key handle, auth_info points to pinned nonce/aad/tag for this call,
    // and `out` has capacity for `plaintext.len()` bytes before `tag`.
    let status = unsafe {
        bindings::BCryptEncrypt(
            hkey.0,
            if plaintext.is_empty() {
                None
            } else {
                Some(plaintext)
            },
            Some((&raw mut auth_info).cast()),
            None,
            if plaintext.is_empty() {
                None
            } else {
                Some(cipher_ptr)
            },
            pt_len_u32,
            &raw mut written,
            0,
        )
    };
    check_ntstatus("BCryptEncrypt(AES-GCM)", status)?;
    let written_usize = usize::try_from(written).unwrap_or(0);
    if written_usize != plaintext.len() {
        return Err(CoreError::Crypto(format!(
            "AES-GCM 密文长度不一致：预期 {}，实际 {written_usize}",
            plaintext.len()
        )));
    }
    out[plaintext.len()..].copy_from_slice(&tag);
    Ok(out)
}

/// Decrypts `cipher_and_tag` (`ciphertext || tag(16)`) with AES-256-GCM.
///
/// # Errors
///
/// Returns [`CoreError::WrongVerificationCode`] on AEAD authentication tag mismatch
/// (`STATUS_AUTH_TAG_MISMATCH`), or [`CoreError::Format`] / [`CoreError::Crypto`] for malformed input.
pub fn aes_256_gcm_decrypt(
    key: &[u8; KEY_SIZE],
    nonce: &[u8; GCM_NONCE_SIZE],
    cipher_and_tag: &[u8],
    aad: &[u8],
) -> CoreResult<Zeroizing<Vec<u8>>> {
    if cipher_and_tag.len() < GCM_TAG_SIZE {
        return Err(CoreError::Format(
            "密文长度不足 16 字节认证标签。".to_owned(),
        ));
    }
    let split = cipher_and_tag.len() - GCM_TAG_SIZE;
    let ciphertext = &cipher_and_tag[..split];
    let ct_len_u32 = u32::try_from(ciphertext.len())
        .map_err(|_| CoreError::InvalidArgument("密文长度超出 32 位限制。".to_owned()))?;

    let mut tag = [0_u8; GCM_TAG_SIZE];
    tag.copy_from_slice(&cipher_and_tag[split..]);

    let (_alg, hkey, _key_obj) = open_aes_gcm_key(key)?;
    let mut auth_info = build_auth_info(nonce, aad, &mut tag)?;

    let mut plain = Zeroizing::new(vec![0_u8; ciphertext.len()]);
    let mut written = 0_u32;

    // SAFETY: Valid AES-GCM key handle, auth_info references valid nonce/aad/tag for this call,
    // and `plain` has capacity matching `ciphertext.len()`.
    let status = unsafe {
        bindings::BCryptDecrypt(
            hkey.0,
            if ciphertext.is_empty() {
                None
            } else {
                Some(ciphertext)
            },
            Some((&raw mut auth_info).cast()),
            None,
            if ciphertext.is_empty() {
                None
            } else {
                Some(plain.as_mut_ptr())
            },
            ct_len_u32,
            &raw mut written,
            0,
        )
    };
    if status == bindings::STATUS_AUTH_TAG_MISMATCH {
        return Err(CoreError::WrongVerificationCode);
    }
    check_ntstatus("BCryptDecrypt(AES-GCM)", status)?;
    let written_usize = usize::try_from(written).unwrap_or(0);
    if written_usize != ciphertext.len() {
        return Err(CoreError::Crypto(format!(
            "AES-GCM 明文长度不一致：预期 {}，实际 {written_usize}",
            ciphertext.len()
        )));
    }
    Ok(plain)
}

/// Constant-time comparison of two equal-role slices.
#[must_use]
pub fn constant_time_equals(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_separators_fullwidth_and_lowercases() {
        assert_eq!(
            normalize_verification_code("ab23-cd45-ef67"),
            "AB23CD45EF67"
        );
        assert_eq!(
            normalize_verification_code(" \u{3000}ａｂ２３—ｃｄ４５＿ｅｆ６７ "),
            "AB23CD45EF67"
        );
        assert_eq!(normalize_verification_code("a\u{2013}b\u{2212}c"), "ABC");
    }

    #[test]
    fn generate_code_format_and_alphabet() {
        let code = generate_verification_code(3, 4).unwrap();
        assert_eq!(code.len(), 14);
        assert_eq!(code.chars().nth(4), Some('-'));
        assert_eq!(code.chars().nth(9), Some('-'));
        for ch in code.chars().filter(|&c| c != '-') {
            assert!(CODE_ALPHABET.contains(&(ch as u8)));
        }
    }

    #[test]
    fn scrypt_cost_bounds_enforce_dos_limits() {
        assert!(is_acceptable_scrypt_cost(16_384, 8, 1));
        assert!(is_acceptable_scrypt_cost(1_024, 1, 1));
        assert!(!is_acceptable_scrypt_cost(1, 8, 1));
        assert!(!is_acceptable_scrypt_cost(16_384 - 1, 8, 1));
        assert!(!is_acceptable_scrypt_cost(1_048_576, 8, 1)); // N*r exceeds 1 MiB*1024 blocks
        assert!(!is_acceptable_scrypt_cost(16_384, 33, 1));
        assert!(!is_acceptable_scrypt_cost(16_384, 8, 17));
    }

    #[test]
    fn scrypt_normalizes_verification_codes_before_derivation() {
        let salt = b"NaCl";
        let dk1 = derive_key_scrypt("password", salt, 1_024, 8, 1, 32).unwrap();
        let dk2 = derive_key_scrypt("ｐａｓｓ－ｗｏｒｄ", salt, 1_024, 8, 1, 32).unwrap();
        assert_eq!(dk1.as_slice(), dk2.as_slice());
        // The public API hashes the normalized (upper-case) code, i.e. raw "PASSWORD".
        let raw = scrypt_raw(b"PASSWORD", salt, 1_024, 8, 1, 32).unwrap();
        assert_eq!(dk1.as_slice(), raw.as_slice());
        let dk_other = derive_key_scrypt("OTHER", salt, 1_024, 8, 1, 32).unwrap();
        assert_ne!(dk1.as_slice(), dk_other.as_slice());
    }

    #[test]
    fn scrypt_matches_rfc7914_section_12_known_answers() {
        // RFC 7914 section 12, test vectors 2 and 3 (64-byte outputs).
        let v2 = scrypt_raw(b"password", b"NaCl", 1_024, 8, 16, 64).unwrap();
        assert_eq!(
            hex_encode(&v2),
            "fdbabe1c9d3472007856e7190d01e9fe7c6ad7cbc8237830e77376634b373162\
             2eaf30d92e22a3886ff109279d9830dac727afb94a83ee6d8360cbdfa2cc0640"
        );
        let v3 = scrypt_raw(b"pleaseletmein", b"SodiumChloride", 16_384, 8, 1, 64).unwrap();
        assert_eq!(
            hex_encode(&v3),
            "7023bdcb3afd7348461c06cd81fd38ebfda8fbba904f8e3ea9b543f6545da1f2\
             d5432955613f0fcf62d49705242a9af9e61e85dc0d651e40dfcf017b45575887"
        );
    }

    // Known answers produced by the v1.1.0 `SecurePdf.Core.dll`
    // (`CodeAndCryptoService.DeriveKeyScrypt` / `EncryptAesGcm` / `ComputeHmacSha256`).
    #[test]
    fn kdf_gcm_and_hmac_match_v1_core_library_known_answers() {
        let salt: Vec<u8> = (0_u8..16).collect();
        let key = derive_key_scrypt(" abcd-efgh jklm ", &salt, 16_384, 8, 1, 32).unwrap();
        assert_eq!(
            hex_encode(&key),
            "d614c2fa1b617a857e5030ae1ab62a51c22ca89480d7178aa0e3d1056f50decb"
        );

        let key_arr: [u8; KEY_SIZE] = key.as_slice().try_into().unwrap();
        let nonce: [u8; GCM_NONCE_SIZE] = std::array::from_fn(|i| 100 + u8::try_from(i).unwrap());
        let aad = b"SPDF-AAD-HEADER";
        let plaintext = "SecurePdf v1 known answer 明文".as_bytes();
        let sealed = aes_256_gcm_encrypt(&key_arr, &nonce, plaintext, aad).unwrap();
        assert_eq!(
            hex_encode(&sealed),
            "6d25507cc9147563d498ba0a3eb225f1addaedd011218225dda3a62f9aaffda2\
             965a2d8c75c28996cbfe4e343d09c7ff"
        );
        let opened = aes_256_gcm_decrypt(&key_arr, &nonce, &sealed, aad).unwrap();
        assert_eq!(opened.as_slice(), plaintext);

        let mac = hmac_sha256(&key, aad).unwrap();
        assert_eq!(
            hex_encode(&mac),
            "454ea04ec42d7d59c6ea4cfe53f10e7c45096b0709a4fb697e093fc486b0681a"
        );
    }

    #[test]
    fn aes_gcm_roundtrip_aad_and_tamper_detection() {
        let mut key = [0_u8; KEY_SIZE];
        random_bytes(&mut key).unwrap();
        let mut nonce = [0_u8; GCM_NONCE_SIZE];
        random_bytes(&mut nonce).unwrap();

        let pt = b"Confidential SecurePdfSuite v2 payload";
        let aad = b"SPDF_AAD_HEADER_V2";

        let ct = aes_256_gcm_encrypt(&key, &nonce, pt, aad).unwrap();
        assert_eq!(ct.len(), pt.len() + GCM_TAG_SIZE);

        let dec = aes_256_gcm_decrypt(&key, &nonce, &ct, aad).unwrap();
        assert_eq!(dec.as_slice(), pt);

        // Wrong AAD must reject with WrongVerificationCode
        assert!(matches!(
            aes_256_gcm_decrypt(&key, &nonce, &ct, b"WRONG_AAD"),
            Err(CoreError::WrongVerificationCode)
        ));

        // Corrupted ciphertext byte must reject
        let mut tampered = ct;
        tampered[0] ^= 0x01;
        assert!(matches!(
            aes_256_gcm_decrypt(&key, &nonce, &tampered, aad),
            Err(CoreError::WrongVerificationCode)
        ));

        // Empty plaintext must roundtrip cleanly too
        let empty_cipher = aes_256_gcm_encrypt(&key, &nonce, b"", aad).unwrap();
        assert_eq!(empty_cipher.len(), GCM_TAG_SIZE);
        let roundtrip_plain = aes_256_gcm_decrypt(&key, &nonce, &empty_cipher, aad).unwrap();
        assert!(roundtrip_plain.is_empty());
    }

    #[test]
    fn sha256_and_hmac_sha256_match_rfc_vectors() {
        let empty_digest = sha256(b"").unwrap();
        assert_eq!(
            hex_encode(&empty_digest),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // RFC 4231 Test Case 2: key = "Jefe", data = "what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?").unwrap();
        assert_eq!(
            hex_encode(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    fn hex_encode(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            write!(&mut out, "{b:02x}").unwrap();
        }
        out
    }
}
