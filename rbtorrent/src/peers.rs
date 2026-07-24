// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`PeerInfo`]: per-peer connection state and statistics, delivered by
//! `peer_info_alert` in response to
//! [`TorrentHandle::post_peer_info`](crate::TorrentHandle::post_peer_info).

use libctorrent_sys as sys;
use std::fmt;
use std::net::SocketAddr;

use crate::types::{PieceBitfield, Sha256Hash, socket_addr_from_ct};

/// Snapshot of peer connection state and statistics.
pub struct PeerInfo {
    ptr: *mut sys::ct_peer_info,
}

impl PeerInfo {
    /// Takes ownership of an owned `ct_peer_info`.
    ///
    /// # Safety
    /// `ptr` must be a valid owned peer info; freed on drop.
    pub(crate) unsafe fn from_ptr(ptr: *mut sys::ct_peer_info) -> Self {
        PeerInfo { ptr }
    }

    /// Client identification string (e.g., "libtorrent/2.1.0").
    pub fn client(&self) -> String {
        unsafe {
            let view = sys::ct_peer_info_client(self.ptr);
            if view.ptr.is_null() {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(view.ptr.cast(), view.len))
                .into_owned()
        }
    }

    /// Total payload bytes downloaded from this peer.
    pub fn total_download(&self) -> i64 {
        unsafe { sys::ct_peer_info_total_download(self.ptr) }
    }

    /// Total payload bytes uploaded to this peer.
    pub fn total_upload(&self) -> i64 {
        unsafe { sys::ct_peer_info_total_upload(self.ptr) }
    }

    /// Microseconds since last request sent.
    pub fn last_request_us(&self) -> i64 {
        unsafe { sys::ct_peer_info_last_request_us(self.ptr) }
    }

    /// Microseconds since last activity.
    pub fn last_active_us(&self) -> i64 {
        unsafe { sys::ct_peer_info_last_active_us(self.ptr) }
    }

    /// Estimated microseconds until download queue is empty.
    pub fn download_queue_time_us(&self) -> i64 {
        unsafe { sys::ct_peer_info_download_queue_time_us(self.ptr) }
    }

    /// Peer flags (combination of `PeerFlags` bits).
    pub fn flags(&self) -> PeerFlags {
        unsafe { PeerFlags::from_bits_truncate(sys::ct_peer_info_flags(self.ptr)) }
    }

    /// Peer source flags (combination of `PeerSourceFlags` bits).
    pub fn source(&self) -> PeerSourceFlags {
        unsafe { PeerSourceFlags::from_bits_truncate(sys::ct_peer_info_source(self.ptr)) }
    }

    /// Current upload speed (bytes per second, including protocol overhead).
    pub fn up_speed(&self) -> i32 {
        unsafe { sys::ct_peer_info_up_speed(self.ptr) }
    }

    /// Current download speed (bytes per second, including protocol overhead).
    pub fn down_speed(&self) -> i32 {
        unsafe { sys::ct_peer_info_down_speed(self.ptr) }
    }

    /// Payload-only upload speed (bytes per second).
    pub fn payload_up_speed(&self) -> i32 {
        unsafe { sys::ct_peer_info_payload_up_speed(self.ptr) }
    }

    /// Payload-only download speed (bytes per second).
    pub fn payload_down_speed(&self) -> i32 {
        unsafe { sys::ct_peer_info_payload_down_speed(self.ptr) }
    }

    /// Peer ID (20 bytes).
    pub fn pid(&self) -> [u8; 20] {
        unsafe {
            let mut pid = [0u8; 20];
            sys::ct_peer_info_pid(self.ptr, pid.as_mut_ptr());
            pid
        }
    }

    /// Bytes requested but not yet received.
    pub fn queue_bytes(&self) -> i32 {
        unsafe { sys::ct_peer_info_queue_bytes(self.ptr) }
    }

    /// Seconds until current request times out (-1 if no request).
    pub fn request_timeout(&self) -> i32 {
        unsafe { sys::ct_peer_info_request_timeout(self.ptr) }
    }

    /// Send buffer size.
    pub fn send_buffer_size(&self) -> i32 {
        unsafe { sys::ct_peer_info_send_buffer_size(self.ptr) }
    }

    /// Used send buffer.
    pub fn used_send_buffer(&self) -> i32 {
        unsafe { sys::ct_peer_info_used_send_buffer(self.ptr) }
    }

    /// Receive buffer size.
    pub fn receive_buffer_size(&self) -> i32 {
        unsafe { sys::ct_peer_info_receive_buffer_size(self.ptr) }
    }

    /// Used receive buffer.
    pub fn used_receive_buffer(&self) -> i32 {
        unsafe { sys::ct_peer_info_used_receive_buffer(self.ptr) }
    }

