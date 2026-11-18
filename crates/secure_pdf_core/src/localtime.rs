//! Local wall-clock conversions (time-zone aware) for user-facing timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::format::{TICKS_PER_SECOND, system_time_to_dotnet_ticks};

/// .NET ticks (100 ns since 0001-01-01) at the Windows `FILETIME` epoch 1601-01-01T00:00:00Z.
const FILETIME_EPOCH_DOTNET_TICKS: i64 = 504_911_232_000_000_000;

#[allow(unsafe_op_in_unsafe_fn, clippy::upper_case_acronyms)]
mod ffi {
    use core::ffi::c_void;
    use windows_core::BOOL;

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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct FileTime {
        pub low: u32,
        pub high: u32,
    }

    windows_core::link!("kernel32.dll" "system" fn FileTimeToSystemTime(
        lpfiletime: *const FileTime,
        lpsystemtime: *mut SystemTime,
    ) -> BOOL);
    windows_core::link!("kernel32.dll" "system" fn SystemTimeToFileTime(
        lpsystemtime: *const SystemTime,
        lpfiletime: *mut FileTime,
    ) -> BOOL);
    windows_core::link!("kernel32.dll" "system" fn SystemTimeToTzSpecificLocalTime(
        lptimezoneinformation: *const c_void,
        lpuniversaltime: *const SystemTime,
        lplocaltime: *mut SystemTime,
    ) -> BOOL);
}

/// A calendar date-time in the computer's current time zone, with its UTC offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    /// Calendar year (e.g. `2026`).
    pub year: u16,
    /// Month `1..=12`.
    pub month: u16,
    /// Day of month `1..=31`.
    pub day: u16,
    /// Hour `0..=23`.
    pub hour: u16,
    /// Minute `0..=59`.
    pub minute: u16,
    /// Second `0..=59`.
    pub second: u16,
    /// Offset of local time from UTC in minutes at that instant (e.g. `480` for UTC+08:00).
    pub utc_offset_minutes: i32,
}

impl LocalDateTime {
    /// Current local date-time.
    #[must_use]
    pub fn now() -> Self {
        Self::from_dotnet_utc_ticks(system_time_to_dotnet_ticks(SystemTime::now())).unwrap_or(
            Self {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                utc_offset_minutes: 0,
            },
        )
    }

    /// Converts .NET UTC ticks to the local date-time, honouring the time-zone rules (including
    /// daylight saving) in effect at that instant. Returns `None` when out of range.
    #[must_use]
    pub fn from_dotnet_utc_ticks(ticks: i64) -> Option<Self> {
        let file_ticks = u64::try_from(ticks.checked_sub(FILETIME_EPOCH_DOTNET_TICKS)?).ok()?;
        let utc_file = ffi::FileTime {
            low: u32::try_from(file_ticks & 0xFFFF_FFFF).ok()?,
            high: u32::try_from(file_ticks >> 32).ok()?,
        };
        let mut utc_parts = ffi::SystemTime::default();
        // SAFETY: Both pointers reference live, properly aligned locals.
        if !unsafe { ffi::FileTimeToSystemTime(&raw const utc_file, &raw mut utc_parts) }.as_bool()
        {
            return None;
        }
        let mut local_parts = ffi::SystemTime::default();
        // SAFETY: A null time-zone pointer selects the currently active time zone; the other
        // pointers reference live locals.
        let converted = unsafe {
            ffi::SystemTimeToTzSpecificLocalTime(
                std::ptr::null(),
                &raw const utc_parts,
                &raw mut local_parts,
            )
        };
        if !converted.as_bool() {
            return None;
        }
        let mut local_file = ffi::FileTime::default();
        // SAFETY: Both pointers reference live, properly aligned locals.
        if !unsafe { ffi::SystemTimeToFileTime(&raw const local_parts, &raw mut local_file) }
            .as_bool()
        {
            return None;
        }
        let local_ticks = (u64::from(local_file.high) << 32) | u64::from(local_file.low);
        // Whole seconds on both sides; the difference is the UTC offset at that instant.
        let utc_seconds = i64::try_from(file_ticks).ok()? / TICKS_PER_SECOND;
        let local_seconds = i64::try_from(local_ticks).ok()? / TICKS_PER_SECOND;
        let offset_seconds = local_seconds - utc_seconds;
        let utc_offset_minutes =
            i32::try_from((offset_seconds + offset_seconds.signum() * 30) / 60).ok()?;
        Some(Self {
            year: local_parts.year,
            month: local_parts.month,
            day: local_parts.day,
            hour: local_parts.hour,
            minute: local_parts.minute,
            second: local_parts.second,
            utc_offset_minutes,
        })
    }

