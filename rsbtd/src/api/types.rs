// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! GraphQL object types over engine and rbtorrent state.

use std::sync::Arc;

use async_graphql::{ComplexObject, Context, Enum, Object, SimpleObject};
use rbtorrent::{TorrentFlags, TorrentHandle, TorrentState as LtState, TorrentStatus};

use super::scalars::{Base64Bytes, Sha1Sum, Sha256Sum};
use crate::engine::events::TrackerInfo;
use crate::engine::registry::TorrentEntry;
use crate::engine::{Engine, EngineError, PeerSnapshot};

/// libtorrent stores "no per-torrent limit" as 0 and treats i32::MAX
/// identically; the API's only sentinel is -1.
fn normalize_rate_limit(raw: i32) -> i32 {
    if raw == 0 || raw == i32::MAX { -1 } else { raw }
}

/// Status query flags for list/detail views: all the cheap extras. Piece
/// bitfields are fetched separately by the `pieces` field.
const STATUS_FLAGS: u32 = TorrentHandle::QUERY_NAME
    | TorrentHandle::QUERY_SAVE_PATH
    | TorrentHandle::QUERY_DISTRIBUTED_COPIES
    | TorrentHandle::QUERY_ACCURATE_DOWNLOAD_COUNTERS
    | TorrentHandle::QUERY_LAST_SEEN_COMPLETE;

/// Torrent lifecycle state: the torrent's current primary task.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TorrentState {
    /// Checking existing files against the piece hashes.
    CheckingFiles,
    /// Downloading metadata (a magnet without its .torrent yet).
    DownloadingMetadata,
    /// Downloading payload data.
    Downloading,
    /// All selected (priority-nonzero) files are complete, but not the
    /// whole torrent.
    Finished,
    /// Every piece is present; seeding.
    Seeding,
    /// Validating resume data against the files on disk.
    CheckingResumeData,
    /// A state this API version does not recognize.
    Unknown,
}

impl From<LtState> for TorrentState {
    fn from(state: LtState) -> Self {
        match state {
            LtState::CheckingFiles => TorrentState::CheckingFiles,
            LtState::DownloadingMetadata => TorrentState::DownloadingMetadata,
            LtState::Downloading => TorrentState::Downloading,
            LtState::Finished => TorrentState::Finished,
            LtState::Seeding => TorrentState::Seeding,
            LtState::CheckingResumeData => TorrentState::CheckingResumeData,
            _ => TorrentState::Unknown,
        }
    }
}

/// Storage allocation mode.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StorageMode {
    /// Full file allocation up front.
    Allocate,
    /// Sparse files, allocated as pieces arrive.
    Sparse,
    /// A mode this API version does not recognize.
    Unknown,
}

impl From<rbtorrent::StatusStorageMode> for StorageMode {
    fn from(mode: rbtorrent::StatusStorageMode) -> Self {
        match mode {
            rbtorrent::StatusStorageMode::Allocate => StorageMode::Allocate,
            rbtorrent::StatusStorageMode::Sparse => StorageMode::Sparse,
            _ => StorageMode::Unknown,
        }
    }
}

/// A torrent option flag. Read via `Torrent.flags`, changed via
/// `setTorrentFlags` (some flags are only meaningful at add time or are
/// managed by the engine, as noted per value).
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TorrentFlag {
    /// Trust that all files are complete and verify pieces lazily, on
    /// first request. Only meaningful when adding a torrent with
    /// metadata; a failed hash check leaves seed mode and triggers a
    /// recheck.
    SeedMode,
    /// Make no piece requests, only upload. Auto-management may clear
    /// it (e.g. after a disk error is resolved).
    UploadMode,
    /// Download only pieces that improve the share ratio instead of the
    /// whole content. Do not combine with manual file/piece priorities.
    ShareMode,
    /// Apply the session IP filter to this torrent. Set by default.
    ApplyIpFilter,
    /// The torrent's own pause flag: no announces, no connections.
    /// Auto-management can resume a paused torrent; prefer the
    /// `pauseTorrent`/`resumeTorrent` mutations.
    Paused,
    /// Let the queue logic start, pause, and rotate this torrent
    /// automatically.
    AutoManaged,
    /// Treat adding a duplicate torrent as an error. The daemon forces
    /// this flag on every add; it has no effect after adding.
    DuplicateIsError,
    /// Include the torrent in changed-status batches (`torrentChanged`
    /// updates and `StateUpdateEvent` batches). Only effective at add
    /// time: libtorrent does not honor runtime changes, so
    /// `setTorrentFlags` rejects them.
    UpdateSubscribe,
    /// Super-seeding (initial seeding) mode; effective only while the
    /// torrent is a seed.
    SuperSeeding,
    /// Prefer downloading pieces in index order. Not a streaming
    /// guarantee; piece deadlines serve streaming better.
    SequentialDownload,
    /// One-shot: when the torrent is ready to transfer data, clear
    /// auto-management, pause it, and clear this flag. Useful for
    /// fetching metadata or checking files without starting a transfer.
    StopWhenReady,
    /// Ephemeral marker that resume data is outdated; normally managed
    /// and cleared by the engine's resume persistence.
    NeedSaveResume,
    /// Do not announce this torrent to the DHT.
    DisableDht,
    /// Do not announce this torrent via Local Service Discovery.
    DisableLsd,
    /// Do not exchange peers for this torrent via PEX.
    DisablePex,
    /// Trust resume data unconditionally, without checking the files on
    /// disk. Unsafe when the resume data or files may be stale.
    NoVerifyFiles,
    /// Files without an explicit priority default to priority 0 (don't
    /// download); useful for magnets, whose file list is unknown at add
    /// time. Only meaningful at add time.
    DefaultDontDownload,
    /// Treat this as an I2P torrent for the mixed-peer policy; normally
    /// inferred from `.i2p` tracker URLs.
    #[graphql(name = "I2P_TORRENT")]
    I2pTorrent,
    /// For hybrid torrents, validate only v2 hashes. Only meaningful at
    /// add time; no effect on v1-only or v2-only torrents.
    DisableV1Hashes,
}

/// A peer connection state or capability flag. Flags this API version
/// does not recognize are omitted.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerFlag {
    /// We want pieces this peer has.
    Interesting,
    /// We are choking this peer (not uploading to it).
    Choked,
    /// The peer wants pieces we have.
    RemoteInterested,
    /// The peer is choking us (not uploading to us).
    RemoteChoked,
    /// The peer supports the extension protocol.
    SupportsExtensions,
    /// We initiated this connection (as opposed to an incoming one).
    OutgoingConnection,
    /// Still handshaking; the peer's capabilities are not known yet.
    Handshake,
    /// The connection is still being established.
    Connecting,
    /// The peer participated in a piece that failed its hash check and
    /// only whole pieces are requested from it until it clears parole.
    OnParole,
    /// The peer has every piece.
    Seed,
    /// The peer holds the optimistic unchoke slot.
    OptimisticUnchoke,
    /// The peer has stopped sending us requested blocks (snubbed).
    Snubbed,
    /// The peer announced it will only upload (e.g. a partial seed).
    UploadOnly,
    /// End-game mode: outstanding blocks may be requested from this
    /// peer redundantly.
    EndgameMode,
    /// The connection was established through NAT hole punching.
    Holepunched,
    /// The connection runs over an I2P socket.
    #[graphql(name = "I2P_SOCKET")]
    I2pSocket,
    /// The connection runs over uTP.
    UtpSocket,
    /// The connection runs over SSL/TLS.
    SslSocket,
    /// The connection is encrypted with the RC4 method (full-stream
    /// encryption).
    #[graphql(name = "RC4_ENCRYPTED")]
    Rc4Encrypted,
    /// The connection used an obfuscated handshake and carries the
    /// payload in plaintext.
    PlaintextEncrypted,
}

