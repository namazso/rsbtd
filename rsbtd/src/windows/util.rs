// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small Win32 helpers shared by the tray app modules.

use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_TOPMOST, MB_YESNO,
    MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::PCWSTR;

/// NUL-terminated UTF-16 for passing to Win32 as `PCWSTR`.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 buffer (without the NUL) back to a `String`, lossily.
pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn message_box(title: &str, text: &str, style: MESSAGEBOX_STYLE) -> i32 {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            style | MB_TOPMOST,
        )
        .0
    }
}

/// A blocking error box; the tray app has no console to print to.
pub fn error_box(text: &str) {
    message_box("rsbtd", text, MB_OK | MB_ICONERROR);
}

/// A blocking OK/Cancel warning box; returns `true` on OK.
pub fn confirm_box(text: &str) -> bool {
    const IDOK: i32 = 1;
    message_box("rsbtd", text, MB_OKCANCEL | MB_ICONWARNING) == IDOK
}

/// A blocking Yes/No question box; returns `true` on Yes.
pub fn ask_box(text: &str) -> bool {
    const IDYES: i32 = 6;
    message_box("rsbtd", text, MB_YESNO | MB_ICONQUESTION) == IDYES
}
