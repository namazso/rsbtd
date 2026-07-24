// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`TorrentStatus`]: a point-in-time snapshot of a torrent's state,
//! progress, rates, and counters.

use libctorrent_sys as sys;
use std::fmt;

use crate::Error;
use crate::types::{InfoHash, PieceBitfield};

/// Snapshot of a torrent's status, returned by `TorrentHandle::status()`.
pub struct TorrentStatus {
    ptr: *mut sys::ct_torrent_status,
}

impl TorrentStatus {
    pub(crate) unsafe fn from_ptr(ptr: *mut sys::ct_torrent_status) -> Self {
        TorrentStatus { ptr }
    }

    /// Session-unique id of the torrent, captured at snapshot time (stays
    /// meaningful after removal). Matches
    /// [`TorrentHandle::id`](crate::TorrentHandle::id); 0 if the torrent
    /// was already gone.
    pub fn id(&self) -> u32 {
        unsafe { sys::ct_torrent_status_id(self.ptr) }
    }

    /// The torrent's info-hashes (v1 and/or v2). Absent hashes are zeroed.
    pub fn info_hashes(&self) -> InfoHash {
        unsafe { InfoHash::from_ct(sys::ct_torrent_status_info_hashes(self.ptr)) }
    }

    /// Main state of the torrent.
    pub fn state(&self) -> State {
        State::from_code(unsafe { sys::ct_torrent_status_state(self.ptr) } as i32)
    }

    /// Error code if torrent is paused due to an error.
    pub fn error(&self) -> Option<Error> {
        // The shim returns a complete ct_error (with category pointer), so
        // the message resolves through the real boost category.
        let err = unsafe { sys::ct_torrent_status_error(self.ptr) };
        Error::from_ct(&err)
    }

    /// File index associated with the error, or -1.
    pub fn error_file(&self) -> i32 {
        unsafe { sys::ct_torrent_status_error_file(self.ptr) }
    }

    /// Save path (only populated if queried with query_save_path flag).
    pub fn save_path(&self) -> String {
        unsafe {
            let view = sys::ct_torrent_status_save_path(self.ptr);
            if view.ptr.is_null() {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(view.ptr.cast(), view.len))
                .into_owned()
        }
    }

    /// Torrent name (only populated if queried with query_name flag).
    pub fn name(&self) -> String {
        unsafe {
            let view = sys::ct_torrent_status_name(self.ptr);
            if view.ptr.is_null() {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(view.ptr.cast(), view.len))
                .into_owned()
        }
    }

    /// Seconds until next tracker announce.
    pub fn next_announce_seconds(&self) -> i64 {
        unsafe { sys::ct_torrent_status_next_announce_seconds(self.ptr) }
    }

    /// URL of the last successful tracker.
    pub fn current_tracker(&self) -> String {
        unsafe {
            let view = sys::ct_torrent_status_current_tracker(self.ptr);
            if view.ptr.is_null() {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(view.ptr.cast(), view.len))
                .into_owned()
        }
    }

