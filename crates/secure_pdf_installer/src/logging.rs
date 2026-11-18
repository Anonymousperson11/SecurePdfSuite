//! Local timestamps and the on-disk failure log for runs that have no visible console.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Log written next to the installer when a run fails (same name as v1.x).
pub const ERROR_LOG_NAME: &str = "install_error.log";

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod ffi {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct SystemTime {
        pub year: u16,
        pub month: u16,
        pub day_of_week: u16,
        pub day: u16,
        pub hour: u16,
        pub minute: u16,
        pub second: u16,
        pub milliseconds: u16,
    }

    windows_core::link!("kernel32.dll" "system" fn GetLocalTime(lpsystemtime: *mut SystemTime));
}

fn local_now() -> ffi::SystemTime {
    let mut now = ffi::SystemTime::default();
    // SAFETY: Writes the current local time into a live, properly aligned local.
    unsafe { ffi::GetLocalTime(&raw mut now) };
    now
}

/// Current local time as `yyyy-MM-dd HH:mm:ss`.
#[must_use]
pub fn local_timestamp() -> String {
    let t = local_now();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// Current local date as `yyyyMMdd` (the `InstallDate` registry format).
#[must_use]
pub fn local_date_compact() -> String {
    let t = local_now();
    format!("{:04}{:02}{:02}", t.year, t.month, t.day)
}

/// Appends one timestamped record to `path`, creating the file (with a UTF-8 BOM so Notepad
/// shows Chinese text correctly) when it does not exist yet.
///
/// # Errors
///
/// Returns the I/O error if the file cannot be opened or written.
pub fn append_log_record(path: &Path, message: &str) -> std::io::Result<()> {
    let is_new = !path.exists();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if is_new {
        file.write_all("\u{feff}".as_bytes())?;
    }
    let body = message.replace("\r\n", "\n").replace('\n', "\r\n    ");
    file.write_all(format!("{} {body}\r\n", local_timestamp()).as_bytes())
}

/// Records a failure where the user can find it: next to the installer executable, or in the
/// temporary directory when that location is not writable. Returns the log path on success.
#[must_use]
pub fn write_failure_log(message: &str) -> Option<PathBuf> {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(ERROR_LOG_NAME)));
    let in_temp = std::env::temp_dir().join(ERROR_LOG_NAME);
    beside_exe
        .into_iter()
        .chain(std::iter::once(in_temp))
        .find(|candidate| append_log_record(candidate, message).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_have_fixed_width_numeric_fields() {
        let stamp = local_timestamp();
        assert_eq!(stamp.len(), 19);
        let bytes = stamp.as_bytes();
        assert_eq!(
            (bytes[4], bytes[7], bytes[10], bytes[13], bytes[16]),
            (b'-', b'-', b' ', b':', b':')
        );
        let date = local_date_compact();
        assert_eq!(date.len(), 8);
        assert!(date.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(&stamp[..4], &date[..4]);
    }

    #[test]
    fn log_records_append_with_bom_once_and_keep_multi_line_messages_together() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ERROR_LOG_NAME);
        append_log_record(&path, "安装失败：第一行\n第二行").unwrap();
        append_log_record(&path, "第二条记录").unwrap();
        let raw = fs::read(&path).unwrap();
        assert_eq!(&raw[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(
            raw.windows(3).filter(|w| w == &[0xEF, 0xBB, 0xBF]).count(),
            1
        );
        let text = String::from_utf8(raw).unwrap();
        let lines: Vec<&str> = text.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert!(lines[0].ends_with("安装失败：第一行"));
        assert_eq!(lines[1], "    第二行");
        assert!(lines[2].ends_with("第二条记录"));
        assert_eq!(lines[3], "");
    }
}
