// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Settings dialog: a modal Win32 dialog built from an in-memory
//! `DLGTEMPLATE` (no resource script; keyboard navigation and the modal
//! loop come free with `DialogBoxIndirectParamW`).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DLGTEMPLATE, DialogBoxIndirectParamW, EndDialog, GetDlgItem, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, SetDlgItemTextW, SetWindowLongPtrW,
    WINDOW_LONG_PTR_INDEX, WM_COMMAND, WM_INITDIALOG, WS_BORDER, WS_CAPTION, WS_CHILD, WS_GROUP,
    WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::PCWSTR;

use super::config::{Editable, parse_listen};
use super::util::{confirm_box, error_box, from_wide, wide};

const IDOK: u16 = 1;
const IDCANCEL: u16 = 2;
const IDC_LISTEN: i32 = 1001;
const IDC_TOKEN: i32 = 1002;
const IDC_STATE_DIR: i32 = 1003;
const IDC_GRACE: i32 = 1004;

// Dialog/control styles not exposed as typed constants by the crate.
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
const DS_CENTER: u32 = 0x0800;
const ES_AUTOHSCROLL: u32 = 0x80;
const ES_NUMBER: u32 = 0x2000;
const BS_DEFPUSHBUTTON: u32 = 0x1;
const CLASS_BUTTON: u16 = 0x0080;
const CLASS_EDIT: u16 = 0x0081;
const CLASS_STATIC: u16 = 0x0082;
const DWLP_USER: i32 = 16; // x64: DWLP_MSGRESULT(8) + sizeof(DLGPROC)

/// Passed to the dialog proc through `DialogBoxIndirectParamW`'s LPARAM.
struct DialogState {
    initial: Editable,
    result: Option<Editable>,
}

/// Shows the modal Settings dialog; returns the validated new settings,
/// or `None` on cancel. Only one instance at a time.
pub fn show(owner: HWND, initial: Editable) -> Option<Editable> {
    static OPEN: AtomicBool = AtomicBool::new(false);
    if OPEN.swap(true, Ordering::AcqRel) {
        return None;
    }
    let template = build_template();
    let mut state = DialogState {
        initial,
        result: None,
    };
    unsafe {
        let module = GetModuleHandleW(None).expect("cannot get the module handle");
        DialogBoxIndirectParamW(
            Some(module.into()),
            template.as_ptr() as *const DLGTEMPLATE,
            Some(owner),
            Some(dialog_proc),
            LPARAM(&raw mut state as isize),
        );
    }
    OPEN.store(false, Ordering::Release);
    state.result
}

unsafe extern "system" fn dialog_proc(
    dialog: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    unsafe {
        match message {
            WM_INITDIALOG => {
                SetWindowLongPtrW(dialog, WINDOW_LONG_PTR_INDEX(DWLP_USER), lparam.0);
                let state = &*(lparam.0 as *const DialogState);
                set_text(dialog, IDC_LISTEN, &state.initial.listen);
                set_text(
                    dialog,
                    IDC_TOKEN,
                    state.initial.token.as_deref().unwrap_or(""),
                );
                set_text(
                    dialog,
                    IDC_STATE_DIR,
                    &state.initial.state_dir.to_string_lossy(),
                );
                set_text(
                    dialog,
                    IDC_GRACE,
                    &state.initial.shutdown_grace_secs.to_string(),
                );
                1
            }
            WM_COMMAND => {
                let command = (wparam.0 & 0xffff) as u16;
                match command {
                    IDOK => {
                        let state = GetWindowLongPtrW(dialog, WINDOW_LONG_PTR_INDEX(DWLP_USER))
                            as *mut DialogState;
                        if let Some(edited) = validate(dialog) {
                            (*state).result = Some(edited);
                            let _ = EndDialog(dialog, 1);
                        }
                        1
                    }
                    IDCANCEL => {
                        let _ = EndDialog(dialog, 0);
                        1
                    }
                    _ => 0,
                }
            }
            _ => 0,
        }
    }
}

fn set_text(dialog: HWND, control: i32, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetDlgItemTextW(dialog, control, PCWSTR(text.as_ptr()));
    }
}

fn get_text(dialog: HWND, control: i32) -> String {
    unsafe {
        let Ok(control) = GetDlgItem(Some(dialog), control) else {
            return String::new();
        };
        let len = GetWindowTextLengthW(control);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(control, &mut buf);
        from_wide(&buf)
    }
}

