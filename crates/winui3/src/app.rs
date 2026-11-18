//! Native `WinUI 3` application lifecycle, metadata provider, and dispatcher utilities.

use std::sync::Mutex;

use windows_core::{Array, HSTRING, Ref};
use windows_implement::implement;

use crate::bindings::Microsoft::UI::Dispatching::{
    DispatcherQueue, DispatcherQueueHandler, DispatcherQueueTimer,
};
use crate::bindings::Microsoft::UI::Xaml::Controls::XamlControlsResources;
use crate::bindings::Microsoft::UI::Xaml::Markup::{
    IXamlMetadataProvider, IXamlMetadataProvider_Impl, IXamlType, XmlnsDefinition,
};
use crate::bindings::Microsoft::UI::Xaml::XamlTypeInfo::XamlControlsXamlMetaDataProvider;
use crate::bindings::Microsoft::UI::Xaml::{
    Application, ApplicationInitializationCallback, IApplicationOverrides,
    IApplicationOverrides_Impl, LaunchActivatedEventArgs,
};
use crate::bindings::Windows::UI::Xaml::Interop::TypeName;
use crate::bootstrap;
use windows_time::TimeSpan;

type LaunchCallback = Box<dyn FnOnce() + Send + 'static>;

#[allow(trivial_casts, clippy::ref_as_ptr, clippy::inline_always)]
mod secure_app_com {
    use super::{
        Application, Array, HSTRING, IApplicationOverrides, IApplicationOverrides_Impl,
        IXamlMetadataProvider, IXamlMetadataProvider_Impl, IXamlType, LaunchActivatedEventArgs,
        LaunchCallback, Mutex, Ref, TypeName, XamlControlsResources,
        XamlControlsXamlMetaDataProvider, XmlnsDefinition, implement,
    };

    #[implement(IApplicationOverrides, IXamlMetadataProvider)]
    pub(super) struct SecureFluentApp {
        provider: XamlControlsXamlMetaDataProvider,
        on_launched: Mutex<Option<LaunchCallback>>,
    }

    impl SecureFluentApp {
        pub(super) fn new(on_launched: LaunchCallback) -> windows_core::Result<Self> {
            let provider = XamlControlsXamlMetaDataProvider::new()?;
            Ok(Self {
                provider,
                on_launched: Mutex::new(Some(on_launched)),
            })
        }
    }

    impl IApplicationOverrides_Impl for SecureFluentApp_Impl {
        fn OnLaunched(&self, _args: Ref<'_, LaunchActivatedEventArgs>) -> windows_core::Result<()> {
            XamlControlsXamlMetaDataProvider::Initialize()?;
            if let Ok(app) = Application::Current()
                && let Ok(resources) = app.Resources()
                && let Ok(merged) = resources.MergedDictionaries()
                && let Ok(controls_res) = XamlControlsResources::new()
            {
                let _ = merged.Append(&controls_res);
            }

            let callback = {
                let mut guard = self.on_launched.lock().map_err(|_| {
                    windows_core::Error::from_hresult(windows_core::HRESULT(-2_147_467_259))
                })?;
                let taken = guard.take();
                drop(guard);
                taken
            };

            if let Some(cb) = callback {
                cb();
            }
            Ok(())
        }
    }

    impl IXamlMetadataProvider_Impl for SecureFluentApp_Impl {
        fn GetXamlType(&self, type_: &TypeName) -> windows_core::Result<IXamlType> {
            self.provider.GetXamlType(type_)
        }

        fn GetXamlTypeByFullName(&self, full_name: &HSTRING) -> windows_core::Result<IXamlType> {
            self.provider.GetXamlTypeByFullName(full_name)
        }

        fn GetXmlnsDefinitions(&self) -> windows_core::Result<Array<XmlnsDefinition>> {
            self.provider.GetXmlnsDefinitions()
        }
    }
}

use secure_app_com::SecureFluentApp;

