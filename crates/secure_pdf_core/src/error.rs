//! Structured error types for `secure_pdf_core`.

use std::fmt;

/// Convenient result alias using [`CoreError`].
pub type CoreResult<T> = Result<T, CoreError>;

/// Errors returned by `secure_pdf_core` operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum CoreError {
    /// Invalid caller-supplied argument or configuration.
    InvalidArgument(String),
    /// Filesystem or operating-system I/O error.
    Io(std::io::Error),
    /// Wrong verification code (AEAD authentication tag mismatch on key-wrap).
    WrongVerificationCode,
    /// Cryptography provider failure (CNG, DPAPI, RNG, scrypt).
    Crypto(String),
    /// Invalid or corrupted `.spdf` container structure or policy.
    Format(String),
    /// Quota record missing, corrupted, tampered, or rolled-back clock detected.
    QuotaTampered(String),
    /// Allowed print quota has been exhausted.
    QuotaExhausted,
    /// Timed access window for this document has expired.
    DocumentExpired,
    /// Inter-process quota mutex timed out.
    QuotaLockTimeout(String),
    /// Device fingerprint or network context inspection failure.
    Context(String),
    /// `PDFium` loading or rendering failure.
    Pdfium(String),
    /// Physical printer validation or GDI print pipeline failure.
    PrinterSecurity(String),
    /// Print job produced zero physical pages or was cancelled.
    PrintCancelledOrEmpty,
    /// The print pipeline failed after a print slot had been reserved.
    PrintFailed {
        /// User-facing description, including whether the print count was deducted.
        message: String,
        /// `true` when the reserved print slot stayed consumed (pages reached the printer or the
        /// rollback failed).
        quota_consumed: bool,
    },
    /// Watermark forensics image loading or enhancement failure.
    Forensics(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(msg)
            | Self::Crypto(msg)
            | Self::Format(msg)
            | Self::QuotaTampered(msg)
            | Self::QuotaLockTimeout(msg)
            | Self::Context(msg)
            | Self::Pdfium(msg)
            | Self::PrinterSecurity(msg)
            | Self::Forensics(msg) => f.write_str(msg),
            Self::PrintFailed { message, .. } => f.write_str(message),
            Self::Io(err) => write!(f, "I/O 错误：{err}"),
            Self::WrongVerificationCode => f.write_str("验证码错误，无法解密文档。"),
            Self::QuotaExhausted => f.write_str("打印配额已用尽。"),
            Self::DocumentExpired => f.write_str("文档授权有效期已到期。"),
            Self::PrintCancelledOrEmpty => f.write_str("打印任务已取消或未输出任何页面。"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
