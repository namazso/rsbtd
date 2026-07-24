// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Token validation for the comma-delimited libtorrent string settings.
//! A bad token cannot crash libtorrent, but it silently disables the
//! feature (or, for `outgoing_interfaces`, rejects incoming
//! connections), so the grouped setters reject malformed tokens instead
//! of forwarding them.

use super::error::SettingsError;

/// Whether `s` parses as an IPv6 address with an optional `%scope`
/// suffix (the scope names a local interface and is resolved by the
/// OS, not validated here).
fn ipv6_literal(s: &str) -> bool {
    let addr = s.split_once('%').map_or(s, |(addr, _scope)| addr);
    addr.parse::<std::net::Ipv6Addr>().is_ok()
}

/// A hostname/interface token for a setting whose entries carry a
/// `:port` suffix: non-empty, no comma or whitespace (the delimiters),
/// and a colon only inside a bracketed IPv6 literal (with an optional
/// `%scope`).
pub(crate) fn host_token(setting: &'static str, field: &str, s: &str) -> Result<(), SettingsError> {
    if s.is_empty() {
        return Err(SettingsError::new(
            setting,
            format!("{field} must not be empty"),
        ));
    }
    if s.contains(',') || s.chars().any(char::is_whitespace) {
        return Err(SettingsError::new(
            setting,
            format!("{field} {s:?} must not contain commas or whitespace"),
        ));
    }
    if let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        if !ipv6_literal(inner) {
            return Err(SettingsError::new(
                setting,
                format!("{field} {s:?} brackets must contain an IPv6 address"),
            ));
        }
    } else if s.contains(':') {
        return Err(SettingsError::new(
            setting,
            format!("{field} {s:?} contains `:` but is not a bracketed IPv6 literal"),
        ));
    }
    Ok(())
}

/// A standalone host token (no `:port` suffix in the stored string): a
/// device name, hostname, IPv4 address, or *bare* IPv6 address with an
/// optional `%scope`.
pub(crate) fn bare_host_token(
    setting: &'static str,
    field: &str,
    s: &str,
) -> Result<(), SettingsError> {
    if s.is_empty() {
        return Err(SettingsError::new(
            setting,
            format!("{field} must not be empty"),
        ));
    }
    if s.contains(',') || s.chars().any(char::is_whitespace) {
        return Err(SettingsError::new(
            setting,
            format!("{field} {s:?} must not contain commas or whitespace"),
        ));
    }
    if s.contains(':') && !ipv6_literal(s) {
        return Err(SettingsError::new(
            setting,
            format!("{field} {s:?} contains `:` but is not a bare (unbracketed) IPv6 address"),
        ));
    }
    Ok(())
}
