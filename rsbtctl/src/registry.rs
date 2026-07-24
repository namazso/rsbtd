// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Installed-daemon defaults. The tray app keeps the daemon's listen
//! address and API token under `HKCU\Software\rsbtd` (see
//! `rsbtd::windows::config`; the token is generated on its first
//! start), so a plain `rsbtctl list` on the same machine talks to the
//! installed daemon without any flags.

const REGISTRY_KEY: &str = r"Software\rsbtd";
const DEFAULT_LISTEN: &str = "127.0.0.1:3928";

/// Converts a registry read into an optional value: `Ok(None)` only when
/// the value (or key) does not exist. Any other failure — wrong type,
/// access denied — is a hard error: silently defaulting a
/// present-but-malformed `Token` would disable authentication.
fn optional<T>(result: windows_registry::Result<T>, what: &str) -> Result<Option<T>, String> {
    // HRESULT-encoded ERROR_FILE_NOT_FOUND: a missing value or key.
    const FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.code().0 == FILE_NOT_FOUND => Ok(None),
        Err(e) => Err(format!(
            "registry value HKCU\\{REGISTRY_KEY}\\{what} is invalid or unreadable: {e}"
        )),
    }
}

/// `(url, token)` of the installed daemon, or `None` when rsbtd never
/// wrote its config key. A missing `Listen` falls back to the daemon's
/// own default; an empty or missing `Token` means authentication is
/// disabled.
pub fn daemon_defaults() -> Result<Option<(String, Option<String>)>, String> {
    // Test seam: the CLI suite points this at an absent key so it stays
    // hermetic on machines where an installed rsbtd wrote real config.
    let key_path = std::env::var("RSBTCTL_REGISTRY_KEY").unwrap_or_else(|_| REGISTRY_KEY.into());
    let Some(key) = optional(windows_registry::CURRENT_USER.open(&key_path), "")? else {
        return Ok(None);
    };
    let listen = optional(key.get_string("Listen"), "Listen")?
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
    let token = optional(key.get_string("Token"), "Token")?.filter(|value| !value.is_empty());
    Ok(Some((format!("http://{listen}"), token)))
}