    /// Receive buffer watermark.
    pub fn receive_buffer_watermark(&self) -> i32 {
        unsafe { sys::ct_peer_info_receive_buffer_watermark(self.ptr) }
    }

    /// Number of pieces this peer sent that failed hash check.
    pub fn num_hashfails(&self) -> i32 {
        unsafe { sys::ct_peer_info_num_hashfails(self.ptr) }
    }

    /// Download queue length.
    pub fn download_queue_length(&self) -> i32 {
        unsafe { sys::ct_peer_info_download_queue_length(self.ptr) }
    }

    /// Timed-out requests still in queue.
    pub fn timed_out_requests(&self) -> i32 {
        unsafe { sys::ct_peer_info_timed_out_requests(self.ptr) }
    }

    /// Busy requests (requested from multiple peers).
    pub fn busy_requests(&self) -> i32 {
        unsafe { sys::ct_peer_info_busy_requests(self.ptr) }
    }

    /// Requests in send buffer.
    pub fn requests_in_buffer(&self) -> i32 {
        unsafe { sys::ct_peer_info_requests_in_buffer(self.ptr) }
    }

    /// Target download queue length.
    pub fn target_dl_queue_length(&self) -> i32 {
        unsafe { sys::ct_peer_info_target_dl_queue_length(self.ptr) }
    }

    /// Upload queue length.
    pub fn upload_queue_length(&self) -> i32 {
        unsafe { sys::ct_peer_info_upload_queue_length(self.ptr) }
    }

    /// Connection failure count.
    pub fn failcount(&self) -> i32 {
        unsafe { sys::ct_peer_info_failcount(self.ptr) }
    }

    /// Index of piece currently being downloaded (-1 if none).
    pub fn downloading_piece_index(&self) -> i32 {
        unsafe { sys::ct_peer_info_downloading_piece_index(self.ptr) }
    }

    /// Block index within the downloading piece.
    pub fn downloading_block_index(&self) -> i32 {
        unsafe { sys::ct_peer_info_downloading_block_index(self.ptr) }
    }

    /// Bytes received of the downloading block.
    pub fn downloading_progress(&self) -> i32 {
        unsafe { sys::ct_peer_info_downloading_progress(self.ptr) }
    }

    /// Total bytes in the downloading block.
    pub fn downloading_total(&self) -> i32 {
        unsafe { sys::ct_peer_info_downloading_total(self.ptr) }
    }

    /// Connection type.
    pub fn connection_type(&self) -> ConnectionType {
        unsafe { ConnectionType::from_bits_truncate(sys::ct_peer_info_connection_type(self.ptr)) }
    }

    /// Bytes waiting to be written to disk.
    pub fn pending_disk_bytes(&self) -> i32 {
        unsafe { sys::ct_peer_info_pending_disk_bytes(self.ptr) }
    }

    /// Bytes waiting to be read from disk.
    pub fn pending_disk_read_bytes(&self) -> i32 {
        unsafe { sys::ct_peer_info_pending_disk_read_bytes(self.ptr) }
    }

    /// Send quota.
    pub fn send_quota(&self) -> i32 {
        unsafe { sys::ct_peer_info_send_quota(self.ptr) }
    }

    /// Receive quota.
    pub fn receive_quota(&self) -> i32 {
        unsafe { sys::ct_peer_info_receive_quota(self.ptr) }
    }

    /// Round-trip time estimate (milliseconds, 0 for incoming).
    pub fn rtt(&self) -> i32 {
        unsafe { sys::ct_peer_info_rtt(self.ptr) }
    }

    /// Number of pieces this peer has.
    pub fn num_pieces(&self) -> i32 {
        unsafe { sys::ct_peer_info_num_pieces(self.ptr) }
    }