/// Bootstraps `Windows App SDK` (when unpackaged), starts the native `WinUI 3` message loop,
/// registers `XamlControlsResources`, and executes `on_launched` on the UI thread.
///
/// # Errors
///
/// Returns an error if `Windows App Runtime` initialization or `Application::Start` fails.
pub fn run_winui_app<F>(app_title: &str, on_launched: F) -> windows_core::Result<()>
where
    F: FnOnce() + Send + 'static,
{
    bootstrap::harden_dll_search_path();
    if let Err(err) = bootstrap::bootstrap_runtime() {
        bootstrap::show_missing_runtime_prompt(app_title, &err);
        return Err(err);
    }
    bootstrap::initialize_ui_thread()?;

    let launch_cb: LaunchCallback = Box::new(on_launched);
    let slot = Mutex::new(Some(launch_cb));
    let start_res = Application::Start(&ApplicationInitializationCallback::new(move |_| {
        let cb = {
            let mut guard = slot.lock().map_err(|_| {
                windows_core::Error::from_hresult(windows_core::HRESULT(-2_147_467_259))
            })?;
            let taken = guard.take();
            drop(guard);
            taken
        };
        if let Some(launch_cb) = cb {
            let app_impl = SecureFluentApp::new(launch_cb)?;
            let _base: Application = Application::compose(app_impl)?;
        }
        Ok(())
    }));

    bootstrap::exit_ui_thread();
    start_res
}

/// Enqueues `f` onto the target `DispatcherQueue` without blocking the caller thread.
pub fn enqueue_ui<F>(queue: &DispatcherQueue, f: F) -> bool
where
    F: FnOnce() + Send + 'static,
{
    let cell = Mutex::new(Some(f));
    let handler = DispatcherQueueHandler::new(move || {
        let taken = cell.lock().ok().and_then(|mut g| g.take());
        if let Some(cb) = taken {
            cb();
        }
        Ok(())
    });
    queue.TryEnqueue(&handler).unwrap_or(false)
}

/// Creates and starts a repeating `DispatcherQueueTimer` firing every `interval_ms` milliseconds.
///
/// # Errors
///
/// Returns an error if creating or configuring the timer fails.
pub fn start_repeating_timer<F>(
    queue: &DispatcherQueue,
    interval_ms: u64,
    on_tick: F,
) -> windows_core::Result<DispatcherQueueTimer>
where
    F: Fn() + Send + 'static,
{
    let timer = queue.CreateTimer()?;
    let ticks_100ns = i64::try_from(interval_ms).unwrap_or(i64::MAX / 10_000) * 10_000;
    timer.SetInterval(TimeSpan {
        duration: ticks_100ns,
    })?;
    timer.SetIsRepeating(true)?;
    timer
        .Tick(move |_, _| {
            on_tick();
        })?
        .forget();
    timer.Start()?;
    Ok(timer)
}

/// Creates a single-shot `DispatcherQueueTimer` that automatically stops after firing `on_fire`.
///
/// # Errors
///
/// Returns an error if creating or configuring the timer fails.
pub fn schedule_once_timer<F>(
    queue: &DispatcherQueue,
    delay_ms: u64,
    on_fire: F,
) -> windows_core::Result<DispatcherQueueTimer>
where
    F: FnMut() + Send + 'static,
{
    let timer = queue.CreateTimer()?;
    let ticks_100ns = i64::try_from(delay_ms).unwrap_or(i64::MAX / 10_000) * 10_000;
    timer.SetInterval(TimeSpan {
        duration: ticks_100ns,
    })?;
    timer.SetIsRepeating(false)?;
    let cb_slot = Mutex::new(Some(on_fire));
    timer
        .Tick(move |sender, _| {
            if let Some(t) = sender.as_ref() {
                let _ = t.Stop();
            }
            let taken = cb_slot.lock().ok().and_then(|mut g| g.take());
            if let Some(mut cb) = taken {
                cb();
            }
        })?
        .forget();
    timer.Start()?;
    Ok(timer)
}