    /// `yyyy-MM-dd HH:mm:ss`.
    #[must_use]
    pub fn format_date_time(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `yy-MM-dd HH:mm:ss` (compact form used inside on-screen watermarks).
    #[must_use]
    pub fn format_short(&self) -> String {
        format!(
            "{:02}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year % 100,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second
        )
    }

    /// `UTC+08:00` style offset label.
    #[must_use]
    pub fn format_utc_offset(&self) -> String {
        let sign = if self.utc_offset_minutes < 0 {
            '-'
        } else {
            '+'
        };
        let abs = self.utc_offset_minutes.unsigned_abs();
        format!("UTC{sign}{:02}:{:02}", abs / 60, abs % 60)
    }

    /// `yyyy-MM-dd HH:mm:ss (UTC+08:00)` — local time with an explicit time-zone label.
    #[must_use]
    pub fn format_with_offset(&self) -> String {
        format!("{} ({})", self.format_date_time(), self.format_utc_offset())
    }
}

/// Formats .NET UTC ticks as local time with an explicit offset label, e.g.
/// `2026-11-16 18:30:00 (UTC+08:00)`; falls back to the raw tick count when out of range.
#[must_use]
pub fn format_dotnet_ticks_local(ticks: i64) -> String {
    LocalDateTime::from_dotnet_utc_ticks(ticks).map_or_else(
        || format!("ticks:{ticks}"),
        |local| local.format_with_offset(),
    )
}

/// Current Unix time in whole seconds, truncated to 32 bits (the microdot payload width).
#[must_use]
pub fn unix_seconds_now() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    u32::try_from(secs & 0xFFFF_FFFF).unwrap_or(0)
}

/// Formats Unix seconds as local time with an explicit offset label.
#[must_use]
pub fn format_unix_seconds_local(unix_seconds: u32) -> String {
    let ticks = crate::format::DOTNET_UNIX_EPOCH_TICKS
        .saturating_add(i64::from(unix_seconds).saturating_mul(TICKS_PER_SECOND));
    format_dotnet_ticks_local(ticks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::DOTNET_UNIX_EPOCH_TICKS;

    #[test]
    fn local_conversion_is_consistent_with_its_reported_utc_offset() {
        // 2026-11-16T10:30:45Z
        let unix = 1_794_825_045_i64;
        let ticks = DOTNET_UNIX_EPOCH_TICKS + unix * TICKS_PER_SECOND;
        let local = LocalDateTime::from_dotnet_utc_ticks(ticks).unwrap();
        assert!((-14 * 60..=14 * 60).contains(&local.utc_offset_minutes));

        // Re-deriving UTC from the local fields and the offset must give the original instant.
        let days = days_from_civil(
            i64::from(local.year),
            i64::from(local.month),
            i64::from(local.day),
        );
        let local_secs = days * 86_400
            + i64::from(local.hour) * 3_600
            + i64::from(local.minute) * 60
            + i64::from(local.second);
        assert_eq!(local_secs - i64::from(local.utc_offset_minutes) * 60, unix);
    }

    #[test]
    fn formatting_is_zero_padded_and_labels_the_time_zone() {
        let t = LocalDateTime {
            year: 2026,
            month: 3,
            day: 7,
            hour: 9,
            minute: 5,
            second: 2,
            utc_offset_minutes: 480,
        };
        assert_eq!(t.format_date_time(), "2026-03-07 09:05:02");
        assert_eq!(t.format_short(), "26-03-07 09:05:02");
        assert_eq!(t.format_utc_offset(), "UTC+08:00");
        assert_eq!(t.format_with_offset(), "2026-03-07 09:05:02 (UTC+08:00)");
        let west = LocalDateTime {
            utc_offset_minutes: -210,
            ..t
        };
        assert_eq!(west.format_utc_offset(), "UTC-03:30");
    }

    #[test]
    fn out_of_range_ticks_fall_back_without_panicking() {
        assert!(LocalDateTime::from_dotnet_utc_ticks(0).is_none());
        assert_eq!(format_dotnet_ticks_local(0), "ticks:0");
        assert!(format_unix_seconds_local(1_794_825_045).contains("(UTC"));
        assert!(unix_seconds_now() > 1_700_000_000);
    }

    /// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
    const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }
}