/// Where we discovered a peer.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerSource {
    /// From a tracker announce response.
    Tracker,
    /// From the DHT.
    Dht,
    /// From peer exchange (PEX) with another peer.
    Pex,
    /// From Local Service Discovery on the local network.
    Lsd,
    /// Restored from resume data.
    ResumeData,
    /// The peer connected to us.
    Incoming,
}

/// Where a tracker URL came from.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackerSource {
    /// Part of the .torrent file.
    Torrent,
    /// Added programmatically.
    Client,
    /// Part of a magnet link.
    MagnetLink,
    /// Received via tracker exchange (TEX).
    Tex,
}

/// Bit-to-enum mapping for torrent flags.
pub const FLAG_TABLE: &[(u64, TorrentFlag)] = &[
    (
        rbtorrent::TorrentFlags::SEED_MODE.bits(),
        TorrentFlag::SeedMode,
    ),
    (
        rbtorrent::TorrentFlags::UPLOAD_MODE.bits(),
        TorrentFlag::UploadMode,
    ),
    (
        rbtorrent::TorrentFlags::SHARE_MODE.bits(),
        TorrentFlag::ShareMode,
    ),
    (
        rbtorrent::TorrentFlags::APPLY_IP_FILTER.bits(),
        TorrentFlag::ApplyIpFilter,
    ),
    (rbtorrent::TorrentFlags::PAUSED.bits(), TorrentFlag::Paused),
    (
        rbtorrent::TorrentFlags::AUTO_MANAGED.bits(),
        TorrentFlag::AutoManaged,
    ),
    (
        rbtorrent::TorrentFlags::DUPLICATE_IS_ERROR.bits(),
        TorrentFlag::DuplicateIsError,
    ),
    (
        rbtorrent::TorrentFlags::UPDATE_SUBSCRIBE.bits(),
        TorrentFlag::UpdateSubscribe,
    ),
    (
        rbtorrent::TorrentFlags::SUPER_SEEDING.bits(),
        TorrentFlag::SuperSeeding,
    ),
    (
        rbtorrent::TorrentFlags::SEQUENTIAL_DOWNLOAD.bits(),
        TorrentFlag::SequentialDownload,
    ),
    (
        rbtorrent::TorrentFlags::STOP_WHEN_READY.bits(),
        TorrentFlag::StopWhenReady,
    ),
    (
        rbtorrent::TorrentFlags::NEED_SAVE_RESUME.bits(),
        TorrentFlag::NeedSaveResume,
    ),
    (
        rbtorrent::TorrentFlags::DISABLE_DHT.bits(),
        TorrentFlag::DisableDht,
    ),
    (
        rbtorrent::TorrentFlags::DISABLE_LSD.bits(),
        TorrentFlag::DisableLsd,
    ),
    (
        rbtorrent::TorrentFlags::DISABLE_PEX.bits(),
        TorrentFlag::DisablePex,
    ),
    (
        rbtorrent::TorrentFlags::NO_VERIFY_FILES.bits(),
        TorrentFlag::NoVerifyFiles,
    ),
    (
        rbtorrent::TorrentFlags::DEFAULT_DONT_DOWNLOAD.bits(),
        TorrentFlag::DefaultDontDownload,
    ),
    (
        rbtorrent::TorrentFlags::I2P_TORRENT.bits(),
        TorrentFlag::I2pTorrent,
    ),
    (
        rbtorrent::TorrentFlags::DISABLE_V1_HASHES.bits(),
        TorrentFlag::DisableV1Hashes,
    ),
];

/// Bit-to-enum mapping for peer flags.
const PEER_FLAG_TABLE: &[(u32, PeerFlag)] = &[
    (
        rbtorrent::PeerFlags::INTERESTING.bits(),
        PeerFlag::Interesting,
    ),
    (rbtorrent::PeerFlags::CHOKED.bits(), PeerFlag::Choked),
    (
        rbtorrent::PeerFlags::REMOTE_INTERESTED.bits(),
        PeerFlag::RemoteInterested,
    ),
    (
        rbtorrent::PeerFlags::REMOTE_CHOKED.bits(),
        PeerFlag::RemoteChoked,
    ),
    (
        rbtorrent::PeerFlags::SUPPORTS_EXTENSIONS.bits(),
        PeerFlag::SupportsExtensions,
    ),
    (
        rbtorrent::PeerFlags::OUTGOING_CONNECTION.bits(),
        PeerFlag::OutgoingConnection,
    ),
    (rbtorrent::PeerFlags::HANDSHAKE.bits(), PeerFlag::Handshake),
    (
        rbtorrent::PeerFlags::CONNECTING.bits(),
        PeerFlag::Connecting,
    ),
    (rbtorrent::PeerFlags::ON_PAROLE.bits(), PeerFlag::OnParole),
    (rbtorrent::PeerFlags::SEED.bits(), PeerFlag::Seed),
    (
        rbtorrent::PeerFlags::OPTIMISTIC_UNCHOKE.bits(),
        PeerFlag::OptimisticUnchoke,
    ),
    (rbtorrent::PeerFlags::SNUBBED.bits(), PeerFlag::Snubbed),
    (
        rbtorrent::PeerFlags::UPLOAD_ONLY.bits(),
        PeerFlag::UploadOnly,
    ),
    (
        rbtorrent::PeerFlags::ENDGAME_MODE.bits(),
        PeerFlag::EndgameMode,
    ),
    (
        rbtorrent::PeerFlags::HOLEPUNCHED.bits(),
        PeerFlag::Holepunched,
    ),
    (rbtorrent::PeerFlags::I2P_SOCKET.bits(), PeerFlag::I2pSocket),
    (rbtorrent::PeerFlags::UTP_SOCKET.bits(), PeerFlag::UtpSocket),
    (rbtorrent::PeerFlags::SSL_SOCKET.bits(), PeerFlag::SslSocket),
    (
        rbtorrent::PeerFlags::RC4_ENCRYPTED.bits(),
        PeerFlag::Rc4Encrypted,
    ),
    (
        rbtorrent::PeerFlags::PLAINTEXT_ENCRYPTED.bits(),
        PeerFlag::PlaintextEncrypted,
    ),
];

