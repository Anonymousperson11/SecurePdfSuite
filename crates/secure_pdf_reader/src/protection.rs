//! Capture-protected window creation with an explicit failure cause.

use winui3::interop::CaptureProtectionLevel;
use winui3::window::{FluentWindow, WindowChromeOptions};

use crate::logic::ProtectionState;

/// Why a reader window could not be opened.
#[derive(Debug)]
pub enum WindowOpenError {
    /// Windows refused to exclude the window from screenshots and recordings.
    CaptureProtection(windows_core::Error),
    /// Any other window or control creation failure.
    Ui(windows_core::Error),
}

impl WindowOpenError {
    /// User-facing explanation naming the real cause.
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::CaptureProtection(err) => format!(
                "当前系统环境无法为阅读窗口启用防截屏保护，为避免文档被截屏或录屏，已停止打开。\n\
                 （错误代码 0x{:08X}）",
                err.code().0.cast_unsigned()
            ),
            Self::Ui(err) => format!(
                "阅读器界面初始化失败，请重新启动程序后再试。\n（错误代码 0x{:08X}）",
                err.code().0.cast_unsigned()
            ),
        }
    }
}

impl From<windows_core::Error> for WindowOpenError {
    fn from(err: windows_core::Error) -> Self {
        Self::Ui(err)
    }
}

/// Debug builds only: `SECUREPDF_DEBUG_ALLOW_CAPTURE` turns capture protection off so a frame
/// can be captured for watermark / microdot diagnostics.
#[cfg(debug_assertions)]
#[must_use]
pub fn debug_capture_allowed() -> bool {
    std::env::var_os("SECUREPDF_DEBUG_ALLOW_CAPTURE").is_some()
}

/// Release builds have no way to turn capture protection off.
#[cfg(not(debug_assertions))]
#[must_use]
pub const fn debug_capture_allowed() -> bool {
    false
}

const fn state_for(level: CaptureProtectionLevel) -> ProtectionState {
    match level {
        CaptureProtectionLevel::ExcludeFromCapture => ProtectionState::ExcludedFromCapture,
        CaptureProtectionLevel::MonitorBlackout => ProtectionState::BlackInCapture,
    }
}

/// Creates a window with capture protection applied before it can ever be shown.
///
/// # Errors
///
/// [`WindowOpenError::CaptureProtection`] when only the protection step fails, otherwise
/// [`WindowOpenError::Ui`].
pub fn create_protected_window(
    opts: &WindowChromeOptions,
) -> Result<FluentWindow, WindowOpenError> {
    let protect = !debug_capture_allowed();
    let opts = WindowChromeOptions {
        capture_protected: protect,
        ..opts.clone()
    };
    match FluentWindow::new(&opts) {
        Ok(win) => Ok(win),
        Err(err) if protect => {
            // Tell the two causes apart: can the same window be created without protection?
            // The probe window is never activated, so nothing is ever displayed by it.
            let probe_opts = WindowChromeOptions {
                capture_protected: false,
                ..opts
            };
            match FluentWindow::new(&probe_opts) {
                Ok(probe) => {
                    let _ = probe.window.Close();
                    Err(WindowOpenError::CaptureProtection(err))
                }
                Err(_) => Err(WindowOpenError::Ui(err)),
            }
        }
        Err(err) => Err(WindowOpenError::Ui(err)),
    }
}

/// Activates the window (protection is re-verified before and after by `FluentWindow`) and
/// reports the protection level in effect.
///
/// # Errors
///
/// [`WindowOpenError::CaptureProtection`] when protection can no longer be confirmed,
/// otherwise [`WindowOpenError::Ui`].
pub fn activate_protected_window(win: &FluentWindow) -> Result<ProtectionState, WindowOpenError> {
    if debug_capture_allowed() {
        win.activate()?;
        return Ok(ProtectionState::DebugDisabled);
    }
    if let Err(err) = win.activate() {
        return Err(match win.reapply_capture_protection() {
            Ok(_) => WindowOpenError::Ui(err),
            Err(protect_err) => WindowOpenError::CaptureProtection(protect_err),
        });
    }
    reverify_protection(win).map_err(WindowOpenError::CaptureProtection)
}

/// Re-applies capture protection and returns the level in effect.
///
/// # Errors
///
/// Returns the Win32 error when Windows no longer accepts any protection level.
pub fn reverify_protection(win: &FluentWindow) -> windows_core::Result<ProtectionState> {
    if debug_capture_allowed() {
        return Ok(ProtectionState::DebugDisabled);
    }
    win.reapply_capture_protection().map(state_for)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_messages_name_the_real_cause_without_raw_system_text() {
        let err = windows_core::Error::new(
            windows_core::HRESULT(0x8007_0005_u32.cast_signed()),
            "Access is denied.",
        );
        let capture = WindowOpenError::CaptureProtection(err.clone()).user_message();
        assert!(capture.contains("防截屏"));
        assert!(capture.contains("0x80070005"));
        assert!(!capture.contains("Access is denied"));

        let ui = WindowOpenError::Ui(err).user_message();
        assert!(ui.contains("界面初始化失败"));
        assert!(!ui.contains("防截屏"));
        assert!(!ui.contains("Access is denied"));
    }

    #[test]
    fn protection_levels_map_to_distinct_states() {
        assert_eq!(
            state_for(CaptureProtectionLevel::ExcludeFromCapture),
            ProtectionState::ExcludedFromCapture
        );
        assert_eq!(
            state_for(CaptureProtectionLevel::MonitorBlackout),
            ProtectionState::BlackInCapture
        );
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_builds_cannot_disable_capture_protection() {
        assert!(!debug_capture_allowed());
    }
}
