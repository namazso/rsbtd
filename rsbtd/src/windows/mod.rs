// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Windows tray application.
//!
//! On Windows, rsbtd is a per-user tray app: the engine and API server
//! run in-process on a background thread (see [`supervisor`]), config
//! lives in the registry (see [`config`]), and a notification-area icon
//! provides Settings / autostart / open-in-browser / exit.
//!
//! Registry values under `HKCU\Software\rsbtd` (all optional; the
//! installer writes none of them). A start without any `Token` value is
//! the first start: the app generates and stores a random token and
//! asks once whether to enable autostart — an *empty* `Token` is the
//! deliberate "no authentication" choice and stays as it is.
//!
//! | Value               | Type         | Default                          |
//! |---------------------|--------------|----------------------------------|
//! | `Token`             | REG_SZ       | generated on the first start; empty = unauthenticated |
//! | `Listen`            | REG_SZ       | `127.0.0.1:3928`                 |
//! | `StateDir`          | REG_SZ       | `%LOCALAPPDATA%\rsbtd\state`     |
//! | `ServeRoot`         | REG_SZ       | `webui` next to the executable   |
//! | `ShutdownGraceSecs` | REG_DWORD    | 15                               |
//! | `Cors`              | REG_MULTI_SZ | empty                            |
//! | `LogFilter`         | REG_SZ       | `info`                           |
//!
//! Logs go to `%LOCALAPPDATA%\rsbtd\logs` (the GUI subsystem has no
//! console). Threading: the Win32 message loop owns the main thread; the
//! daemon runs on the supervisor thread and pokes the hidden window with
//! `WM_APP_STATUS` on status changes.

pub mod autostart;
pub mod browser;
pub mod config;
pub mod logging;
pub mod settings;
pub mod supervisor;
pub mod tray;
mod util;

use std::process::ExitCode;
use std::sync::OnceLock;

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    IsWindow, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
    TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_DESTROY, WM_ENDSESSION,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_QUERYENDSESSION, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::PCWSTR;

use crate::Config;
use supervisor::{Status, Supervisor};
use util::{ask_box, error_box, wide};

/// Posted by the shell for tray icon interactions.
const WM_APP_TRAY: u32 = WM_APP + 1;
/// Posted by the supervisor thread after a status change.
const WM_APP_STATUS: u32 = WM_APP + 2;

struct App {
    supervisor: Supervisor,
    /// Broadcast by Explorer when the taskbar (re)starts; re-add the icon.
    taskbar_created_message: u32,
}

static APP: OnceLock<App> = OnceLock::new();

fn app() -> Option<&'static App> {
    APP.get()
}

/// Entry point for the Windows binary (see `main.rs`).
pub fn tray_main() -> ExitCode {
    // One tray app per session; a second launch exits quietly. The handle
    // leaks deliberately so the mutex lives as long as the process.
    let mutex_name = wide(r"Local\rsbtd-tray-singleton");
    let mutex = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) };
    match mutex {
        Ok(_) if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS => return ExitCode::SUCCESS,
        Ok(_) => {}
        Err(e) => {
            error_box(&format!("Cannot create the single-instance mutex: {e}"));
            return ExitCode::FAILURE;
        }
    }

    let _log_guard = match logging::init(&config::log_filter()) {
        Ok(guard) => guard,
        Err(e) => {
            error_box(&format!("Cannot set up logging: {e}"));
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("rsbtd {} starting", env!("CARGO_PKG_VERSION"));

    // Generate and store the API token on the first start for this user.
    // A failure is reported but not fatal: the daemon then comes up like
    // a bare unconfigured exe (unauthenticated on loopback).
    let first_run = match config::initialize() {
        Ok(first) => first,
        Err(e) => {
            tracing::error!("first-start initialization failed: {e}");
            error_box(&format!("Cannot initialize the configuration: {e}"));
            false
        }
    };

    let hwnd = match create_hidden_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            tracing::error!("cannot create the tray window: {e}");
            error_box(&format!("Cannot create the tray window: {e}"));
            return ExitCode::FAILURE;
        }
    };

    // Window first, then supervisor, then APP: status notifications post
    // to the window and just wait in the queue until the loop starts.
    let notify_hwnd = hwnd.0 as isize;
    let supervisor = Supervisor::spawn(config::load(), move || {
        let hwnd = HWND(notify_hwnd as *mut _);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_STATUS, WPARAM(0), LPARAM(0));
        }
    });
    let app = App {
        supervisor,
        taskbar_created_message: unsafe {
            let name = wide("TaskbarCreated");
            RegisterWindowMessageW(PCWSTR(name.as_ptr()))
        },
    };
    if APP.set(app).is_err() {
        unreachable!("tray_main runs once");
    }

    // No taskbar is fine: the daemon still runs, and the icon reappears
    // via TaskbarCreated when Explorer arrives.
    if !tray::add(hwnd, WM_APP_TRAY, &tooltip_text()) {
        tracing::warn!("cannot add the tray icon; continuing without it");
    }

    // The one-time autostart offer, deliberately after the daemon is
    // already starting: the answer is not load-bearing, and an ignored
    // prompt (or a headless session) must not keep the API down. The
    // prompt's modal loop dispatches window messages, so the installer's
    // WM_CLOSE still shuts us down mid-prompt — but then MessageBoxW
    // merely reports the default button once WM_QUIT breaks its loop,
    // so a Yes only counts while our window survived the prompt.
    if first_run {
        let yes = ask_box(
            "Start rsbtd automatically when you sign in?\n\n\
             You can change this later from the tray menu.",
        );
        if yes
            && unsafe { IsWindow(Some(hwnd)) }.as_bool()
            && let Err(e) = autostart::set(true)
        {
            error_box(&format!("Cannot enable autostart: {e}"));
        }
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    tracing::info!("rsbtd exiting");
    ExitCode::SUCCESS
}