/// Bit-to-enum mapping for peer source flags.
const PEER_SOURCE_TABLE: &[(u8, PeerSource)] = &[
    (
        rbtorrent::PeerSourceFlags::TRACKER.bits(),
        PeerSource::Tracker,
    ),
    (rbtorrent::PeerSourceFlags::DHT.bits(), PeerSource::Dht),
    (rbtorrent::PeerSourceFlags::PEX.bits(), PeerSource::Pex),
    (rbtorrent::PeerSourceFlags::LSD.bits(), PeerSource::Lsd),
    (
        rbtorrent::PeerSourceFlags::RESUME_DATA.bits(),
        PeerSource::ResumeData,
    ),
    (
        rbtorrent::PeerSourceFlags::INCOMING.bits(),
        PeerSource::Incoming,
    ),
];

/// Bit-to-enum mapping for tracker source flags (libtorrent announce_entry::tracker_source).
const TRACKER_SOURCE_TABLE: &[(u32, TrackerSource)] = &[
    (1, TrackerSource::Torrent),    // source_torrent
    (2, TrackerSource::Client),     // source_client
    (4, TrackerSource::MagnetLink), // source_magnet_link
    (8, TrackerSource::Tex),        // source_tex
];

/// Expands a flag bitmask into the recognized flag enums.
pub fn flags_to_list(bits: u64) -> Vec<TorrentFlag> {
    FLAG_TABLE
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|&(_, flag)| flag)
        .collect()
}

/// The bitmask of one flag enum.
pub fn flag_bits(flag: TorrentFlag) -> u64 {
    FLAG_TABLE
        .iter()
        .find(|&&(_, f)| f == flag)
        .map(|&(bits, _)| bits)
        .unwrap_or(0)
}

/// Collapses a flag enum list into a bitmask.
pub fn flags_to_bits(flags: &[TorrentFlag]) -> u64 {
    flags.iter().fold(0, |bits, &f| bits | flag_bits(f))
}

/// Expands peer flags into enum list.
fn peer_flags_to_list(bits: u32) -> Vec<PeerFlag> {
    PEER_FLAG_TABLE
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|&(_, flag)| flag)
        .collect()
}

/// Expands peer source flags into enum list.
fn peer_source_to_list(bits: u8) -> Vec<PeerSource> {
    PEER_SOURCE_TABLE
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|&(_, source)| source)
        .collect()
}

/// Expands tracker source flags into enum list.
fn tracker_source_to_list(bits: u32) -> Vec<TrackerSource> {
    TRACKER_SOURCE_TABLE
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|&(_, source)| source)
        .collect()
}

/// A torrent-level error (the torrent is stopped until it is cleared).
#[derive(SimpleObject)]
pub struct TorrentErrorInfo {
    /// Human-readable error message.
    pub message: String,
    /// Index of the file the error relates to, or a negative sentinel:
    /// -1 none/not file-specific, -3 SSL context, -4 metadata,
    /// -5 internal exception, -6 partfile.
    pub file: i32,
}

/// Piece availability.
#[derive(SimpleObject)]
pub struct PieceInfo {
    /// Total number of pieces; `null` until metadata is available.
    pub total: Option<i32>,
    /// Number of pieces downloaded.
    pub have: i32,
    /// Packed have-bitfield (bit 7 of byte 0 is piece 0, descending
    /// bits; unused trailing bits are zero); only present when requested
    /// via `includeBitfield`.
    pub bitfield: Option<Base64Bytes>,
}

/// One file within a torrent, in metadata (index) order.
#[derive(SimpleObject)]
pub struct TorrentFile {
    /// Zero-based file index, as used by `renameFile` and the priority
    /// mutations.
    pub index: i32,
    /// Path relative to the save path.
    pub path: String,
    /// File size in bytes.
    pub size: i64,
    /// Byte offset within the torrent.
    pub offset: i64,
    /// Download priority (0 = don't download, 1..=7, default 4). A
    /// priority-0 file can still gain partial data from boundary pieces
    /// it shares with wanted files.
    pub priority: i32,
    /// Bytes of this file downloaded so far.
    pub progress_bytes: i64,
    /// Whether this is a pad file: alignment filler defined by the
    /// metadata, not ordinary payload.
    pub is_pad_file: bool,
    /// Whether the file is stored as a symlink (BEP 47).
    pub is_symlink: bool,
    /// The symlink's target path, when `isSymlink` is set.
    pub symlink_target: Option<String>,
    /// Whether the file has the executable attribute.
    pub is_executable: bool,
    /// Whether the file has the hidden attribute.
    pub is_hidden: bool,
}

/// One tracker of a torrent.
pub struct Tracker {
    pub url: String,
    pub trackerid: String,
    pub tier: i32,
    pub fail_limit: i32,
    pub verified: bool,
    source_raw: u32,
}

/// One tracker of a torrent.
#[Object]
impl Tracker {
    /// The announce URL.
    async fn url(&self) -> &str {
        &self.url
    }

    /// The tracker-issued tracker id, if it sent one (often empty).
    async fn tracker_id(&self) -> &str {
        &self.trackerid
    }

    /// Failover tier: lower tiers are tried first (unless the
    /// `announceToAllTiers`/`announceToAllTrackers` settings widen
    /// announcing).
    async fn tier(&self) -> i32 {
        self.tier
    }

    /// Announce failures after which this tracker is no longer tried;
    /// 0 means never give up.
    async fn fail_limit(&self) -> i32 {
        self.fail_limit
    }

    /// Whether this tracker has answered an announce.
    async fn verified(&self) -> bool {
        self.verified
    }

    /// Where this tracker URL came from.
    async fn source(&self) -> Vec<TrackerSource> {
        tracker_source_to_list(self.source_raw)
    }
}

impl From<TrackerInfo> for Tracker {
    fn from(t: TrackerInfo) -> Self {
        Tracker {
            url: t.url,
            trackerid: t.trackerid,
            tier: t.tier,
            fail_limit: t.fail_limit,
            source_raw: t.source,
            verified: t.verified,
        }
    }
}

/// How a peer is connected.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerConnectionType {
    /// A regular BitTorrent peer connection.
    Bittorrent,
    /// A web seed (BEP 19, `url-list`).
    WebSeed,
    /// An HTTP seed (BEP 17, `httpseeds`).
    HttpSeed,
}

/// One peer connection.
pub struct Peer {
    pub address: Option<String>,
    /// Our endpoint of the connection (IP:port).
    pub local_endpoint: Option<String>,
    /// The peer's 20-byte peer ID, base64-encoded.
    pub peer_id: Base64Bytes,
    pub client: String,
    pub connection_type: PeerConnectionType,
    flags_raw: u32,
    source_raw: u8,
    pub progress_ppm: i32,
    pub down_speed: i32,
    pub up_speed: i32,
    pub payload_down_speed: i32,
    pub payload_up_speed: i32,
    pub total_download: i64,
    pub total_upload: i64,
    /// Microseconds since the last request was sent to this peer.
    pub last_request_us: i64,
    /// Microseconds since the last activity on this connection.
    pub last_active_us: i64,
    /// Number of pieces from this peer that failed the hash check.
    pub num_hashfails: i32,
    /// Number of failed connection attempts to this peer.
    pub failcount: i32,
    /// Highest download rate seen, bytes/s.
    pub download_rate_peak: i32,
    /// Highest upload rate seen, bytes/s.
    pub upload_rate_peak: i32,
    pub num_pieces: i32,
    /// Estimated round-trip time in milliseconds.
    pub rtt: i32,
}

