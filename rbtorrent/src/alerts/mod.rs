// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The alert stream: libtorrent's event mechanism.
//!
//! [`Session::alerts`](crate::Session::alerts) returns the [`Alerts`]
//! receiver; awaiting [`Alerts::next_batch`] pops all pending alerts as a
//! [`Batch`] of zero-copy [`Alert`] views, valid only until the next pop
//! (borrow-enforced).
//!
//! **Polling the stream also drives request/response futures**
//! (e.g. `Session::session_stats`): responses resolve as a side effect of
//! popping; an unpolled stream never resolves them.

// Generated file keeps the generator's formatting (regen-diff CI).
#[rustfmt::skip]
mod generated;
pub(crate) mod requests;

use std::borrow::Cow;
use std::ops::Deref;
use std::ptr::NonNull;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libctorrent_sys as sys;

use crate::error::{Error, Result, with_error};
use crate::session::Session;
use crate::types::{InfoHash, PeerRequest, Sha1Hash, socket_addr_from_ct};

pub use generated::AlertType;

/// Bitmask of alert categories (`lt::alert_category_t`) for `SettingsPack::alert_mask`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlertCategory(u32);

#[allow(missing_docs)]
impl AlertCategory {
    pub const ERROR: Self = Self(sys::CT_ALERT_CAT_ERROR);
    pub const PEER: Self = Self(sys::CT_ALERT_CAT_PEER);
    pub const PORT_MAPPING: Self = Self(sys::CT_ALERT_CAT_PORT_MAPPING);
    pub const STORAGE: Self = Self(sys::CT_ALERT_CAT_STORAGE);
    pub const TRACKER: Self = Self(sys::CT_ALERT_CAT_TRACKER);
    pub const CONNECT: Self = Self(sys::CT_ALERT_CAT_CONNECT);
    pub const STATUS: Self = Self(sys::CT_ALERT_CAT_STATUS);
    pub const IP_BLOCK: Self = Self(sys::CT_ALERT_CAT_IP_BLOCK);
    pub const PERFORMANCE_WARNING: Self = Self(sys::CT_ALERT_CAT_PERFORMANCE_WARNING);
    pub const DHT: Self = Self(sys::CT_ALERT_CAT_DHT);
    pub const STATS: Self = Self(sys::CT_ALERT_CAT_STATS);
    pub const SESSION_LOG: Self = Self(sys::CT_ALERT_CAT_SESSION_LOG);
    pub const TORRENT_LOG: Self = Self(sys::CT_ALERT_CAT_TORRENT_LOG);
    pub const PEER_LOG: Self = Self(sys::CT_ALERT_CAT_PEER_LOG);
    pub const INCOMING_REQUEST: Self = Self(sys::CT_ALERT_CAT_INCOMING_REQUEST);
    pub const DHT_LOG: Self = Self(sys::CT_ALERT_CAT_DHT_LOG);
    pub const DHT_OPERATION: Self = Self(sys::CT_ALERT_CAT_DHT_OPERATION);
    pub const PORT_MAPPING_LOG: Self = Self(sys::CT_ALERT_CAT_PORT_MAPPING_LOG);
    pub const PICKER_LOG: Self = Self(sys::CT_ALERT_CAT_PICKER_LOG);
    pub const FILE_PROGRESS: Self = Self(sys::CT_ALERT_CAT_FILE_PROGRESS);
    pub const PIECE_PROGRESS: Self = Self(sys::CT_ALERT_CAT_PIECE_PROGRESS);
    pub const UPLOAD: Self = Self(sys::CT_ALERT_CAT_UPLOAD);
    pub const BLOCK_PROGRESS: Self = Self(sys::CT_ALERT_CAT_BLOCK_PROGRESS);
    pub const ALL: Self = Self(sys::CT_ALERT_CAT_ALL);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    /// For `SettingsPack::alert_mask` (an `i32` setting).
    pub const fn bits_i32(self) -> i32 {
        self.0 as i32
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for AlertCategory {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for AlertCategory {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::fmt::Debug for AlertCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AlertCategory({:#x})", self.0)
    }
}

/// An untyped alert: base accessors available on every alert.
///
/// The lifetime ties it to the [`Batch`] it came from; the session
/// reference pairs alert-derived handles with the posting session.
#[derive(Clone, Copy)]
pub struct RawAlert<'a> {
    ptr: NonNull<sys::ct_alert>,
    session: &'a Session,
}

// Alerts are immutable once posted; concurrent reads are fine.
unsafe impl Send for RawAlert<'_> {}
unsafe impl Sync for RawAlert<'_> {}

impl<'a> RawAlert<'a> {
    /// The raw `lt::alert::type()` id.
    pub fn type_raw(&self) -> i32 {
        // SAFETY: alert is valid for 'a.
        unsafe { sys::ct_alert_type(self.ptr.as_ptr()) }
    }

    /// The alert type, if these bindings know it.
    pub fn alert_type(&self) -> Option<AlertType> {
        AlertType::from_raw(self.type_raw())
    }

    pub fn category(&self) -> AlertCategory {
        // SAFETY: alert is valid for 'a.
        AlertCategory(unsafe { sys::ct_alert_category(self.ptr.as_ptr()) })
    }

    /// The alert type's name, e.g. `"torrent_finished"`.
    pub fn what(&self) -> &'static str {
        // SAFETY: static storage; the names cross a shared-library
        // boundary, so check UTF-8 rather than trust.
        unsafe {
            let v = sys::ct_alert_what(self.ptr.as_ptr());
            if v.ptr.is_null() {
                return "";
            }
            str::from_utf8(std::slice::from_raw_parts(v.ptr.cast(), v.len))
                .unwrap_or("<non-utf8 alert name>")
        }
    }

    /// Human-readable one-line description (allocates).
    pub fn message(&self) -> String {
        // SAFETY: alert valid; out is an owned string we must free.
        unsafe {
            let mut out = sys::ct_str::default();
            sys::ct_alert_message(self.ptr.as_ptr(), &mut out);
            let text = if out.ptr.is_null() {
                String::new()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(out.ptr.cast(), out.len))
                    .into_owned()
            };
            sys::ct_str_free(&mut out);
            text
        }
    }

    /// When the alert was posted.
    pub fn timestamp(&self) -> SystemTime {
        // SAFETY: alert is valid for 'a.
        let us = unsafe { sys::ct_alert_timestamp_us(self.ptr.as_ptr()) };
        UNIX_EPOCH + Duration::from_micros(us.max(0) as u64)
    }

    /// The affected torrent's handle for torrent-related alerts (`None`
    /// otherwise); it may already be invalid if the torrent was removed.
    ///
    /// Batch-scoped: it cannot outlive this batch (clone does not extend
    /// that). To keep control past the batch, keep the id/info-hashes and
    /// re-derive via [`Session::find_torrent`](crate::Session::find_torrent).
    pub fn torrent_handle(&self) -> Option<crate::handle::TorrentHandle<'a>> {
        // SAFETY: alert valid for 'a; the handle is cloned to own, and the
        // alert was posted by `self.session`, so the pairing is correct.
        unsafe {
            let ptr = sys::ct_alert_torrent_handle(self.ptr.as_ptr());
            (!ptr.is_null()).then(|| crate::handle::TorrentHandle::from_ptr(ptr, self.session))
        }
    }

