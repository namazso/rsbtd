// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`Error`] and [`Result`]: `boost::system` error codes captured at the C
//! boundary, with their category and rendered message.

// bindgen types C enum constants per platform ABI (unsigned on the
// GNU/clang targets, signed on MSVC), so the `as i32` casts below are
// required on one platform and flagged as unnecessary on the other.
#![allow(clippy::unnecessary_cast)]

use std::fmt;

use libctorrent_sys as sys;

/// Which error domain an [`Error`] belongs to (the `boost::system` categories).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Category {
    /// POSIX errno domain.
    Generic,
    /// OS error domain.
    System,
    /// libtorrent's own error codes.
    Libtorrent,
    /// HTTP status errors from trackers / web seeds.
    Http,
    /// Bencoding parse errors.
    Bdecode,
    Gzip,
    Socks,
    Upnp,
    Pcp,
    I2p,
    /// An error produced by the bindings themselves (not libtorrent).
    Bindings,
    /// An error produced by client code layered on the bindings (see
    /// [`Error::client`]).
    Client,
    /// A category (or exception) the bindings don't recognize.
    Unknown,
}

impl Category {
    fn from_ct(raw: i32) -> Category {
        match raw as sys::ct_error_category {
            sys::CT_ERROR_CAT_GENERIC => Category::Generic,
            sys::CT_ERROR_CAT_SYSTEM => Category::System,
            sys::CT_ERROR_CAT_LIBTORRENT => Category::Libtorrent,
            sys::CT_ERROR_CAT_HTTP => Category::Http,
            sys::CT_ERROR_CAT_BDECODE => Category::Bdecode,
            sys::CT_ERROR_CAT_GZIP => Category::Gzip,
            sys::CT_ERROR_CAT_SOCKS => Category::Socks,
            sys::CT_ERROR_CAT_UPNP => Category::Upnp,
            sys::CT_ERROR_CAT_PCP => Category::Pcp,
            sys::CT_ERROR_CAT_I2P => Category::I2p,
            _ => Category::Unknown,
        }
    }
}

/// An error reported by libtorrent (or the bindings' C boundary). The message
/// is captured eagerly at the call site, so the value is `Send + Sync`.
#[derive(Clone, Debug)]
pub struct Error {
    value: i32,
    category: Category,
    message: String,
}

impl Error {
    /// The raw error value within its [`Category`].
    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether this error reports an operation aborted at the caller's
    /// request (e.g. a [`set_piece_hashes`](crate::set_piece_hashes)
    /// progress callback returning false).
    pub fn is_cancelled(&self) -> bool {
        self.category == Category::Generic && self.value == sys::CT_ERRC_OPERATION_CANCELED as i32
    }

    /// An error originating in the bindings (e.g. a closed session).
    pub(crate) fn binding(message: &str) -> Error {
        Error {
            value: 0,
            category: Category::Bindings,
            message: message.to_owned(),
        }
    }

    /// An error originating in client code, for implementations of traits
    /// this crate calls back into (e.g.
    /// [`ClientData::from_bencode`](crate::ClientData::from_bencode)).
    pub fn client(message: impl Into<String>) -> Error {
        Error {
            value: 0,
            category: Category::Client,
            message: message.into(),
        }
    }

    /// A libtorrent error that carries only diagnostic text, no error code
    /// (e.g. a session error posted for an uncaught exception).
    pub(crate) fn from_message(message: String) -> Error {
        Error {
            value: 0,
            category: Category::Unknown,
            message,
        }
    }

    /// Converts a `ct_error` out-parameter into `Some(Error)` if it is set.
    /// Must be called on the thread that produced the error, before any
    /// other shim call on it (exception-carried messages are thread-local).
    pub(crate) fn from_ct(err: &sys::ct_error) -> Option<Error> {
        if err.category == sys::CT_ERROR_CAT_NONE as i32 {
            return None;
        }
        let message = unsafe {
            let mut msg = sys::ct_str::default();
            sys::ct_error_message(err, &mut msg);
            let text = if msg.ptr.is_null() {
                String::new()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(msg.ptr.cast(), msg.len))
                    .into_owned()
            };
            sys::ct_str_free(&mut msg);
            text
        };
        Some(Error {
            value: err.value,
            category: Category::from_ct(err.category),
            message,
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?}:{})", self.message, self.category, self.value)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Calls a shim function that reports failure through a trailing
/// `ct_error*` out-parameter and converts the outcome to a [`Result`].
#[inline]
pub(crate) fn with_error<T>(f: impl FnOnce(*mut sys::ct_error) -> T) -> Result<T> {
    let mut err = sys::ct_error {
        value: 0,
        category: sys::CT_ERROR_CAT_NONE as i32,
        category_ptr: std::ptr::null(),
    };
    let value = f(&mut err);
    match Error::from_ct(&err) {
        None => Ok(value),
        Some(e) => Err(e),
    }
}
