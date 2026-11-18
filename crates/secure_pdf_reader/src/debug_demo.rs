//! Debug builds only: opens the reader window on a synthetic in-memory document so the window,
//! overlay and lock behaviour can be inspected without unlocking a real `.spdf` file.
//!
//! The demo never touches the local authorisation store: printing is disabled by policy and the
//! process exits on its own before the first periodic store re-check would run.

use std::sync::Arc;
use std::time::Duration;

use secure_pdf_core::format::{
    DocId, SecurityFlags, SpdfPolicy, TICKS_PER_SECOND, utc_now_dotnet_ticks,
};
use secure_pdf_core::licensing::collect_device_snapshot;
use secure_pdf_core::{CoreError, MemoryPdfDocument, MoireLevel, QuotaStatus};
use zeroize::Zeroizing;

use crate::reader_window::open_reader_window;
use crate::session::ReaderSession;

/// Longest demo run; the first full authorisation re-check happens after 120 seconds.
const MAX_DEMO_SECONDS: u64 = 90;

/// `SECUREPDF_DEBUG_DEMO=<seconds>` requests a demo run of at most [`MAX_DEMO_SECONDS`].
#[must_use]
pub fn demo_seconds() -> Option<u64> {
    let raw = std::env::var("SECUREPDF_DEBUG_DEMO").ok()?;
    let seconds = raw.trim().parse::<u64>().ok()?;
    Some(seconds.clamp(5, MAX_DEMO_SECONDS))
}

/// `SECUREPDF_DEBUG_DEMO_EXPIRE=<seconds>` makes the demo document expire that soon.
fn demo_expiry_ticks() -> Option<i64> {
    let raw = std::env::var("SECUREPDF_DEBUG_DEMO_EXPIRE").ok()?;
    let seconds = raw.trim().parse::<i64>().ok()?.clamp(1, 3_600);
    Some(utc_now_dotnet_ticks() + seconds * TICKS_PER_SECOND)
}

/// Three blank A4 pages.
fn demo_pdf() -> Vec<u8> {
    b"%PDF-1.4\n\
      1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
      2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >> endobj\n\
      3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\n\
      4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\n\
      5 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\n\
      trailer << /Size 6 /Root 1 0 R >>\n%%EOF\n"
        .to_vec()
}

fn demo_session() -> Result<Arc<ReaderSession>, CoreError> {
    let device = collect_device_snapshot()?;
    let pdf = MemoryPdfDocument::open_bytes(Zeroizing::new(demo_pdf()))?;
    let policy = SpdfPolicy {
        doc_id: DocId::new_random()?,
        // Printing stays disabled so the demo can never reach the authorisation store.
        max_prints: 0,
        expires_at_utc_ticks: demo_expiry_ticks(),
        moire_level: MoireLevel::Medium,
        flags: SecurityFlags::from_bits(
            SecurityFlags::HIDDEN_WATERMARK
                | SecurityFlags::VISIBLE_WATERMARK
                | SecurityFlags::ALLOW_USER_ADJUST_MOIRE,
        ),
        original_file_name: "调试演示文档.pdf".to_owned(),
        watermark_note: "张三专用".to_owned(),
    };
    let quota = QuotaStatus {
        max_prints: 0,
        used_prints: 0,
        remaining_prints: 0,
        sequence_number: 1,
        max_seen_utc_ticks: utc_now_dotnet_ticks(),
    };
    Ok(ReaderSession::new(pdf, policy, device, quota, true))
}

/// Opens the demo reader window and arms the automatic exit.
///
/// # Errors
///
/// Returns a user-facing description when the demo document or the window cannot be created.
pub fn open_demo_reader(seconds: u64) -> Result<(), String> {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        std::process::exit(0);
    });
    let session = demo_session().map_err(|e| e.to_string())?;
    open_reader_window(&session).map_err(|e| e.user_message())
}
