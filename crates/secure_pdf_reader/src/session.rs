//! Shared authenticated reader session state held strictly in memory.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use secure_pdf_core::format::{SecurityFlags, SpdfPolicy, utc_now_dotnet_ticks};
use secure_pdf_core::printing::build_print_watermark_text;
use secure_pdf_core::watermark::build_screen_watermark_text;
use secure_pdf_core::{DeviceSnapshot, MemoryPdfDocument, QuotaStatus};

/// Unlocked in-memory session shared by the reader window, the render worker and the print job.
///
/// The decrypted document lives behind a lock so that it can be released (closed and zeroized)
/// deterministically when the window closes, the document is locked, or the process panics —
/// independent of how many `Arc` clones are still alive.
pub struct ReaderSession {
    pdf: RwLock<Option<MemoryPdfDocument>>,
    /// Total page count of the unlocked document.
    pub page_count: i32,
    /// Authenticated `.spdf` document policy.
    pub policy: SpdfPolicy,
    /// Host hardware fingerprint and initial NIC addresses.
    pub device: DeviceSnapshot,
    /// UTC .NET ticks recorded when the document was unlocked.
    pub unlock_utc_ticks: i64,
    /// Monotonic instant when unlock occurred (resistant to wall-clock rollback).
    pub unlock_instant: Instant,
    /// Whether the local authorisation record had to be repaired while unlocking.
    pub store_repaired: bool,
    current_ipv4: Mutex<String>,
    quota: Mutex<QuotaStatus>,
}

impl ReaderSession {
    /// Initializes an authenticated [`ReaderSession`].
    #[must_use]
    pub fn new(
        pdf: MemoryPdfDocument,
        policy: SpdfPolicy,
        device: DeviceSnapshot,
        quota_status: QuotaStatus,
        store_repaired: bool,
    ) -> Arc<Self> {
        let ipv4 = device.primary_ipv4.clone();
        Arc::new(Self {
            page_count: pdf.page_count(),
            pdf: RwLock::new(Some(pdf)),
            policy,
            device,
            unlock_utc_ticks: utc_now_dotnet_ticks(),
            unlock_instant: Instant::now(),
            store_repaired,
            current_ipv4: Mutex::new(ipv4),
            quota: Mutex::new(quota_status),
        })
    }

    /// Runs `f` against the decrypted document; `None` once the plaintext has been released.
    pub fn with_pdf<T>(&self, f: impl FnOnce(&MemoryPdfDocument) -> T) -> Option<T> {
        let guard = self.pdf.read().ok()?;
        guard.as_ref().map(f)
    }

    /// Closes the decrypted document and zeroizes its backing bytes.
    ///
    /// Never blocks the caller: if a render or print job is using the document right now, the
    /// release is completed on a helper thread as soon as that job lets go of it.
    pub fn release_plaintext(self: &Arc<Self>) {
        if let Ok(mut guard) = self.pdf.try_write() {
            guard.take();
            return;
        }
        let session = Arc::clone(self);
        std::thread::spawn(move || {
            if let Ok(mut guard) = session.pdf.write() {
                guard.take();
            }
        });
    }

    /// Best-effort release for the panic path: never blocks and never spawns.
    pub fn release_plaintext_now_or_never(&self) {
        if let Ok(mut guard) = self.pdf.try_write() {
            guard.take();
        }
    }

    /// Latest observed primary IPv4 address.
    #[must_use]
    pub fn current_ip(&self) -> String {
        self.current_ipv4
            .lock()
            .map_or_else(|_| self.device.primary_ipv4.clone(), |g| g.clone())
    }

    /// Records a newly observed primary IPv4 address; returns `true` when it changed.
    pub fn update_ip(&self, new_ip: &str) -> bool {
        self.current_ipv4.lock().is_ok_and(|mut g| {
            if *g == new_ip {
                false
            } else {
                new_ip.clone_into(&mut g);
                true
            }
        })
    }

    /// Latest verified print quota state.
    #[must_use]
    pub fn quota_status(&self) -> QuotaStatus {
        self.quota.lock().map_or_else(
            |_| {
                // Fail closed: an unreadable state grants no prints.
                let max = self.policy.max_prints.max(0);
                QuotaStatus {
                    max_prints: max,
                    used_prints: max,
                    remaining_prints: 0,
                    sequence_number: 0,
                    max_seen_utc_ticks: 0,
                }
            },
            |g| *g,
        )
    }

    /// Stores a freshly verified quota state.
    pub fn set_quota_status(&self, status: QuotaStatus) {
        if let Ok(mut g) = self.quota.lock() {
            *g = status;
        }
    }

    /// `true` when the policy asks for the hidden trace watermark and microdots.
    #[must_use]
    pub const fn hidden_watermark_enabled(&self) -> bool {
        self.policy.flags.contains(SecurityFlags::HIDDEN_WATERMARK)
    }

