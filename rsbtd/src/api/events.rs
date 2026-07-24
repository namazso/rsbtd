// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! GraphQL event types - strongly-typed representations of engine events.

use async_graphql::{SimpleObject, Union};

use super::scalars::{Base64Bytes, InfoHash};
use super::types::{Torrent, TorrentState, Tracker};
use crate::engine::events::{Event, EventKind};

/// Union of all possible torrent events.
#[derive(Union)]
pub enum TorrentEvent {
    TorrentAdded(TorrentAddedEvent),
    TorrentRemoved(TorrentRemovedEvent),
    TorrentFinished(TorrentFinishedEvent),
    MetadataReceived(MetadataReceivedEvent),
    MetadataFailed(MetadataFailedEvent),
    TorrentError(TorrentErrorEvent),
    StateChanged(StateChangedEvent),
    StateUpdate(StateUpdateEvent),
    ResumeDataSaved(ResumeDataSavedEvent),
    ResumeDataFailed(ResumeDataFailedEvent),
    FileRenamed(FileRenamedEvent),
    FileRenameFailed(FileRenameFailedEvent),
    StorageMoved(StorageMovedEvent),
    StorageMovedFailed(StorageMovedFailedEvent),
    ReadPiece(ReadPieceEvent),
    Trackers(TrackersEvent),
    Peers(PeersEvent),
    FileProgress(FileProgressEvent),
    ScrapeReply(ScrapeReplyEvent),
    ScrapeFailed(ScrapeFailedEvent),
    TorrentDeleted(TorrentDeletedEvent),
    TorrentDeleteFailed(TorrentDeleteFailedEvent),
    SessionError(SessionErrorEvent),
}

impl TorrentEvent {
    /// Convert an engine event to a GraphQL event.
    pub fn from_engine_event(event: &Event, engine: &crate::engine::Engine) -> Option<Self> {
        match &event.kind {
            EventKind::TorrentAdded => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentAdded(TorrentAddedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::TorrentRemoved => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentRemoved(TorrentRemovedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::TorrentFinished => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentFinished(TorrentFinishedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::MetadataReceived => {
                let torrent = event.torrent?;
                Some(TorrentEvent::MetadataReceived(MetadataReceivedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::MetadataFailed { error } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::MetadataFailed(MetadataFailedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    error: error.as_ref().map(|e| e.to_string()),
                }))
            }
            EventKind::TorrentError { error, filename } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentError(TorrentErrorEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    error: error.as_ref().map(|e| e.to_string()),
                    filename: (!filename.is_empty()).then(|| filename.clone()),
                }))
            }
            EventKind::StateChanged { state, prev_state } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::StateChanged(StateChangedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    state: rbtorrent::TorrentState::from_code(*state).into(),
                    prev_state: rbtorrent::TorrentState::from_code(*prev_state).into(),
                }))
            }
            EventKind::StateUpdate(statuses) => {
                let torrents: Vec<Torrent> = statuses
                    .iter()
                    .filter_map(|status| {
                        let entry = engine.registry().get(status.id())?;
                        Torrent::load(engine, entry).ok()
                    })
                    .collect();
                Some(TorrentEvent::StateUpdate(StateUpdateEvent { torrents }))
            }
            EventKind::ResumeDataSaved => {
                let torrent = event.torrent?;
                Some(TorrentEvent::ResumeDataSaved(ResumeDataSavedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::ResumeDataFailed { message } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::ResumeDataFailed(ResumeDataFailedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    error: Some(message.clone()),
                }))
            }
            EventKind::FileRenamed { index, new_name } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::FileRenamed(FileRenamedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    file_index: *index,
                    new_name: new_name.clone(),
                }))
            }
            EventKind::FileRenameFailed { index, error } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::FileRenameFailed(FileRenameFailedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    file_index: *index,
                    error: error.as_ref().map(|e| e.to_string()),
                }))
            }
            EventKind::StorageMoved { path } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::StorageMoved(StorageMovedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    path: path.clone(),
                }))
            }
            EventKind::StorageMovedFailed { error } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::StorageMovedFailed(StorageMovedFailedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    error: error.as_ref().map(|e| e.to_string()),
                }))
            }
            EventKind::ReadPiece { piece, data, error } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::ReadPiece(ReadPieceEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    piece: *piece,
                    data: error.is_none().then(|| Base64Bytes(data.clone())),
                    error: error.as_ref().map(|e| e.to_string()),
                }))
            }
            EventKind::Trackers(trackers) => {
                let torrent = event.torrent?;
                Some(TorrentEvent::Trackers(TrackersEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    trackers: trackers.iter().cloned().map(Tracker::from).collect(),
                }))
            }
            EventKind::Peers(peers) => {
                let torrent = event.torrent?;
                Some(TorrentEvent::Peers(PeersEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    peer_count: peers.len() as i32,
                }))
            }
            EventKind::FileProgress(progress) => {
                let torrent = event.torrent?;
                Some(TorrentEvent::FileProgress(FileProgressEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    progress: progress.clone(),
                }))
            }
            EventKind::ScrapeReply {
                tracker_url,
                incomplete,
                complete,
            } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::ScrapeReply(ScrapeReplyEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    tracker_url: tracker_url.clone(),
                    incomplete: *incomplete,
                    complete: *complete,
                }))
            }
            EventKind::ScrapeFailed {
                tracker_url,
                error_message,
            } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::ScrapeFailed(ScrapeFailedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                    tracker_url: tracker_url.clone(),
                    error: Some(error_message.clone()),
                }))
            }
            EventKind::TorrentDeleted => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentDeleted(TorrentDeletedEvent {
                    torrent_id: torrent.id,
                    info_hash: InfoHash(torrent.info_hash),
                }))
            }
            EventKind::TorrentDeleteFailed { error } => {
                let torrent = event.torrent?;
                Some(TorrentEvent::TorrentDeleteFailed(
                    TorrentDeleteFailedEvent {
                        torrent_id: torrent.id,
                        info_hash: InfoHash(torrent.info_hash),
                        error: error.as_ref().map(|e| e.to_string()),
                    },
                ))
            }
            EventKind::SessionError { error } => {
                Some(TorrentEvent::SessionError(SessionErrorEvent {
                    error: Some(error.to_string()),
                }))
            }
        }
    }
}

