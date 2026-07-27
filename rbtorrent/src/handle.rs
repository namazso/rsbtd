// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`TorrentHandle`]: control and query a torrent in a session.
//!
//! Query methods are synchronous and may briefly block on libtorrent's
//! session mutex. Mutating methods post an asynchronous message and return
//! immediately; results that come back later (`post_*`, `save_resume_data`,
//! `read_piece`) arrive as alerts on the session's stream (see
//! [`crate::alerts`]).
//!
//! All methods take `&self`; clone to share a torrent between tasks.

use libctorrent_sys as sys;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::client_data::ClientData;
use crate::params::DownloadPriority;
use crate::session::{RemoveFlags, Session};
use crate::types::{InfoHash, socket_addr_to_ct};
use crate::util::{str_view, take_ct_str, view_to_cow};

/// libtorrent's internal "unlimited" sentinel for per-torrent
/// upload/connection counts.
const UNLIMITED_PEERS: i32 = (1 << 24) - 1;

/// Validates a per-torrent rate limit: -1 (unlimited) or a positive rate
/// in bytes/sec. libtorrent quietly treats 0 and `i32::MAX` as unlimited
/// too; rejecting them keeps a stored limit reading back as what was
/// set.
pub fn check_rate_limit(limit: i32) -> Result<(), crate::Error> {
    if limit != -1 && !(1..i32::MAX).contains(&limit) {
        return Err(crate::Error::binding(&format!(
            "rate limit must be -1 (unlimited) or a positive rate in bytes/s, got {limit}"
        )));
    }
    Ok(())
}

/// Validates a max-uploads/max-connections limit: -1 (unlimited) or
/// `2..=16_777_214`. libtorrent asserts `limit >= 2 || limit == -1` (0
/// and 1 silently become unlimited in release builds) and stores the
/// limit in a 24-bit field whose all-ones value is the unlimited
/// sentinel — anything larger silently truncates (16_777_216 becomes 0,
/// blocking every peer).
pub fn check_peer_limit(limit: i32) -> Result<(), crate::Error> {
    if limit != -1 && !(2..UNLIMITED_PEERS).contains(&limit) {
        return Err(crate::Error::binding(&format!(
            "limit must be -1 (unlimited) or between 2 and 16777214, got {limit}"
        )));
    }
    Ok(())
}

/// A handle to a torrent in a session.
///
/// Clone shares the same underlying torrent. Invalid handles are safe to
/// call (methods return default values or are silent no-ops).
///
/// # Lifetime
///
/// A handle borrows the [`Session`] it belongs to: it cannot outlive it,
/// and while any handle exists the session cannot be closed or dropped,
/// making a handle call overlapping teardown a compile error. The borrow
/// also pins the pairing: handles only ever come from the session that
/// owns their torrent.
///
/// # Ownership
///
/// The C++ `lt::torrent_handle` is stored inline (masquerade pattern):
/// clone runs the C++ copy constructor and drop the destructor, exactly
/// once — do **not** break that with `std::mem::forget` or the like.
/// Rust moves relocate the object bytewise, which is sound here; see
/// `libctorrent/src/abi_asserts.cpp` ("masquerade relocatability").
pub struct TorrentHandle<'s> {
    inner: sys::ct_torrent_handle,
    session: &'s Session,
}

// SAFETY: lt::torrent_handle is thread-safe (const, internally synchronized
// members), and `&Session` is Send + Sync because Session is Sync.
unsafe impl Send for TorrentHandle<'_> {}
unsafe impl Sync for TorrentHandle<'_> {}