    /// `true` when the policy asks for the visible watermark.
    #[must_use]
    pub const fn visible_watermark_enabled(&self) -> bool {
        self.policy.flags.contains(SecurityFlags::VISIBLE_WATERMARK)
    }

    /// On-screen trace label: creator's note, current IP, the given local time and machine code.
    #[must_use]
    pub fn screen_watermark_text(&self, local_time: &str) -> String {
        build_screen_watermark_text(
            &self.policy.watermark_note,
            &self.current_ip(),
            local_time,
            &self.device.machine_code,
        )
    }

    /// Custom watermark text rendered in the bottom underlay of printed pages (80% transparent);
    /// empty when the sender configured no custom watermark.
    #[must_use]
    pub fn print_watermark_text(&self) -> String {
        build_print_watermark_text(&self.policy.watermark_note)
    }
}

static ACTIVE_SESSION: Mutex<Option<Arc<ReaderSession>>> = Mutex::new(None);

/// Remembers the session shown by the reader window so abnormal exits can release it.
pub fn register_active_session(session: &Arc<ReaderSession>) {
    if let Ok(mut slot) = ACTIVE_SESSION.lock() {
        *slot = Some(Arc::clone(session));
    }
}

/// Forgets and releases the active session (window closed or document locked).
pub fn release_active_session() {
    let taken = ACTIVE_SESSION.lock().ok().and_then(|mut slot| slot.take());
    if let Some(session) = taken {
        session.release_plaintext();
    }
}

/// Panic-path release: must not block (the panicking thread may hold locks) and must not spawn.
pub fn release_active_session_on_panic() {
    if let Ok(mut slot) = ACTIVE_SESSION.try_lock()
        && let Some(session) = slot.take()
    {
        session.release_plaintext_now_or_never();
    }
}

/// Builds real sessions (decrypted PDF loaded through the bundled `pdfium.dll`) for tests.
#[cfg(test)]
pub mod test_support {
    use std::path::PathBuf;

    use secure_pdf_core::format::{DocId, SecurityFlags, SpdfPolicy};
    use secure_pdf_core::{DeviceSnapshot, MemoryPdfDocument, MoireLevel, QuotaStatus};
    use zeroize::Zeroizing;

    use super::*;