// Every torrent-scoped event carries the session-local `torrentId` and
// the durable `infoHash` (v1 preferred); key persistent client state by
// hash. A nullable `error` means the underlying alert supplied no
// concrete error object.

/// A torrent was added and registered in the session.
#[derive(SimpleObject)]
pub struct TorrentAddedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// A torrent was removed from the session. Disk deletion, if requested,
/// completes separately (`TorrentDeletedEvent`).
#[derive(SimpleObject)]
pub struct TorrentRemovedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// All selected (priority-nonzero) content finished downloading.
#[derive(SimpleObject)]
pub struct TorrentFinishedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// Magnet metadata was received. A hybrid magnet may gain its second
/// info-hash at this point.
#[derive(SimpleObject)]
pub struct MetadataReceivedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// Magnet metadata acquisition failed.
#[derive(SimpleObject)]
pub struct MetadataFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Failure description, when the engine supplied one.
    pub error: Option<String>,
}

/// The torrent entered an error state and stopped.
#[derive(SimpleObject)]
pub struct TorrentErrorEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Error description, when the engine supplied one.
    pub error: Option<String>,
    /// The affected file path; `null` when the error is not
    /// file-related.
    pub filename: Option<String>,
}

/// The torrent's lifecycle state changed.
#[derive(SimpleObject)]
pub struct StateChangedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// The new state.
    pub state: TorrentState,
    /// The previous state.
    pub prev_state: TorrentState,
}

/// A batch of status snapshots of torrents that changed recently.
/// Produced about once per second while an unfiltered `torrentEvents`
/// or a `torrentChanged` subscription is active.
#[derive(SimpleObject)]
pub struct StateUpdateEvent {
    /// The changed torrents.
    pub torrents: Vec<Torrent>,
}

