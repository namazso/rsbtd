// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Torrent creation API (create_torrent, list_files, set_piece_hashes).

use crate::error::{Error, Result};
use crate::util::path_cstring;
use libctorrent_sys as sys;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;
use std::time::SystemTime;

/// Builder for creating .torrent files.
pub struct CreateTorrent {
    ptr: *mut sys::ct_create_torrent,
}

/// Splits a relative torrent path into components on `/` (and `\` on
/// Windows, where libtorrent accepts both separators).
fn split_path(path: &[u8]) -> impl Iterator<Item = &[u8]> + Clone {
    path.split(|&b| b == b'/' || (cfg!(windows) && b == b'\\'))
}

impl CreateTorrent {
    /// Largest accepted piece size (128 MiB); libtorrent rejects anything larger.
    pub const MAX_PIECE_SIZE: u32 = 128 * 1024 * 1024;

    /// Largest piece count libtorrent can represent (`lt::file_storage::max_num_pieces`).
    pub const MAX_PIECE_COUNT: u32 = (1 << 30) - 1;

    /// `piece_size` is in bytes (0 = auto; must be a power of 2, min
    /// 16 KiB, max [`MAX_PIECE_SIZE`](Self::MAX_PIECE_SIZE)).
    pub fn new(files: &[FileEntry], piece_size: u32, flags: CreateFlags) -> Result<Self> {
        // Validate before the u32 -> i32 cast: e.g. 2^31 would wrap
        // negative and trip libtorrent asserts (which abort).
        if piece_size != 0 && piece_size > Self::MAX_PIECE_SIZE {
            return Err(Error::binding("piece_size exceeds 128 MiB maximum"));
        }
        // libtorrent applies the minimum inconsistently (silently clamped
        // for v2/hybrid, retained for v1), so enforce it here.
        if piece_size != 0 && piece_size < 16 * 1024 {
            return Err(Error::binding("piece_size is below the 16 KiB minimum"));
        }
        // One domain across every mode: libtorrent only rejects
        // non-powers-of-two for v2/hybrid (v1 accepts any multiple of
        // 16 KiB), which would make the accepted values mode-dependent.
        if piece_size != 0 && !piece_size.is_power_of_two() {
            return Err(Error::binding("piece_size must be a power of two"));
        }
        // `FileEntry::size` is public, so negative sizes and aggregate
        // overflow must be re-checked here regardless of FileEntry::new.
        let mut total_size: i64 = 0;
        let mut seen_paths = std::collections::HashSet::new();
        let mut common_root: Option<&[u8]> = None;
        for f in files.iter() {
            if f.size < 0 {
                return Err(Error::binding("file entry has a negative size"));
            }
            total_size = total_size
                .checked_add(f.size)
                .ok_or_else(|| Error::binding("total file size overflows"))?;

            let path = f.path.to_bytes();
            let display = || String::from_utf8_lossy(path).into_owned();
            if path.is_empty() {
                return Err(Error::binding("file entry has an empty path"));
            }
            // Duplicates would make the emitted metadata (and the manual
            // v2 hash mapping) ambiguous.
            if !seen_paths.insert(path) {
                return Err(Error::binding(&format!("duplicate path {}", display())));
            }
            let mut components = split_path(path);
            if components
                .clone()
                .any(|c| c.is_empty() || c == b"." || c == b"..")
            {
                return Err(Error::binding(&format!(
                    "path {} has an empty, `.`, or `..` component",
                    display()
                )));
            }
            // Multi-file torrents put every file under one root directory
            // that doubles as the torrent name; libtorrent only asserts
            // this, and otherwise silently rewrites disagreeing paths
            // under the first entry's root when generating.
            if files.len() > 1 {
                let root = components.next().expect("path is non-empty");
                if components.next().is_none() {
                    return Err(Error::binding(&format!(
                        "path {} must be inside the torrent's root directory \
                         (multi-file layout)",
                        display()
                    )));
                }
                match common_root {
                    None => common_root = Some(root),
                    Some(expected) if expected != root => {
                        return Err(Error::binding(&format!(
                            "path {} is outside the root directory {}: all \
                             files of a multi-file torrent share one root",
                            display(),
                            String::from_utf8_lossy(expected)
                        )));
                    }
                    Some(_) => {}
                }
            }
        }
        // libtorrent merely asserts the max_num_pieces domain;
        // canonicalization pads each file to a piece boundary, adding at
        // most one piece per file to the count.
        if piece_size != 0 {
            let ps = i64::from(piece_size);
            let partial = i64::from(total_size % ps != 0);
            let nfiles = i64::try_from(files.len()).unwrap_or(i64::MAX);
            let budget = i64::from(Self::MAX_PIECE_COUNT)
                .saturating_sub(partial)
                .saturating_sub(nfiles);
            if total_size / ps > budget {
                return Err(Error::binding(
                    "total size / piece_size exceeds the maximum piece count",
                ));
            }
        }

        let mut err = sys::ct_error::default();

        let c_entries: Vec<sys::ct_create_file_entry> = files
            .iter()
            .map(|f| sys::ct_create_file_entry {
                path: f.path.as_ptr(),
                size: f.size,
                flags: f.flags.bits(),
                mtime: f.mtime.unwrap_or(0),
                symlink_target: f
                    .symlink_target
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
            })
            .collect();

        let ptr = unsafe {
            sys::ct_create_torrent_new(
                c_entries.as_ptr(),
                c_entries.len(),
                piece_size as i32,
                flags.bits(),
                &mut err,
            )
        };

        if ptr.is_null() {
            return Err(
                Error::from_ct(&err).unwrap_or_else(|| Error::binding("create_torrent_new failed"))
            );
        }

        Ok(CreateTorrent { ptr })
    }