    /// The pieces this peer has (`None` if not known yet).
    pub fn pieces(&self) -> Option<PieceBitfield<'_>> {
        let mut bits = 0usize;
        // SAFETY: peer info valid; the returned buffer is owned by the peer
        // info object and lives until self is dropped, matching the borrow.
        unsafe {
            let ptr = sys::ct_peer_info_pieces(self.ptr, &mut bits);
            if ptr.is_null() {
                return None;
            }
            Some(PieceBitfield::from_raw(ptr, bits))
        }
    }

    /// Peak download rate (bytes per second).
    pub fn download_rate_peak(&self) -> i32 {
        unsafe { sys::ct_peer_info_download_rate_peak(self.ptr) }
    }

    /// Peak upload rate (bytes per second).
    pub fn upload_rate_peak(&self) -> i32 {
        unsafe { sys::ct_peer_info_upload_rate_peak(self.ptr) }
    }

    /// Progress as a fraction (0.0 to 1.0).
    pub fn progress(&self) -> f32 {
        unsafe { sys::ct_peer_info_progress(self.ptr) }
    }

    /// Progress in parts per million (0 to 1000000).
    pub fn progress_ppm(&self) -> i32 {
        unsafe { sys::ct_peer_info_progress_ppm(self.ptr) }
    }

    /// Remote endpoint (IP:port); `None` for I2P peers.
    pub fn remote_endpoint(&self) -> Option<SocketAddr> {
        unsafe {
            let mut ep = std::mem::zeroed();
            if sys::ct_peer_info_remote_endpoint(self.ptr, &mut ep) {
                Some(socket_addr_from_ct(&ep))
            } else {
                None
            }
        }
    }

    /// Local endpoint (IP:port); `None` for I2P peers.
    pub fn local_endpoint(&self) -> Option<SocketAddr> {
        unsafe {
            let mut ep = std::mem::zeroed();
            if sys::ct_peer_info_local_endpoint(self.ptr, &mut ep) {
                Some(socket_addr_from_ct(&ep))
            } else {
                None
            }
        }
    }

    /// Hash of the peer's I2P destination; `None` unless this is an I2P
    /// peer (I2P peers have no IP endpoints).
    pub fn i2p_destination(&self) -> Option<Sha256Hash> {
        unsafe {
            let mut hash = sys::ct_sha256::default();
            sys::ct_peer_info_i2p_destination(self.ptr, &mut hash)
                .then(|| Sha256Hash::from_ct(&hash))
        }
    }
}

impl Drop for PeerInfo {
    fn drop(&mut self) {
        unsafe {
            sys::ct_peer_info_free(self.ptr);
        }
    }
}

unsafe impl Send for PeerInfo {}
unsafe impl Sync for PeerInfo {}

impl fmt::Debug for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerInfo")
            .field("client", &self.client())
            .field("remote_endpoint", &self.remote_endpoint())
            .field("up_speed", &self.up_speed())
            .field("down_speed", &self.down_speed())
            .field("progress", &self.progress())
            .finish()
    }
}

bitflags::bitflags! {
    /// Peer state flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PeerFlags: u32 {
        const INTERESTING = sys::CT_PEER_INTERESTING;
        const CHOKED = sys::CT_PEER_CHOKED;
        const REMOTE_INTERESTED = sys::CT_PEER_REMOTE_INTERESTED;
        const REMOTE_CHOKED = sys::CT_PEER_REMOTE_CHOKED;
        const SUPPORTS_EXTENSIONS = sys::CT_PEER_SUPPORTS_EXTENSIONS;
        const OUTGOING_CONNECTION = sys::CT_PEER_OUTGOING_CONNECTION;
        const HANDSHAKE = sys::CT_PEER_HANDSHAKE;
        const CONNECTING = sys::CT_PEER_CONNECTING;
        const ON_PAROLE = sys::CT_PEER_ON_PAROLE;
        const SEED = sys::CT_PEER_SEED;
        const OPTIMISTIC_UNCHOKE = sys::CT_PEER_OPTIMISTIC_UNCHOKE;
        const SNUBBED = sys::CT_PEER_SNUBBED;
        const UPLOAD_ONLY = sys::CT_PEER_UPLOAD_ONLY;
        const ENDGAME_MODE = sys::CT_PEER_ENDGAME_MODE;
        const HOLEPUNCHED = sys::CT_PEER_HOLEPUNCHED;
        const I2P_SOCKET = sys::CT_PEER_I2P_SOCKET;
        const UTP_SOCKET = sys::CT_PEER_UTP_SOCKET;
        const SSL_SOCKET = sys::CT_PEER_SSL_SOCKET;
        const RC4_ENCRYPTED = sys::CT_PEER_RC4_ENCRYPTED;
        const PLAINTEXT_ENCRYPTED = sys::CT_PEER_PLAINTEXT_ENCRYPTED;
    }
}

bitflags::bitflags! {
    /// Peer source flags (how we discovered the peer).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PeerSourceFlags: u8 {
        const TRACKER = sys::CT_PEER_SOURCE_TRACKER as u8;
        const DHT = sys::CT_PEER_SOURCE_DHT as u8;
        const PEX = sys::CT_PEER_SOURCE_PEX as u8;
        const LSD = sys::CT_PEER_SOURCE_LSD as u8;
        const RESUME_DATA = sys::CT_PEER_SOURCE_RESUME_DATA as u8;
        const INCOMING = sys::CT_PEER_SOURCE_INCOMING as u8;
    }
}

bitflags::bitflags! {
    /// Connection type.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConnectionType: u8 {
        const STANDARD_BITTORRENT = sys::CT_CONNECTION_STANDARD_BITTORRENT as u8;
        const WEB_SEED = sys::CT_CONNECTION_WEB_SEED as u8;
        const HTTP_SEED = sys::CT_CONNECTION_HTTP_SEED as u8;
    }
}
