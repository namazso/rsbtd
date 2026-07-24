// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The "start automatically at login" toggle: a value under the per-user
//! Run key pointing at this executable (the MSI writes the same value).

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "rsbtd";

pub fn enabled() -> bool {
    windows_registry::CURRENT_USER
        .open(RUN_KEY)
        .and_then(|key| key.get_string(VALUE_NAME))
        .is_ok()
}

pub fn set(enable: bool) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER
        .create(RUN_KEY)
        .map_err(|e| format!("cannot open the Run key: {e}"))?;
    if enable {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot resolve the executable path: {e}"))?;
        key.set_string(VALUE_NAME, &format!("\"{}\"", exe.display()))
            .map_err(|e| format!("cannot write the Run value: {e}"))
    } else if key.get_string(VALUE_NAME).is_ok() {
        key.remove_value(VALUE_NAME)
            .map_err(|e| format!("cannot remove the Run value: {e}"))
    } else {
        Ok(())
    }
}