    /// `assets/pdfium.dll` of the workspace that contains the running test executable.
    pub fn workspace_pdfium() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .map(|dir| dir.join("assets").join("pdfium.dll"))
            .find(|candidate| candidate.is_file())
            .expect("assets/pdfium.dll must exist in the workspace")
    }

    /// One-page 595 x 842 pt (A4) blank PDF.
    pub fn a4_pdf() -> Vec<u8> {
        b"%PDF-1.4\n\
          1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
          2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
          3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >> endobj\n\
          trailer << /Size 4 /Root 1 0 R >>\n%%EOF\n"
            .to_vec()
    }

    pub fn session(flags: u32, note: &str, max_prints: i32) -> Arc<ReaderSession> {
        let pdf = MemoryPdfDocument::open_bytes_with_dll(
            Zeroizing::new(a4_pdf()),
            Some(&workspace_pdfium()),
        )
        .unwrap();
        let policy = SpdfPolicy {
            doc_id: DocId::from_wire_bytes([9; 16]),
            max_prints,
            expires_at_utc_ticks: None,
            moire_level: MoireLevel::Medium,
            flags: SecurityFlags::from_bits(flags),
            original_file_name: "合同.pdf".to_owned(),
            watermark_note: note.to_owned(),
        };
        let device = DeviceSnapshot {
            machine_code: "YVN8-2N88-7RN5".to_owned(),
            machine_hash: [7; 32],
            computer_name: "WORKSTATION-A".to_owned(),
            primary_ipv4: "192.168.1.20".to_owned(),
            ipv4_list: vec!["192.168.1.20".to_owned()],
            machine_guid: String::new(),
            bios_uuid: String::new(),
            volume_serial: String::new(),
        };
        let quota = QuotaStatus {
            max_prints,
            used_prints: 0,
            remaining_prints: max_prints,
            sequence_number: 1,
            max_seen_utc_ticks: 1,
        };
        ReaderSession::new(pdf, policy, device, quota, false)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use secure_pdf_core::format::SecurityFlags;

    use super::test_support::session;
    use super::*;

    #[test]
    fn screen_watermark_text_carries_note_ip_time_and_machine_code() {
        let s = session(SecurityFlags::VISIBLE_WATERMARK, " 张三专用 ", 2);
        assert_eq!(
            s.screen_watermark_text("26-11-16 18:30:45"),
            "张三专用 | IP:192.168.1.20 | 26-11-16 18:30:45 | YVN8-2N88-7RN5"
        );
        // A long Chinese note is kept in full (no byte truncation, no ASCII-only filter).
        let long_note =
            "仅供张三（市场部）内部审阅，严禁外传、翻拍或转发给任何第三方，违者追究责任";
        let long = session(SecurityFlags::VISIBLE_WATERMARK, long_note, 2);
        let text = long.screen_watermark_text("26-11-16 18:30:45");
        assert!(text.starts_with(long_note));
        assert!(text.ends_with("| IP:192.168.1.20 | 26-11-16 18:30:45 | YVN8-2N88-7RN5"));

        // Without a note the label still has IP, time and machine code.
        let bare = session(0, "", 2);
        assert_eq!(
            bare.screen_watermark_text("26-11-16 18:30:45"),
            "IP:192.168.1.20 | 26-11-16 18:30:45 | YVN8-2N88-7RN5"
        );
    }

    #[test]
    fn watermarks_follow_the_live_ip_and_the_time_passed_in() {
        let s = session(SecurityFlags::HIDDEN_WATERMARK, "张三专用", 2);
        assert!(!s.update_ip("192.168.1.20"));
        assert!(s.update_ip("10.0.0.8"));
        assert_eq!(s.current_ip(), "10.0.0.8");
        assert_eq!(
            s.screen_watermark_text("26-11-16 18:31:00"),
            "张三专用 | IP:10.0.0.8 | 26-11-16 18:31:00 | YVN8-2N88-7RN5"
        );
        // Printed pages carry only the custom watermark note (or empty when none was set).
        assert_eq!(s.print_watermark_text(), "张三专用");
        let bare = session(SecurityFlags::HIDDEN_WATERMARK, "   ", 2);
        assert_eq!(bare.print_watermark_text(), "");
    }

    #[test]
    fn policy_flags_select_the_watermark_layers() {
        let hidden = session(SecurityFlags::HIDDEN_WATERMARK, "", 1);
        assert!(hidden.hidden_watermark_enabled() && !hidden.visible_watermark_enabled());
        let visible = session(SecurityFlags::VISIBLE_WATERMARK, "", 1);
        assert!(!visible.hidden_watermark_enabled() && visible.visible_watermark_enabled());
        let neither = session(SecurityFlags::BLUR_ON_UNFOCUS, "", 1);
        assert!(!neither.hidden_watermark_enabled() && !neither.visible_watermark_enabled());
    }

    #[test]
    fn releasing_the_session_closes_the_document_for_every_holder() {
        let s = session(0, "", 1);
        let other_holder = Arc::clone(&s);
        assert_eq!(s.page_count, 1);
        assert_eq!(s.with_pdf(MemoryPdfDocument::page_count), Some(1));

        s.release_plaintext();
        assert_eq!(s.with_pdf(MemoryPdfDocument::page_count), None);
        assert_eq!(other_holder.with_pdf(MemoryPdfDocument::page_count), None);
        // Releasing twice is harmless.
        s.release_plaintext();
        other_holder.release_plaintext_now_or_never();
    }

    #[test]
    fn release_does_not_block_and_completes_once_a_running_job_lets_go() {
        let s = session(0, "", 1);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker_session = Arc::clone(&s);
        let job = std::thread::spawn(move || {
            worker_session.with_pdf(|pdf| {
                started_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(300));
                pdf.page_count()
            })
        });
        started_rx.recv().unwrap();

        // The document is in use: the panic-path release must give up immediately...
        s.release_plaintext_now_or_never();
        // ...and the normal release must return at once, finishing in the background.
        let before = Instant::now();
        s.release_plaintext();
        assert!(before.elapsed() < Duration::from_millis(150));

        assert_eq!(job.join().unwrap(), Some(1));
        let deadline = Instant::now() + Duration::from_secs(5);
        while s.with_pdf(|_| ()).is_some() {
            assert!(Instant::now() < deadline, "document was never released");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn active_session_registry_releases_on_close_and_on_panic_path() {
        let first = session(0, "", 1);
        register_active_session(&first);
        release_active_session();
        assert!(first.with_pdf(|_| ()).is_none());
        // Nothing registered any more: both paths are no-ops.
        release_active_session();
        release_active_session_on_panic();

        let second = session(0, "", 1);
        register_active_session(&second);
        release_active_session_on_panic();
        assert!(second.with_pdf(|_| ()).is_none());
    }

    #[test]
    fn quota_state_is_shared_and_updated_in_place() {
        let s = session(0, "", 3);
        assert_eq!(s.quota_status().remaining_prints, 3);
        s.set_quota_status(QuotaStatus {
            max_prints: 3,
            used_prints: 2,
            remaining_prints: 1,
            sequence_number: 5,
            max_seen_utc_ticks: 9,
        });
        let q = s.quota_status();
        assert_eq!(
            (q.used_prints, q.remaining_prints, q.sequence_number),
            (2, 1, 5)
        );
    }
}
