//! Native `WinUI 3` protected `.spdf` Reader (`SecurePdfReader.exe`).

#![cfg_attr(
    all(target_os = "windows", not(test), not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(debug_assertions)]
mod debug_demo;
mod logic;
mod print_dialog;
mod protection;
mod reader_window;
mod render_worker;
mod session;
mod unlock_window;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use winui3::app::run_winui_app;
use winui3::bindings::Windows::Win32::HWND;
use winui3::bootstrap::{
    bootstrap_runtime, exit_ui_thread, harden_dll_search_path, show_missing_runtime_prompt,
};
use winui3::interop::message_box;

const APP_TITLE: &str = "SecurePdf 安全阅读器";

/// Process exit code: the application could not start or hit an unexpected error.
const EXIT_STARTUP_FAILED: i32 = 1;
/// Process exit code: Windows App Runtime is not installed.
const EXIT_RUNTIME_MISSING: i32 = 2;

static CRASH_REPORTED: AtomicBool = AtomicBool::new(false);

const fn no_owner() -> HWND {
    HWND(std::ptr::null_mut())
}

/// On an unexpected error anywhere in the process: release the decrypted document first, then
/// tell the user and exit, so plaintext does not linger behind an error dialog.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|_| {
        session::release_active_session_on_panic();
        if !CRASH_REPORTED.swap(true, Ordering::AcqRel) {
            message_box(
                no_owner(),
                APP_TITLE,
                "程序遇到意外错误，即将关闭。已打开的文档内容已从内存中清除。",
                true,
            );
        }
        std::process::exit(EXIT_STARTUP_FAILED);
    }));
}

fn main() {
    harden_dll_search_path();
    install_panic_hook();

    // Checked up front so a missing runtime is reported as exactly that.
    if let Err(err) = bootstrap_runtime() {
        show_missing_runtime_prompt(APP_TITLE, &err);
        std::process::exit(EXIT_RUNTIME_MISSING);
    }

    #[cfg(debug_assertions)]
    if let Some(seconds) = debug_demo::demo_seconds() {
        let started = run_winui_app(APP_TITLE, move || {
            if let Err(reason) = debug_demo::open_demo_reader(seconds) {
                eprintln!("demo reader failed: {reason}");
                exit_ui_thread();
            }
        });
        if let Err(err) = started {
            eprintln!("demo start failed: {err:?}");
        }
        return;
    }

    let initial_spdf = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    let started = run_winui_app(APP_TITLE, move || {
        if let Err(err) = unlock_window::open_unlock_window(initial_spdf) {
            message_box(no_owner(), APP_TITLE, &err.user_message(), true);
            exit_ui_thread();
        }
    });
    if let Err(err) = started {
        message_box(
            no_owner(),
            APP_TITLE,
            &format!(
                "阅读器启动失败，请重新启动电脑后再试；如仍无法启动，请重新安装本程序。\n（错误代码 0x{:08X}）",
                err.code().0.cast_unsigned()
            ),
            true,
        );
        std::process::exit(EXIT_STARTUP_FAILED);
    }
}