    /// The affected torrent's name (or hash as text) for torrent-related alerts.
    pub fn torrent_name(&self) -> Option<Cow<'a, str>> {
        // SAFETY: alert valid; the view shares the batch lifetime 'a.
        unsafe {
            let mut out = sys::ct_str_view {
                ptr: std::ptr::null(),
                len: 0,
            };
            sys::ct_alert_torrent_name(self.ptr.as_ptr(), &mut out).then(|| view_to_cow(out))
        }
    }

    /// The tracker URL for tracker-related alerts.
    pub fn tracker_url(&self) -> Option<Cow<'a, str>> {
        // SAFETY: as above.
        unsafe {
            let mut out = sys::ct_str_view {
                ptr: std::ptr::null(),
                len: 0,
            };
            sys::ct_alert_tracker_url(self.ptr.as_ptr(), &mut out).then(|| view_to_cow(out))
        }
    }

    /// Remote endpoint and peer id for peer-related alerts (None for i2p peers).
    pub fn peer_endpoint(&self) -> Option<(std::net::SocketAddr, Sha1Hash)> {
        // SAFETY: alert is valid for 'a; out params are PODs.
        unsafe {
            let mut ep = sys::ct_endpoint::default();
            let mut pid = sys::ct_sha1::default();
            sys::ct_alert_peer_endpoint(self.ptr.as_ptr(), &mut ep, &mut pid)
                .then(|| (socket_addr_from_ct(&ep), Sha1Hash::from_ct(&pid)))
        }
    }

    pub(crate) fn ptr(&self) -> *const sys::ct_alert {
        self.ptr.as_ptr()
    }
}

impl std::fmt::Debug for RawAlert<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawAlert")
            .field("type", &self.what())
            .field("message", &self.message())
            .finish()
    }
}

use crate::util::{span_to_slice, view_to_cow};

fn ct_error_to_option(err: &sys::ct_error) -> Option<Error> {
    Error::from_ct(err)
}

// Defines a typed alert wrapper over a C view struct.
macro_rules! alert_view {
    ($(#[$doc:meta])* $name:ident, $view:ty, $fill:path) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $name<'a> {
            raw: RawAlert<'a>,
            view: $view,
        }

        impl<'a> $name<'a> {
            fn from_raw(raw: RawAlert<'a>) -> Option<Self> {
                let mut view = <$view>::default();
                // SAFETY: fill only reads the alert; view members share 'a.
                unsafe { $fill(raw.ptr(), &mut view) }.then_some(Self { raw, view })
            }
        }

        impl<'a> Deref for $name<'a> {
            type Target = RawAlert<'a>;
            fn deref(&self) -> &RawAlert<'a> {
                &self.raw
            }
        }
    };
}

alert_view!(
    /// A listen socket was opened.
    ListenSucceededAlert, sys::ct_listen_succeeded_view,
    sys::ct_alert_as_listen_succeeded);

impl ListenSucceededAlert<'_> {
    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.endpoint)
    }

    pub fn socket_type(&self) -> i32 {
        self.view.socket_type
    }
}

alert_view!(
    /// Opening a listen socket failed.
    ListenFailedAlert, sys::ct_listen_failed_view,
    sys::ct_alert_as_listen_failed);

impl<'a> ListenFailedAlert<'a> {
    pub fn interface_name(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.interface_name) }
    }

    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.endpoint)
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn socket_type(&self) -> i32 {
        self.view.socket_type
    }
}

alert_view!(
    /// Our external IP address was learned.
    ExternalIpAlert, sys::ct_external_ip_view, sys::ct_alert_as_external_ip);

impl ExternalIpAlert<'_> {
    pub fn address(&self) -> std::net::IpAddr {
        crate::types::ip_addr_from_ct(&self.view.address)
    }
}

alert_view!(
    /// A UDP-level error.
    UdpErrorAlert, sys::ct_udp_error_view, sys::ct_alert_as_udp_error);

impl UdpErrorAlert<'_> {
    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.endpoint)
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// Response to `Session::session_stats` (also matched by the request registry).
    SessionStatsAlert, sys::ct_session_stats_view,
    sys::ct_alert_as_session_stats);

impl<'a> SessionStatsAlert<'a> {
    /// The counter values, indexed per
    /// [`session_stats_metrics`](crate::stats::session_stats_metrics).
    pub fn counters(&self) -> &'a [i64] {
        if self.view.counters.is_null() {
            return &[];
        }
        // SAFETY: the span shares the batch lifetime 'a.
        unsafe { std::slice::from_raw_parts(self.view.counters, self.view.len) }
    }
}

alert_view!(
    /// A fatal session error.
    SessionErrorAlert, sys::ct_session_error_view,
    sys::ct_alert_as_session_error);

impl SessionErrorAlert<'_> {
    /// The session error. Errors carrying only message text (no code)
    /// yield an [`Error`] with the message and a zero code.
    pub fn error(&self) -> Error {
        ct_error_to_option(&self.view.error).unwrap_or_else(|| Error::from_message(self.message()))
    }
}

alert_view!(
    /// Result of `Session::add_torrent`.
    AddTorrentAlert, sys::ct_add_torrent_view,
    sys::ct_alert_as_add_torrent);

impl<'a> AddTorrentAlert<'a> {
    /// The torrent handle (batch-scoped; see [`RawAlert::torrent_handle`]).
    pub fn handle(&self) -> crate::handle::TorrentHandle<'a> {
        // SAFETY: view.handle is cloned to own, and the alert was posted by
        // `raw.session`, so the pairing is correct.
        unsafe { crate::handle::TorrentHandle::from_ptr(self.view.handle, self.raw.session) }
    }

    /// An owned copy of libtorrent's params snapshot for the add. This is
    /// a selected-field subset (flags, metadata, name, save path,
    /// userdata, tracker id, info hashes) — not the full submitted
    /// params: trackers, web seeds, limits, priorities and resume state
    /// are absent (with `"trackers"`/`"url-list"` present but empty when
    /// serialized). Never persist it as resume data; serialize the
    /// params the add was made with, or request a real resume save.
    pub fn params(&self) -> crate::params::AddTorrentParams {
        // SAFETY: view.params is valid for 'a; the clone is an owned copy.
        unsafe {
            let ptr = sys::ct_atp_clone(self.view.params);
            crate::params::AddTorrentParams::from_owned_ptr(ptr)
        }
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    #[allow(dead_code)]
    pub(crate) fn userdata(&self) -> u64 {
        self.view.userdata as u64
    }
}

alert_view!(
    /// A torrent was removed from the session.
    TorrentRemovedAlert, sys::ct_torrent_removed_view,
    sys::ct_alert_as_torrent_removed);

impl TorrentRemovedAlert<'_> {
    pub fn info_hashes(&self) -> InfoHash {
        InfoHash::from_ct(self.view.info_hashes)
    }
}