/// Resume data was generated and persisted.
#[derive(SimpleObject)]
pub struct ResumeDataSavedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// Generating or persisting resume data failed.
#[derive(SimpleObject)]
pub struct ResumeDataFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Failure description. Nullable only so that `error` merges across
    /// event types in one selection; this event always carries one.
    pub error: Option<String>,
}

/// A file was renamed (the outcome of `renameFile`).
#[derive(SimpleObject)]
pub struct FileRenamedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Zero-based index of the renamed file.
    pub file_index: i32,
    /// The accepted new path, relative to the save path.
    pub new_name: String,
}

/// A file rename failed.
#[derive(SimpleObject)]
pub struct FileRenameFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Zero-based index of the file that failed to rename.
    pub file_index: i32,
    /// Failure description, when the engine supplied one.
    pub error: Option<String>,
}

/// The torrent's storage finished moving (the outcome of
/// `moveStorage`).
#[derive(SimpleObject)]
pub struct StorageMovedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// The new save path.
    pub path: String,
}

/// Moving the torrent's storage failed.
#[derive(SimpleObject)]
pub struct StorageMovedFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Failure description, when the engine supplied one.
    pub error: Option<String>,
}

/// A piece read completed (the outcome of `readPiece`).
#[derive(SimpleObject)]
pub struct ReadPieceEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Zero-based piece index.
    pub piece: i32,
    /// The exact piece bytes (the final piece may be shorter than
    /// `pieceLength`); `null` when the read failed.
    pub data: Option<Base64Bytes>,
    /// Failure description, when the read failed.
    pub error: Option<String>,
}

/// A tracker-list snapshot. There is no operation to request one
/// directly; it is broadcast whenever serving any client's request
/// fetches the tracker list (a `Torrent.trackers` selection, or a
/// mutation that resolves tracker indexes).
#[derive(SimpleObject)]
pub struct TrackersEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// The torrent's trackers, in current tracker order.
    pub trackers: Vec<Tracker>,
}

/// A peer-list snapshot; only the count is exposed. There is no
/// operation to request one directly; it is broadcast whenever serving
/// any client's request fetches the peer list (a `Torrent.peers`
/// selection).
#[derive(SimpleObject)]
pub struct PeersEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Number of connected peers in the snapshot.
    pub peer_count: i32,
}

/// A file-progress snapshot. There is no operation to request one
/// directly; it is broadcast whenever serving any client's request
/// fetches per-file progress (a `Torrent.files` selection).
#[derive(SimpleObject)]
pub struct FileProgressEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Downloaded bytes per file, indexed by file index.
    pub progress: Vec<i64>,
}

/// A tracker answered a scrape (the outcome of `scrapeTracker`).
#[derive(SimpleObject)]
pub struct ScrapeReplyEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// The URL of the tracker that answered, when known.
    pub tracker_url: Option<String>,
    /// Downloaders in the swarm.
    pub incomplete: i32,
    /// Seeds in the swarm.
    pub complete: i32,
}

/// A tracker scrape failed.
#[derive(SimpleObject)]
pub struct ScrapeFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// The URL of the tracker that failed, when known.
    pub tracker_url: Option<String>,
    /// Failure description. Nullable only so that `error` merges across
    /// event types in one selection; this event always carries one.
    pub error: Option<String>,
}

/// Payload/partfile deletion after `removeTorrent(deleteFiles: true)`
/// completed.
#[derive(SimpleObject)]
pub struct TorrentDeletedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
}

/// Payload/partfile deletion after `removeTorrent(deleteFiles: true)`
/// failed.
#[derive(SimpleObject)]
pub struct TorrentDeleteFailedEvent {
    /// Session-local torrent id.
    pub torrent_id: u32,
    /// The torrent's info-hash (v1 preferred).
    pub info_hash: InfoHash,
    /// Failure description, when the engine supplied one.
    pub error: Option<String>,
}

/// A fatal session-level error.
#[derive(SimpleObject)]
pub struct SessionErrorEvent {
    /// Error description. Nullable only to stay merge-compatible with the
    /// `error` field of the other event types; it is always set.
    pub error: Option<String>,
}
