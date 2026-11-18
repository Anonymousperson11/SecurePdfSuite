//! Non-blocking `WinUI 3` `ContentDialog` helpers scoped cleanly to an active `XamlRoot`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_core::{HSTRING, Interface};
use windows_future::AsyncOperationCompletedHandler;

use crate::bindings::Microsoft::UI::Xaml::Controls::{
    ContentDialog, ContentDialogButton, ContentDialogResult,
};
use crate::bindings::Microsoft::UI::Xaml::{ElementTheme, FrameworkElement, XamlRoot};
use crate::controls::{TEXT_PRIMARY, TEXT_SECONDARY, stack_v, text_block};

static DIALOG_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Which button a dialog highlights and triggers on Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogDefault {
    /// The primary (confirming) button.
    Primary,
    /// The close (cancelling / "go back") button.
    Close,
}

/// Returns `true` while one of this module's dialogs is on screen (only one may be open).
#[must_use]
pub fn is_dialog_active() -> bool {
    DIALOG_ACTIVE.load(Ordering::Acquire)
}

/// Shows an informational or warning `ContentDialog` with a single close button without blocking the UI loop.
///
/// Returns `false` when the dialog could not be shown (another dialog is open or creation
/// failed), so callers can avoid silently dropping the request.
pub fn show_alert_dialog(
    xaml_root: &XamlRoot,
    title: &str,
    message: &str,
    close_label: &str,
) -> bool {
    if DIALOG_ACTIVE.swap(true, Ordering::AcqRel) {
        return false;
    }
    if show_alert_inner(xaml_root, title, message, close_label).is_err() {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        return false;
    }
    true
}

fn show_alert_inner(
    xaml_root: &XamlRoot,
    title: &str,
    message: &str,
    close_label: &str,
) -> windows_core::Result<()> {
    let dlg = ContentDialog::new()?;
    dlg.SetXamlRoot(xaml_root)?;
    dlg.SetRequestedTheme(ElementTheme::Dark)?;
    dlg.SetTitle(&text_block(title, 17.0, true, TEXT_PRIMARY)?)?;
    dlg.SetContent(&text_block(message, 13.5, false, TEXT_SECONDARY)?)?;
    dlg.SetCloseButtonText(&HSTRING::from(close_label))?;
    dlg.SetDefaultButton(ContentDialogButton::Close)?;

    let op = dlg.ShowAsync()?;
    let handler = AsyncOperationCompletedHandler::new(move |_, _| {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        Ok(())
    });
    op.SetCompleted(&handler)
}

/// Shows an interactive confirm/cancel `ContentDialog` and invokes `on_result(true)` if the primary button is chosen.
///
/// The close ("go back") button is the default so that Enter never confirms a destructive
/// action by accident. Returns `false` when the dialog could not be shown.
pub fn show_confirm_dialog<F>(
    xaml_root: &XamlRoot,
    title: &str,
    message: &str,
    primary_label: &str,
    close_label: &str,
    on_result: F,
) -> bool
where
    F: FnOnce(bool) + Send + 'static,
{
    if DIALOG_ACTIVE.swap(true, Ordering::AcqRel) {
        return false;
    }
    let Ok(body) = text_block(message, 13.5, false, TEXT_SECONDARY) else {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        return false;
    };
    let Ok(el) = body.cast::<FrameworkElement>() else {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        return false;
    };
    if show_custom_inner(
        xaml_root,
        title,
        &el,
        Some(primary_label),
        close_label,
        DialogDefault::Close,
        on_result,
    )
    .is_err()
    {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        return false;
    }
    true
}

/// Shows a `ContentDialog` with custom `FrameworkElement` content and calls `on_result(confirmed)`.
///
/// Returns `false` when the dialog could not be shown (another dialog is open or creation
/// failed).
pub fn show_custom_dialog<F>(
    xaml_root: &XamlRoot,
    title: &str,
    content: &FrameworkElement,
    primary_label: Option<&str>,
    close_label: &str,
    on_result: F,
) -> bool
where
    F: FnOnce(bool) + Send + 'static,
{
    if DIALOG_ACTIVE.swap(true, Ordering::AcqRel) {
        return false;
    }
    if show_custom_inner(
        xaml_root,
        title,
        content,
        primary_label,
        close_label,
        DialogDefault::Primary,
        on_result,
    )
    .is_err()
    {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        return false;
    }
    true
}

fn show_custom_inner<F>(
    xaml_root: &XamlRoot,
    title: &str,
    content: &FrameworkElement,
    primary_label: Option<&str>,
    close_label: &str,
    default_button: DialogDefault,
    on_result: F,
) -> windows_core::Result<()>
where
    F: FnOnce(bool) + Send + 'static,
{
    let dlg = ContentDialog::new()?;
    dlg.SetXamlRoot(xaml_root)?;
    dlg.SetRequestedTheme(ElementTheme::Dark)?;

    let title_tb = text_block(title, 17.0, true, TEXT_PRIMARY)?;
    dlg.SetTitle(&title_tb)?;

    let wrapper = stack_v(8.0)?;
    wrapper.Children()?.Append(content)?;
    dlg.SetContent(&wrapper)?;

    if let Some(primary) = primary_label {
        dlg.SetPrimaryButtonText(&HSTRING::from(primary))?;
        dlg.SetDefaultButton(match default_button {
            DialogDefault::Primary => ContentDialogButton::Primary,
            DialogDefault::Close => ContentDialogButton::Close,
        })?;
    } else {
        dlg.SetDefaultButton(ContentDialogButton::Close)?;
    }
    dlg.SetCloseButtonText(&HSTRING::from(close_label))?;

    let op = dlg.ShowAsync()?;
    let boxed_cb: Box<dyn FnOnce(bool) + Send + 'static> = Box::new(on_result);
    let cb_slot = Mutex::new(Some(boxed_cb));
    let handler = AsyncOperationCompletedHandler::new(move |info, _| {
        DIALOG_ACTIVE.store(false, Ordering::Release);
        let confirmed = info
            .as_ref()
            .and_then(|async_op| async_op.GetResults().ok())
            .is_some_and(|res| res == ContentDialogResult::Primary);
        let cb = cb_slot.lock().ok().and_then(|mut g| g.take());
        if let Some(f) = cb {
            f(confirmed);
        }
        Ok(())
    });
    op.SetCompleted(&handler)
}