/// A torrent finished downloading (100% of selected files). Handwritten
/// rather than via `alert_view!`: its C view struct has no fields.
#[derive(Debug)]
pub struct TorrentFinishedAlert<'a> {
    raw: RawAlert<'a>,
}

impl<'a> TorrentFinishedAlert<'a> {
    fn from_raw(raw: RawAlert<'a>) -> Option<Self> {
        let mut view = sys::ct_torrent_finished_view::default();
        // SAFETY: fill only reads the alert; this view carries no fields.
        unsafe { sys::ct_alert_as_torrent_finished(raw.ptr(), &mut view) }.then_some(Self { raw })
    }
}

impl<'a> Deref for TorrentFinishedAlert<'a> {
    type Target = RawAlert<'a>;
    fn deref(&self) -> &RawAlert<'a> {
        &self.raw
    }
}

alert_view!(
    /// The alert queue overflowed and alerts were dropped.
    AlertsDroppedAlert, sys::ct_alerts_dropped_view,
    sys::ct_alert_as_alerts_dropped);

impl AlertsDroppedAlert<'_> {
    /// Whether alerts with raw type id `ty` were dropped.
    pub fn dropped(&self, ty: i32) -> bool {
        let ty = ty as usize;
        ty < 128 && self.view.dropped[ty / 8] & (1 << (ty % 8)) != 0
    }
}

alert_view!(
    /// An incoming connection was accepted.
    IncomingConnectionAlert, sys::ct_incoming_connection_view,
    sys::ct_alert_as_incoming_connection);

impl IncomingConnectionAlert<'_> {
    pub fn socket_type(&self) -> i32 {
        self.view.socket_type
    }

    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.endpoint)
    }
}

alert_view!(
    /// A port mapping succeeded.
    PortmapAlert, sys::ct_portmap_view, sys::ct_alert_as_portmap);

impl PortmapAlert<'_> {
    pub fn mapping(&self) -> i32 {
        self.view.mapping
    }

    pub fn external_port(&self) -> i32 {
        self.view.external_port
    }

    pub fn protocol(&self) -> i32 {
        self.view.protocol
    }

    pub fn transport(&self) -> i32 {
        self.view.transport
    }

    pub fn local_address(&self) -> std::net::IpAddr {
        crate::types::ip_addr_from_ct(&self.view.local_address)
    }
}

alert_view!(
    /// A port mapping failed.
    PortmapErrorAlert, sys::ct_portmap_error_view,
    sys::ct_alert_as_portmap_error);

impl PortmapErrorAlert<'_> {
    pub fn mapping(&self) -> i32 {
        self.view.mapping
    }

    pub fn transport(&self) -> i32 {
        self.view.transport
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// SOCKS5 proxy error.
    Socks5Alert, sys::ct_socks5_view, sys::ct_alert_as_socks5);

impl Socks5Alert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.ip)
    }
}

alert_view!(
    /// i2p SAM bridge error.
    I2pAlert, sys::ct_i2p_view, sys::ct_alert_as_i2p);

impl I2pAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// Local service discovery error.
    LsdErrorAlert, sys::ct_lsd_error_view, sys::ct_alert_as_lsd_error);

impl LsdErrorAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn local_address(&self) -> std::net::IpAddr {
        crate::types::ip_addr_from_ct(&self.view.local_address)
    }
}

alert_view!(
    /// Session log line (requires `AlertCategory::SESSION_LOG`).
    LogAlert, sys::ct_log_view, sys::ct_alert_as_log);

impl<'a> LogAlert<'a> {
    pub fn log_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.message) }
    }
}

alert_view!(
    /// Torrent log line (requires `AlertCategory::TORRENT_LOG`).
    TorrentLogAlert, sys::ct_log_view, sys::ct_alert_as_torrent_log);

impl<'a> TorrentLogAlert<'a> {
    pub fn log_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.message) }
    }
}

alert_view!(
    /// Port mapping log line.
    PortmapLogAlert, sys::ct_log_view, sys::ct_alert_as_portmap_log);

impl<'a> PortmapLogAlert<'a> {
    pub fn log_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.message) }
    }
}

alert_view!(
    /// DHT log line.
    DhtLogAlert, sys::ct_dht_log_view, sys::ct_alert_as_dht_log);

impl<'a> DhtLogAlert<'a> {
    pub fn log_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.message) }
    }

    pub fn module(&self) -> i32 {
        self.view.module
    }
}

alert_view!(
    /// Peer log line.
    PeerLogAlert, sys::ct_peer_log_view, sys::ct_alert_as_peer_log);

impl<'a> PeerLogAlert<'a> {
    pub fn log_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.message) }
    }

    pub fn event_type(&self) -> i32 {
        self.view.event_type
    }

    pub fn direction(&self) -> i32 {
        self.view.direction
    }
}

alert_view!(
    /// A torrent changed state.
    StateChangedAlert, sys::ct_state_changed_view,
    sys::ct_alert_as_state_changed);

impl StateChangedAlert<'_> {
    pub fn state(&self) -> i32 {
        self.view.state
    }

    pub fn prev_state(&self) -> i32 {
        self.view.prev_state
    }
}

alert_view!(
    /// A torrent entered an error state.
    TorrentErrorAlert, sys::ct_torrent_error_view,
    sys::ct_alert_as_torrent_error);

impl<'a> TorrentErrorAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn filename(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.filename) }
    }
}

alert_view!(
    /// A torrent's files were deleted.
    TorrentDeletedAlert, sys::ct_torrent_deleted_view,
    sys::ct_alert_as_torrent_deleted);

impl TorrentDeletedAlert<'_> {
    pub fn info_hashes(&self) -> InfoHash {
        InfoHash::from_ct(self.view.info_hashes)
    }
}

alert_view!(
    /// Deleting a torrent's files failed.
    TorrentDeleteFailedAlert, sys::ct_torrent_delete_failed_view,
    sys::ct_alert_as_torrent_delete_failed);

impl TorrentDeleteFailedAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn info_hashes(&self) -> InfoHash {
        InfoHash::from_ct(self.view.info_hashes)
    }
}

alert_view!(
    /// A performance warning.
    PerformanceAlert, sys::ct_performance_view, sys::ct_alert_as_performance);

impl PerformanceAlert<'_> {
    pub fn warning_code(&self) -> i32 {
        self.view.warning_code
    }
}

alert_view!(
    /// Fetching metadata (magnet) failed.
    MetadataFailedAlert, sys::ct_metadata_failed_view,
    sys::ct_alert_as_metadata_failed);

impl MetadataFailedAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// Resume data was rejected.
    FastresumeRejectedAlert, sys::ct_fastresume_rejected_view,
    sys::ct_alert_as_fastresume_rejected);

impl<'a> FastresumeRejectedAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn file_path(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.file_path) }
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }
}

alert_view!(
    /// Response to
    /// [`TorrentHandle::save_resume_data`](crate::TorrentHandle::save_resume_data):
    /// carries the resume data itself.
    SaveResumeDataAlert, sys::ct_save_resume_data_view,
    sys::ct_alert_as_save_resume_data);