    /// Total bytes downloaded this session (including protocol overhead).
    pub fn total_download(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_download(self.ptr) }
    }

    /// Total bytes uploaded this session (including protocol overhead).
    pub fn total_upload(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_upload(self.ptr) }
    }

    /// Total payload bytes downloaded this session.
    pub fn total_payload_download(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_payload_download(self.ptr) }
    }

    /// Total payload bytes uploaded this session.
    pub fn total_payload_upload(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_payload_upload(self.ptr) }
    }

    /// Bytes downloaded that failed hash check.
    pub fn total_failed_bytes(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_failed_bytes(self.ptr) }
    }

    /// Bytes re-downloaded (duplicate requests).
    pub fn total_redundant_bytes(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_redundant_bytes(self.ptr) }
    }

    /// Total bytes downloaded and verified.
    pub fn total_done(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_done(self.ptr) }
    }

    /// Total size of torrent (bytes to download).
    pub fn total(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total(self.ptr) }
    }

    /// Bytes downloaded of wanted pieces.
    pub fn total_wanted_done(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_wanted_done(self.ptr) }
    }

    /// Total size of wanted pieces.
    pub fn total_wanted(&self) -> i64 {
        unsafe { sys::ct_torrent_status_total_wanted(self.ptr) }
    }

    /// All-time upload (persistent across sessions).
    pub fn all_time_upload(&self) -> i64 {
        unsafe { sys::ct_torrent_status_all_time_upload(self.ptr) }
    }

    /// All-time download (persistent across sessions).
    pub fn all_time_download(&self) -> i64 {
        unsafe { sys::ct_torrent_status_all_time_download(self.ptr) }
    }

    /// Time when torrent was added (POSIX timestamp).
    pub fn added_time(&self) -> i64 {
        unsafe { sys::ct_torrent_status_added_time(self.ptr) }
    }

    /// Time when torrent completed (POSIX timestamp, 0 if not finished).
    pub fn completed_time(&self) -> i64 {
        unsafe { sys::ct_torrent_status_completed_time(self.ptr) }
    }

    /// Last time a complete copy was seen (POSIX timestamp).
    pub fn last_seen_complete(&self) -> i64 {
        unsafe { sys::ct_torrent_status_last_seen_complete(self.ptr) }
    }

    /// Storage allocation mode.
    pub fn storage_mode(&self) -> StorageMode {
        unsafe {
            match sys::ct_torrent_status_storage_mode(self.ptr) {
                sys::CT_STORAGE_MODE_ALLOCATE => StorageMode::Allocate,
                sys::CT_STORAGE_MODE_SPARSE => StorageMode::Sparse,
                other => StorageMode::Unknown(other as i32),
            }
        }
    }

    /// Progress as a fraction (0.0 to 1.0).
    pub fn progress(&self) -> f32 {
        unsafe { sys::ct_torrent_status_progress(self.ptr) }
    }

    /// Progress in parts per million (0 to 1000000).
    pub fn progress_ppm(&self) -> i32 {
        unsafe { sys::ct_torrent_status_progress_ppm(self.ptr) }
    }

    /// Queue position (0-based, -1 if not queued).
    pub fn queue_position(&self) -> i32 {
        unsafe { sys::ct_torrent_status_queue_position(self.ptr) }
    }

    /// Current download rate (bytes per second).
    pub fn download_rate(&self) -> i32 {
        unsafe { sys::ct_torrent_status_download_rate(self.ptr) }
    }

    /// Current upload rate (bytes per second).
    pub fn upload_rate(&self) -> i32 {
        unsafe { sys::ct_torrent_status_upload_rate(self.ptr) }
    }

    /// Current payload download rate (bytes per second).
    pub fn download_payload_rate(&self) -> i32 {
        unsafe { sys::ct_torrent_status_download_payload_rate(self.ptr) }
    }

    /// Current payload upload rate (bytes per second).
    pub fn upload_payload_rate(&self) -> i32 {
        unsafe { sys::ct_torrent_status_upload_payload_rate(self.ptr) }
    }

    /// Number of connected seeders.
    pub fn num_seeds(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_seeds(self.ptr) }
    }

    /// Number of connected peers.
    pub fn num_peers(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_peers(self.ptr) }
    }

    /// Total seeders reported by tracker (-1 if unavailable).
    pub fn num_complete(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_complete(self.ptr) }
    }

    /// Total leechers reported by tracker (-1 if unavailable).
    pub fn num_incomplete(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_incomplete(self.ptr) }
    }

    /// Seeders in peer list.
    pub fn list_seeds(&self) -> i32 {
        unsafe { sys::ct_torrent_status_list_seeds(self.ptr) }
    }

    /// Total peers in peer list.
    pub fn list_peers(&self) -> i32 {
        unsafe { sys::ct_torrent_status_list_peers(self.ptr) }
    }

    /// Number of peers we can connect to.
    pub fn connect_candidates(&self) -> i32 {
        unsafe { sys::ct_torrent_status_connect_candidates(self.ptr) }
    }

    /// Number of pieces downloaded.
    pub fn num_pieces(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_pieces(self.ptr) }
    }

    /// Full distributed copies available.
    pub fn distributed_full_copies(&self) -> i32 {
        unsafe { sys::ct_torrent_status_distributed_full_copies(self.ptr) }
    }

    /// Fractional distributed copies (0-1000).
    pub fn distributed_fraction(&self) -> i32 {
        unsafe { sys::ct_torrent_status_distributed_fraction(self.ptr) }
    }

    /// Distributed copies (floating point).
    pub fn distributed_copies(&self) -> f32 {
        unsafe { sys::ct_torrent_status_distributed_copies(self.ptr) }
    }

    /// Block size in bytes (typically 16 KiB).
    pub fn block_size(&self) -> i32 {
        unsafe { sys::ct_torrent_status_block_size(self.ptr) }
    }

    /// Number of unchoked peers.
    pub fn num_uploads(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_uploads(self.ptr) }
    }

    /// Number of connections (including half-open).
    pub fn num_connections(&self) -> i32 {
        unsafe { sys::ct_torrent_status_num_connections(self.ptr) }
    }

    /// Upload slot limit.
    pub fn uploads_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_status_uploads_limit(self.ptr) }
    }

    /// Connection limit.
    pub fn connections_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_status_connections_limit(self.ptr) }
    }

    /// Upload rate limit (bytes per second, -1 = unlimited).
    pub fn upload_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_status_upload_limit(self.ptr) }
    }

    /// Download rate limit (bytes per second, -1 = unlimited).
    pub fn download_limit(&self) -> i32 {
        unsafe { sys::ct_torrent_status_download_limit(self.ptr) }
    }

    /// Peers waiting for upload bandwidth.
    pub fn up_bandwidth_queue(&self) -> i32 {
        unsafe { sys::ct_torrent_status_up_bandwidth_queue(self.ptr) }
    }

    /// Peers waiting for download bandwidth.
    pub fn down_bandwidth_queue(&self) -> i32 {
        unsafe { sys::ct_torrent_status_down_bandwidth_queue(self.ptr) }
    }

    /// Seed rank (importance for seeding).
    pub fn seed_rank(&self) -> i32 {
        unsafe { sys::ct_torrent_status_seed_rank(self.ptr) }
    }

    /// Resume data save flags.
    pub fn need_save_resume_data(&self) -> u32 {
        unsafe { sys::ct_torrent_status_need_save_resume_data(self.ptr) }
    }

    /// True if all pieces are downloaded and checked.
    pub fn is_seeding(&self) -> bool {
        unsafe { sys::ct_torrent_status_is_seeding(self.ptr) }
    }

    /// True if all *wanted* pieces (of files with non-zero priority) are
    /// downloaded; [`TorrentStatus::is_seeding`] checks every piece.
    pub fn is_finished(&self) -> bool {
        unsafe { sys::ct_torrent_status_is_finished(self.ptr) }
    }

    /// True if metadata is available.
    pub fn has_metadata(&self) -> bool {
        unsafe { sys::ct_torrent_status_has_metadata(self.ptr) }
    }

    /// True if there has been an incoming connection attempt.
    pub fn has_incoming(&self) -> bool {
        unsafe { sys::ct_torrent_status_has_incoming(self.ptr) }
    }

    /// True if storage is being moved.
    pub fn moving_storage(&self) -> bool {
        unsafe { sys::ct_torrent_status_moving_storage(self.ptr) }
    }

    /// True if announcing to trackers.
    pub fn announcing_to_trackers(&self) -> bool {
        unsafe { sys::ct_torrent_status_announcing_to_trackers(self.ptr) }
    }

    /// True if announcing to LSD.
    pub fn announcing_to_lsd(&self) -> bool {
        unsafe { sys::ct_torrent_status_announcing_to_lsd(self.ptr) }
    }

    /// True if announcing to DHT.
    pub fn announcing_to_dht(&self) -> bool {
        unsafe { sys::ct_torrent_status_announcing_to_dht(self.ptr) }
    }

    /// The have-piece bitfield. `None` unless the status was queried with
    /// [`TorrentHandle::QUERY_PIECES`](crate::TorrentHandle::QUERY_PIECES)
    /// (or the torrent has no pieces yet).
    pub fn pieces(&self) -> Option<PieceBitfield<'_>> {
        let mut bits = 0usize;
        // SAFETY: status valid; the returned buffer is owned by the status
        // object and lives until self is dropped, matching the borrow.
        unsafe {
            let ptr = sys::ct_torrent_status_pieces(self.ptr, &mut bits);
            if ptr.is_null() {
                return None;
            }
            Some(PieceBitfield::from_raw(ptr, bits))
        }
    }

    /// The verified-piece bitfield (seed mode only). `None` unless the
    /// status was queried with
    /// [`TorrentHandle::QUERY_VERIFIED_PIECES`](crate::TorrentHandle::QUERY_VERIFIED_PIECES).
    pub fn verified_pieces(&self) -> Option<PieceBitfield<'_>> {
        let mut bits = 0usize;
        // SAFETY: as in `pieces`.
        unsafe {
            let ptr = sys::ct_torrent_status_verified_pieces(self.ptr, &mut bits);
            if ptr.is_null() {
                return None;
            }
            Some(PieceBitfield::from_raw(ptr, bits))
        }
    }
}

