// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Torrent metadata: [`TorrentInfo`] and its file list.

use std::borrow::Cow;
use std::ptr::NonNull;

use libctorrent_sys as sys;

use crate::types::{InfoHash, PeerRequest, Sha1Hash, Sha256Hash};
use crate::util::{span_to_slice, take_ct_str, view_to_cow};

/// The immutable metadata of a torrent (the parsed info section plus the
/// file layout). Obtained from
/// [`AddTorrentParams::ti`](crate::AddTorrentParams::ti), or constructed
/// metadata-less from an [`InfoHash`]. Cloning is cheap (shared object).
pub struct TorrentInfo {
    ptr: NonNull<sys::ct_torrent_info>,
}

// SAFETY: the underlying torrent_info is never mutated through this API and
// shared ownership uses an atomically refcounted shared_ptr.
unsafe impl Send for TorrentInfo {}
unsafe impl Sync for TorrentInfo {}

impl TorrentInfo {
    /// A metadata-less `TorrentInfo` carrying only an info-hash (magnet-style).
    /// [`is_valid`](Self::is_valid) returns `false` for it.
    pub fn from_info_hash(hash: &InfoHash) -> TorrentInfo {
        let ih = hash.to_ct();
        // SAFETY: ih is only read during the call.
        let ptr = unsafe { sys::ct_torrent_info_from_info_hash(&ih) };
        TorrentInfo {
            ptr: NonNull::new(ptr).expect("out of memory"),
        }
    }

    /// Wraps a shim handle. Takes ownership; `ptr` must be valid.
    pub(crate) unsafe fn from_owned_ptr(ptr: *mut sys::ct_torrent_info) -> Option<TorrentInfo> {
        NonNull::new(ptr).map(|ptr| TorrentInfo { ptr })
    }

    pub(crate) fn as_ptr(&self) -> *const sys::ct_torrent_info {
        self.ptr.as_ptr()
    }

    /// Whether metadata is loaded. `false` for an info-hash-only object
    /// (e.g. from a magnet link whose metadata hasn't been fetched yet).
    pub fn is_valid(&self) -> bool {
        // SAFETY: self.ptr is a valid handle (all accessors below alike).
        unsafe { sys::ct_torrent_info_is_valid(self.as_ptr()) }
    }