impl SaveResumeDataAlert<'_> {
    /// The resume data (an owned copy). Serialize it with
    /// [`write_resume_data`](SaveResumeDataAlert::write_resume_data) (which
    /// embeds the torrent's client data) or
    /// [`Session::write_resume_data`](crate::Session::write_resume_data)
    /// (which does not), or add it back as-is.
    pub fn params(&self) -> crate::params::AddTorrentParams {
        // SAFETY: view.params is valid for 'a; the clone is an owned copy.
        unsafe {
            let ptr = sys::ct_atp_clone(self.view.params);
            crate::params::AddTorrentParams::from_owned_ptr(ptr)
        }
    }

    /// Serializes the resume data, embedding the torrent's
    /// [`ClientData`](crate::ClientData) under the `"rbt-data"` key (see
    /// [`TorrentHandle::write_resume_data`](crate::TorrentHandle::write_resume_data)).
    /// When the torrent no longer exists — this alert may trail its
    /// removal — the plain data is written without the key.
    pub fn write_resume_data(&self) -> Result<Vec<u8>> {
        match self.raw.torrent_handle() {
            Some(handle) => handle.write_resume_data(&self.params()),
            None => crate::Session::write_resume_data(&self.params()),
        }
    }
}

alert_view!(
    /// Generating resume data failed (posted instead of [`SaveResumeDataAlert`]).
    SaveResumeDataFailedAlert, sys::ct_save_resume_data_failed_view,
    sys::ct_alert_as_save_resume_data_failed);

impl SaveResumeDataFailedAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// Response to
    /// [`Session::post_torrent_updates`](crate::Session::post_torrent_updates) /
    /// [`TorrentHandle::post_status`](crate::TorrentHandle::post_status):
    /// status snapshots of the torrents that changed since the last update.
    StateUpdateAlert, sys::ct_state_update_view,
    sys::ct_alert_as_state_update);

impl StateUpdateAlert<'_> {
    /// The number of status snapshots in this update.
    pub fn len(&self) -> usize {
        self.view.count
    }

    pub fn is_empty(&self) -> bool {
        self.view.count == 0
    }

    /// Status snapshot `i` (an owned copy); `None` if out of range. Map it
    /// to its torrent via [`TorrentStatus::id`](crate::TorrentStatus::id)
    /// or [`TorrentStatus::info_hashes`](crate::TorrentStatus::info_hashes).
    pub fn status(&self, i: usize) -> Option<crate::status::TorrentStatus> {
        // SAFETY: alert valid; the shim returns an owned copy or NULL.
        unsafe {
            let ptr = sys::ct_alert_state_update_status(self.raw.ptr(), i);
            (!ptr.is_null()).then(|| crate::status::TorrentStatus::from_ptr(ptr))
        }
    }

    /// All status snapshots (owned copies).
    pub fn statuses(&self) -> Vec<crate::status::TorrentStatus> {
        (0..self.len()).filter_map(|i| self.status(i)).collect()
    }
}

alert_view!(
    /// Response to
    /// [`TorrentHandle::post_peer_info`](crate::TorrentHandle::post_peer_info):
    /// the torrent's currently connected peers.
    PeerInfoAlert, sys::ct_peer_info_list_view, sys::ct_alert_as_peer_info);

impl PeerInfoAlert<'_> {
    /// The number of connected peers in this snapshot.
    pub fn len(&self) -> usize {
        self.view.count
    }

    pub fn is_empty(&self) -> bool {
        self.view.count == 0
    }

    /// Peer `i` (an owned copy); `None` if out of range.
    pub fn peer(&self, i: usize) -> Option<crate::peers::PeerInfo> {
        // SAFETY: alert valid; the shim returns an owned copy or NULL.
        unsafe {
            let ptr = sys::ct_alert_peer_info(self.raw.ptr(), i);
            (!ptr.is_null()).then(|| crate::peers::PeerInfo::from_ptr(ptr))
        }
    }

    /// All peers (owned copies).
    pub fn peers(&self) -> Vec<crate::peers::PeerInfo> {
        (0..self.len()).filter_map(|i| self.peer(i)).collect()
    }
}

alert_view!(
    /// Response to
    /// [`TorrentHandle::post_file_progress`](crate::TorrentHandle::post_file_progress):
    /// bytes completed per file.
    FileProgressAlert, sys::ct_file_progress_view,
    sys::ct_alert_as_file_progress);

impl<'a> FileProgressAlert<'a> {
    /// Bytes completed per file, indexed by file index.
    pub fn progress(&self) -> &'a [i64] {
        if self.view.progress.is_null() {
            return &[];
        }
        // SAFETY: the span shares the batch lifetime 'a.
        unsafe { std::slice::from_raw_parts(self.view.progress, self.view.len) }
    }
}

alert_view!(
    /// Response to
    /// [`TorrentHandle::post_trackers`](crate::TorrentHandle::post_trackers):
    /// the torrent's tracker list.
    TrackerListAlert, sys::ct_tracker_list_view,
    sys::ct_alert_as_tracker_list);

/// One tracker in a [`TrackerListAlert`].
#[derive(Debug)]
pub struct TrackerEntry<'a> {
    /// Announce URL as it appeared in the torrent/magnet.
    pub url: Cow<'a, str>,
    /// The `&trackerid=` argument sent to the tracker (normally empty).
    pub trackerid: Cow<'a, str>,
    /// The tier this tracker belongs to.
    pub tier: i32,
    /// Announce failures in a row before the tracker is given up on (0 = unlimited).
    pub fail_limit: i32,
    /// Where we heard about this tracker (`lt::announce_entry::tracker_source` bits).
    pub source: u32,
    /// True once the tracker has responded to an announce.
    pub verified: bool,
}

impl<'a> TrackerListAlert<'a> {
    /// The number of trackers.
    pub fn len(&self) -> usize {
        self.view.count
    }

    pub fn is_empty(&self) -> bool {
        self.view.count == 0
    }

    /// Tracker `i`; `None` if out of range.
    pub fn get(&self, i: usize) -> Option<TrackerEntry<'a>> {
        let mut entry = sys::ct_tracker_list_entry::default();
        // SAFETY: alert valid; the entry's string views share 'a.
        unsafe {
            sys::ct_alert_tracker_list_entry(self.raw.ptr(), i, &mut entry).then(|| TrackerEntry {
                url: view_to_cow(entry.url),
                trackerid: view_to_cow(entry.trackerid),
                tier: entry.tier,
                fail_limit: entry.fail_limit,
                source: entry.source,
                verified: entry.verified,
            })
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = TrackerEntry<'a>> + '_ {
        (0..self.len()).filter_map(move |i| self.get(i))
    }
}

alert_view!(
    /// Response to `read_piece`.
    ReadPieceAlert, sys::ct_read_piece_view, sys::ct_alert_as_read_piece);

impl<'a> ReadPieceAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn piece(&self) -> i32 {
        self.view.piece
    }

    /// The piece data (empty on failure).
    pub fn data(&self) -> &'a [u8] {
        if self.view.buffer.is_null() || self.view.size <= 0 {
            return &[];
        }
        // SAFETY: buffer shares the batch lifetime 'a.
        unsafe { std::slice::from_raw_parts(self.view.buffer, self.view.size as usize) }
    }
}