#[allow(missing_docs)]
impl TorrentHandle<'_> {
    // ---- status() query flags (lt::torrent_handle::query_*) -------------
    pub const QUERY_DISTRIBUTED_COPIES: u32 = sys::CT_STATUS_QUERY_DISTRIBUTED_COPIES;
    pub const QUERY_ACCURATE_DOWNLOAD_COUNTERS: u32 =
        sys::CT_STATUS_QUERY_ACCURATE_DOWNLOAD_COUNTERS;
    pub const QUERY_LAST_SEEN_COMPLETE: u32 = sys::CT_STATUS_QUERY_LAST_SEEN_COMPLETE;
    pub const QUERY_PIECES: u32 = sys::CT_STATUS_QUERY_PIECES;
    pub const QUERY_VERIFIED_PIECES: u32 = sys::CT_STATUS_QUERY_VERIFIED_PIECES;
    pub const QUERY_TORRENT_FILE: u32 = sys::CT_STATUS_QUERY_TORRENT_FILE;
    pub const QUERY_NAME: u32 = sys::CT_STATUS_QUERY_NAME;
    pub const QUERY_SAVE_PATH: u32 = sys::CT_STATUS_QUERY_SAVE_PATH;

    // ---- save_resume_data() flags (lt::torrent_handle) -------------------
    pub const RESUME_FLUSH_DISK_CACHE: u32 = sys::CT_RESUME_FLUSH_DISK_CACHE;
    pub const RESUME_SAVE_INFO_DICT: u32 = sys::CT_RESUME_SAVE_INFO_DICT;
    pub const RESUME_IF_COUNTERS_CHANGED: u32 = sys::CT_RESUME_IF_COUNTERS_CHANGED;
    pub const RESUME_IF_DOWNLOAD_PROGRESS: u32 = sys::CT_RESUME_IF_DOWNLOAD_PROGRESS;
    pub const RESUME_IF_CONFIG_CHANGED: u32 = sys::CT_RESUME_IF_CONFIG_CHANGED;
    pub const RESUME_IF_STATE_CHANGED: u32 = sys::CT_RESUME_IF_STATE_CHANGED;
    pub const RESUME_IF_METADATA_CHANGED: u32 = sys::CT_RESUME_IF_METADATA_CHANGED;
    pub const RESUME_ONLY_IF_MODIFIED: u32 = sys::CT_RESUME_ONLY_IF_MODIFIED;

    // ---- pause() flags ----------------------------------------------------
    pub const PAUSE_GRACEFUL: u32 = sys::CT_PAUSE_GRACEFUL;

    // ---- post_file_progress() flags -----------------------------------------
    pub const FILE_PROGRESS_PIECE_GRANULARITY: u32 = sys::CT_FILE_PROGRESS_PIECE_GRANULARITY;

    // ---- force_reannounce() flags -------------------------------------------
    pub const REANNOUNCE_IGNORE_MIN_INTERVAL: u32 = sys::CT_REANNOUNCE_IGNORE_MIN_INTERVAL;
    pub const REANNOUNCE_HIGH_PRIORITY: u32 = sys::CT_REANNOUNCE_HIGH_PRIORITY;

    // ---- set_piece_deadline() flags ----------------------------------------
    pub const DEADLINE_ALERT_WHEN_AVAILABLE: u32 = sys::CT_DEADLINE_ALERT_WHEN_AVAILABLE;

    // ---- move_storage() modes (an enum, pass exactly one) ------------------
    pub const MOVE_ALWAYS_REPLACE_FILES: u32 = sys::CT_MOVE_ALWAYS_REPLACE_FILES;
    pub const MOVE_FAIL_IF_EXIST: u32 = sys::CT_MOVE_FAIL_IF_EXIST;
    pub const MOVE_DONT_REPLACE: u32 = sys::CT_MOVE_DONT_REPLACE;
    pub const MOVE_RESET_SAVE_PATH: u32 = sys::CT_MOVE_RESET_SAVE_PATH;
    pub const MOVE_RESET_SAVE_PATH_UNCHECKED: u32 = sys::CT_MOVE_RESET_SAVE_PATH_UNCHECKED;
}

impl<'s> TorrentHandle<'s> {
    /// Clones the handle behind `ptr` into an owned handle paired with `session`.
    ///
    /// # Safety
    /// `ptr` must be a valid `ct_torrent_handle` for the duration of the call,
    /// and the underlying torrent must belong to `session`.
    pub(crate) unsafe fn from_ptr(
        ptr: *const sys::ct_torrent_handle,
        session: &'s Session,
    ) -> TorrentHandle<'s> {
        // The shim's _clone returns early on null without constructing,
        // leaving `inner` as zeroed scratch that Drop then destructs — UB,
        // so the check must hold in release builds too.
        assert!(!ptr.is_null(), "TorrentHandle::from_ptr got a null handle");
        // SAFETY: `inner` starts zeroed only as scratch storage; the shim
        // placement-constructs a real handle into it before it is used.
        unsafe {
            let mut inner = std::mem::zeroed();
            sys::ct_torrent_handle_clone(ptr, &mut inner);
            TorrentHandle { inner, session }
        }
    }

    /// Wraps already-owned handle bytes (the `find_torrent_v*` path); the
    /// torrent must belong to `session`.
    pub(crate) fn from_owned(
        inner: sys::ct_torrent_handle,
        session: &'s Session,
    ) -> TorrentHandle<'s> {
        TorrentHandle { inner, session }
    }

    /// Re-pairs raw handle bytes with the session they came from; the
    /// caller must pass the session whose alerts produced the
    /// [`RawHandle`].
    pub(crate) fn from_raw(raw: RawHandle, session: &'s Session) -> TorrentHandle<'s> {
        TorrentHandle {
            inner: raw.into_inner(),
            session,
        }
    }

    /// Removes this torrent from its session. Cleanup is asynchronous; a
    /// `torrent_removed_alert` is posted when complete. To also delete the
    /// data on disk, pass [`RemoveFlags::DELETE_FILES`] (usually together
    /// with [`RemoveFlags::DELETE_PARTFILE`]).
    ///
    /// Consuming `self` makes post-removal use of this handle a compile
    /// error; other clones just become invalid (safe no-ops).
    pub fn remove(self, flags: RemoveFlags) {
        // SAFETY: session and handle are valid and belong together (the
        // borrow pins the pairing); invalid handles are silent no-ops.
        unsafe {
            sys::ct_session_remove_torrent(self.session.ptr(), self.as_ptr(), flags.bits());
        }
    }
}