    /// The name of the torrent (UTF-8).
    pub fn name(&self) -> Cow<'_, str> {
        unsafe { view_to_cow(sys::ct_torrent_info_name(self.as_ptr())) }
    }

    /// Total byte count of the torrent's data, including pad files.
    pub fn total_size(&self) -> i64 {
        unsafe { sys::ct_torrent_info_total_size(self.as_ptr()) }
    }

    /// The sum of all non-pad file sizes (what is actually written to disk).
    pub fn size_on_disk(&self) -> i64 {
        unsafe { sys::ct_torrent_info_size_on_disk(self.as_ptr()) }
    }

    /// The byte length of each piece (except possibly the last one).
    pub fn piece_length(&self) -> i32 {
        unsafe { sys::ct_torrent_info_piece_length(self.as_ptr()) }
    }

    pub fn num_pieces(&self) -> i32 {
        unsafe { sys::ct_torrent_info_num_pieces(self.as_ptr()) }
    }

    /// The number of 16 kiB blocks in a typical piece.
    pub fn blocks_per_piece(&self) -> i32 {
        unsafe { sys::ct_torrent_info_blocks_per_piece(self.as_ptr()) }
    }

    pub fn info_hashes(&self) -> InfoHash {
        let h = unsafe { sys::ct_torrent_info_info_hashes(self.as_ptr()) };
        InfoHash::from_ct(h)
    }

    /// Whether the torrent has v1 (SHA-1) metadata.
    pub fn has_v1(&self) -> bool {
        unsafe { sys::ct_torrent_info_has_v1(self.as_ptr()) }
    }

    /// Whether the torrent has v2 (SHA-256, BEP 52) metadata.
    pub fn has_v2(&self) -> bool {
        unsafe { sys::ct_torrent_info_has_v2(self.as_ptr()) }
    }

    /// The exact size of the given piece, accounting for a shorter last piece.
    /// `None` if the index is out of range.
    pub fn piece_size(&self, piece: i32) -> Option<i32> {
        self.valid_piece(piece)
            .then(|| unsafe { sys::ct_torrent_info_piece_size(self.as_ptr(), piece) })
    }

    /// Like [`piece_size`](Self::piece_size), but for computing request
    /// sizes: v2-only torrents truncate pieces at file boundaries.
    pub fn piece_size_for_req(&self, piece: i32) -> Option<i32> {
        self.valid_piece(piece)
            .then(|| unsafe { sys::ct_torrent_info_piece_size_for_req(self.as_ptr(), piece) })
    }

    /// The SHA-1 hash of a piece. `None` for out-of-range indices and for
    /// v2-only torrents (which have no per-piece SHA-1 hashes).
    pub fn hash_for_piece(&self, piece: i32) -> Option<Sha1Hash> {
        if !self.valid_piece(piece) || !self.has_v1() {
            return None;
        }
        let h = unsafe { sys::ct_torrent_info_hash_for_piece(self.as_ptr(), piece) };
        Some(Sha1Hash::from_ct(&h))
    }

    /// The SSL root certificate (PEM) for SSL torrents; `None` otherwise.
    pub fn ssl_cert(&self) -> Option<Cow<'_, str>> {
        let v = unsafe { sys::ct_torrent_info_ssl_cert(self.as_ptr()) };
        if v.len == 0 {
            return None;
        }
        Some(unsafe { view_to_cow(v) })
    }

    /// Whether the torrent is private (BEP 27): no DHT or LSD announces.
    pub fn is_private(&self) -> bool {
        unsafe { sys::ct_torrent_info_is_private(self.as_ptr()) }
    }

    /// Whether this is an i2p torrent (a tracker URL has an `.i2p` domain).
    pub fn is_i2p(&self) -> bool {
        unsafe { sys::ct_torrent_info_is_i2p(self.as_ptr()) }
    }

    /// The raw bencoded info section.
    pub fn info_section(&self) -> &[u8] {
        unsafe { span_to_slice(sys::ct_torrent_info_info_section(self.as_ptr())) }
    }

    /// BEP 38: info-hashes of torrents that share files with this one.
    pub fn similar_torrents(&self) -> Vec<Sha1Hash> {
        let bytes = crate::util::take_ct_buf(unsafe {
            sys::ct_torrent_info_similar_torrents(self.as_ptr())
        });
        bytes
            .chunks_exact(20)
            .map(|c| Sha1Hash(c.try_into().unwrap()))
            .collect()
    }

    /// BEP 38: the collections this torrent belongs to.
    pub fn collections(&self) -> Vec<String> {
        // SAFETY: the list handle is valid until freed below.
        unsafe {
            let list = sys::ct_torrent_info_collections(self.as_ptr());
            if list.is_null() {
                return Vec::new();
            }
            let len = sys::ct_str_list_len(list);
            let items = (0..len)
                .map(|i| view_to_cow(sys::ct_str_list_get(list, i)).into_owned())
                .collect();
            sys::ct_str_list_free(list);
            items
        }
    }

    // ---- file list ------------------------------------------------------

    pub fn num_files(&self) -> i32 {
        unsafe { sys::ct_torrent_info_num_files(self.as_ptr()) }
    }

    /// Accessor for the file at `index`, or `None` if out of range.
    pub fn file(&self, index: i32) -> Option<File<'_>> {
        (index >= 0 && index < self.num_files()).then_some(File { info: self, index })
    }

    /// Iterates over all files in the torrent.
    pub fn files(&self) -> impl ExactSizeIterator<Item = File<'_>> + '_ {
        (0..self.num_files()).map(|index| File { info: self, index })
    }

    // ---- piece/file mapping ----------------------------------------------

    /// Maps a byte range within a file into piece space. `None` if `file`
    /// is out of range or the byte range exceeds the file.
    pub fn map_file(&self, file: i32, offset: i64, size: i32) -> Option<PeerRequest> {
        let f = self.file(file)?;
        // Subtraction form: `offset + size` could overflow for hostile
        // offsets; `f.size() - offset` cannot (both operands are >= 0 here).
        if offset < 0 || size < 0 || i64::from(size) > f.size() - offset {
            return None;
        }
        let r = unsafe { sys::ct_torrent_info_map_file(self.as_ptr(), file, offset, size) };
        Some(PeerRequest::from_ct(&r))
    }

    /// Maps a byte range within a piece to the file windows storing it.
    /// Empty if the range is invalid.
    pub fn map_block(&self, piece: i32, offset: i64, size: i32) -> Vec<FileSlice> {
        if !self.valid_piece(piece) || offset < 0 || size < 0 {
            return Vec::new();
        }
        // piece < num_pieces and piece_length <= i32::MAX, so this product
        // fits in i64; from there subtraction form avoids overflowing on a
        // hostile offset (`remaining` and `offset` are both >= 0 here).
        let piece_start = i64::from(piece) * i64::from(self.piece_length());
        let remaining = self.total_size() - piece_start;
        if i64::from(size) > remaining - offset {
            return Vec::new();
        }
        // SAFETY: the array is valid until freed below.
        unsafe {
            let mut array = sys::ct_torrent_info_map_block(self.as_ptr(), piece, offset, size);
            let slices = if array.ptr.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(array.ptr, array.len)
                    .iter()
                    .map(|s| FileSlice {
                        file_index: s.file_index,
                        offset: s.offset,
                        size: s.size,
                    })
                    .collect()
            };
            sys::ct_file_slice_array_free(&mut array);
            slices
        }
    }

    /// The file containing the given byte offset of the torrent's data.
    pub fn file_index_at_offset(&self, offset: i64) -> Option<i32> {
        let index = unsafe { sys::ct_torrent_info_file_index_at_offset(self.as_ptr(), offset) };
        (index >= 0).then_some(index)
    }

    /// The first file overlapping the given piece.
    pub fn file_index_at_piece(&self, piece: i32) -> Option<i32> {
        let index = unsafe { sys::ct_torrent_info_file_index_at_piece(self.as_ptr(), piece) };
        (index >= 0).then_some(index)
    }

    /// The last file overlapping the given piece.
    pub fn last_file_index_at_piece(&self, piece: i32) -> Option<i32> {
        let index = unsafe { sys::ct_torrent_info_last_file_index_at_piece(self.as_ptr(), piece) };
        (index >= 0).then_some(index)
    }

    /// The file with the given v2 merkle root.
    pub fn file_index_for_root(&self, root: &Sha256Hash) -> Option<i32> {
        let h = sys::ct_sha256 { data: root.0 };
        let index = unsafe { sys::ct_torrent_info_file_index_for_root(self.as_ptr(), &h) };
        (index >= 0).then_some(index)
    }

    fn valid_piece(&self, piece: i32) -> bool {
        piece >= 0 && piece < self.num_pieces()
    }
}