/// One peer connection of a torrent.
#[Object]
impl Peer {
    /// The peer's remote `IP:port` (IPv6 bracketed); `null` for I2P
    /// peers.
    async fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// Our local `IP:port` of this connection (IPv6 bracketed); `null`
    /// for I2P peers.
    async fn local_endpoint(&self) -> Option<&str> {
        self.local_endpoint.as_deref()
    }

    /// The peer's 20-byte BitTorrent peer ID, base64-encoded.
    async fn peer_id(&self) -> &Base64Bytes {
        &self.peer_id
    }

    /// Best-effort client name/version derived from the peer ID or the
    /// extension handshake.
    async fn client(&self) -> &str {
        &self.client
    }

    /// How this peer is connected.
    async fn connection_type(&self) -> PeerConnectionType {
        self.connection_type
    }

    /// Peer connection state flags.
    async fn flags(&self) -> Vec<PeerFlag> {
        peer_flags_to_list(self.flags_raw)
    }

    /// Where we discovered this peer (all sources it was seen from).
    async fn source(&self) -> Vec<PeerSource> {
        peer_source_to_list(self.source_raw)
    }

    /// The peer's completion of the torrent, in parts per million
    /// (0..=1,000,000).
    async fn progress_ppm(&self) -> i32 {
        self.progress_ppm
    }

    /// Current download rate from this peer including protocol
    /// overhead, bytes/s.
    async fn down_speed(&self) -> i32 {
        self.down_speed
    }

    /// Current upload rate to this peer including protocol overhead,
    /// bytes/s.
    async fn up_speed(&self) -> i32 {
        self.up_speed
    }

    /// Current payload-only download rate from this peer, bytes/s.
    async fn payload_down_speed(&self) -> i32 {
        self.payload_down_speed
    }

    /// Current payload-only upload rate to this peer, bytes/s.
    async fn payload_up_speed(&self) -> i32 {
        self.payload_up_speed
    }

    /// Payload bytes received from this peer on this connection.
    async fn total_download(&self) -> i64 {
        self.total_download
    }

    /// Payload bytes sent to this peer on this connection.
    async fn total_upload(&self) -> i64 {
        self.total_upload
    }

    /// Microseconds since the last request was sent to this peer.
    async fn last_request_us(&self) -> i64 {
        self.last_request_us
    }

    /// Microseconds since the last transfer activity on this
    /// connection.
    async fn last_active_us(&self) -> i64 {
        self.last_active_us
    }

    /// Number of pieces involving this peer that failed the hash check.
    async fn num_hashfails(&self) -> i32 {
        self.num_hashfails
    }

    /// Number of failed connection attempts to this peer; rediscovery
    /// from a peer source can decrease it.
    async fn failcount(&self) -> i32 {
        self.failcount
    }

    /// Highest download rate seen on this connection, bytes/s.
    async fn download_rate_peak(&self) -> i32 {
        self.download_rate_peak
    }

    /// Highest upload rate seen on this connection, bytes/s.
    async fn upload_rate_peak(&self) -> i32 {
        self.upload_rate_peak
    }

    /// Number of pieces the peer has.
    async fn num_pieces(&self) -> i32 {
        self.num_pieces
    }

    /// Estimated connect-time round-trip time in milliseconds; may be 0
    /// for incoming connections.
    async fn rtt(&self) -> i32 {
        self.rtt
    }
}

impl From<PeerSnapshot> for Peer {
    fn from(p: PeerSnapshot) -> Self {
        let connection_type = if p
            .connection_type
            .contains(rbtorrent::ConnectionType::WEB_SEED)
        {
            PeerConnectionType::WebSeed
        } else if p
            .connection_type
            .contains(rbtorrent::ConnectionType::HTTP_SEED)
        {
            PeerConnectionType::HttpSeed
        } else {
            PeerConnectionType::Bittorrent
        };
        Peer {
            address: p.address.map(|a| a.to_string()),
            local_endpoint: p.local_endpoint.map(|a| a.to_string()),
            peer_id: Base64Bytes(p.peer_id.to_vec()),
            client: p.client,
            connection_type,
            flags_raw: p.flags.bits(),
            source_raw: p.source.bits(),
            progress_ppm: p.progress_ppm,
            down_speed: p.down_speed,
            up_speed: p.up_speed,
            payload_down_speed: p.payload_down_speed,
            payload_up_speed: p.payload_up_speed,
            total_download: p.total_download,
            total_upload: p.total_upload,
            last_request_us: p.last_request_us,
            last_active_us: p.last_active_us,
            num_hashfails: p.num_hashfails,
            failcount: p.failcount,
            download_rate_peak: p.download_rate_peak,
            upload_rate_peak: p.upload_rate_peak,
            num_pieces: p.num_pieces,
            rtt: p.rtt,
        }
    }
}

/// Session-level state.
#[derive(SimpleObject)]
pub struct SessionInfo {
    /// Whether the whole session is paused (`pauseSession`). Independent
    /// of each torrent's own pause flag.
    pub is_paused: bool,
    /// Whether at least one listen socket is open.
    pub is_listening: bool,
    /// Whether the DHT node is running.
    pub is_dht_running: bool,
    /// Port the session listens on for peer connections (0 if none).
    pub listen_port: i32,
    /// Port the session listens on for SSL torrent connections (0 if none).
    pub ssl_listen_port: i32,
    /// Number of torrents in the session.
    pub torrent_count: i64,
}

/// Component versions.
#[derive(SimpleObject)]
pub struct VersionInfo {
    /// The rsbtd daemon version.
    pub daemon: String,
    /// The version of the embedded BitTorrent engine.
    pub libtorrent: String,
}

/// Whether a metric only ever increases or fluctuates.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatKind {
    /// The value only ever increases.
    Counter,
    /// The value can rise and fall.
    Gauge,
}

/// One session-statistics metric sample.
#[derive(SimpleObject)]
pub struct StatValue {
    /// Metric name, e.g. `net.recv_payload_bytes`. The set of names
    /// depends on the daemon build; discover it by querying
    /// `sessionStats` without a `names` filter.
    pub name: String,
    /// Whether the metric is a counter or a gauge.
    pub kind: StatKind,
    /// The current value.
    pub value: i64,
}

/// One IP filter rule.
#[derive(SimpleObject)]
pub struct IpFilterRule {
    /// First address of the range (inclusive).
    pub first: String,
    /// Last address of the range (inclusive).
    pub last: String,
    /// Whether addresses in the range are blocked (true) or explicitly
    /// allowed (false).
    pub blocked: bool,
}