impl TorrentHandle<'_> {
    pub(crate) fn as_ptr(&self) -> *const sys::ct_torrent_handle {
        &self.inner
    }

    /// Returns whether this handle refers to a valid torrent (it may have been removed).
    pub fn is_valid(&self) -> bool {
        unsafe { sys::ct_torrent_handle_is_valid(self.as_ptr()) }
    }

    /// Session-unique id for this torrent (stable across resume); 0 for invalid handles.
    pub fn id(&self) -> u32 {
        unsafe { sys::ct_torrent_handle_id(self.as_ptr()) }
    }

    /// Returns the info hashes (v1 and/or v2). Absent hashes are zeroed.
    pub fn info_hashes(&self) -> InfoHash {
        unsafe {
            let h = sys::ct_torrent_handle_info_hashes(self.as_ptr());
            InfoHash::from_ct(h)
        }
    }

    /// Returns true if this handle is currently tracked by the session.
    pub fn in_session(&self) -> bool {
        unsafe { sys::ct_torrent_handle_in_session(self.as_ptr()) }
    }

    /// The userdata token the bindings attached at add time — the
    /// torrent's key for
    /// [`Session::find_torrent_by_token`](crate::Session::find_torrent_by_token)
    /// and its client data — or an error for expired handles (whose
    /// torrent no longer exists).
    pub fn client_data_token(&self) -> Result<u64, crate::Error> {
        // SAFETY: handle valid; expired handles return 0.
        let token = unsafe { sys::ct_torrent_handle_userdata(self.as_ptr()) };
        if token == 0 {
            return Err(crate::Error::binding("the torrent handle is expired"));
        }
        Ok(token)
    }

    /// The [`ClientData`] attached when the torrent was added (see
    /// [`Session::add_torrent`]). Errors when the handle is expired or the
    /// torrent was removed from the session.
    pub fn client_data(&self) -> Result<Arc<dyn ClientData>, crate::Error> {
        let token = self.client_data_token()?;
        self.session
            .inner()
            .registry
            .client_data(token)
            .ok_or_else(|| crate::Error::binding("the torrent was removed"))
    }

    /// [`client_data`](TorrentHandle::client_data) downcast to its concrete
    /// type; errors additionally when the stored data is not a `T`.
    pub fn client_data_as<T: ClientData>(&self) -> Result<Arc<T>, crate::Error> {
        let data: Arc<dyn std::any::Any + Send + Sync> = self.client_data()?;
        data.downcast::<T>()
            .map_err(|_| crate::Error::binding("the client data has a different type"))
    }

    /// Replaces the torrent's [`ClientData`] (an `Arc` swap; it is
    /// persisted by the next resume-data write). Errors when the handle is
    /// expired or the torrent was removed from the session.
    pub fn set_client_data(&self, data: Arc<dyn ClientData>) -> Result<(), crate::Error> {
        let token = self.client_data_token()?;
        self.session.inner().registry.set_client_data(token, data)
    }

    /// Serializes `params` like
    /// [`Session::write_resume_data`](Session::write_resume_data),
    /// additionally embedding this torrent's [`ClientData`] under the
    /// resume data's `"rbt-data"` key. Degrades to the plain serialization
    /// (no key) when the handle is expired, the torrent was removed, or
    /// the data serializes to nothing.
    pub fn write_resume_data(
        &self,
        params: &crate::params::AddTorrentParams,
    ) -> Result<Vec<u8>, crate::Error> {
        // SAFETY: handle valid; expired handles return 0.
        let token = unsafe { sys::ct_torrent_handle_userdata(self.as_ptr()) };
        let blob = if token == 0 {
            Vec::new()
        } else {
            self.session
                .inner()
                .registry
                .client_data(token)
                // to_bencode runs outside the registry lock.
                .map(|data| data.to_bencode())
                .unwrap_or_default()
        };
        // SAFETY: params and blob are valid; on success we own the buffer
        // and must free it. An empty blob writes no "rbt-data" key.
        let mut buf = crate::error::with_error(|err| unsafe {
            sys::ct_write_resume_data_buf_ex(
                params.as_ptr(),
                sys::ct_span {
                    ptr: blob.as_ptr(),
                    len: blob.len(),
                },
                err,
            )
        })?;
        let bytes = if buf.ptr.is_null() {
            Vec::new()
        } else {
            // SAFETY: ptr/len describe the owned buffer.
            unsafe { std::slice::from_raw_parts(buf.ptr, buf.len) }.to_vec()
        };
        // SAFETY: frees the buffer returned above.
        unsafe { sys::ct_buf_free(&mut buf) };
        Ok(bytes)
    }

    /// Returns the current torrent flags (see [`TorrentFlags`](crate::TorrentFlags)).
    pub fn flags(&self) -> u64 {
        unsafe { sys::ct_torrent_handle_flags(self.as_ptr()) }
    }

    /// Sets the bits selected by `mask` to the corresponding bits of `flags`; other
    /// bits are left unchanged.
    pub fn set_flags(&self, flags: u64, mask: u64) {
        unsafe { sys::ct_torrent_handle_set_flags(self.as_ptr(), flags, mask) }
    }

    /// Clears the bits in `flags`.
    pub fn unset_flags(&self, flags: u64) {
        unsafe { sys::ct_torrent_handle_unset_flags(self.as_ptr(), flags) }
    }

    /// Connects to a peer at the given address (fire-and-forget; the
    /// outcome arrives via peer_connect_alert). Errors if the handle is
    /// invalid or the endpoint is malformed.
    pub fn connect_peer(&self, addr: SocketAddr) -> Result<(), crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let success = sys::ct_torrent_handle_connect_peer(
                self.as_ptr(),
                socket_addr_to_ct(addr),
                &mut err,
            );
            if !success {
                if let Some(e) = crate::Error::from_ct(&err) {
                    return Err(e);
                }
                return Err(crate::Error::binding("connect_peer failed"));
            }
            Ok(())
        }
    }

    /// Requests that a piece be read from disk; delivered via `ReadPieceAlert`.
    /// Multiple requests are queued.
    pub fn read_piece(&self, piece: i32) {
        unsafe {
            sys::ct_torrent_handle_read_piece(self.as_ptr(), piece);
        }
    }

    /// Returns true if this torrent has the given piece.
    pub fn have_piece(&self, piece: i32) -> bool {
        unsafe { sys::ct_torrent_handle_have_piece(self.as_ptr(), piece) }
    }

    /// Schedules a piece to be downloaded with a deadline (milliseconds from now).
    /// Flags: [`TorrentHandle::DEADLINE_ALERT_WHEN_AVAILABLE`]. Requires
    /// metadata; errors if it is unavailable or `piece` is out of range
    /// (libtorrent asserts both preconditions instead of checking them).
    pub fn set_piece_deadline(
        &self,
        piece: i32,
        deadline_ms: i32,
        flags: u32,
    ) -> Result<(), crate::Error> {
        self.check_piece(piece)?;
        unsafe {
            sys::ct_torrent_handle_set_piece_deadline(self.as_ptr(), piece, deadline_ms, flags);
        }
        Ok(())
    }

    /// Requires metadata and an in-range piece index; the piece-deadline
    /// and piece-priority calls assert these preconditions upstream
    /// instead of checking them.
    fn check_piece(&self, piece: i32) -> Result<(), crate::Error> {
        let info = self
            .torrent_file()?
            .ok_or_else(|| crate::Error::binding("torrent metadata is not available yet"))?;
        let count = info.num_pieces();
        if piece < 0 || piece >= count {
            return Err(crate::Error::binding(&format!(
                "piece index {piece} is outside 0..{count}"
            )));
        }
        Ok(())
    }

    /// Removes the deadline for a piece.
    pub fn reset_piece_deadline(&self, piece: i32) {
        unsafe {
            sys::ct_torrent_handle_reset_piece_deadline(self.as_ptr(), piece);
        }
    }

    /// Clears all piece deadlines.
    pub fn clear_piece_deadlines(&self) {
        unsafe {
            sys::ct_torrent_handle_clear_piece_deadlines(self.as_ptr());
        }
    }

    /// Posts a state_update_alert with this torrent's status.
    /// Flags: `TorrentHandle::QUERY_*` (0 = all non-optional fields).
    pub fn post_status(&self, flags: u32) {
        unsafe {
            sys::ct_torrent_handle_post_status(self.as_ptr(), flags);
        }
    }

    /// Requests resume data be saved and delivered via save_resume_data_alert.
    /// Flags: `TorrentHandle::RESUME_*` (0 = defaults). Returns `false` when
    /// the request was not posted (expired handle): no alert will follow.
    #[must_use]
    pub fn save_resume_data(&self, flags: u32) -> bool {
        unsafe { sys::ct_torrent_handle_save_resume_data(self.as_ptr(), flags) }
    }

    /// Returns true if resume data has changed since last save.
    pub fn need_save_resume_data(&self) -> bool {
        unsafe { sys::ct_torrent_handle_need_save_resume_data(self.as_ptr()) }
    }

    /// Posts a file_progress alert with per-file byte counts. Flags:
    /// [`TorrentHandle::FILE_PROGRESS_PIECE_GRANULARITY`] counts whole
    /// pieces instead of exact bytes (cheaper); 0 = exact byte counts.
    pub fn post_file_progress(&self, flags: u32) {
        unsafe {
            sys::ct_torrent_handle_post_file_progress(self.as_ptr(), flags);
        }
    }

    /// Sets the download priority for a piece. Requires metadata; errors
    /// if it is unavailable or `piece` is out of range.
    pub fn set_piece_priority(
        &self,
        piece: i32,
        priority: DownloadPriority,
    ) -> Result<(), crate::Error> {
        self.check_piece(piece)?;
        unsafe {
            sys::ct_torrent_handle_piece_priority_set(self.as_ptr(), piece, priority.value());
        }
        Ok(())
    }

    /// Gets the download priority for a piece. Requires metadata; errors
    /// if it is unavailable or `piece` is out of range (libtorrent
    /// assert-fails on out-of-range indices once a piece picker exists).
    pub fn piece_priority(&self, piece: i32) -> Result<DownloadPriority, crate::Error> {
        self.check_piece(piece)?;
        let raw = unsafe { sys::ct_torrent_handle_piece_priority_get(self.as_ptr(), piece) };
        Ok(DownloadPriority::new(raw).unwrap_or(DownloadPriority::TOP))
    }

    /// Sets download priorities for pieces, in piece-index order; pieces
    /// beyond the end of the list keep their current priority. Requires
    /// metadata; errors if it is unavailable or `priorities` is longer
    /// than the number of pieces.
    pub fn prioritize_pieces(&self, priorities: &[DownloadPriority]) -> Result<(), crate::Error> {
        let info = self
            .torrent_file()?
            .ok_or_else(|| crate::Error::binding("torrent metadata is not available yet"))?;
        if priorities.len() > usize::try_from(info.num_pieces()).unwrap_or(0) {
            return Err(crate::Error::binding(
                "piece priority list is longer than the number of pieces",
            ));
        }
        // DownloadPriority is repr(transparent) over u8.
        unsafe {
            sys::ct_torrent_handle_prioritize_pieces(
                self.as_ptr(),
                priorities.as_ptr().cast(),
                priorities.len(),
            );
        }
        Ok(())
    }

    /// Sets the download priority for a file. Requires metadata; errors
    /// if it is unavailable or `file` is out of range.
    pub fn set_file_priority(
        &self,
        file: i32,
        priority: DownloadPriority,
    ) -> Result<(), crate::Error> {
        let info = self
            .torrent_file()?
            .ok_or_else(|| crate::Error::binding("torrent metadata is not available yet"))?;
        let count = info.num_files();
        if file < 0 || file >= count {
            return Err(crate::Error::binding(&format!(
                "file index {file} is outside 0..{count}"
            )));
        }
        unsafe {
            sys::ct_torrent_handle_file_priority_set(self.as_ptr(), file, priority.value());
        }
        Ok(())
    }

    /// Gets the download priority for a file. Returns
    /// [`DownloadPriority::DONT_DOWNLOAD`] for an out-of-range index.
    pub fn file_priority(&self, file: i32) -> DownloadPriority {
        let raw = unsafe { sys::ct_torrent_handle_file_priority_get(self.as_ptr(), file) };
        DownloadPriority::new(raw).unwrap_or(DownloadPriority::TOP)
    }

    /// Sets download priorities for files, in file-index order; files
    /// beyond the end of the list reset to the default priority. Requires
    /// metadata; errors if it is unavailable or `priorities` is longer
    /// than the number of files.
    pub fn prioritize_files(&self, priorities: &[DownloadPriority]) -> Result<(), crate::Error> {
        let info = self
            .torrent_file()?
            .ok_or_else(|| crate::Error::binding("torrent metadata is not available yet"))?;
        if priorities.len() > usize::try_from(info.num_files()).unwrap_or(0) {
            return Err(crate::Error::binding(
                "file priority list is longer than the number of files",
            ));
        }
        // DownloadPriority is repr(transparent) over u8.
        unsafe {
            sys::ct_torrent_handle_prioritize_files(
                self.as_ptr(),
                priorities.as_ptr().cast(),
                priorities.len(),
            );
        }
        Ok(())
    }

    /// Sets the upload rate limit in bytes/sec; see [`check_rate_limit`]
    /// for the accepted range (-1 = unlimited).
    pub fn set_upload_limit(&self, limit: i32) -> Result<(), crate::Error> {
        check_rate_limit(limit)?;
        unsafe {
            sys::ct_torrent_handle_set_upload_limit(self.as_ptr(), limit);
        }
        Ok(())
    }

    /// Sets the download rate limit in bytes/sec; see
    /// [`check_rate_limit`] for the accepted range (-1 = unlimited).
    pub fn set_download_limit(&self, limit: i32) -> Result<(), crate::Error> {
        check_rate_limit(limit)?;
        unsafe {
            sys::ct_torrent_handle_set_download_limit(self.as_ptr(), limit);
        }
        Ok(())
    }

    /// Gets the upload rate limit in bytes/sec (0 = unlimited).
    pub fn upload_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_handle_upload_limit(self.as_ptr()) }
    }

    /// Gets the download rate limit in bytes/sec (0 = unlimited).
    pub fn download_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_handle_download_limit(self.as_ptr()) }
    }

    /// Sets the max simultaneous uploads; see [`check_peer_limit`] for
    /// the accepted range (-1 = unlimited).
    pub fn set_max_uploads(&self, limit: i32) -> Result<(), crate::Error> {
        check_peer_limit(limit)?;
        unsafe {
            sys::ct_torrent_handle_set_max_uploads(self.as_ptr(), limit);
        }
        Ok(())
    }

    /// Sets the max connections; see [`check_peer_limit`] for the
    /// accepted range (-1 = unlimited).
    pub fn set_max_connections(&self, limit: i32) -> Result<(), crate::Error> {
        check_peer_limit(limit)?;
        unsafe {
            sys::ct_torrent_handle_set_max_connections(self.as_ptr(), limit);
        }
        Ok(())
    }

    /// Gets the max uploads setting (-1 = unlimited).
    pub fn max_uploads(&self) -> i32 {
        let raw = unsafe { sys::ct_torrent_handle_max_uploads(self.as_ptr()) };
        if raw == UNLIMITED_PEERS { -1 } else { raw }
    }

    /// Gets the max connections setting (-1 = unlimited).
    pub fn max_connections(&self) -> i32 {
        let raw = unsafe { sys::ct_torrent_handle_max_connections(self.as_ptr()) };
        if raw == UNLIMITED_PEERS { -1 } else { raw }
    }

    /// Posts a tracker_list_alert with current trackers.
    pub fn post_trackers(&self) {
        unsafe {
            sys::ct_torrent_handle_post_trackers(self.as_ptr());
        }
    }

    /// Adds a tracker announce URL with the given tier (tiers are 8-bit in libtorrent).
    pub fn add_tracker(&self, url: &str, tier: u8) {
        unsafe {
            sys::ct_torrent_handle_add_tracker(self.as_ptr(), str_view(url), tier);
        }
    }

    /// Replaces the full tracker list with `(url, tier)` pairs (tiers are
    /// 8-bit in libtorrent); an empty list removes all trackers.
    pub fn replace_trackers(&self, trackers: &[(&str, u8)]) {
        let urls: Vec<sys::ct_str_view> = trackers.iter().map(|&(url, _)| str_view(url)).collect();
        let tiers: Vec<u8> = trackers.iter().map(|&(_, tier)| tier).collect();
        unsafe {
            sys::ct_torrent_handle_replace_trackers(
                self.as_ptr(),
                urls.as_ptr(),
                tiers.as_ptr(),
                urls.len(),
            );
        }
    }

    /// Forces a tracker announce in `seconds` seconds (0 = now) to `tracker_index`
    /// (-1 = all trackers). Flags: `TorrentHandle::REANNOUNCE_*` (0 = none).
    pub fn force_reannounce(&self, seconds: i32, tracker_index: i32, flags: u32) {
        unsafe {
            sys::ct_torrent_handle_force_reannounce(self.as_ptr(), seconds, tracker_index, flags);
        }
    }

    /// Triggers a tracker scrape request.
    pub fn scrape_tracker(&self, tracker_index: i32) {
        unsafe {
            sys::ct_torrent_handle_scrape_tracker(self.as_ptr(), tracker_index);
        }
    }

    /// Adds a web seed URL (BEP 19).
    pub fn add_url_seed(&self, url: &str) {
        unsafe {
            sys::ct_torrent_handle_add_url_seed(self.as_ptr(), str_view(url));
        }
    }

    /// Removes a web seed URL.
    pub fn remove_url_seed(&self, url: &str) {
        unsafe {
            sys::ct_torrent_handle_remove_url_seed(self.as_ptr(), str_view(url));
        }
    }

    /// Returns the current web seed URLs (BEP 19).
    pub fn url_seeds(&self) -> Result<Vec<String>, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let list = sys::ct_torrent_handle_url_seeds(self.as_ptr(), &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            if list.is_null() {
                return Ok(Vec::new());
            }
            let len = sys::ct_str_list_len(list);
            let seeds = (0..len)
                .map(|i| view_to_cow(sys::ct_str_list_get(list, i)).into_owned())
                .collect();
            sys::ct_str_list_free(list);
            Ok(seeds)
        }
    }

    /// Returns the queue position (0-based, -1 if not queued or invalid).
    pub fn queue_position(&self) -> i32 {
        unsafe { sys::ct_torrent_handle_queue_position(self.as_ptr()) }
    }

    /// Moves the torrent up in the queue.
    pub fn queue_position_up(&self) {
        unsafe {
            sys::ct_torrent_handle_queue_position_up(self.as_ptr());
        }
    }

    /// Moves the torrent down in the queue.
    pub fn queue_position_down(&self) {
        unsafe {
            sys::ct_torrent_handle_queue_position_down(self.as_ptr());
        }
    }

    /// Moves the torrent to the top of the queue.
    pub fn queue_position_top(&self) {
        unsafe {
            sys::ct_torrent_handle_queue_position_top(self.as_ptr());
        }
    }

    /// Moves the torrent to the bottom of the queue.
    pub fn queue_position_bottom(&self) {
        unsafe {
            sys::ct_torrent_handle_queue_position_bottom(self.as_ptr());
        }
    }

    /// Sets the queue position to a specific value; `pos` must be
    /// non-negative (positions past the end of the queue are clamped;
    /// libtorrent asserts non-negativity instead of checking).
    pub fn set_queue_position(&self, pos: i32) -> Result<(), crate::Error> {
        if pos < 0 {
            return Err(crate::Error::binding("queue position must be >= 0"));
        }
        unsafe {
            sys::ct_torrent_handle_queue_position_set(self.as_ptr(), pos);
        }
        Ok(())
    }

    /// Pauses the torrent. Flags: [`TorrentHandle::PAUSE_GRACEFUL`].
    pub fn pause(&self, flags: u32) {
        unsafe {
            sys::ct_torrent_handle_pause(self.as_ptr(), flags);
        }
    }

    /// Resumes the torrent.
    pub fn resume(&self) {
        unsafe {
            sys::ct_torrent_handle_resume(self.as_ptr());
        }
    }

    /// Forces a full hash recheck of all pieces.
    pub fn force_recheck(&self) {
        unsafe {
            sys::ct_torrent_handle_force_recheck(self.as_ptr());
        }
    }

    /// Flushes the disk write cache for this torrent.
    pub fn flush_cache(&self) {
        unsafe {
            sys::ct_torrent_handle_flush_cache(self.as_ptr());
        }
    }

    /// Clears the error status (allows auto-managed torrents to retry).
    pub fn clear_error(&self) {
        unsafe {
            sys::ct_torrent_handle_clear_error(self.as_ptr());
        }
    }

    /// Forces a DHT announce (if DHT is enabled).
    pub fn force_dht_announce(&self) {
        unsafe {
            sys::ct_torrent_handle_force_dht_announce(self.as_ptr());
        }
    }

    /// Moves storage to a new path. `mode` is one of the `TorrentHandle::MOVE_*` values.
    pub fn move_storage(&self, path: &str, mode: u32) {
        unsafe {
            sys::ct_torrent_handle_move_storage(self.as_ptr(), str_view(path), mode);
        }
    }

    /// Renames a file (relative to save_path). Requires metadata; errors
    /// if it is unavailable or `index` is out of range.
    pub fn rename_file(&self, index: i32, name: &str) -> Result<(), crate::Error> {
        let info = self
            .torrent_file()?
            .ok_or_else(|| crate::Error::binding("torrent metadata is not available yet"))?;
        let count = info.num_files();
        if index < 0 || index >= count {
            return Err(crate::Error::binding(&format!(
                "file index {index} is outside 0..{count}"
            )));
        }
        unsafe {
            sys::ct_torrent_handle_rename_file(self.as_ptr(), index, str_view(name));
        }
        Ok(())
    }

    /// Posts a peer_info_alert with the current peer list.
    pub fn post_peer_info(&self) {
        unsafe {
            sys::ct_torrent_handle_post_peer_info(self.as_ptr());
        }
    }

    /// Returns the save path (a `status()` query under the hood).
    pub fn save_path(&self) -> Result<String, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let s = sys::ct_torrent_handle_save_path(self.as_ptr(), &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            Ok(take_ct_str(s))
        }
    }

    /// Returns the torrent name (a `status()` query under the hood).
    pub fn name(&self) -> Result<String, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let s = sys::ct_torrent_handle_name(self.as_ptr(), &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            Ok(take_ct_str(s))
        }
    }

    /// Returns true if the torrent is paused (a flags query).
    pub fn is_paused(&self) -> bool {
        self.flags() & (sys::CT_TORRENT_FLAG_PAUSED as u64) != 0
    }

    /// Returns true if the torrent is auto-managed (a flags query).
    pub fn is_auto_managed(&self) -> bool {
        self.flags() & (sys::CT_TORRENT_FLAG_AUTO_MANAGED as u64) != 0
    }

    /// Returns true if sequential download is enabled (a flags query).
    pub fn is_sequential_download(&self) -> bool {
        self.flags() & (sys::CT_TORRENT_FLAG_SEQUENTIAL_DOWNLOAD as u64) != 0
    }

    /// Returns a snapshot of this torrent's current status. `flags`
    /// selects the optional fields (`TorrentHandle::QUERY_*`; 0 = all
    /// non-optional fields).
    pub fn status(&self, flags: u32) -> Result<crate::status::TorrentStatus, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let ptr = sys::ct_torrent_handle_status(self.as_ptr(), flags, &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            if ptr.is_null() {
                return Err(crate::Error::binding("status returned null"));
            }
            Ok(crate::status::TorrentStatus::from_ptr(ptr))
        }
    }

    /// Returns the torrent's metadata, or `Ok(None)` if not available yet
    /// (e.g. a magnet link still fetching it). The returned
    /// [`TorrentInfo`](crate::TorrentInfo) stays valid after the torrent
    /// is removed from the session.
    pub fn torrent_file(&self) -> Result<Option<crate::TorrentInfo>, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let ptr = sys::ct_torrent_handle_torrent_file(self.as_ptr(), &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            Ok(crate::info::TorrentInfo::from_owned_ptr(ptr))
        }
    }

    /// Returns each file's current path in file-index order (relative to
    /// the save path unless renamed to an absolute path), reflecting
    /// renames applied via [`rename_file`](Self::rename_file); metadata
    /// paths keep the original names. `Ok(None)` before metadata is
    /// available.
    pub fn file_paths(&self) -> Result<Option<Vec<String>>, crate::Error> {
        unsafe {
            let mut err = std::mem::zeroed();
            let list = sys::ct_torrent_handle_file_paths(self.as_ptr(), &mut err);
            if let Some(e) = crate::Error::from_ct(&err) {
                return Err(e);
            }
            if list.is_null() {
                return Ok(None);
            }
            let len = sys::ct_str_list_len(list);
            let paths = (0..len)
                .map(|i| view_to_cow(sys::ct_str_list_get(list, i)).into_owned())
                .collect();
            sys::ct_str_list_free(list);
            Ok(Some(paths))
        }
    }
}