/// Reads and validates the fields; reports problems in message boxes and
/// keeps the dialog open (returns `None`) until they are fixed.
fn validate(dialog: HWND) -> Option<Editable> {
    let listen = get_text(dialog, IDC_LISTEN).trim().to_string();
    if let Err(e) = parse_listen(&listen) {
        error_box(&e);
        return None;
    }
    let token = get_text(dialog, IDC_TOKEN).trim().to_string();
    let token = if token.is_empty() {
        if !confirm_box(
            "No API token is set: anyone who can reach the listen address \
             fully controls the daemon.\n\nContinue without authentication?",
        ) {
            return None;
        }
        None
    } else {
        Some(token)
    };
    let state_dir = get_text(dialog, IDC_STATE_DIR).trim().to_string();
    if state_dir.is_empty() {
        error_box("The state directory must not be empty.");
        return None;
    }
    let state_dir = std::path::PathBuf::from(state_dir);
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        error_box(&format!(
            "Cannot create state directory {}:\n{e}",
            state_dir.display()
        ));
        return None;
    }
    let grace = get_text(dialog, IDC_GRACE);
    let Ok(shutdown_grace_secs) = grace.trim().parse::<u32>() else {
        error_box("Shutdown grace must be a number of seconds.");
        return None;
    };
    Some(Editable {
        listen,
        token,
        state_dir,
        shutdown_grace_secs,
    })
}

/// Serializes the dialog template (16-bit stream; items DWORD-aligned).
fn build_template() -> Vec<u16> {
    let mut t = TemplateWriter::default();

    // Header: style, exstyle, item count, x, y, cx, cy.
    t.dword(DS_SETFONT | DS_MODALFRAME | DS_CENTER | WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);
    t.dword(0);
    t.word(13); // item count; keep in sync with the items below
    t.rect(0, 0, 288, 104);
    t.word(0); // no menu
    t.word(0); // default dialog class
    t.string("rsbtd Settings");
    t.word(8); // font size
    t.string("MS Shell Dlg");

    let label = WS_CHILD.0 | WS_VISIBLE.0;
    let edit = WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL;
    let button = WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0;

    t.item(
        label,
        8,
        10,
        92,
        8,
        u16::MAX,
        CLASS_STATIC,
        "Listen address:",
    );
    t.item(
        edit | WS_GROUP.0,
        104,
        8,
        176,
        12,
        IDC_LISTEN as u16,
        CLASS_EDIT,
        "",
    );
    t.item(label, 8, 28, 92, 8, u16::MAX, CLASS_STATIC, "API token:");
    t.item(edit, 104, 26, 176, 12, IDC_TOKEN as u16, CLASS_EDIT, "");
    t.item(
        label,
        8,
        46,
        92,
        8,
        u16::MAX,
        CLASS_STATIC,
        "State directory:",
    );
    t.item(edit, 104, 44, 176, 12, IDC_STATE_DIR as u16, CLASS_EDIT, "");
    t.item(
        label,
        8,
        64,
        92,
        8,
        u16::MAX,
        CLASS_STATIC,
        "Shutdown grace (s):",
    );
    t.item(
        edit | ES_NUMBER,
        104,
        62,
        40,
        12,
        IDC_GRACE as u16,
        CLASS_EDIT,
        "",
    );
    t.item(
        label,
        8,
        87,
        160,
        8,
        u16::MAX,
        CLASS_STATIC,
        "Applying restarts the daemon.",
    );
    t.item(
        button | BS_DEFPUSHBUTTON | WS_GROUP.0,
        176,
        84,
        50,
        14,
        IDOK,
        CLASS_BUTTON,
        "OK",
    );
    t.item(button, 230, 84, 50, 14, IDCANCEL, CLASS_BUTTON, "Cancel");

    t.finish()
}

#[derive(Default)]
struct TemplateWriter {
    words: Vec<u16>,
    items: u16,
}

impl TemplateWriter {
    fn word(&mut self, v: u16) {
        self.words.push(v);
    }
    fn dword(&mut self, v: u32) {
        self.words.push((v & 0xffff) as u16);
        self.words.push((v >> 16) as u16);
    }
    fn rect(&mut self, x: i16, y: i16, cx: i16, cy: i16) {
        for v in [x, y, cx, cy] {
            self.words.push(v as u16);
        }
    }
    fn string(&mut self, s: &str) {
        self.words.extend(s.encode_utf16());
        self.words.push(0);
    }
    #[allow(clippy::too_many_arguments)]
    fn item(
        &mut self,
        style: u32,
        x: i16,
        y: i16,
        cx: i16,
        cy: i16,
        id: u16,
        class: u16,
        text: &str,
    ) {
        // Each DLGITEMTEMPLATE starts on a DWORD boundary.
        if self.words.len() % 2 == 1 {
            self.words.push(0);
        }
        self.dword(style);
        self.dword(0); // extended style
        self.rect(x, y, cx, cy);
        self.word(id);
        self.word(0xffff); // class by ordinal follows
        self.word(class);
        self.string(text);
        self.word(0); // no creation data
        self.items += 1;
    }
    fn finish(self) -> Vec<u16> {
        // The item count sits at word offset 4 (after style + exstyle).
        let mut words = self.words;
        words[4] = self.items;
        words
    }
}
