// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Crate-internal FFI conversion helpers shared by all modules.

use std::borrow::Cow;
use std::ffi::CString;
use std::path::Path;

use libctorrent_sys as sys;

use crate::error::{Error, Result};

/// The raw bytes of a path: the OS bytes on unix (no UTF-8 requirement),
/// UTF-8 elsewhere. Rejects embedded NULs: the path ends up in C string
/// APIs (`fopen`) downstream, which would silently truncate at the NUL.
pub(crate) fn path_bytes(path: &Path) -> Result<&[u8]> {
    let bytes = {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            path.as_os_str().as_bytes()
        }
        #[cfg(not(unix))]
        {
            path.to_str()
                .ok_or_else(|| Error::binding("path is not valid UTF-8"))?
                .as_bytes()
        }
    };
    if bytes.contains(&0) {
        return Err(Error::binding("path contains a NUL byte"));
    }
    Ok(bytes)
}

/// A NUL-terminated copy of a path for `const char*` parameters.
pub(crate) fn path_cstring(path: &Path) -> Result<CString> {
    CString::new(path_bytes(path)?).map_err(|_| Error::binding("path contains a NUL byte"))
}

/// SAFETY-relevant helper: `v` must be valid for 'a.
pub(crate) unsafe fn view_to_cow<'a>(v: sys::ct_str_view) -> Cow<'a, str> {
    if v.ptr.is_null() || v.len == 0 {
        return Cow::Borrowed("");
    }
    // SAFETY: caller contract.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(v.ptr.cast(), v.len) })
}

/// SAFETY-relevant helper: `v` must be valid for 'a.
pub(crate) unsafe fn span_to_slice<'a>(v: sys::ct_span) -> &'a [u8] {
    if v.ptr.is_null() || v.len == 0 {
        return &[];
    }
    // SAFETY: caller contract.
    unsafe { std::slice::from_raw_parts(v.ptr, v.len) }
}

/// A borrowed `ct_str_view` over a Rust string, for input parameters.
pub(crate) fn str_view(s: &str) -> sys::ct_str_view {
    sys::ct_str_view {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}

/// Consumes an owned `ct_str` into a `String` (lossily) and frees the box.
pub(crate) fn take_ct_str(mut s: sys::ct_str) -> String {
    let out = if s.ptr.is_null() {
        String::new()
    } else {
        // SAFETY: an owned ct_str's (ptr, len) is valid until freed.
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr.cast(), s.len) })
            .into_owned()
    };
    // SAFETY: s is an owned ct_str; freeing it exactly once.
    unsafe { sys::ct_str_free(&mut s) };
    out
}

/// Consumes an owned `ct_buf` into a `Vec<u8>` and frees the box.
pub(crate) fn take_ct_buf(mut b: sys::ct_buf) -> Vec<u8> {
    let out = if b.ptr.is_null() {
        Vec::new()
    } else {
        // SAFETY: an owned ct_buf's (ptr, len) is valid until freed.
        unsafe { std::slice::from_raw_parts(b.ptr, b.len) }.to_vec()
    };
    // SAFETY: b is an owned ct_buf; freeing it exactly once.
    unsafe { sys::ct_buf_free(&mut b) };
    out
}
