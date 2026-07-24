// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! "Open in browser": launches the served web UI in the default browser,
//! passing the API token as a hash param the UI consumes (and strips from
//! the address bar) on load.

use std::net::SocketAddr;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use super::util::{error_box, wide};

/// Escape everything but RFC 3986 unreserved characters (the token rides
/// in the URL fragment).
const FRAGMENT_ESCAPE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// The UI URL for a daemon bound to `addr`. An unspecified bind address
/// (`0.0.0.0`/`[::]`) becomes the loopback address of the same family.
pub fn ui_url(addr: SocketAddr, token: Option<&str>) -> String {
    let host = if addr.ip().is_unspecified() {
        if addr.is_ipv6() { "[::1]" } else { "127.0.0.1" }.to_string()
    } else if addr.is_ipv6() {
        format!("[{}]", addr.ip())
    } else {
        addr.ip().to_string()
    };
    let mut url = format!("http://{host}:{}/", addr.port());
    if let Some(token) = token {
        url.push_str("#token=");
        url.push_str(&utf8_percent_encode(token, FRAGMENT_ESCAPE).to_string());
    }
    url
}

pub fn open(addr: SocketAddr, token: Option<&str>) {
    let url = wide(&ui_url(addr, token));
    let verb = wide("open");
    // ShellExecuteW reports failure via a pseudo-HINSTANCE <= 32.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(url.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize <= 32 {
        let code = result.0 as usize;
        tracing::warn!("cannot open the browser (ShellExecuteW returned {code})");
        error_box(&format!(
            "Cannot open the browser (ShellExecuteW returned {code})."
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_for_unspecified_bind() {
        let url = ui_url("0.0.0.0:3928".parse().unwrap(), Some("abc"));
        assert_eq!(url, "http://127.0.0.1:3928/#token=abc");
    }

    #[test]
    fn ipv6_loopback_for_unspecified_ipv6_bind() {
        let url = ui_url("[::]:3928".parse().unwrap(), Some("abc"));
        assert_eq!(url, "http://[::1]:3928/#token=abc");
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        let url = ui_url("[::1]:8080".parse().unwrap(), None);
        assert_eq!(url, "http://[::1]:8080/");
    }

    #[test]
    fn token_is_percent_encoded() {
        let url = ui_url("127.0.0.1:1".parse().unwrap(), Some("a&b #c"));
        assert_eq!(url, "http://127.0.0.1:1/#token=a%26b%20%23c");
    }
}