/// Conflict handling for `moveStorage`.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveMode {
    /// Replace whatever is at the target (the default).
    AlwaysReplaceFiles,
    /// Abort the move if any target file exists.
    FailIfExist,
    /// Keep existing target files, moving only the missing ones.
    DontReplace,
    /// Only change the save path; no files are moved. The files found
    /// at the new path are then re-checked.
    ResetSavePath,
    /// Only change the save path; do not move or check files.
    ResetSavePathUnchecked,
}

impl MoveMode {
    pub fn bits(self) -> u32 {
        match self {
            MoveMode::AlwaysReplaceFiles => TorrentHandle::MOVE_ALWAYS_REPLACE_FILES,
            MoveMode::FailIfExist => TorrentHandle::MOVE_FAIL_IF_EXIST,
            MoveMode::DontReplace => TorrentHandle::MOVE_DONT_REPLACE,
            MoveMode::ResetSavePath => TorrentHandle::MOVE_RESET_SAVE_PATH,
            MoveMode::ResetSavePathUnchecked => TorrentHandle::MOVE_RESET_SAVE_PATH_UNCHECKED,
        }
    }
}

/// The response to a tracker scrape.
#[derive(SimpleObject)]
pub struct ScrapeResult {
    /// The URL of the tracker that answered, when known.
    pub tracker_url: Option<String>,
    /// Seeds in the swarm.
    pub complete: i32,
    /// Downloaders in the swarm.
    pub incomplete: i32,
}

/// One IP filter rule for `setIpFilter`.
#[derive(async_graphql::InputObject)]
pub struct IpFilterRuleInput {
    /// First address of the range (inclusive).
    pub first: String,
    /// Last address of the range (inclusive; same family as `first`).
    pub last: String,
    /// Whether to block addresses in the range (true) or explicitly
    /// allow them (false). Later rules win on overlap.
    pub blocked: bool,
}

/// Input for `addTorrent`. Provide exactly one source: `magnetUri` or
/// `torrentData`.
#[derive(async_graphql::InputObject)]
pub struct AddTorrentInput {
    /// A `magnet:?...` link.
    pub magnet_uri: Option<String>,
    /// A .torrent file, base64-encoded.
    pub torrent_data: Option<Base64Bytes>,
    /// Directory to store the torrent in.
    pub save_path: String,
    /// Display name used until metadata arrives (magnets only).
    pub name: Option<String>,
    /// Add paused (also detaches the torrent from auto-management, so it
    /// stays paused).
    pub paused: Option<bool>,
    /// Explicitly set (true) or clear (false) the `SEQUENTIAL_DOWNLOAD`
    /// flag; omitted leaves the default.
    pub sequential_download: Option<bool>,
    /// Additional flags (e.g. `SEED_MODE`), ORed into the add-time
    /// defaults (`UPDATE_SUBSCRIBE`, `AUTO_MANAGED`, `PAUSED`,
    /// `APPLY_IP_FILTER`, `NEED_SAVE_RESUME`); this cannot clear a
    /// default flag. Auto-management promptly resumes the default
    /// `PAUSED`; use `paused` to add actually paused. The daemon
    /// always adds `DUPLICATE_IS_ERROR`.
    pub flags: Option<Vec<TorrentFlag>>,
    /// Tracker URLs (all tier 0), added on top of the metadata's own.
    pub trackers: Option<Vec<String>>,
    /// HTTP/web seed URLs, added on top of the metadata's own.
    pub url_seeds: Option<Vec<String>>,
    /// Upload rate limit in positive bytes/s (-1 = unlimited).
    pub upload_limit: Option<i32>,
    /// Download rate limit in positive bytes/s (-1 = unlimited).
    pub download_limit: Option<i32>,
    /// Maximum unchoked (upload) slots for this torrent
    /// (-1 = unlimited, else 2..=16777214).
    pub max_uploads: Option<i32>,
    /// Maximum peer connections for this torrent
    /// (-1 = unlimited, else 2..=16777214).
    pub max_connections: Option<i32>,
}

/// The lifecycle state of a torrent-creation job.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreateJobState {
    /// Enumerating the source files.
    Listing,
    /// Hashing piece data.
    Hashing,
    /// Done; the .torrent is available (terminal).
    Finished,
    /// Failed; see `error` (terminal).
    Failed,
    /// Cancelled by `cancelCreateJob` (terminal).
    Cancelled,
}

impl From<crate::engine::jobs::JobState> for CreateJobState {
    fn from(state: crate::engine::jobs::JobState) -> Self {
        use crate::engine::jobs::JobState as S;
        match state {
            S::Listing => CreateJobState::Listing,
            S::Hashing => CreateJobState::Hashing,
            S::Finished => CreateJobState::Finished,
            S::Failed => CreateJobState::Failed,
            S::Cancelled => CreateJobState::Cancelled,
        }
    }
}

/// A torrent-creation job. Jobs are in memory only; terminal jobs are
/// pruned after about an hour.
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct CreateJob {
    /// Job id, unique within one daemon run.
    pub id: u64,
    /// Current lifecycle state.
    pub state: CreateJobState,
    /// Pieces hashed so far.
    pub pieces_done: u32,
    /// Total pieces to hash (0 until listing completes).
    pub pieces_total: u32,
    /// Failure message; only set in the `FAILED` state.
    pub error: Option<String>,
    /// Whether `torrentData` is available (finished without an
    /// `outputPath`).
    pub has_torrent_data: bool,
    /// Where the .torrent was (or will be) written, when requested.
    pub output_path: Option<String>,
    /// Shared with the job store; only cloned into a response when
    /// `torrentData` is actually selected.
    #[graphql(skip)]
    torrent: Option<std::sync::Arc<Vec<u8>>>,
}

#[ComplexObject]
impl CreateJob {
    /// The generated .torrent, when finished without an `outputPath`.
    /// This is the whole file as base64 — select it only when needed.
    async fn torrent_data(&self) -> Option<Base64Bytes> {
        self.torrent
            .as_ref()
            .map(|bytes| Base64Bytes(bytes.as_ref().clone()))
    }
}

impl From<&crate::engine::jobs::JobSnapshot> for CreateJob {
    fn from(snapshot: &crate::engine::jobs::JobSnapshot) -> Self {
        CreateJob {
            id: snapshot.id,
            state: snapshot.state.into(),
            pieces_done: snapshot.pieces_done,
            pieces_total: snapshot.pieces_total,
            error: snapshot.error.clone(),
            has_torrent_data: snapshot.torrent.is_some(),
            output_path: snapshot
                .output_path
                .as_ref()
                .map(|p| p.display().to_string()),
            torrent: snapshot.torrent.clone(),
        }
    }
}

/// A torrent-creation option.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreateFlag {
    /// Include file modification times.
    ModificationTime,
    /// Store symlinks as symlinks (BEP 47).
    Symlinks,
    /// v2-only torrent (no SHA-1 piece hashes).
    V2Only,
    /// v1-only torrent (no merkle trees).
    V1Only,
    /// Canonical file order and padding (BEP 52).
    CanonicalFiles,
    /// Skip file attributes (executable, hidden).
    NoAttributes,
    /// Canonical files without tail padding of the last file.
    CanonicalFilesNoTailPadding,
}

