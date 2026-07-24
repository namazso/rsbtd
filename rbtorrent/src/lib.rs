// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ergonomic Rust bindings for [libtorrent-rasterbar](https://libtorrent.org)
//! 2.1+, layered over the `libctorrent` C shim.
//!
//! # Quick start
//!
//! ```no_run
//! use rbtorrent::{AddTorrentParams, Alert, Session, SessionParams};
//!
//! #[tokio::main]
//! async fn main() -> rbtorrent::Result<()> {
//!     let session = Session::new(SessionParams::new())?;
//!     let mut alerts = session.alerts();
//!
//!     let mut params = AddTorrentParams::from_torrent_file("example.torrent")?;
//!     params.set_save_path("./downloads");
//!
//!     // `add_torrent` resolves as a side effect of polling the alert
//!     // stream, so poll both concurrently. The completion alert may
//!     // arrive in any batch (even this early), so inspect every batch.
//!     let mut finished = false;
//!     let handle = {
//!         let add = session.add_torrent(&params);
//!         tokio::pin!(add);
//!         loop {
//!             tokio::select! {
//!                 result = &mut add => break result?,
//!                 batch = alerts.next_batch() => {
//!                     for alert in batch?.iter() {
//!                         if matches!(alert, Alert::TorrentFinished(_)) {
//!                             finished = true;
//!                         }
//!                     }
//!                 }
//!             }
//!         }
//!     };
//!
//!     // The completion alert may already have been consumed above, so
//!     // check the status before (and between) waiting for more alerts.
//!     while !finished && !handle.status(0)?.is_finished() {
//!         for alert in alerts.next_batch().await?.iter() {
//!             if matches!(alert, Alert::TorrentFinished(_)) {
//!                 finished = true;
//!             }
//!         }
//!     }
//!     println!("downloaded {} bytes", handle.status(0)?.total_done());
//!
//!     // Release the session borrows, then shut down asynchronously:
//!     // dropping the Session without close() blocks the runtime.
//!     drop(handle);
//!     drop(alerts);
//!     session.close().await;
//!     Ok(())
//! }
//! ```
//!
//! Runnable programs live in the crate's `examples/` directory:
//! `cli_download`, `make_torrent`, and `alert_dump`.
//!
//! # The alert stream contract
//!
//! libtorrent reports everything — events, errors, responses to
//! asynchronous requests — through a single queue of *alerts*, exposed as
//! the [`Alerts`] stream ([`Session::alerts`]). No hidden task pumps it;
//! your code decides when alerts are popped. Two rules follow:
//!
//! 1. **Request futures resolve only while the stream is polled.**
//!    [`Session::add_torrent`] and [`Session::session_stats`] complete as
//!    a side effect of [`Alerts::next_batch`] observing the matching
//!    alert; awaiting them without polling the stream deadlocks. Other
//!    requests (`post_*`, [`TorrentHandle::save_resume_data`],
//!    [`TorrentHandle::read_piece`]) are fire-and-forget: match their
//!    response alerts on the stream yourself, attributing them via
//!    [`RawAlert::torrent_handle`]. All request-response alerts ignore
//!    the `alert_mask` setting, which governs only spontaneous events.
//! 2. **Alert views borrow the batch** and are valid only until the next
//!    pop; copy out whatever must outlive it. Alert-derived
//!    [`TorrentHandle`]s are batch-scoped too (cloning does not extend
//!    that); keep the torrent's id or info-hashes instead and re-derive a
//!    handle with [`Session::find_torrent`].
//!
//! # Blocking
//!
//! Synchronous query methods may block briefly on libtorrent's internal
//! mutex; that is fine in async code. Genuinely blocking: dropping a
//! [`Session`] (prefer [`Session::close`](Session::close)) and
//! [`set_piece_hashes`].
//!
//! # Cargo features
//!
//! - `vendored` — build the pinned libtorrent 2.1 from the `vendor/`
//!   submodule instead of using the system library, and link it statically
//!   into the binary. Requires `git submodule update --init --recursive`.
//!
//! # Errors
//!
//! Fallible calls return [`Result`]`<T>` with an [`Error`] carrying the
//! `boost::system` error code libtorrent produced: a numeric value, its
//! [`Category`], and the rendered message.

pub mod alerts;
pub mod create;
mod error;
pub mod filter;
mod handle;
pub mod info;
pub mod params;
pub mod peers;
pub mod session;
pub mod settings;
pub mod stats;
pub mod status;
mod types;
mod util;

pub use alerts::{Alert, AlertCategory, AlertType, Alerts, Batch, RawAlert};
pub use create::{CreateFlags, CreateTorrent, FileEntry, FileFlags, list_files, set_piece_hashes};
pub use error::{Category, Error, Result};
pub use filter::{IpFilter, PortFilter};
pub use handle::TorrentHandle;
pub use info::{File, FileFlags as InfoFileFlags, FileSlice, TorrentInfo};
pub use params::{
    AddTorrentParams, DownloadPriority, LoadTorrentLimits, StorageMode, TorrentFlags,
};
pub use peers::{ConnectionType, PeerFlags, PeerInfo, PeerSourceFlags};
pub use session::{DiskIo, RemoveFlags, SaveStateFlags, Session, SessionParams};
pub use settings::{
    Credentials, HostPort, I2pConfig, I2pTunnels, ListenEndpoint, ProxyConfig, ProxyProtocol,
    SettingKey, SettingKind, SettingsError, SettingsPack,
};
pub use stats::{MetricKind, StatsMetric, find_metric_index, session_stats_metrics};
pub use status::{State as TorrentState, StorageMode as StatusStorageMode, TorrentStatus};
pub use types::{InfoHash, PeerRequest, PieceBitfield, Sha1Hash, Sha256Hash};

/// Version string of the libtorrent shared library actually loaded at
/// runtime, e.g. `"2.1.0.0"`.
pub fn libtorrent_version() -> &'static str {
    // SAFETY: ct_libtorrent_version returns a view of libtorrent's static
    // version string, which is ASCII and lives for the program duration.
    unsafe {
        let v = libctorrent_sys::ct_libtorrent_version();
        str::from_utf8(std::slice::from_raw_parts(v.ptr.cast(), v.len))
            .expect("libtorrent version string is ASCII")
    }
}

/// `LIBTORRENT_VERSION_NUM` of the headers the bindings were built against,
/// e.g. `20100` for 2.1.0.
pub fn libtorrent_version_num() -> u32 {
    // SAFETY: trivial constant accessor.
    unsafe { libctorrent_sys::ct_libtorrent_version_num() }
}

/// The `TORRENT_ABI_VERSION` the shim and the linked libtorrent were built
/// with (2 = the distro default; 100 = deprecated APIs removed).
pub fn libtorrent_abi_version() -> u32 {
    // SAFETY: trivial constant accessor.
    unsafe { libctorrent_sys::ct_libtorrent_abi_version() }
}