impl Clone for TorrentInfo {
    fn clone(&self) -> TorrentInfo {
        // SAFETY: cloning copies the shared_ptr.
        let ptr = unsafe { sys::ct_torrent_info_clone(self.as_ptr()) };
        TorrentInfo {
            ptr: NonNull::new(ptr).expect("out of memory"),
        }
    }
}

impl Drop for TorrentInfo {
    fn drop(&mut self) {
        // SAFETY: self.ptr is owned and dropped exactly once.
        unsafe { sys::ct_torrent_info_free(self.ptr.as_ptr()) }
    }
}

impl std::fmt::Debug for TorrentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentInfo")
            .field("name", &self.name())
            .field("info_hashes", &self.info_hashes())
            .field("num_files", &self.num_files())
            .field("total_size", &self.total_size())
            .finish_non_exhaustive()
    }
}

/// Attributes of a file, as stored in the torrent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileFlags(u8);

impl FileFlags {
    /// The file is a pad file: zeros, not written to disk, aligning the next
    /// file to a piece boundary.
    pub fn is_pad_file(&self) -> bool {
        self.0 & sys::CT_FILE_FLAG_PAD_FILE as u8 != 0
    }

    /// The file has the (Windows) hidden attribute.
    pub fn is_hidden(&self) -> bool {
        self.0 & sys::CT_FILE_FLAG_HIDDEN as u8 != 0
    }

    /// The file has the (POSIX) executable attribute.
    pub fn is_executable(&self) -> bool {
        self.0 & sys::CT_FILE_FLAG_EXECUTABLE as u8 != 0
    }

    /// The file is a symbolic link (see [`File::symlink`]).
    pub fn is_symlink(&self) -> bool {
        self.0 & sys::CT_FILE_FLAG_SYMLINK as u8 != 0
    }
}