impl CreateFlag {
    pub fn bits(self) -> rbtorrent::CreateFlags {
        use rbtorrent::CreateFlags as F;
        match self {
            CreateFlag::ModificationTime => F::MODIFICATION_TIME,
            CreateFlag::Symlinks => F::SYMLINKS,
            CreateFlag::V2Only => F::V2_ONLY,
            CreateFlag::V1Only => F::V1_ONLY,
            CreateFlag::CanonicalFiles => F::CANONICAL_FILES,
            CreateFlag::NoAttributes => F::NO_ATTRIBUTES,
            CreateFlag::CanonicalFilesNoTailPadding => F::CANONICAL_FILES_NO_TAIL_PADDING,
        }
    }
}

/// A tracker announce URL with its failover tier.
#[derive(async_graphql::InputObject)]
pub struct TrackerInput {
    /// The announce URL.
    pub url: String,
    /// Failover tier to place the tracker in (lower is preferred).
    #[graphql(default = 0)]
    pub tier: i32,
}

/// Input for `startCreateTorrent`.
#[derive(async_graphql::InputObject)]
pub struct CreateTorrentInput {
    /// The daemon-local file or directory to create the torrent from.
    pub source_path: String,
    /// Piece size in bytes (a power of two, 16 KiB to 128 MiB); omit or
    /// 0 for automatic.
    pub piece_size: Option<u32>,
    /// Creation options. Without `V1_ONLY` or `V2_ONLY` (which are
    /// mutually exclusive), a hybrid v1+v2 torrent is created.
    pub flags: Option<Vec<CreateFlag>>,
    /// Trackers to embed in the metadata, with explicit tiers.
    pub trackers: Option<Vec<TrackerInput>>,
    /// HTTP/web seed URLs to embed in the metadata.
    pub url_seeds: Option<Vec<String>>,
    /// Free-form comment to embed in the metadata.
    pub comment: Option<String>,
    /// "Created by" string to embed in the metadata.
    pub creator: Option<String>,
    /// Mark the torrent private (BEP 27): peers come from trackers
    /// only, disabling DHT/PEX/LSD.
    #[graphql(default = false)]
    pub private: bool,
    /// Write the .torrent to this daemon-local path instead of returning
    /// it inline as base64.
    pub output_path: Option<String>,
}

/// A torrent in the session.
pub struct Torrent {
    entry: Arc<TorrentEntry>,
    status: TorrentStatus,
    /// Torrent flag bits captured together with the status snapshot.
    flags_bits: u64,
    /// Per-request metadata memo. [`rbtorrent::TorrentInfo`] is an owned,
    /// refcounted snapshot with no session tie, so storing it here is
    /// safe; the memo keeps every metadata-derived field of one request
    /// coherent and avoids re-fetching per field.
    info: tokio::sync::OnceCell<Option<rbtorrent::TorrentInfo>>,
}

impl Torrent {
    /// Snapshots `entry`'s status and flags for the field resolvers.
    pub fn load(engine: &Engine, entry: Arc<TorrentEntry>) -> Result<Torrent, EngineError> {
        let (status, flags_bits) = engine.with_handle(&entry, |h| {
            let status = h.status(STATUS_FLAGS)?;
            Ok::<_, EngineError>((status, h.flags()))
        })??;
        Ok(Torrent {
            entry,
            status,
            flags_bits,
            info: tokio::sync::OnceCell::new(),
        })
    }

    /// The state as the GraphQL enum (used by query filters too).
    pub fn state_value(&self) -> TorrentState {
        self.status.state().into()
    }

    /// The torrent's metadata, or `None` before it's available
    /// (memoized per request).
    async fn torrent_info(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<&Option<rbtorrent::TorrentInfo>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(self
            .info
            .get_or_try_init(|| async {
                engine
                    .with_handle(&self.entry, |h| h.torrent_file())?
                    .map_err(EngineError::from)
            })
            .await?)
    }
}

/// A torrent in the session.
///
/// Scalar fields come from one status snapshot taken when the object
/// was resolved; `pieces`, `files`, `trackers`, and `peers` issue
/// separate live requests and can reflect a slightly later state.
#[Object]
impl Torrent {
    /// The torrent's durable identifier: minted when the torrent is
    /// added, stable across daemon restarts, and the sole key for all
    /// torrent operations.
    async fn uuid(&self) -> uuid::Uuid {
        self.entry.uuid
    }

    /// The v1 (SHA-1) info-hash, if the torrent has one. A hybrid
    /// magnet may gain it when metadata arrives.
    async fn info_hash_v1(&self) -> Option<Sha1Sum> {
        self.status.info_hashes().v1().map(Sha1Sum)
    }

    /// The v2 (SHA-256) info-hash, if the torrent has one. A hybrid
    /// magnet may gain it when metadata arrives.
    async fn info_hash_v2(&self) -> Option<Sha256Sum> {
        self.status.info_hashes().v2().map(Sha256Sum)
    }

    /// Display name: from metadata, or the provisional add/magnet name
    /// before metadata.
    async fn name(&self) -> String {
        self.status.name()
    }

    /// The torrent's current primary task.
    async fn state(&self) -> TorrentState {
        self.state_value()
    }

    /// Completion of the currently selected files, 0..=1.
    async fn progress(&self) -> f64 {
        f64::from(self.status.progress())
    }

    /// Completion in parts-per-million (avoids float rounding).
    async fn progress_ppm(&self) -> i32 {
        self.status.progress_ppm()
    }

    /// The error that stopped the torrent, or `null` if none.
    async fn error(&self) -> Option<TorrentErrorInfo> {
        self.status.error().map(|e| TorrentErrorInfo {
            message: e.to_string(),
            file: self.status.error_file(),
        })
    }

    /// The daemon-local directory the torrent is stored in.
    async fn save_path(&self) -> String {
        self.status.save_path()
    }

    /// URL of the last working tracker, or `null` if none has responded
    /// yet.
    async fn current_tracker(&self) -> Option<String> {
        let tracker = self.status.current_tracker();
        (!tracker.is_empty()).then_some(tracker)
    }

    /// Seconds until the next scheduled tracker announce.
    async fn next_announce_seconds(&self) -> i64 {
        self.status.next_announce_seconds()
    }

    /// Bytes received during the current active run, including protocol
    /// overhead; resets when the torrent is paused and restarted.
    async fn total_download(&self) -> i64 {
        self.status.total_download()
    }

    /// Bytes sent during the current active run, including protocol
    /// overhead; resets when the torrent is paused and restarted.
    async fn total_upload(&self) -> i64 {
        self.status.total_upload()
    }

    /// Payload bytes received during the current active run; resets
    /// when the torrent is paused and restarted.
    async fn total_payload_download(&self) -> i64 {
        self.status.total_payload_download()
    }

    /// Payload bytes sent during the current active run; resets when
    /// the torrent is paused and restarted.
    async fn total_payload_upload(&self) -> i64 {
        self.status.total_payload_upload()
    }

    /// Bytes downloaded that failed the hash check, for the current
    /// active run.
    async fn total_failed_bytes(&self) -> i64 {
        self.status.total_failed_bytes()
    }

