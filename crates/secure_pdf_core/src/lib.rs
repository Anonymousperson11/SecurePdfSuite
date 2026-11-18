//! Core security, container, quota, PDF rendering, printer guard, and watermark forensics library.
//!
//! This crate forms the `rustcore` backend for `SecurePdfSuite`, completely decoupled from UI
//! frameworks while relying strictly on Win32 CNG, DPAPI, Registry, WMI, GDI, and in-memory `PDFium`.

#[allow(
    dead_code,
    elided_lifetimes_in_paths,
    missing_docs,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_qualifications,
    warnings,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
mod bindings;
pub mod crypto;
pub mod error;
pub mod forensics;
pub mod format;
pub mod licensing;
pub mod localtime;
pub mod pdf;
pub mod printing;
pub mod storage;
pub mod watermark;
mod win32_util;

pub use error::{CoreError, CoreResult};
pub use forensics::{EnhanceMode, ForensicsImage};
pub use format::{
    CreateSpdfOptions, DecryptedSpdf, DocId, MoireLevel, SecurityFlags, SpdfContainerInfo,
    SpdfPolicy,
};
pub use licensing::DeviceSnapshot;
pub use localtime::LocalDateTime;
pub use pdf::MemoryPdfDocument;
pub use printing::{
    PrintJobContext, PrinterCandidate, PrinterPolicyEvaluation, PrinterPortMetadata,
};
pub use storage::{
    LocalQuotaStore, QuotaBackend, QuotaProtector, QuotaStatus, QuotaStoreHealth,
    WindowsDpapiProtector, WindowsDualQuotaBackend,
};