/// A window of a file, as returned by [`TorrentInfo::map_block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSlice {
    /// The index of the file.
    pub file_index: i32,
    /// The byte offset within that file where the window starts.
    pub offset: i64,
    /// The number of bytes in the window.
    pub size: i64,
}

/// A view of one file within a [`TorrentInfo`].
#[derive(Clone, Copy)]
pub struct File<'a> {
    info: &'a TorrentInfo,
    index: i32,
}

impl<'a> File<'a> {
    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn size(&self) -> i64 {
        // SAFETY: self.index is in range by construction (all accessors
        // below alike).
        unsafe { sys::ct_torrent_info_file_size(self.info.as_ptr(), self.index) }
    }

    /// The full path of the file inside the torrent (UTF-8).
    pub fn path(&self) -> String {
        take_ct_str(unsafe { sys::ct_torrent_info_file_path(self.info.as_ptr(), self.index) })
    }

    /// Just the file name (the last path component).
    pub fn name(&self) -> Cow<'a, str> {
        unsafe {
            view_to_cow(sys::ct_torrent_info_file_name(
                self.info.as_ptr(),
                self.index,
            ))
        }
    }

    /// The byte offset of the file within the torrent's data.
    pub fn offset(&self) -> i64 {
        unsafe { sys::ct_torrent_info_file_offset(self.info.as_ptr(), self.index) }
    }

    pub fn flags(&self) -> FileFlags {
        FileFlags(unsafe { sys::ct_torrent_info_file_flags(self.info.as_ptr(), self.index) })
    }

    /// The symlink target, if the file is a symbolic link.
    pub fn symlink(&self) -> Option<String> {
        if !self.flags().is_symlink() {
            return None;
        }
        Some(take_ct_str(unsafe {
            sys::ct_torrent_info_file_symlink(self.info.as_ptr(), self.index)
        }))
    }

    /// The posix modification time recorded in the torrent, if any.
    pub fn mtime(&self) -> Option<i64> {
        let t = unsafe { sys::ct_torrent_info_file_mtime(self.info.as_ptr(), self.index) };
        (t != 0).then_some(t)
    }

    /// The SHA-256 merkle root of the file (v2 torrents only).
    pub fn root(&self) -> Option<Sha256Hash> {
        let h = unsafe { sys::ct_torrent_info_file_root(self.info.as_ptr(), self.index) };
        let root = Sha256Hash::from_ct(&h);
        (!root.is_all_zeros()).then_some(root)
    }

    /// The number of pieces the file spans. `None` unless the torrent has
    /// v2 metadata: only v2 guarantees the piece alignment the count is
    /// derived from, and libtorrent asserts it instead of checking. Also
    /// `None` for pad files, which have no piece/merkle dimensions — the
    /// same signal [`File::root`] gives for them.
    pub fn num_pieces(&self) -> Option<i32> {
        if !self.info.has_v2() || self.flags().is_pad_file() {
            return None;
        }
        Some(unsafe { sys::ct_torrent_info_file_num_pieces(self.info.as_ptr(), self.index) })
    }

    /// The number of 16 kiB blocks the file spans, gated exactly like
    /// [`File::num_pieces`].
    pub fn num_blocks(&self) -> Option<i32> {
        if !self.info.has_v2() || self.flags().is_pad_file() {
            return None;
        }
        Some(unsafe { sys::ct_torrent_info_file_num_blocks(self.info.as_ptr(), self.index) })
    }

    /// Whether the file was renamed to an absolute path outside the save path.
    pub fn is_absolute_path(&self) -> bool {
        unsafe { sys::ct_torrent_info_file_absolute_path(self.info.as_ptr(), self.index) }
    }

    /// The first piece overlapping this file.
    pub fn first_piece(&self) -> i32 {
        unsafe { sys::ct_torrent_info_piece_index_at_file(self.info.as_ptr(), self.index) }
    }

    /// The last piece overlapping this file.
    pub fn last_piece(&self) -> i32 {
        unsafe { sys::ct_torrent_info_last_piece_index_at_file(self.info.as_ptr(), self.index) }
    }
}

impl std::fmt::Debug for File<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("index", &self.index)
            .field("path", &self.path())
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}