impl Clone for TorrentHandle<'_> {
    fn clone(&self) -> Self {
        // SAFETY: self.inner is a valid handle owned by self.session.
        unsafe { TorrentHandle::from_ptr(self.as_ptr(), self.session) }
    }
}

impl Drop for TorrentHandle<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle is dropped exactly once (clone/drop protocol).
        unsafe { sys::ct_torrent_handle_drop(&mut self.inner) }
    }
}

impl fmt::Debug for TorrentHandle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TorrentHandle")
            .field("id", &self.id())
            .field("valid", &self.is_valid())
            .finish()
    }
}

/// Owned `lt::torrent_handle` bytes without a session pairing.
///
/// Deliberately no public API: only the per-session request registry mints
/// these, and only [`TorrentHandle::from_raw`] consumes them, re-pairing
/// with the same session — preserving the handle/session pairing
/// invariant. Dropping after session teardown is safe: releasing a
/// weak_ptr only touches the shared_ptr control block.
pub(crate) struct RawHandle(sys::ct_torrent_handle);

// SAFETY: lt::torrent_handle is thread-safe.
unsafe impl Send for RawHandle {}

impl RawHandle {
    /// Clones the handle behind `ptr`.
    ///
    /// # Safety
    /// `ptr` must be a valid `ct_torrent_handle` for the duration of the call.
    pub(crate) unsafe fn from_ptr(ptr: *const sys::ct_torrent_handle) -> RawHandle {
        assert!(!ptr.is_null(), "RawHandle::from_ptr got a null handle");
        // SAFETY: zeroed scratch storage; the shim placement-constructs a
        // real handle into it before it is used (see TorrentHandle::from_ptr).
        unsafe {
            let mut inner = std::mem::zeroed();
            sys::ct_torrent_handle_clone(ptr, &mut inner);
            RawHandle(inner)
        }
    }

    /// Clones the raw handle (runs the C++ copy constructor, bumping the
    /// underlying control-block count).
    pub(crate) fn clone_raw(&self) -> RawHandle {
        // SAFETY: `&self.0` is a valid ct_torrent_handle for the call.
        unsafe { RawHandle::from_ptr(&self.0) }
    }

    /// Takes the handle bytes out without running `Drop`.
    fn into_inner(self) -> sys::ct_torrent_handle {
        let this = std::mem::ManuallyDrop::new(self);
        // SAFETY: ownership transfers by bytewise relocation (masquerade
        // relocatability); ManuallyDrop suppresses the double drop.
        unsafe { std::ptr::read(&this.0) }
    }
}

impl Drop for RawHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is dropped exactly once (clone/drop protocol).
        unsafe { sys::ct_torrent_handle_drop(&mut self.0) }
    }
}
