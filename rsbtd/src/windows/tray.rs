// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The notification-area icon and its context menu.

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, IDI_APPLICATION, IMAGE_ICON,
    LR_DEFAULTSIZE, LoadIconW, LoadImageW, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING,
    SetForegroundWindow, SetMenuDefaultItem, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenuEx,
};
use windows::core::PCWSTR;

use super::util::wide;

/// Identifies our single icon within the window's tray registrations.
const ICON_UID: u32 = 1;

/// The icon resource id `build.rs` embeds (`set_icon_with_id(..., "1")`).
const ICON_RESOURCE_ID: u16 = 1;

/// Menu commands, returned by [`show_menu`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuChoice {
    OpenInBrowser,
    Settings,
    ToggleAutostart,
    Exit,
}

pub struct MenuState {
    pub running: bool,
    pub autostart: bool,
}

fn app_icon() -> HICON {
    unsafe {
        let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .expect("cannot get the module handle");
        LoadImageW(
            Some(module.into()),
            PCWSTR(ICON_RESOURCE_ID as usize as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )
        .map(|handle| HICON(handle.0))
        // A plain `cargo build` without the embedded resource still runs.
        .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).expect("cannot load a stock icon"))
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        ..Default::default()
    }
}

fn set_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    let tip = wide(tip);
    let len = tip.len().min(data.szTip.len() - 1);
    data.szTip[..len].copy_from_slice(&tip[..len]);
}

/// Adds the tray icon; `callback_message` is posted to `hwnd` for icon
/// interactions. Failure (no shell running) is tolerated: the daemon
/// works without the icon.
pub fn add(hwnd: HWND, callback_message: u32, tip: &str) -> bool {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = callback_message;
    data.hIcon = app_icon();
    set_tip(&mut data, tip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool()
}

/// Updates the hover tooltip (daemon status).
pub fn set_tooltip(hwnd: HWND, tip: &str) {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_TIP;
    set_tip(&mut data, tip);
    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

/// Shows a warning balloon (e.g. daemon failed to start).
pub fn warn_balloon(hwnd: HWND, title: &str, text: &str) {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_WARNING;
    let title = wide(title);
    let len = title.len().min(data.szInfoTitle.len() - 1);
    data.szInfoTitle[..len].copy_from_slice(&title[..len]);
    let text = wide(text);
    let len = text.len().min(data.szInfo.len() - 1);
    data.szInfo[..len].copy_from_slice(&text[..len]);
    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
}

pub fn remove(hwnd: HWND) {
    let data = icon_data(hwnd);
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

/// Pops up the context menu at the cursor and returns the choice. Blocks
/// in the menu's modal loop (fine: it runs on the hidden tray window).
pub fn show_menu(hwnd: HWND, state: &MenuState) -> Option<MenuChoice> {
    const CMD_OPEN: usize = 101;
    const CMD_SETTINGS: usize = 102;
    const CMD_AUTOSTART: usize = 103;
    const CMD_EXIT: usize = 104;

    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let open_flags = if state.running {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let open = wide("Open in browser");
        let _ = AppendMenuW(menu, open_flags, CMD_OPEN, PCWSTR(open.as_ptr()));
        let _ = SetMenuDefaultItem(menu, CMD_OPEN as u32, 0 /* by id, not position */);
        let settings = wide("Settings…");
        let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, PCWSTR(settings.as_ptr()));
        let auto_flags = if state.autostart {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let autostart = wide("Start automatically");
        let _ = AppendMenuW(menu, auto_flags, CMD_AUTOSTART, PCWSTR(autostart.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let exit = wide("Exit");
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT, PCWSTR(exit.as_ptr()));

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        // Required for the menu to dismiss when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let choice = TrackPopupMenuEx(
            menu,
            (TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_NONOTIFY).0,
            cursor.x,
            cursor.y,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match choice.0 as usize {
            CMD_OPEN => Some(MenuChoice::OpenInBrowser),
            CMD_SETTINGS => Some(MenuChoice::Settings),
            CMD_AUTOSTART => Some(MenuChoice::ToggleAutostart),
            CMD_EXIT => Some(MenuChoice::Exit),
            _ => None,
        }
    }
}