    /// Generate the .torrent file as a bencoded buffer.
    pub fn generate(&self) -> Result<Vec<u8>> {
        let mut err = sys::ct_error::default();
        let mut buf = unsafe { sys::ct_create_torrent_generate_buf(self.ptr, &mut err) };

        if buf.ptr.is_null() {
            return Err(
                Error::from_ct(&err).unwrap_or_else(|| Error::binding("generate_buf failed"))
            );
        }

        let data = unsafe { std::slice::from_raw_parts(buf.ptr, buf.len) };
        let vec = data.to_vec();
        unsafe { sys::ct_buf_free(&mut buf) };
        Ok(vec)
    }

    /// Set the comment field (UTF-8).
    pub fn set_comment(&mut self, comment: &str) -> Result<()> {
        let c_comment =
            CString::new(comment).map_err(|_| Error::binding("comment contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_comment(self.ptr, c_comment.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set the creator field (UTF-8).
    pub fn set_creator(&mut self, creator: &str) -> Result<()> {
        let c_creator =
            CString::new(creator).map_err(|_| Error::binding("creator contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_creator(self.ptr, c_creator.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set the creation date recorded in the torrent (seconds since the Unix
    /// epoch; defaults to the time the builder was created).
    pub fn set_creation_date(&mut self, timestamp: SystemTime) -> Result<()> {
        let secs = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::binding("Invalid timestamp"))?
            .as_secs() as i64;

        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_creation_date(self.ptr, secs, &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set a v1 piece hash (SHA-1, 20 bytes). Errors if `piece_index` is
    /// out of range (see [`num_pieces`](Self::num_pieces)) or this is a
    /// v2-only torrent.
    pub fn set_hash(&mut self, piece_index: u32, hash: &[u8; 20]) -> Result<()> {
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_hash(self.ptr, piece_index as i32, hash.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set a v2 piece hash (SHA-256, 32 bytes). `file_index` is the
    /// position in the entry list passed to [`CreateTorrent::new`]; the
    /// pad entries that v2/hybrid construction interleaves never consume
    /// indices. `piece` is relative to the first piece of the file.
    /// Errors if `file_index` is out of range or a pad file, if `piece`
    /// is out of range for that file, or if this is a v1-only torrent.
    pub fn set_hash2(&mut self, file_index: u32, piece: u32, hash: &[u8; 32]) -> Result<()> {
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_hash2(
                self.ptr,
                file_index as i32,
                piece as i32,
                hash.as_ptr(),
                &mut err,
            );
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Add a URL seed.
    pub fn add_url_seed(&mut self, url: &str) -> Result<()> {
        let c_url = CString::new(url).map_err(|_| Error::binding("url contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_add_url_seed(self.ptr, c_url.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Add a tracker URL at the specified tier.
    pub fn add_tracker(&mut self, url: &str, tier: i32) -> Result<()> {
        let c_url = CString::new(url).map_err(|_| Error::binding("url contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_add_tracker(self.ptr, c_url.as_ptr(), tier, &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Add a DHT bootstrap node.
    pub fn add_node(&mut self, hostname: &str, port: u16) -> Result<()> {
        let c_hostname =
            CString::new(hostname).map_err(|_| Error::binding("hostname contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_add_node(self.ptr, c_hostname.as_ptr(), port as i32, &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set root certificate for SSL torrents (PEM format).
    pub fn set_root_cert(&mut self, pem: &str) -> Result<()> {
        let c_pem = CString::new(pem).map_err(|_| Error::binding("pem contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_set_root_cert(self.ptr, c_pem.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set the private flag.
    pub fn set_priv(&mut self, is_private: bool) {
        unsafe {
            sys::ct_create_torrent_set_priv(self.ptr, is_private);
        }
    }

    /// Get the private flag.
    pub fn is_private(&self) -> bool {
        unsafe { sys::ct_create_torrent_priv(self.ptr) }
    }

    /// Add a similar torrent (BEP 38).
    pub fn add_similar_torrent(&mut self, info_hash: &[u8; 20]) -> Result<()> {
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_add_similar_torrent(self.ptr, info_hash.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Add a collection name (BEP 38).
    pub fn add_collection(&mut self, name: &str) -> Result<()> {
        let c_name = CString::new(name).map_err(|_| Error::binding("name contains null byte"))?;
        let mut err = sys::ct_error::default();
        unsafe {
            sys::ct_create_torrent_add_collection(self.ptr, c_name.as_ptr(), &mut err);
        }
        match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Check if this is a v2-only torrent.
    pub fn is_v2_only(&self) -> bool {
        unsafe { sys::ct_create_torrent_is_v2_only(self.ptr) }
    }

    /// Check if this is a v1-only torrent.
    pub fn is_v1_only(&self) -> bool {
        unsafe { sys::ct_create_torrent_is_v1_only(self.ptr) }
    }

    /// Get the number of pieces.
    pub fn num_pieces(&self) -> u32 {
        unsafe { sys::ct_create_torrent_num_pieces(self.ptr) as u32 }
    }

    /// Get the piece length in bytes.
    pub fn piece_length(&self) -> u32 {
        unsafe { sys::ct_create_torrent_piece_length(self.ptr) as u32 }
    }

    /// Get the size of piece `index` in bytes (the last piece may be
    /// shorter). `None` if out of range.
    pub fn piece_size(&self, index: u32) -> Option<u32> {
        if index >= self.num_pieces() {
            return None;
        }
        Some(unsafe { sys::ct_create_torrent_piece_size(self.ptr, index as i32) as u32 })
    }

    /// Get the total size of all files in bytes.
    pub fn total_size(&self) -> u64 {
        unsafe { sys::ct_create_torrent_total_size(self.ptr) as u64 }
    }
}

impl Drop for CreateTorrent {
    fn drop(&mut self) {
        unsafe {
            sys::ct_create_torrent_free(self.ptr);
        }
    }
}

unsafe impl Send for CreateTorrent {}
unsafe impl Sync for CreateTorrent {}

/// File entry for torrent creation.
pub struct FileEntry {
    path: CString,
    pub size: i64,
    pub flags: FileFlags,
    pub mtime: Option<i64>,
    symlink_target: Option<CString>,
}

impl FileEntry {
    /// Errors if `size` is negative.
    pub fn new(path: impl AsRef<Path>, size: i64) -> Result<Self> {
        let path = path_cstring(path.as_ref())?;
        if size < 0 {
            return Err(Error::binding("file size must not be negative"));
        }

        Ok(FileEntry {
            path,
            size,
            flags: FileFlags::empty(),
            mtime: None,
            symlink_target: None,
        })
    }

    /// Set file flags.
    pub fn with_flags(mut self, flags: FileFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Set modification time.
    pub fn with_mtime(mut self, mtime: i64) -> Self {
        self.mtime = Some(mtime);
        self
    }

    /// Set symlink target.
    pub fn with_symlink(mut self, target: impl AsRef<Path>) -> Result<Self> {
        self.symlink_target = Some(path_cstring(target.as_ref())?);
        self.flags |= FileFlags::SYMLINK;
        Ok(self)
    }
}

bitflags::bitflags! {
    /// File flags for torrent creation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FileFlags: u32 {
        const PAD_FILE = sys::CT_FILE_FLAG_PAD_FILE;
        const HIDDEN = sys::CT_FILE_FLAG_HIDDEN;
        const EXECUTABLE = sys::CT_FILE_FLAG_EXECUTABLE;
        const SYMLINK = sys::CT_FILE_FLAG_SYMLINK;
    }
}

bitflags::bitflags! {
    /// Creation flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CreateFlags: u32 {
        const MODIFICATION_TIME = sys::CT_CREATE_MODIFICATION_TIME;
        const SYMLINKS = sys::CT_CREATE_SYMLINKS;
        const V2_ONLY = sys::CT_CREATE_V2_ONLY;
        const V1_ONLY = sys::CT_CREATE_V1_ONLY;
        const CANONICAL_FILES = sys::CT_CREATE_CANONICAL_FILES;
        const NO_ATTRIBUTES = sys::CT_CREATE_NO_ATTRIBUTES;
        const CANONICAL_FILES_NO_TAIL_PADDING = sys::CT_CREATE_CANONICAL_FILES_NO_TAIL_PADDING;
    }
}

/// Recursively list files in a directory for torrent creation.
pub fn list_files(path: impl AsRef<Path>, flags: CreateFlags) -> Result<Vec<FileEntry>> {
    let c_path = path_cstring(path.as_ref())?;

    let mut err = sys::ct_error::default();
    let mut count = 0;

    let list = unsafe { sys::ct_list_files(c_path.as_ptr(), flags.bits(), &mut count, &mut err) };

    if list.is_null() {
        // Null means failure (*err set) or an empty directory (success,
        // count 0).
        return match Error::from_ct(&err) {
            Some(e) => Err(e),
            None => Ok(Vec::new()),
        };
    }

    let entries_slice = unsafe { std::slice::from_raw_parts(list, count) };
    let mut entries = Vec::with_capacity(count);

    for e in entries_slice {
        let path = unsafe { CStr::from_ptr(e.path) }.to_owned();
        let symlink_target = if !e.symlink_target.is_null() {
            Some(unsafe { CStr::from_ptr(e.symlink_target) }.to_owned())
        } else {
            None
        };

        entries.push(FileEntry {
            path,
            size: e.size,
            flags: FileFlags::from_bits_truncate(e.flags),
            mtime: if e.mtime != 0 { Some(e.mtime) } else { None },
            symlink_target,
        });
    }

    unsafe {
        sys::ct_create_file_list_free(list, count);
    }

    Ok(entries)
}

/// Progress callback for set_piece_hashes.
pub trait PieceHashProgress {
    /// Called once per hashed piece. Return `true` to keep hashing;
    /// `false` aborts the run and [`set_piece_hashes`] fails with an
    /// error for which [`Error::is_cancelled`] holds.
    fn on_piece(&mut self, piece_index: u32) -> bool;
}

impl<F> PieceHashProgress for F
where
    F: FnMut(u32) -> bool,
{
    fn on_piece(&mut self, piece_index: u32) -> bool {
        self(piece_index)
    }
}

/// Read files (relative to `base_path`) and compute piece hashes.
/// `progress` is an optional callback; returning `false` aborts the run
/// with an error for which [`Error::is_cancelled`] holds.
pub fn set_piece_hashes<P: AsRef<Path>, F: PieceHashProgress>(
    ct: &mut CreateTorrent,
    base_path: P,
    mut progress: Option<F>,
) -> Result<()> {
    let c_path = path_cstring(base_path.as_ref())?;

    let mut err = sys::ct_error::default();

    if let Some(ref mut callback) = progress {
        // Panics must not unwind into the C++ frames driving the hashing:
        // catch them here and resume the unwind after the FFI call returns.
        struct State<'a, F> {
            callback: &'a mut F,
            panic: Option<Box<dyn std::any::Any + Send>>,
        }

        extern "C" fn trampoline<F: PieceHashProgress>(
            piece_index: i32,
            userdata: *mut c_void,
        ) -> bool {
            // SAFETY: userdata is the State on set_piece_hashes' stack,
            // only invoked during the ct_set_piece_hashes call below.
            let state = unsafe { &mut *(userdata as *mut State<'_, F>) };
            if state.panic.is_some() {
                return false;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.callback.on_piece(piece_index as u32)
            }));
            match result {
                Ok(keep_going) => keep_going,
                Err(payload) => {
                    state.panic = Some(payload);
                    false
                }
            }
        }

        let mut state = State {
            callback,
            panic: None,
        };

        unsafe {
            sys::ct_set_piece_hashes(
                ct.ptr,
                c_path.as_ptr(),
                Some(trampoline::<F>),
                (&raw mut state).cast::<c_void>(),
                &mut err,
            );
        }
        if let Some(payload) = state.panic {
            std::panic::resume_unwind(payload);
        }
    } else {
        unsafe {
            sys::ct_set_piece_hashes(ct.ptr, c_path.as_ptr(), None, ptr::null_mut(), &mut err);
        }
    }

    match Error::from_ct(&err) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
