//! Native `WinUI 3` window chrome with `MicaBackdrop`, dark title bar, centering, and capture protection.

use windows_core::{HSTRING, Interface};

use crate::bindings::Microsoft::UI::Dispatching::DispatcherQueue;
use crate::bindings::Microsoft::UI::Windowing::{
    AppWindow, DisplayArea, DisplayAreaFallback, OverlappedPresenter, TitleBarTheme,
};
use crate::bindings::Microsoft::UI::Xaml::Media::MicaBackdrop;
use crate::bindings::Microsoft::UI::Xaml::{
    ElementTheme, FrameworkElement, UIElement, Window, XamlRoot,
};
use crate::bindings::Windows::Graphics::{PointInt32, SizeInt32};
use crate::controls::{BG_ELEVATED, TEXT_PRIMARY, TEXT_SECONDARY, argb};
use crate::interop::{
    CaptureProtectionLevel, enforce_window_capture_protection, window_dpi, window_hwnd,
};

/// Configuration options for creating a native `FluentWindow`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct WindowChromeOptions {
    /// Window title displayed in the caption and taskbar.
    pub title: String,
    /// Initial width in device-independent pixels (`96 DPI`).
    pub width_dip: i32,
    /// Initial height in device-independent pixels (`96 DPI`).
    pub height_dip: i32,
    /// Allow manual user resizing.
    pub resizable: bool,
    /// Allow window maximize button.
    pub maximizable: bool,
    /// Allow window minimize button.
    pub minimizable: bool,
    /// Keep window above standard windows.
    pub always_on_top: bool,
    /// Enforce OS-level screenshot/recording exclusion (`SetWindowDisplayAffinity`).
    pub capture_protected: bool,
    /// Apply `MicaBackdrop` when supported by the OS compositor.
    pub use_mica: bool,
}

impl Default for WindowChromeOptions {
    fn default() -> Self {
        Self {
            title: String::from("SecurePdfSuite"),
            width_dip: 1080,
            height_dip: 720,
            resizable: true,
            maximizable: true,
            minimizable: true,
            always_on_top: false,
            capture_protected: false,
            use_mica: true,
        }
    }
}

/// High-level wrapper pairing a `WinUI 3` `Window` with its `AppWindow`.
#[derive(Clone)]
pub struct FluentWindow {
    /// Underlying `WinUI 3` window object.
    pub window: Window,
    /// Associated `AppWindow` controller.
    pub app_window: AppWindow,
    capture_protected: bool,
}

impl FluentWindow {
    /// Creates and configures a native `WinUI 3` window according to `opts`.
    ///
    /// When `opts.capture_protected` is `true`, screenshot protection is applied fail-closed
    /// before the window is activated.
    ///
    /// # Errors
    ///
    /// Returns an error if window creation, presenter setup, or capture protection fails.
    pub fn new(opts: &WindowChromeOptions) -> windows_core::Result<Self> {
        let window = Window::new()?;
        let app_window = window.AppWindow()?;
        app_window.SetTitle(&HSTRING::from(&opts.title))?;
        window.SetTitle(&HSTRING::from(&opts.title))?;

        if opts.use_mica
            && let Ok(mica) = MicaBackdrop::new()
        {
            let _ = window.SetSystemBackdrop(&mica);
        }

        if let Ok(title_bar) = app_window.TitleBar() {
            let _ = title_bar.SetPreferredTheme(TitleBarTheme::Dark);
            let _ = title_bar.SetBackgroundColor(Some(argb(255, 24, 24, 24)));
            let _ = title_bar.SetForegroundColor(Some(TEXT_PRIMARY));
            let _ = title_bar.SetInactiveBackgroundColor(Some(argb(255, 28, 28, 28)));
            let _ = title_bar.SetInactiveForegroundColor(Some(TEXT_SECONDARY));
            let _ = title_bar.SetButtonBackgroundColor(Some(argb(255, 24, 24, 24)));
            let _ = title_bar.SetButtonForegroundColor(Some(TEXT_PRIMARY));
            let _ = title_bar.SetButtonHoverBackgroundColor(Some(BG_ELEVATED));
            let _ = title_bar.SetButtonHoverForegroundColor(Some(TEXT_PRIMARY));
            let _ = title_bar.SetButtonInactiveBackgroundColor(Some(argb(255, 28, 28, 28)));
            let _ = title_bar.SetButtonInactiveForegroundColor(Some(TEXT_SECONDARY));
        }

        if let Ok(presenter) = app_window.Presenter()
            && let Ok(overlapped) = presenter.cast::<OverlappedPresenter>()
        {
            let _ = overlapped.SetIsResizable(opts.resizable);
            let _ = overlapped.SetIsMaximizable(opts.maximizable);
            let _ = overlapped.SetIsMinimizable(opts.minimizable);
            if opts.always_on_top {
                let _ = overlapped.SetIsAlwaysOnTop(true);
            }
        }

        let wrapper = Self {
            window,
            app_window,
            capture_protected: opts.capture_protected,
        };
        wrapper.resize_and_center(opts.width_dip, opts.height_dip);

        if opts.capture_protected {
            wrapper.reapply_capture_protection()?;
        }

        Ok(wrapper)
    }