alert_view!(
    /// A piece completed and passed its hash check.
    PieceFinishedAlert, sys::ct_piece_finished_view,
    sys::ct_alert_as_piece_finished);

impl PieceFinishedAlert<'_> {
    pub fn piece_index(&self) -> i32 {
        self.view.piece_index
    }
}

alert_view!(
    /// A piece failed its hash check.
    HashFailedAlert, sys::ct_hash_failed_view, sys::ct_alert_as_hash_failed);

impl HashFailedAlert<'_> {
    pub fn piece_index(&self) -> i32 {
        self.view.piece_index
    }
}

alert_view!(
    /// A block event (finished/downloading/timeout/... — check [`RawAlert::alert_type`]).
    BlockAlert, sys::ct_block_view, sys::ct_alert_as_block);

impl BlockAlert<'_> {
    pub fn block_index(&self) -> i32 {
        self.view.block_index
    }

    pub fn piece_index(&self) -> i32 {
        self.view.piece_index
    }
}

alert_view!(
    /// A peer sent an invalid request.
    InvalidRequestAlert, sys::ct_invalid_request_view,
    sys::ct_alert_as_invalid_request);

impl InvalidRequestAlert<'_> {
    pub fn request(&self) -> PeerRequest {
        PeerRequest::from_ct(&self.view.request)
    }

    pub fn we_have(&self) -> bool {
        self.view.we_have
    }

    pub fn peer_interested(&self) -> bool {
        self.view.peer_interested
    }

    pub fn withheld(&self) -> bool {
        self.view.withheld
    }
}

alert_view!(
    /// An incoming block request while those alerts are enabled.
    IncomingRequestAlert, sys::ct_incoming_request_view,
    sys::ct_alert_as_incoming_request);

impl IncomingRequestAlert<'_> {
    pub fn request(&self) -> PeerRequest {
        PeerRequest::from_ct(&self.view.request)
    }
}

alert_view!(
    /// A file finished downloading.
    FileCompletedAlert, sys::ct_file_completed_view,
    sys::ct_alert_as_file_completed);

impl FileCompletedAlert<'_> {
    pub fn index(&self) -> i32 {
        self.view.index
    }
}

alert_view!(
    /// A file was renamed.
    FileRenamedAlert, sys::ct_file_renamed_view,
    sys::ct_alert_as_file_renamed);

impl<'a> FileRenamedAlert<'a> {
    pub fn index(&self) -> i32 {
        self.view.index
    }

    pub fn new_name(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.new_name) }
    }

    pub fn old_name(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.old_name) }
    }
}

alert_view!(
    /// Renaming a file failed.
    FileRenameFailedAlert, sys::ct_file_rename_failed_view,
    sys::ct_alert_as_file_rename_failed);

impl FileRenameFailedAlert<'_> {
    pub fn index(&self) -> i32 {
        self.view.index
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// A file-level I/O error.
    FileErrorAlert, sys::ct_file_error_view, sys::ct_alert_as_file_error);

impl<'a> FileErrorAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn filename(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.filename) }
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }
}

/// A file priority update completed (upstream only posts it on success).
/// Handwritten rather than via `alert_view!`: its C view struct has no
/// fields.
#[derive(Debug)]
pub struct FilePrioAlert<'a> {
    raw: RawAlert<'a>,
}

impl<'a> FilePrioAlert<'a> {
    fn from_raw(raw: RawAlert<'a>) -> Option<Self> {
        let mut view = sys::ct_file_prio_view::default();
        // SAFETY: fill only reads the alert; this view carries no fields.
        unsafe { sys::ct_alert_as_file_prio(raw.ptr(), &mut view) }.then_some(Self { raw })
    }
}

impl<'a> Deref for FilePrioAlert<'a> {
    type Target = RawAlert<'a>;
    fn deref(&self) -> &RawAlert<'a> {
        &self.raw
    }
}

alert_view!(
    /// Storage was moved to a new path.
    StorageMovedAlert, sys::ct_storage_moved_view,
    sys::ct_alert_as_storage_moved);

impl<'a> StorageMovedAlert<'a> {
    pub fn storage_path(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.storage_path) }
    }

    pub fn old_path(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.old_path) }
    }
}

alert_view!(
    /// Moving storage failed.
    StorageMovedFailedAlert, sys::ct_storage_moved_failed_view,
    sys::ct_alert_as_storage_moved_failed);

impl<'a> StorageMovedFailedAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn file_path(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.file_path) }
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }
}

alert_view!(
    /// A peer connection was established.
    PeerConnectAlert, sys::ct_peer_connect_view,
    sys::ct_alert_as_peer_connect);

impl PeerConnectAlert<'_> {
    pub fn direction(&self) -> i32 {
        self.view.direction
    }

    pub fn socket_type(&self) -> i32 {
        self.view.socket_type
    }
}

alert_view!(
    /// A peer disconnected.
    PeerDisconnectedAlert, sys::ct_peer_disconnected_view,
    sys::ct_alert_as_peer_disconnected);

impl PeerDisconnectedAlert<'_> {
    pub fn socket_type(&self) -> i32 {
        self.view.socket_type
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn reason(&self) -> i32 {
        self.view.reason
    }
}

alert_view!(
    /// A peer-level protocol error.
    PeerErrorAlert, sys::ct_peer_error_view, sys::ct_alert_as_peer_error);

impl PeerErrorAlert<'_> {
    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// A peer was blocked from connecting.
    PeerBlockedAlert, sys::ct_peer_blocked_view,
    sys::ct_alert_as_peer_blocked);

impl PeerBlockedAlert<'_> {
    pub fn reason(&self) -> i32 {
        self.view.reason
    }
}

alert_view!(
    /// A tracker announce failed.
    TrackerErrorAlert, sys::ct_tracker_error_view,
    sys::ct_alert_as_tracker_error);

impl<'a> TrackerErrorAlert<'a> {
    pub fn times_in_row(&self) -> i32 {
        self.view.times_in_row
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }

    pub fn failure_reason(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.failure_reason) }
    }
}

alert_view!(
    /// A tracker sent a warning.
    TrackerWarningAlert, sys::ct_tracker_warning_view,
    sys::ct_alert_as_tracker_warning);

impl<'a> TrackerWarningAlert<'a> {
    pub fn warning_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.warning_message) }
    }
}

alert_view!(
    /// A tracker announce succeeded.
    TrackerReplyAlert, sys::ct_tracker_reply_view,
    sys::ct_alert_as_tracker_reply);

impl TrackerReplyAlert<'_> {
    pub fn num_peers(&self) -> i32 {
        self.view.num_peers
    }
}

alert_view!(
    /// A tracker announce was sent.
    TrackerAnnounceAlert, sys::ct_tracker_announce_view,
    sys::ct_alert_as_tracker_announce);

impl TrackerAnnounceAlert<'_> {
    pub fn event(&self) -> i32 {
        self.view.event
    }
}

alert_view!(
    /// A scrape succeeded.
    ScrapeReplyAlert, sys::ct_scrape_reply_view,
    sys::ct_alert_as_scrape_reply);