impl Drop for TorrentStatus {
    fn drop(&mut self) {
        unsafe {
            sys::ct_torrent_status_free(self.ptr);
        }
    }
}

unsafe impl Send for TorrentStatus {}
unsafe impl Sync for TorrentStatus {}

impl fmt::Debug for TorrentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TorrentStatus")
            .field("state", &self.state())
            .field("progress", &self.progress())
            .field("download_rate", &self.download_rate())
            .field("upload_rate", &self.upload_rate())
            .field("num_seeds", &self.num_seeds())
            .field("num_peers", &self.num_peers())
            .finish()
    }
}

/// Torrent state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum State {
    CheckingFiles,
    DownloadingMetadata,
    Downloading,
    Finished,
    Seeding,
    CheckingResumeData,
    /// A state value these bindings don't recognize (from a newer
    /// libtorrent).
    Unknown(i32),
}

impl State {
    /// Maps a raw libtorrent `state_t` code (as carried by state-changed
    /// alerts) to the typed state.
    pub fn from_code(code: i32) -> State {
        match code as sys::ct_torrent_state_t {
            sys::CT_TORRENT_STATE_CHECKING_FILES => State::CheckingFiles,
            sys::CT_TORRENT_STATE_DOWNLOADING_METADATA => State::DownloadingMetadata,
            sys::CT_TORRENT_STATE_DOWNLOADING => State::Downloading,
            sys::CT_TORRENT_STATE_FINISHED => State::Finished,
            sys::CT_TORRENT_STATE_SEEDING => State::Seeding,
            sys::CT_TORRENT_STATE_CHECKING_RESUME_DATA => State::CheckingResumeData,
            _ => State::Unknown(code),
        }
    }
}

/// Storage allocation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageMode {
    Allocate,
    Sparse,
    /// A mode value these bindings don't recognize.
    Unknown(i32),
}