    /// Returns the window's DPI scale factor (`1.0` at `96 DPI`).
    #[must_use]
    pub fn dpi_scale(&self) -> f64 {
        f64::from(window_dpi(&self.window)) / 96.0
    }

    /// Returns the raw `HWND` value as `isize`.
    ///
    /// # Errors
    ///
    /// Returns an error if `IWindowNative` handle extraction fails.
    pub fn hwnd_raw(&self) -> windows_core::Result<isize> {
        Ok(window_hwnd(&self.window)?.0 as isize)
    }

    /// Returns the `DispatcherQueue` associated with this UI thread/window.
    ///
    /// # Errors
    ///
    /// Returns an error if querying `DispatcherQueue` fails.
    pub fn dispatcher_queue(&self) -> windows_core::Result<DispatcherQueue> {
        self.window.DispatcherQueue()
    }

    /// Sets the root XAML element and applies `ElementTheme::Dark`.
    ///
    /// # Errors
    ///
    /// Returns an error if setting the theme or window content fails.
    pub fn set_content(&self, root: &FrameworkElement) -> windows_core::Result<()> {
        root.SetRequestedTheme(ElementTheme::Dark)?;
        let ui: UIElement = root.cast()?;
        self.window.SetContent(&ui)
    }

    /// Returns the active `XamlRoot` after window content is attached.
    ///
    /// # Errors
    ///
    /// Returns an error if window content is missing or `XamlRoot` is unavailable.
    pub fn xaml_root(&self) -> windows_core::Result<XamlRoot> {
        self.window.Content()?.XamlRoot()
    }

    /// Activates and presents the window, re-verifying capture protection when required.
    ///
    /// # Errors
    ///
    /// Returns an error if `Activate` or fail-closed capture protection fails.
    pub fn activate(&self) -> windows_core::Result<()> {
        if self.capture_protected {
            self.reapply_capture_protection()?;
        }
        self.window.Activate()?;
        if self.capture_protected {
            self.reapply_capture_protection()?;
        }
        Ok(())
    }

    /// Applies `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE` -> `WDA_MONITOR`) fail-closed.
    ///
    /// # Errors
    ///
    /// Returns an error if neither capture-exclusion level is accepted by Windows.
    pub fn reapply_capture_protection(&self) -> windows_core::Result<CaptureProtectionLevel> {
        enforce_window_capture_protection(&self.window)
    }

    /// Updates the window title dynamically.
    pub fn set_title(&self, title: &str) {
        let h = HSTRING::from(title);
        let _ = self.app_window.SetTitle(&h);
        let _ = self.window.SetTitle(&h);
    }

    /// Resizes using DIP dimensions and centers the window within the primary work area.
    pub fn resize_and_center(&self, width_dip: i32, height_dip: i32) {
        let scale = self.dpi_scale();
        #[allow(clippy::cast_possible_truncation)]
        let mut target_w = (f64::from(width_dip) * scale).round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let mut target_h = (f64::from(height_dip) * scale).round() as i32;

        if let Ok(id) = self.app_window.Id()
            && let Ok(area) = DisplayArea::GetFromWindowId(id, DisplayAreaFallback::Primary)
            && let Ok(work) = area.WorkArea()
        {
            target_w = target_w.min((work.Width - 32).max(360));
            target_h = target_h.min((work.Height - 32).max(280));
            let _ = self.app_window.Resize(SizeInt32 {
                Width: target_w,
                Height: target_h,
            });
            let pos_x = work.X + ((work.Width - target_w) / 2).max(0);
            let pos_y = work.Y + ((work.Height - target_h) / 2).max(0);
            let _ = self.app_window.Move(PointInt32 { X: pos_x, Y: pos_y });
            return;
        }

        let _ = self.app_window.Resize(SizeInt32 {
            Width: target_w,
            Height: target_h,
        });
    }
}