impl ScrapeReplyAlert<'_> {
    pub fn incomplete(&self) -> i32 {
        self.view.incomplete
    }

    pub fn complete(&self) -> i32 {
        self.view.complete
    }
}

alert_view!(
    /// A scrape failed.
    ScrapeFailedAlert, sys::ct_scrape_failed_view,
    sys::ct_alert_as_scrape_failed);

impl<'a> ScrapeFailedAlert<'a> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn error_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.error_message) }
    }
}

alert_view!(
    /// The DHT returned peers for a torrent.
    DhtReplyAlert, sys::ct_dht_reply_view, sys::ct_alert_as_dht_reply);

impl DhtReplyAlert<'_> {
    pub fn num_peers(&self) -> i32 {
        self.view.num_peers
    }
}

alert_view!(
    /// A tracker assigned us a tracker id.
    TrackeridAlert, sys::ct_trackerid_view, sys::ct_alert_as_trackerid);

impl<'a> TrackeridAlert<'a> {
    pub fn trackerid(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.trackerid) }
    }
}

alert_view!(
    /// A web seed reported an error.
    UrlSeedAlert, sys::ct_url_seed_view, sys::ct_alert_as_url_seed);

impl<'a> UrlSeedAlert<'a> {
    pub fn server_url(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.server_url) }
    }

    pub fn error_message(&self) -> Cow<'a, str> {
        // SAFETY: view members are valid for 'a.
        unsafe { view_to_cow(self.view.error_message) }
    }

    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }
}

alert_view!(
    /// A DHT node announced to an info-hash.
    DhtAnnounceAlert, sys::ct_dht_announce_view,
    sys::ct_alert_as_dht_announce);

impl DhtAnnounceAlert<'_> {
    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.ip)
    }

    pub fn info_hash(&self) -> Sha1Hash {
        Sha1Hash::from_ct(&self.view.info_hash)
    }
}

alert_view!(
    /// A DHT get_peers request was received.
    DhtGetPeersAlert, sys::ct_dht_get_peers_view,
    sys::ct_alert_as_dht_get_peers);

impl DhtGetPeersAlert<'_> {
    pub fn info_hash(&self) -> Sha1Hash {
        Sha1Hash::from_ct(&self.view.info_hash)
    }
}

alert_view!(
    /// A DHT-level error.
    DhtErrorAlert, sys::ct_dht_error_view, sys::ct_alert_as_dht_error);

impl DhtErrorAlert<'_> {
    pub fn error(&self) -> Option<Error> {
        ct_error_to_option(&self.view.error)
    }

    pub fn operation(&self) -> i32 {
        self.view.operation
    }
}

alert_view!(
    /// A DHT put completed.
    DhtPutAlert, sys::ct_dht_put_view, sys::ct_alert_as_dht_put);

impl<'a> DhtPutAlert<'a> {
    pub fn target(&self) -> Sha1Hash {
        Sha1Hash::from_ct(&self.view.target)
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.view.public_key
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.view.signature
    }

    pub fn salt(&self) -> &'a [u8] {
        // SAFETY: view members are valid for 'a.
        unsafe {
            span_to_slice(sys::ct_span {
                ptr: self.view.salt.ptr.cast(),
                len: self.view.salt.len,
            })
        }
    }

    pub fn seq(&self) -> i64 {
        self.view.seq
    }

    pub fn num_success(&self) -> i32 {
        self.view.num_success
    }
}

alert_view!(
    /// An outgoing DHT get_peers request.
    DhtOutgoingGetPeersAlert, sys::ct_dht_outgoing_get_peers_view,
    sys::ct_alert_as_dht_outgoing_get_peers);

impl DhtOutgoingGetPeersAlert<'_> {
    pub fn info_hash(&self) -> Sha1Hash {
        Sha1Hash::from_ct(&self.view.info_hash)
    }

    pub fn obfuscated_info_hash(&self) -> Sha1Hash {
        Sha1Hash::from_ct(&self.view.obfuscated_info_hash)
    }

    pub fn endpoint(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.endpoint)
    }
}

alert_view!(
    /// A raw DHT packet (requires `AlertCategory::DHT_LOG`).
    DhtPktAlert, sys::ct_dht_pkt_view, sys::ct_alert_as_dht_pkt);

impl<'a> DhtPktAlert<'a> {
    pub fn packet(&self) -> &'a [u8] {
        // SAFETY: view members are valid for 'a.
        unsafe { span_to_slice(self.view.packet) }
    }

    pub fn direction(&self) -> i32 {
        self.view.direction
    }

    pub fn node(&self) -> std::net::SocketAddr {
        socket_addr_from_ct(&self.view.node)
    }
}

