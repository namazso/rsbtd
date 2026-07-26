// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Owned engine events, fanned out from the alert pump.
//!
//! The **event bus** ([`Event`]): structured, owned translations of the
//! alerts the daemon acts on, awaited by the correlator (request/response
//! semantics for fire-and-forget calls) and by subscription change feeds.

use rbtorrent::{TorrentStatus, peers::PeerInfo};
use uuid::Uuid;

/// A structured engine event (owned; shared as `Arc<Event>`).
#[derive(Debug)]
pub struct Event {
    /// The durable identity of the torrent this event belongs to; `None`
    /// for session-scoped events and for alerts that cannot be attributed
    /// to a torrent.
    pub torrent: Option<Uuid>,
    pub kind: EventKind,
}

/// One tracker entry, copied out of a `TrackerList` alert.
#[derive(Clone, Debug)]
pub struct TrackerInfo {
    pub url: String,
    pub trackerid: String,
    pub tier: i32,
    pub fail_limit: i32,
    pub source: u32,
    pub verified: bool,
}

/// The payload of an [`Event`].
#[derive(Debug)]
pub enum EventKind {
    /// A torrent was added to the session (registry entry exists).
    TorrentAdded,
    /// A torrent was removed from the session (registry entry gone).
    TorrentRemoved,
    /// A torrent finished downloading all selected files.
    TorrentFinished,
    /// Metadata for a magnet-added torrent arrived.
    MetadataReceived,
    MetadataFailed {
        error: Option<rbtorrent::Error>,
    },
    /// The torrent entered an error state.
    TorrentError {
        error: Option<rbtorrent::Error>,
        filename: String,
    },
    StateChanged {
        state: i32,
        prev_state: i32,
    },
    /// Status snapshots of torrents that changed since the last
    /// `post_torrent_updates`. The snapshots are handle-less owned
    /// copies; key registry lookups by [`TorrentStatus::id`].
    StateUpdate(Vec<TorrentStatus>),
    /// Resume data was generated and persisted to the state directory.
    ResumeDataSaved,
    /// Resume data generation or persistence failed.
    ResumeDataFailed {
        message: String,
    },
    FileRenamed {
        index: i32,
        new_name: String,
    },
    FileRenameFailed {
        index: i32,
        error: Option<rbtorrent::Error>,
    },
    StorageMoved {
        path: String,
    },
    StorageMovedFailed {
        error: Option<rbtorrent::Error>,
    },
    /// Response to `read_piece`.
    ReadPiece {
        piece: i32,
        data: Vec<u8>,
        error: Option<rbtorrent::Error>,
    },
    /// Response to `post_trackers`.
    Trackers(Vec<TrackerInfo>),
    /// Response to `post_peer_info`.
    Peers(Vec<PeerInfo>),
    /// Response to `post_file_progress` (bytes done, indexed by file).
    FileProgress(Vec<i64>),
    /// A tracker scrape succeeded.
    ScrapeReply {
        tracker_url: Option<String>,
        incomplete: i32,
        complete: i32,
    },
    ScrapeFailed {
        tracker_url: Option<String>,
        error_message: String,
    },
    /// The torrent's files were deleted from disk.
    TorrentDeleted,
    TorrentDeleteFailed {
        error: Option<rbtorrent::Error>,
    },
    /// A fatal session error.
    SessionError {
        error: rbtorrent::Error,
    },
}