    /// Bytes downloaded more than once (redundant), for the current
    /// active run.
    async fn total_redundant_bytes(&self) -> i64 {
        self.status.total_redundant_bytes()
    }

    /// Bytes downloaded and verified (of the whole torrent).
    async fn total_done(&self) -> i64 {
        self.status.total_done()
    }

    /// Size of the whole torrent in bytes; `null` until metadata is
    /// available.
    async fn total_size(&self) -> Option<i64> {
        self.status.has_metadata().then(|| self.status.total())
    }

    /// Bytes done of the selected files.
    async fn total_wanted_done(&self) -> i64 {
        self.status.total_wanted_done()
    }

    /// Bytes of the selected files.
    async fn total_wanted(&self) -> i64 {
        self.status.total_wanted()
    }

    /// Persistent payload bytes sent across all runs, restored from
    /// resume data.
    async fn all_time_upload(&self) -> i64 {
        self.status.all_time_upload()
    }

    /// Persistent payload bytes received across all runs, restored from
    /// resume data.
    async fn all_time_download(&self) -> i64 {
        self.status.all_time_download()
    }

    /// Unix time the torrent was added.
    async fn added_time(&self) -> i64 {
        self.status.added_time()
    }

    /// Unix time the torrent finished; `null` if it never has.
    async fn completed_time(&self) -> Option<i64> {
        let t = self.status.completed_time();
        (t != 0).then_some(t)
    }

    /// Unix time a complete copy of the torrent was last seen in the
    /// swarm; `null` if never.
    async fn last_seen_complete(&self) -> Option<i64> {
        let t = self.status.last_seen_complete();
        (t != 0).then_some(t)
    }

    /// How disk space for the torrent is allocated.
    async fn storage_mode(&self) -> StorageMode {
        self.status.storage_mode().into()
    }

    /// Position in the download queue; `null` for torrents not in the
    /// queue (seeds and finished torrents).
    async fn queue_position(&self) -> Option<i32> {
        let p = self.status.queue_position();
        (p >= 0).then_some(p)
    }

    /// Total download rate, bytes/s.
    async fn download_rate(&self) -> i32 {
        self.status.download_rate()
    }

    /// Total upload rate, bytes/s.
    async fn upload_rate(&self) -> i32 {
        self.status.upload_rate()
    }

    /// Payload-only download rate, bytes/s.
    async fn download_payload_rate(&self) -> i32 {
        self.status.download_payload_rate()
    }

    /// Payload-only upload rate, bytes/s.
    async fn upload_payload_rate(&self) -> i32 {
        self.status.upload_payload_rate()
    }

    /// Connected peers that are seeds.
    async fn num_seeds(&self) -> i32 {
        self.status.num_seeds()
    }

    /// Established peer connections (including seeds).
    async fn num_peers(&self) -> i32 {
        self.status.num_peers()
    }

    /// Seeds in the swarm per the tracker scrape; `null` when no scrape
    /// data is available.
    async fn num_complete(&self) -> Option<i32> {
        let n = self.status.num_complete();
        (n >= 0).then_some(n)
    }

    /// Downloaders in the swarm per the tracker scrape; `null` when no
    /// scrape data is available.
    async fn num_incomplete(&self) -> Option<i32> {
        let n = self.status.num_incomplete();
        (n >= 0).then_some(n)
    }

    /// Seeds in the known-peer list (connected or not, including failed
    /// and banned entries).
    async fn list_seeds(&self) -> i32 {
        self.status.list_seeds()
    }

    /// Peers in the known-peer list (connected or not, including failed
    /// and banned entries).
    async fn list_peers(&self) -> i32 {
        self.status.list_peers()
    }

    /// Known peers currently usable as connection candidates.
    async fn connect_candidates(&self) -> i32 {
        self.status.connect_candidates()
    }

    /// Number of pieces downloaded (the total is `pieces.total`).
    async fn pieces_have(&self) -> i32 {
        self.status.num_pieces()
    }

    /// Estimated distinct copies of the rarest pieces across connected
    /// peers; the fraction is the share of pieces with more copies than
    /// the rarest. `null` when unavailable (e.g. while seeding).
    async fn distributed_copies(&self) -> Option<f64> {
        let c = f64::from(self.status.distributed_copies());
        (c >= 0.0).then_some(c)
    }

    /// Block size used for peer requests, in bytes (normally 16 KiB).
    async fn block_size(&self) -> i32 {
        self.status.block_size()
    }

    /// Peers currently unchoked (being uploaded to).
    async fn num_uploads(&self) -> i32 {
        self.status.num_uploads()
    }

    /// Peer connections including half-open ones; at least `numPeers`.
    async fn num_connections(&self) -> i32 {
        self.status.num_connections()
    }

    /// Configured unchoke-slot cap for this torrent (-1 = unlimited).
    async fn uploads_limit(&self) -> i32 {
        self.status.uploads_limit()
    }

    /// Configured connection cap for this torrent (-1 = unlimited).
    async fn connections_limit(&self) -> i32 {
        self.status.connections_limit()
    }

    /// Per-torrent upload limit, bytes/s (-1 = no per-torrent limit;
    /// session-wide limits still apply).
    async fn upload_limit(&self) -> i32 {
        normalize_rate_limit(self.status.upload_limit())
    }

    /// Per-torrent download limit, bytes/s (-1 = no per-torrent limit;
    /// session-wide limits still apply).
    async fn download_limit(&self) -> i32 {
        normalize_rate_limit(self.status.download_limit())
    }

    /// Peers waiting for this torrent's upload rate limiter.
    async fn up_bandwidth_queue(&self) -> i32 {
        self.status.up_bandwidth_queue()
    }

    /// Peers waiting for this torrent's download rate limiter.
    async fn down_bandwidth_queue(&self) -> i32 {
        self.status.down_bandwidth_queue()
    }

    /// Auto-management seeding importance; higher ranks keep their
    /// seeding slot longer.
    async fn seed_rank(&self) -> i32 {
        self.status.seed_rank()
    }

    /// Whether the torrent has state changes not yet saved to resume
    /// data.
    async fn need_save_resume_data(&self) -> bool {
        self.status.need_save_resume_data() != 0
    }

    /// Whether every piece is present. Differs from `isFinished` when
    /// files or pieces are skipped via priorities.
    async fn is_seeding(&self) -> bool {
        self.status.is_seeding()
    }

    /// Whether every priority-nonzero piece is present.
    async fn is_finished(&self) -> bool {
        self.status.is_finished()
    }

    /// The torrent's own pause flag. Does not reflect a global
    /// `pauseSession`; check `session.isPaused` too.
    async fn is_paused(&self) -> bool {
        self.flags_bits & TorrentFlags::PAUSED.bits() != 0
    }

    /// Whether queue logic may start and stop this torrent (the
    /// `AUTO_MANAGED` flag).
    async fn is_auto_managed(&self) -> bool {
        self.flags_bits & TorrentFlags::AUTO_MANAGED.bits() != 0
    }