/// A typed alert, borrowed from a [`Batch`]. Alerts without a typed
/// wrapper yet (and types newer than these bindings) appear as
/// [`Alert::Other`]; every variant derefs to [`RawAlert`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Alert<'a> {
    ListenSucceeded(ListenSucceededAlert<'a>),
    ListenFailed(ListenFailedAlert<'a>),
    ExternalIp(ExternalIpAlert<'a>),
    UdpError(UdpErrorAlert<'a>),
    SessionStats(SessionStatsAlert<'a>),
    SessionError(SessionErrorAlert<'a>),
    AddTorrent(AddTorrentAlert<'a>),
    TorrentRemoved(TorrentRemovedAlert<'a>),
    TorrentFinished(TorrentFinishedAlert<'a>),
    AlertsDropped(AlertsDroppedAlert<'a>),
    IncomingConnection(IncomingConnectionAlert<'a>),
    Portmap(PortmapAlert<'a>),
    PortmapError(PortmapErrorAlert<'a>),
    Socks5(Socks5Alert<'a>),
    I2p(I2pAlert<'a>),
    LsdError(LsdErrorAlert<'a>),
    Log(LogAlert<'a>),
    TorrentLog(TorrentLogAlert<'a>),
    PortmapLog(PortmapLogAlert<'a>),
    DhtLog(DhtLogAlert<'a>),
    PeerLog(PeerLogAlert<'a>),
    StateChanged(StateChangedAlert<'a>),
    TorrentError(TorrentErrorAlert<'a>),
    TorrentDeleted(TorrentDeletedAlert<'a>),
    TorrentDeleteFailed(TorrentDeleteFailedAlert<'a>),
    Performance(PerformanceAlert<'a>),
    MetadataFailed(MetadataFailedAlert<'a>),
    FastresumeRejected(FastresumeRejectedAlert<'a>),
    SaveResumeData(SaveResumeDataAlert<'a>),
    SaveResumeDataFailed(SaveResumeDataFailedAlert<'a>),
    StateUpdate(StateUpdateAlert<'a>),
    PeerInfo(PeerInfoAlert<'a>),
    FileProgress(FileProgressAlert<'a>),
    TrackerList(TrackerListAlert<'a>),
    ReadPiece(ReadPieceAlert<'a>),
    PieceFinished(PieceFinishedAlert<'a>),
    HashFailed(HashFailedAlert<'a>),
    RequestDropped(BlockAlert<'a>),
    BlockTimeout(BlockAlert<'a>),
    BlockFinished(BlockAlert<'a>),
    BlockDownloading(BlockAlert<'a>),
    UnwantedBlock(BlockAlert<'a>),
    BlockUploaded(BlockAlert<'a>),
    InvalidRequest(InvalidRequestAlert<'a>),
    IncomingRequest(IncomingRequestAlert<'a>),
    FileCompleted(FileCompletedAlert<'a>),
    FileRenamed(FileRenamedAlert<'a>),
    FileRenameFailed(FileRenameFailedAlert<'a>),
    FileError(FileErrorAlert<'a>),
    FilePrio(FilePrioAlert<'a>),
    StorageMoved(StorageMovedAlert<'a>),
    StorageMovedFailed(StorageMovedFailedAlert<'a>),
    PeerConnect(PeerConnectAlert<'a>),
    PeerDisconnected(PeerDisconnectedAlert<'a>),
    PeerError(PeerErrorAlert<'a>),
    PeerBlocked(PeerBlockedAlert<'a>),
    TrackerError(TrackerErrorAlert<'a>),
    TrackerWarning(TrackerWarningAlert<'a>),
    TrackerReply(TrackerReplyAlert<'a>),
    TrackerAnnounce(TrackerAnnounceAlert<'a>),
    ScrapeReply(ScrapeReplyAlert<'a>),
    ScrapeFailed(ScrapeFailedAlert<'a>),
    DhtReply(DhtReplyAlert<'a>),
    Trackerid(TrackeridAlert<'a>),
    UrlSeed(UrlSeedAlert<'a>),
    DhtAnnounce(DhtAnnounceAlert<'a>),
    DhtGetPeers(DhtGetPeersAlert<'a>),
    DhtError(DhtErrorAlert<'a>),
    DhtPut(DhtPutAlert<'a>),
    DhtOutgoingGetPeers(DhtOutgoingGetPeersAlert<'a>),
    DhtPkt(DhtPktAlert<'a>),
    /// Payload-less status alerts and alerts without a typed wrapper yet.
    Other(RawAlert<'a>),
}

impl<'a> Alert<'a> {
    fn from_raw(raw: RawAlert<'a>) -> Alert<'a> {
        use AlertType as T;
        macro_rules! v {
            ($variant:ident, $wrapper:ident) => {
                $wrapper::from_raw(raw).map(Alert::$variant)
            };
        }
        let Some(ty) = raw.alert_type() else {
            return Alert::Other(raw);
        };
        let alert = match ty {
            T::ListenSucceeded => v!(ListenSucceeded, ListenSucceededAlert),
            T::ListenFailed => v!(ListenFailed, ListenFailedAlert),
            T::ExternalIp => v!(ExternalIp, ExternalIpAlert),
            T::UdpError => v!(UdpError, UdpErrorAlert),
            T::SessionStats => v!(SessionStats, SessionStatsAlert),
            T::SessionError => v!(SessionError, SessionErrorAlert),
            T::AddTorrent => v!(AddTorrent, AddTorrentAlert),
            T::TorrentRemoved => v!(TorrentRemoved, TorrentRemovedAlert),
            T::TorrentFinished => v!(TorrentFinished, TorrentFinishedAlert),
            T::AlertsDropped => v!(AlertsDropped, AlertsDroppedAlert),
            T::IncomingConnection => {
                v!(IncomingConnection, IncomingConnectionAlert)
            }
            T::Portmap => v!(Portmap, PortmapAlert),
            T::PortmapError => v!(PortmapError, PortmapErrorAlert),
            T::Socks5 => v!(Socks5, Socks5Alert),
            T::I2p => v!(I2p, I2pAlert),
            T::LsdError => v!(LsdError, LsdErrorAlert),
            T::Log => v!(Log, LogAlert),
            T::TorrentLog => v!(TorrentLog, TorrentLogAlert),
            T::PortmapLog => v!(PortmapLog, PortmapLogAlert),
            T::DhtLog => v!(DhtLog, DhtLogAlert),
            T::PeerLog => v!(PeerLog, PeerLogAlert),
            T::StateChanged => v!(StateChanged, StateChangedAlert),
            T::TorrentError => v!(TorrentError, TorrentErrorAlert),
            T::TorrentDeleted => v!(TorrentDeleted, TorrentDeletedAlert),
            T::TorrentDeleteFailed => {
                v!(TorrentDeleteFailed, TorrentDeleteFailedAlert)
            }
            T::Performance => v!(Performance, PerformanceAlert),
            T::MetadataFailed => v!(MetadataFailed, MetadataFailedAlert),
            T::FastresumeRejected => {
                v!(FastresumeRejected, FastresumeRejectedAlert)
            }
            T::SaveResumeData => v!(SaveResumeData, SaveResumeDataAlert),
            T::SaveResumeDataFailed => {
                v!(SaveResumeDataFailed, SaveResumeDataFailedAlert)
            }
            T::StateUpdate => v!(StateUpdate, StateUpdateAlert),
            T::PeerInfo => v!(PeerInfo, PeerInfoAlert),
            T::FileProgress => v!(FileProgress, FileProgressAlert),
            T::TrackerList => v!(TrackerList, TrackerListAlert),
            T::ReadPiece => v!(ReadPiece, ReadPieceAlert),
            T::PieceFinished => v!(PieceFinished, PieceFinishedAlert),
            T::HashFailed => v!(HashFailed, HashFailedAlert),
            T::RequestDropped => v!(RequestDropped, BlockAlert),
            T::BlockTimeout => v!(BlockTimeout, BlockAlert),
            T::BlockFinished => v!(BlockFinished, BlockAlert),
            T::BlockDownloading => v!(BlockDownloading, BlockAlert),
            T::UnwantedBlock => v!(UnwantedBlock, BlockAlert),
            T::BlockUploaded => v!(BlockUploaded, BlockAlert),
            T::InvalidRequest => v!(InvalidRequest, InvalidRequestAlert),
            T::IncomingRequest => v!(IncomingRequest, IncomingRequestAlert),
            T::FileCompleted => v!(FileCompleted, FileCompletedAlert),
            T::FileRenamed => v!(FileRenamed, FileRenamedAlert),
            T::FileRenameFailed => v!(FileRenameFailed, FileRenameFailedAlert),
            T::FileError => v!(FileError, FileErrorAlert),
            T::FilePrio => v!(FilePrio, FilePrioAlert),
            T::StorageMoved => v!(StorageMoved, StorageMovedAlert),
            T::StorageMovedFailed => {
                v!(StorageMovedFailed, StorageMovedFailedAlert)
            }
            T::PeerConnect => v!(PeerConnect, PeerConnectAlert),
            T::PeerDisconnected => v!(PeerDisconnected, PeerDisconnectedAlert),
            T::PeerError => v!(PeerError, PeerErrorAlert),
            T::PeerBlocked => v!(PeerBlocked, PeerBlockedAlert),
            T::TrackerError => v!(TrackerError, TrackerErrorAlert),
            T::TrackerWarning => v!(TrackerWarning, TrackerWarningAlert),
            T::TrackerReply => v!(TrackerReply, TrackerReplyAlert),
            T::TrackerAnnounce => v!(TrackerAnnounce, TrackerAnnounceAlert),
            T::ScrapeReply => v!(ScrapeReply, ScrapeReplyAlert),
            T::ScrapeFailed => v!(ScrapeFailed, ScrapeFailedAlert),
            T::DhtReply => v!(DhtReply, DhtReplyAlert),
            T::Trackerid => v!(Trackerid, TrackeridAlert),
            T::UrlSeed => v!(UrlSeed, UrlSeedAlert),
            T::DhtAnnounce => v!(DhtAnnounce, DhtAnnounceAlert),
            T::DhtGetPeers => v!(DhtGetPeers, DhtGetPeersAlert),
            T::DhtError => v!(DhtError, DhtErrorAlert),
            T::DhtPut => v!(DhtPut, DhtPutAlert),
            T::DhtOutgoingGetPeers => {
                v!(DhtOutgoingGetPeers, DhtOutgoingGetPeersAlert)
            }
            T::DhtPkt => v!(DhtPkt, DhtPktAlert),
            _ => None,
        };
        alert.unwrap_or(Alert::Other(raw))
    }

    /// The untyped view of this alert.
    pub fn raw(&self) -> &RawAlert<'a> {
        macro_rules! arms {
            ($($variant:ident),* $(,)?) => {
                match self {
                    $(Alert::$variant(a) => &a.raw,)*
                    Alert::Other(raw) => raw,
                }
            };
        }
        arms!(
            ListenSucceeded,
            ListenFailed,
            ExternalIp,
            UdpError,
            SessionStats,
            SessionError,
            AddTorrent,
            TorrentRemoved,
            TorrentFinished,
            AlertsDropped,
            IncomingConnection,
            Portmap,
            PortmapError,
            Socks5,
            I2p,
            LsdError,
            Log,
            TorrentLog,
            PortmapLog,
            DhtLog,
            PeerLog,
            StateChanged,
            TorrentError,
            TorrentDeleted,
            TorrentDeleteFailed,
            Performance,
            MetadataFailed,
            FastresumeRejected,
            SaveResumeData,
            SaveResumeDataFailed,
            StateUpdate,
            PeerInfo,
            FileProgress,
            TrackerList,
            ReadPiece,
            PieceFinished,
            HashFailed,
            RequestDropped,
            BlockTimeout,
            BlockFinished,
            BlockDownloading,
            UnwantedBlock,
            BlockUploaded,
            InvalidRequest,
            IncomingRequest,
            FileCompleted,
            FileRenamed,
            FileRenameFailed,
            FileError,
            FilePrio,
            StorageMoved,
            StorageMovedFailed,
            PeerConnect,
            PeerDisconnected,
            PeerError,
            PeerBlocked,
            TrackerError,
            TrackerWarning,
            TrackerReply,
            TrackerAnnounce,
            ScrapeReply,
            ScrapeFailed,
            DhtReply,
            Trackerid,
            UrlSeed,
            DhtAnnounce,
            DhtGetPeers,
            DhtError,
            DhtPut,
            DhtOutgoingGetPeers,
            DhtPkt,
        )
    }
}

/// Receiver for the session's alerts, obtained from [`Session::alerts`](crate::Session::alerts).
pub struct Alerts<'s> {
    session: &'s Session,
    batch: NonNull<sys::ct_alert_batch>,
}

// SAFETY: the batch is exclusively owned (popping requires &mut) and
// lt::session::pop_alerts is thread-safe; &self only exposes reads of the
// popped batch. The `&Session` borrow keeps the session alive.
unsafe impl Send for Alerts<'_> {}
unsafe impl Sync for Alerts<'_> {}

impl<'s> Alerts<'s> {
    pub(crate) fn new(session: &'s Session) -> Alerts<'s> {
        // SAFETY: constructor returns an owned batch (null on OOM).
        let batch = unsafe { sys::ct_alert_batch_new() };
        Alerts {
            session,
            batch: NonNull::new(batch).expect("allocation failed"),
        }
    }

    /// Waits until at least one alert is pending and pops all of them.
    /// Popping invalidates the previous batch (borrow-enforced) and
    /// resolves pending request futures whose responses are in the new
    /// batch.
    pub async fn next_batch(&mut self) -> Result<Batch<'_, 's>> {
        loop {
            if self.pop()? > 0 {
                return Ok(Batch { alerts: self });
            }
            self.session.inner().notify.notified().await;
        }
    }

    /// Pops without waiting; `None` if no alerts were pending.
    pub fn try_next_batch(&mut self) -> Result<Option<Batch<'_, 's>>> {
        if self.pop()? > 0 {
            Ok(Some(Batch { alerts: self }))
        } else {
            Ok(None)
        }
    }

    fn pop(&mut self) -> Result<usize> {
        // SAFETY: session and batch are valid; popping transfers alert
        // validity to this batch until the next pop (gated on &mut self).
        with_error(|err| unsafe {
            sys::ct_session_pop_alerts(self.session.ptr(), self.batch.as_ptr(), err);
        })?;
        // SAFETY: batch is valid.
        let len = unsafe { sys::ct_alert_batch_len(self.batch.as_ptr()) };
        if len > 0 {
            self.session.inner().registry.process(self.batch.as_ptr());
        }
        Ok(len)
    }
}

impl Drop for Alerts<'_> {
    fn drop(&mut self) {
        // SAFETY: we own the batch.
        unsafe { sys::ct_alert_batch_free(self.batch.as_ptr()) };
        self.session.release_alerts();
    }
}

/// One popped batch of alerts. Alerts borrowed from it stay valid until the
/// next [`Alerts::next_batch`] call (enforced by borrows).
pub struct Batch<'a, 's> {
    alerts: &'a Alerts<'s>,
}

impl<'a> Batch<'a, '_> {
    pub fn len(&self) -> usize {
        // SAFETY: batch is valid.
        unsafe { sys::ct_alert_batch_len(self.alerts.batch.as_ptr()) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, i: usize) -> Option<Alert<'a>> {
        if i >= self.len() {
            return None;
        }
        // SAFETY: index checked; the alert lives as long as the batch ('a).
        let ptr = unsafe { sys::ct_alert_batch_get(self.alerts.batch.as_ptr(), i) };
        let raw = RawAlert {
            ptr: NonNull::new(ptr.cast_mut()).expect("null alert in batch"),
            session: self.alerts.session,
        };
        Some(Alert::from_raw(raw))
    }

    pub fn iter(&self) -> impl Iterator<Item = Alert<'a>> + '_ {
        (0..self.len()).filter_map(move |i| self.get(i))
    }
}

pub(crate) fn raw_alert_for_registry(
    batch: *mut sys::ct_alert_batch,
    i: usize,
) -> *const sys::ct_alert {
    // SAFETY: caller guarantees i < len.
    unsafe { sys::ct_alert_batch_get(batch, i) }
}

pub(crate) fn batch_len_for_registry(batch: *mut sys::ct_alert_batch) -> usize {
    // SAFETY: caller passes a valid batch.
    unsafe { sys::ct_alert_batch_len(batch) }
}
