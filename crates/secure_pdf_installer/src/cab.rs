//! Windows `cabinet.dll` native `LZMS` (`16 MiB` block) compressor and decompressor wrappers.

use core::ffi::c_void;
use core::mem::size_of;
use core::ptr::null_mut;

use crate::bindings;

/// Maximum allowed decompressed payload size (`512 MiB`) to prevent allocation bombs.
pub const MAX_DECOMPRESSED_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;
const LZMS_BLOCK_SIZE_BYTES: u32 = 16 * 1024 * 1024;

struct CompressorGuard(bindings::COMPRESSOR_HANDLE);

impl Drop for CompressorGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid compressor handle created by `CreateCompressor`.
            let _ = unsafe { bindings::CloseCompressor(self.0) };
            self.0 = bindings::COMPRESSOR_HANDLE(null_mut());
        }
    }
}

struct DecompressorGuard(bindings::DECOMPRESSOR_HANDLE);

impl Drop for DecompressorGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: Valid decompressor handle created by `CreateDecompressor`.
            let _ = unsafe { bindings::CloseDecompressor(self.0) };
            self.0 = bindings::COMPRESSOR_HANDLE(null_mut());
        }
    }
}

/// Compresses `raw` using Windows `cabinet.dll` `LZMS` with a `16 MiB` block size hint.
///
/// # Errors
///
/// Returns a descriptive error string if Windows compression fails.
pub fn compress_lzms(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let algo = u32::try_from(bindings::COMPRESS_ALGORITHM_LZMS).unwrap_or(5);
    let mut handle = bindings::COMPRESSOR_HANDLE(null_mut());
    // SAFETY: `handle` receives an initialized `COMPRESSOR_HANDLE` on success.
    let created = unsafe { bindings::CreateCompressor(algo, None, &raw mut handle) };
    if !created.as_bool() || handle.0.is_null() {
        return Err(format!(
            "CreateCompressor(LZMS) 失败（错误码 {}）。",
            last_err()
        ));
    }
    let guard = CompressorGuard(handle);

    let block_size = LZMS_BLOCK_SIZE_BYTES;
    // SAFETY: Sets the LZMS block size from a live `u32`; the size argument matches its type.
    let configured = unsafe {
        bindings::SetCompressorInformation(
            guard.0,
            bindings::COMPRESS_INFORMATION_CLASS_BLOCK_SIZE,
            (&raw const block_size).cast::<c_void>(),
            size_of::<u32>(),
        )
    };
    if !configured.as_bool() {
        return Err(format!(
            "SetCompressorInformation(块大小 {LZMS_BLOCK_SIZE_BYTES}) 失败（错误码 {}）。",
            last_err()
        ));
    }

    let mut needed: usize = 0;
    // SAFETY: Querying required compressed buffer length (`compressedbuffer = None`).
    let _ = unsafe {
        bindings::Compress(
            guard.0,
            Some(raw.as_ptr().cast::<c_void>()),
            raw.len(),
            None,
            0,
            &raw mut needed,
        )
    };
    if needed == 0 {
        return Err(format!(
            "Compress 查询缓冲区长度失败（错误码 {}）。",
            last_err()
        ));
    }

    let mut out = vec![0_u8; needed];
    let mut written: usize = 0;
    // SAFETY: Compresses `raw` into owned `out` buffer sized per query above.
    let ok = unsafe {
        bindings::Compress(
            guard.0,
            Some(raw.as_ptr().cast::<c_void>()),
            raw.len(),
            Some(out.as_mut_ptr().cast::<c_void>()),
            out.len(),
            &raw mut written,
        )
    };
    if !ok.as_bool() || written == 0 {
        return Err(format!(
            "Compress(LZMS) 执行失败（错误码 {}）。",
            last_err()
        ));
    }
    out.truncate(written);
    Ok(out)
}

/// Decompresses `LZMS` payload `compressed` into an exact buffer of `expected_len` bytes.
///
/// # Errors
///
/// Returns an error if `expected_len` exceeds bounds, Windows decompression fails, or size mismatches.
pub fn decompress_lzms(compressed: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    if expected_len > MAX_DECOMPRESSED_PAYLOAD_BYTES {
        return Err(format!(
            "安装包解压体积超出 512 MiB 安全上限（{expected_len} 字节）。"
        ));
    }
    if expected_len == 0 && compressed.is_empty() {
        return Ok(Vec::new());
    }
    if compressed.is_empty() {
        return Err("压缩载荷为空，但期望解压长度非零。".to_owned());
    }

    let algo = u32::try_from(bindings::COMPRESS_ALGORITHM_LZMS).unwrap_or(5);
    let mut handle = bindings::COMPRESSOR_HANDLE(null_mut());
    // SAFETY: `handle` receives an initialized `DECOMPRESSOR_HANDLE` on success.
    let created = unsafe { bindings::CreateDecompressor(algo, None, &raw mut handle) };
    if !created.as_bool() || handle.0.is_null() {
        return Err(format!(
            "CreateDecompressor(LZMS) 失败（错误码 {}）。",
            last_err()
        ));
    }
    let guard = DecompressorGuard(handle);

    let mut out = vec![0_u8; expected_len];
    let mut actual: usize = 0;
    // SAFETY: `out` is sized cleanly to `expected_len`; `Decompress` validates output bounds.
    let ok = unsafe {
        bindings::Decompress(
            guard.0,
            Some(compressed.as_ptr().cast::<c_void>()),
            compressed.len(),
            Some(out.as_mut_ptr().cast::<c_void>()),
            out.len(),
            Some(&raw mut actual),
        )
    };
    if !ok.as_bool() || actual != expected_len {
        return Err(format!(
            "Decompress(LZMS) 失败（预期 {expected_len} 字节，实际 {actual} 字节，错误码 {}）。",
            last_err()
        ));
    }
    Ok(out)
}

fn last_err() -> u32 {
    // SAFETY: Thread-local Win32 `GetLastError`.
    unsafe { bindings::GetLastError() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzms_roundtrips_and_actually_compresses_redundant_data() {
        let raw: Vec<u8> = (0..200_000_u32)
            .map(|i| u8::try_from(i % 7).unwrap_or(0))
            .collect();
        let packed = compress_lzms(&raw).unwrap();
        assert!(
            packed.len() < raw.len() / 10,
            "packed {} bytes",
            packed.len()
        );
        assert_eq!(decompress_lzms(&packed, raw.len()).unwrap(), raw);
        assert_eq!(compress_lzms(&[]).unwrap(), Vec::<u8>::new());
        assert_eq!(decompress_lzms(&[], 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decompression_rejects_wrong_length_truncation_and_oversized_claims() {
        let raw = vec![42_u8; 50_000];
        let packed = compress_lzms(&raw).unwrap();
        assert!(decompress_lzms(&packed, raw.len() + 1).is_err());
        assert!(decompress_lzms(&packed, raw.len() - 1).is_err());
        assert!(decompress_lzms(&packed[..packed.len() / 2], raw.len()).is_err());
        assert!(decompress_lzms(&[], 10).is_err());
        assert!(decompress_lzms(&packed, MAX_DECOMPRESSED_PAYLOAD_BYTES + 1).is_err());
    }
}