fn create_hidden_window() -> Result<HWND, windows::core::Error> {
    // A real (never-shown) top-level window, not a message-only one:
    // WM_QUERYENDSESSION and the installer's WM_CLOSE only reach windows
    // that exist in the normal broadcast order.
    unsafe {
        let module = GetModuleHandleW(None)?;
        let class_name = wide("rsbtd-tray");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: module.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let title = wide("rsbtd");
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
    }
}

fn tooltip_text() -> String {
    match app().map(|app| app.supervisor.status()) {
        Some(Status::Running { addr, .. }) => format!("rsbtd — running on {addr}"),
        Some(Status::Failed(_)) => "rsbtd — failed to start (see Settings)".to_string(),
        Some(Status::Stopped) => "rsbtd — stopped".to_string(),
        Some(Status::Starting) | None => "rsbtd — starting…".to_string(),
    }
}

fn open_in_browser() {
    if let Some(app) = app()
        && let Status::Running { addr, token } = app.supervisor.status()
    {
        browser::open(addr, token.as_deref());
    }
}

fn show_settings(hwnd: HWND) {
    let initial = match config::editable() {
        Ok(editable) => editable,
        Err(e) => {
            error_box(&format!("Cannot read the configuration: {e}"));
            return;
        }
    };
    let Some(edited) = settings::show(hwnd, initial) else {
        return;
    };
    if let Err(e) = config::save(&edited) {
        error_box(&format!("Cannot save the configuration: {e}"));
        return;
    }
    // Re-read the full config so non-dialog values (Cors, ServeRoot) stay
    // in effect, then restart the daemon on it.
    match config::load() {
        Ok(config) => apply(config),
        Err(e) => error_box(&format!("The saved configuration is invalid: {e}")),
    }
}

fn apply(config: Config) {
    if let Some(app) = app() {
        app.supervisor.apply(config);
    }
}

fn toggle_autostart() {
    if let Err(e) = autostart::set(!autostart::enabled()) {
        error_box(&format!("Cannot update the autostart setting: {e}"));
    }
}

fn handle_menu(hwnd: HWND) {
    let running = matches!(
        app().map(|app| app.supervisor.status()),
        Some(Status::Running { .. })
    );
    let state = tray::MenuState {
        running,
        autostart: autostart::enabled(),
    };
    match tray::show_menu(hwnd, &state) {
        Some(tray::MenuChoice::OpenInBrowser) => open_in_browser(),
        Some(tray::MenuChoice::Settings) => show_settings(hwnd),
        Some(tray::MenuChoice::ToggleAutostart) => toggle_autostart(),
        Some(tray::MenuChoice::Exit) => unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        },
        None => {}
    }
}

/// Stops the daemon; blocking but bounded (HTTP grace + persistence
/// deadline). Idempotent, so both the Exit path and a logoff can call it.
fn shutdown_daemon() {
    if let Some(app) = app() {
        app.supervisor.shutdown();
    }
}

extern "system" fn wndproc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_APP_TRAY => {
            match lparam.0 as u32 {
                WM_RBUTTONUP | WM_LBUTTONUP => handle_menu(hwnd),
                WM_LBUTTONDBLCLK => open_in_browser(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_STATUS => {
            tray::set_tooltip(hwnd, &tooltip_text());
            if let Some(app) = app()
                && let Status::Failed(e) = app.supervisor.status()
            {
                tray::warn_balloon(hwnd, "rsbtd failed to start", &e);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Exit menu item, or the installer closing us for an upgrade.
            tray::remove(hwnd);
            shutdown_daemon();
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_QUERYENDSESSION => {
            // Tell the shutdown UI why we might take a moment, and agree
            // to end. Windows only budgets a few seconds once
            // WM_ENDSESSION arrives, so start flushing state now, without
            // blocking the message loop (the session end may still be
            // cancelled, in which case the daemon just keeps running).
            if let Some(app) = app() {
                app.supervisor.checkpoint();
            }
            let reason = wide("Saving torrent state…");
            unsafe {
                let _ = ShutdownBlockReasonCreate(hwnd, PCWSTR(reason.as_ptr()));
            }
            LRESULT(1)
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                tracing::info!("session ending; stopping the daemon");
                tray::remove(hwnd);
                // The fast path: state was already checkpointed in
                // WM_QUERYENDSESSION, and the OS kills the process about
                // five seconds after this handler returns — the regular
                // stop's transport graces alone would overrun that.
                if let Some(app) = app() {
                    app.supervisor.shutdown_fast();
                }
            }
            unsafe {
                let _ = ShutdownBlockReasonDestroy(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => {
            // Explorer restarted: the icon registration died with it.
            if let Some(app) = app()
                && message == app.taskbar_created_message
                && message != 0
            {
                tray::add(hwnd, WM_APP_TRAY, &tooltip_text());
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}