    /// Whether metadata is available (false for a magnet still fetching
    /// it).
    async fn has_metadata(&self) -> bool {
        self.status.has_metadata()
    }

    /// Sum of all non-pad file sizes (what is actually written to disk);
    /// `null` until metadata is available.
    async fn size_on_disk(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<i64>> {
        Ok(self
            .torrent_info(ctx)
            .await?
            .as_ref()
            .map(|info| info.size_on_disk()))
    }

    /// Byte length of each piece (except possibly the last one); `null`
    /// until metadata is available.
    async fn piece_length(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<i32>> {
        Ok(self
            .torrent_info(ctx)
            .await?
            .as_ref()
            .map(|info| info.piece_length()))
    }

    /// Whether the torrent is private (BEP 27); `null` until metadata is
    /// available.
    async fn is_private(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<bool>> {
        Ok(self
            .torrent_info(ctx)
            .await?
            .as_ref()
            .map(|info| info.is_private()))
    }

    /// Whether this is an i2p torrent (a tracker URL has an `.i2p`
    /// domain); `null` until metadata is available.
    #[graphql(name = "isI2P")]
    async fn is_i2p(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<bool>> {
        Ok(self
            .torrent_info(ctx)
            .await?
            .as_ref()
            .map(|info| info.is_i2p()))
    }

    /// Whether an incoming connection has ever been received for this
    /// torrent (an indication of reachability, not a live connection).
    async fn has_incoming(&self) -> bool {
        self.status.has_incoming()
    }

    /// Whether a `moveStorage` operation is in progress.
    async fn moving_storage(&self) -> bool {
        self.status.moving_storage()
    }

    /// Whether auto-management currently lets the torrent announce to
    /// its trackers.
    async fn announcing_to_trackers(&self) -> bool {
        self.status.announcing_to_trackers()
    }

    /// Whether auto-management currently lets the torrent announce via
    /// Local Service Discovery.
    async fn announcing_to_lsd(&self) -> bool {
        self.status.announcing_to_lsd()
    }

    /// Whether auto-management currently lets the torrent announce to
    /// the DHT.
    async fn announcing_to_dht(&self) -> bool {
        self.status.announcing_to_dht()
    }

    /// The torrent's active option flags.
    async fn flags(&self) -> Vec<TorrentFlag> {
        flags_to_list(self.flags_bits)
    }

    /// A magnet link (info-hashes and display name only).
    async fn magnet_uri(&self) -> String {
        let hashes = self.status.info_hashes();
        let mut uri = String::from("magnet:?");
        let mut sep = "";
        if let Some(v1) = hashes.v1() {
            uri.push_str(&format!("xt=urn:btih:{v1}"));
            sep = "&";
        }
        if let Some(v2) = hashes.v2() {
            uri.push_str(&format!("{sep}xt=urn:btmh:1220{v2}"));
            sep = "&";
        }
        let name = self.status.name();
        if !name.is_empty() {
            uri.push_str(&format!("{sep}dn={}", percent_encode(&name)));
        }
        uri
    }

    /// Piece availability; set `includeBitfield` for the packed bitfield.
    async fn pieces(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = false)] include_bitfield: bool,
    ) -> async_graphql::Result<PieceInfo> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let status = engine
            .with_handle(&self.entry, |h| h.status(TorrentHandle::QUERY_PIECES))?
            .map_err(EngineError::from)?;
        let mut total = None;
        let mut have = 0i32;
        let mut bitfield = None;
        // The bitfield is sized to the piece count; it is empty before
        // metadata (a real torrent always has at least one piece).
        if let Some(bits) = status.pieces().filter(|b| !b.is_empty()) {
            total = Some(bits.len() as i32);
            have = bits.count_ones() as i32;
            if include_bitfield {
                let mut packed = vec![0u8; bits.len().div_ceil(8)];
                for (i, bit) in bits.iter().enumerate() {
                    if bit {
                        packed[i / 8] |= 0x80 >> (i % 8);
                    }
                }
                bitfield = Some(Base64Bytes(packed));
            }
        }
        if total.is_none() {
            total = self
                .torrent_info(ctx)
                .await?
                .as_ref()
                .map(|info| info.num_pieces());
        }
        Ok(PieceInfo {
            total,
            have,
            bitfield,
        })
    }

    /// Per-file details; `null` until metadata is available. Paths
    /// reflect renames applied via `renameFile`.
    async fn files(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Vec<TorrentFile>>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        // One handle scope covers every handle-derived piece: the
        // metadata (original names), the live paths (renamed-file
        // overlay), and the per-file priorities.
        let Some((info, paths, priorities)) = engine
            .with_handle(&self.entry, |h| {
                let Some(info) = h.torrent_file()? else {
                    return Ok(None);
                };
                let paths = h.file_paths()?.unwrap_or_default();
                let priorities: Vec<i32> = (0..info.num_files())
                    .map(|i| i32::from(h.file_priority(i).value()))
                    .collect();
                Ok::<_, rbtorrent::Error>(Some((info, paths, priorities)))
            })?
            .map_err(EngineError::from)?
        else {
            return Ok(None);
        };
        let progress = engine.file_progress(&self.entry).await?;
        let files = info
            .files()
            .map(|f| {
                let flags = f.flags();
                TorrentFile {
                    index: f.index(),
                    path: paths
                        .get(f.index() as usize)
                        .cloned()
                        .unwrap_or_else(|| f.path()),
                    size: f.size(),
                    offset: f.offset(),
                    priority: priorities.get(f.index() as usize).copied().unwrap_or(0),
                    progress_bytes: progress.get(f.index() as usize).copied().unwrap_or(0),
                    is_pad_file: flags.is_pad_file(),
                    is_symlink: flags.is_symlink(),
                    symlink_target: f.symlink(),
                    is_executable: flags.is_executable(),
                    is_hidden: flags.is_hidden(),
                }
            })
            .collect();
        Ok(Some(files))
    }

    /// The torrent's trackers.
    async fn trackers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Tracker>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let trackers = engine.trackers(&self.entry).await?;
        Ok(trackers.into_iter().map(Tracker::from).collect())
    }

    /// The torrent's current HTTP/web seed URLs (BEP 19), including ones
    /// added via `addUrlSeed`.
    async fn url_seeds(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<String>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(engine
            .with_handle(&self.entry, |h| h.url_seeds())?
            .map_err(EngineError::from)?)
    }

    /// Currently connected peers.
    async fn peers(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Peer>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let peers = engine.peers(&self.entry).await?;
        Ok(peers.into_iter().map(Peer::from).collect())
    }
}

/// Percent-encodes everything but RFC 3986 unreserved characters.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_expand_to_enums() {
        let bits = rbtorrent::TorrentFlags::SEED_MODE.bits()
            | rbtorrent::TorrentFlags::SEQUENTIAL_DOWNLOAD.bits();
        let flags = flags_to_list(bits);
        assert_eq!(
            flags,
            vec![TorrentFlag::SeedMode, TorrentFlag::SequentialDownload]
        );
        assert!(flags_to_list(0).is_empty());
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("a b/\u{e4}"), "a%20b%2F%C3%A4");
    }
}
