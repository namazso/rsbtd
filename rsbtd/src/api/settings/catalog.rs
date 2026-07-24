// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The scalar settings: libtorrent settings exposed unchanged, one
//! documented GraphQL field per setting.
//!
//! Doc text is derived from libtorrent's `settings_pack.hpp` comments,
//! rewritten to be self-contained per field (no C++ API references, no
//! shared multi-setting blocks); maintain it by hand when the vendored
//! libtorrent changes. The declaration list is deliberately explicit, in
//! the generated table's declaration order: adding a libtorrent setting
//! here is the review step the module's default-deny rule requires.

use async_graphql::{InputObject, MaybeUndefined, SimpleObject};
use rbtorrent::SettingsPack;

use super::{SettingsError, defined, get_bool, get_int, get_str, missing, sat_u8, sat_u16};

/// Reads one scalar setting from the effective pack, dispatched on the
/// declared field type. The `u16`/`u8` fields read leniently: values
/// written through the typed setters are always in range, and anything
/// else (a foreign state blob) saturates rather than failing the whole
/// read.
macro_rules! scalar_get {
    (String, $pack:expr, $name:expr) => {
        get_str($pack, $name)?
    };
    (String checked, $pack:expr, $name:expr) => {
        get_str($pack, $name)?
    };
    (i32, $pack:expr, $name:expr) => {
        get_int($pack, $name)?
    };
    (i32 checked, $pack:expr, $name:expr) => {
        get_int($pack, $name)?
    };
    // The stored value is the whole traffic-class byte; the API value is
    // the DSCP code point. Decoding lives in the rbtorrent getter.
    (i32 dscp, $pack:expr, $name:expr) => {
        $pack.get_peer_dscp().ok_or_else(|| missing($name))?
    };
    (bool, $pack:expr, $name:expr) => {
        get_bool($pack, $name)?
    };
    (u16, $pack:expr, $name:expr) => {
        sat_u16(get_int($pack, $name)?)
    };
    (u8, $pack:expr, $name:expr) => {
        sat_u8(get_int($pack, $name)?)
    };
}

/// Stages one scalar setting into a delta by calling the rbtorrent
/// setter of the same name. Value domains live in those setters
/// (`i32 checked` marks the fallible ones); rejections fail the whole
/// `applySettings` delta before anything reaches the session.
macro_rules! scalar_set {
    (String, $delta:expr, $name:ident, $value:expr) => {
        $delta.$name(&$value)
    };
    (String checked, $delta:expr, $name:ident, $value:expr) => {
        $delta
            .$name(&$value)
            .map_err(|e| super::rb_err(stringify!($name), &e))?
    };
    (i32, $delta:expr, $name:ident, $value:expr) => {
        $delta.$name($value)
    };
    (i32 checked, $delta:expr, $name:ident, $value:expr) => {
        $delta
            .$name($value)
            .map_err(|e| super::rb_err(stringify!($name), &e))?
    };
    (i32 dscp, $delta:expr, $name:ident, $value:expr) => {
        $delta
            .$name($value)
            .map_err(|e| super::rb_err(stringify!($name), &e))?
    };
    (bool, $delta:expr, $name:ident, $value:expr) => {
        $delta.$name($value)
    };
    (u16, $delta:expr, $name:ident, $value:expr) => {
        $delta.$name($value)
    };
    (u8, $delta:expr, $name:ident, $value:expr) => {
        $delta.$name($value)
    };
}

/// A valid value per field, for per-field test inputs. Fields with a
/// constrained domain (`i32 checked`) use the libtorrent default, which
/// is in-domain by definition; the rest use arbitrary values.
#[cfg(test)]
macro_rules! scalar_sample {
    (String, $name:ident) => {
        MaybeUndefined::Value(String::from("sample"))
    };
    (String checked, $name:ident) => {
        MaybeUndefined::Value(String::from("sample"))
    };
    (i32, $name:ident) => {
        MaybeUndefined::Value(7)
    };
    (i32 checked, $name:ident) => {
        MaybeUndefined::Value(super::default_int(stringify!($name)))
    };
    (i32 dscp, $name:ident) => {
        // A code point, not the stored traffic-class byte (46 = EF).
        MaybeUndefined::Value(46)
    };
    (bool, $name:ident) => {
        MaybeUndefined::Value(true)
    };
    (u16, $name:ident) => {
        MaybeUndefined::Value(7u16)
    };
    (u8, $name:ident) => {
        MaybeUndefined::Value(7u8)
    };
}

/// Expands one declaration per scalar setting into every artifact the
/// module needs, so they cannot drift apart:
///
/// - a documented [`ScalarSettings`] field (read view),
/// - a documented [`ScalarSettingsInput`] field (write view; omitted =
///   unchanged, null = rejected),
/// - the field's arm in [`read_scalars`] (struct literal, so a missing
///   arm is a compile error),
/// - the field's arm in [`write_scalars`] (full destructure, so an
///   unhandled field is an `unused_variables` error),
/// - its [`SCALAR_BACKING`] entry (a scalar's backing setting is itself),
/// - test-only per-field sample inputs and a read→input converter.
macro_rules! scalar_settings {
    ($($(#[$doc:meta])* $name:ident: $ty:ident $($extra:ident)?;)+) => {
        /// The scalar libtorrent settings, exposed unchanged.
        #[derive(Debug, Clone, PartialEq, SimpleObject)]
        pub struct ScalarSettings {
            $($(#[$doc])* pub $name: $ty,)+
        }

        /// Scalar-setting changes: omitted fields are left unchanged;
        /// explicit `null` is rejected.
        #[derive(InputObject)]
        pub struct ScalarSettingsInput {
            $($(#[$doc])* pub $name: MaybeUndefined<$ty>,)+
        }

        /// Reads every scalar setting from the full effective pack.
        #[allow(clippy::too_many_lines)]
        pub(super) fn read_scalars(pack: &SettingsPack) -> Result<ScalarSettings, SettingsError> {
            Ok(ScalarSettings {
                $($name: scalar_get!($ty $($extra)?, pack, stringify!($name)),)+
            })
        }

        /// Validates `input` and stages every defined scalar into `delta`.
        #[allow(clippy::too_many_lines)]
        pub(super) fn write_scalars(
            delta: &mut SettingsPack,
            input: ScalarSettingsInput,
        ) -> Result<(), SettingsError> {
            let ScalarSettingsInput { $($name),+ } = input;
            $(
                if let Some(value) = defined($name, stringify!($name))? {
                    scalar_set!($ty $($extra)?, delta, $name, value);
                }
            )+
            Ok(())
        }

        /// `(public field, backing libtorrent settings)` per scalar
        /// setting.
        pub(super) static SCALAR_BACKING: &[(&str, &[&str])] = &[
            $((stringify!($name), &[stringify!($name)]),)+
        ];

        #[cfg(test)]
        impl ScalarSettingsInput {
            /// An input with every field omitted.
            pub(super) fn undefined() -> Self {
                Self {
                    $($name: MaybeUndefined::Undefined,)+
                }
            }
        }

        /// One input per scalar field, with only that field defined.
        #[cfg(test)]
        pub(super) fn scalar_single_field_inputs()
        -> Vec<(&'static str, ScalarSettingsInput)> {
            vec![
                $((
                    stringify!($name),
                    ScalarSettingsInput {
                        $name: scalar_sample!($ty $($extra)?, $name),
                        ..ScalarSettingsInput::undefined()
                    },
                ),)+
            ]
        }

        #[cfg(test)]
        impl ScalarSettings {
            /// Converts a read view into an input that sets every field
            /// to the value read (for roundtrip tests).
            pub(super) fn into_input(self) -> ScalarSettingsInput {
                ScalarSettingsInput {
                    $($name: MaybeUndefined::Value(self.$name),)+
                }
            }
        }
    };
}

scalar_settings! {
    /// The IP address to pass to trackers as the `&ip=` announce
    /// parameter. Empty (the default) omits the parameter.
    ///
    /// Only useful in the special case where the seed runs on the same
    /// host as the tracker and the tracker honors the parameter (normal
    /// trackers do not). Do not set it unless you also control the
    /// tracker.
    announce_ip: String;
    /// The client name and version identifier sent to peers in the
    /// extension handshake. Empty (the default) sends the `userAgent`
    /// identity instead. Must be valid UTF-8.
    handshake_client_version: String;
    /// The fingerprint prefix of the peer ID sent to peers,
    /// conventionally encoding the client name and version (e.g.
    /// `-qB5050-`). A value of 20 bytes or longer is truncated to 20
    /// bytes and used as the entire peer ID.
    peer_fingerprint: String checked;
    /// Overrides the gateway address used by the NAT-PMP service. When
    /// empty (the default), the default gateway is discovered
    /// automatically. Only read when NAT-PMP starts: to change it while
    /// NAT-PMP is running, disable and re-enable `enableNatpmp`.
    natpmp_gateway: String;
    /// Whether to accept multiple peer connections from the same IP
    /// address. Rejecting them (the default) prevents abusive behavior,
    /// and the logic that identifies duplicate peers is more reliable
    /// that way; enabling this is not recommended.
    allow_multiple_connections_per_ip: bool;
    /// Whether to send `have` messages to peers that already have the
    /// piece. Typically unnecessary, but occasionally useful for peers
    /// that collect statistics.
    send_redundant_have: bool;
    /// When true, the DHT is only used for torrents whose trackers have
    /// all failed (by explicit error or timeout). When false, the DHT is
    /// used regardless of tracker state.
    use_dht_as_fallback: bool;
    /// Whether the UPnP service ignores broadcast responses from devices
    /// outside the local subnet — a way to avoid talking to other
    /// people's routers by mistake.
    upnp_ignore_nonrouters: bool;
    /// Whether to use parole mode: a peer that participated in a piece
    /// that failed its hash check may only download whole pieces. If a
    /// whole piece downloaded from a peer on parole fails its hash
    /// check, that peer is banned; a passing piece releases it from
    /// parole.
    use_parole_mode: bool;
    /// When true, auto-management prefers seeding torrents when handing
    /// out active slots; when false, downloading torrents are preferred.
    auto_manage_prefer_seeds: bool;
    /// When true, torrents without any payload transfer are not counted
    /// against the `activeSeeds` and `activeDownloads` limits, so idle
    /// torrents do not occupy active slots and available bandwidth is
    /// more likely to be used. `inactiveDownRate`, `inactiveUpRate`, and
    /// `autoManageStartup` control what counts as inactive.
    dont_count_slow_torrents: bool;
    /// Whether to close connections that no longer serve a purpose for
    /// either end, for instance between two peers that have both
    /// completed their downloads.
    close_redundant_connections: bool;
    /// When true, partially downloaded pieces are picked before rarer
    /// pieces. When false, rare pieces always win unless the number of
    /// partial pieces grows out of proportion.
    prioritize_partial_pieces: bool;
    /// When true, estimated TCP/IP overhead is drained from the rate
    /// limiters, so total traffic stays within the configured limits.
    rate_limit_ip_overhead: bool;
    /// When true, one tracker from every tier is announced to (the
    /// uTorrent behavior). When false, announcing follows the
    /// multi-tracker specification and stops at the first responding
    /// tier. See also `announceToAllTrackers`.
    announce_to_all_tiers: bool;
    /// When true, all trackers within a tier are announced to in
    /// parallel; if every tracker in tier 0 fails, all of tier 1 is
    /// announced too. When false, announcing follows the multi-tracker
    /// specification, trying one tracker at a time within a tier. See
    /// also `announceToAllTiers`.
    announce_to_all_trackers: bool;
    /// When true, trackers may be reordered so that a UDP tracker is
    /// tried before HTTP trackers of the same hostname. When false,
    /// tracker tiers are respected with no protocol preference.
    prefer_udp_trackers: bool;
    /// Linux only: open files with `O_NOATIME`, which can improve disk
    /// performance by skipping access-time updates.
    no_atime_storage: bool;
    /// Whether an incoming peer connection may start a torrent that
    /// auto-management has paused (queued). Queued torrents normally
    /// stay stopped to save announce overhead and unchoke slots, but a
    /// peer that still finds us can be served when this is enabled.
    incoming_starts_queued_torrents: bool;
    /// When true, the downloaded counter sent to trackers includes
    /// redundant payload bytes; when false, redundancy is excluded.
    report_true_downloaded: bool;
    /// Controls when a block may be requested from a second peer. When
    /// true, a block is only requested twice once every piece left to
    /// download already has an outstanding request; this avoids
    /// redundant downloads at some cost in progress. When false, every
    /// peer connection is kept busy even if that duplicates requests
    /// already outstanding elsewhere.
    strict_end_game_mode: bool;
    /// What to do when resume data is missing or incomplete: true skips
    /// checking existing files and goes straight to downloading
    /// (assuming none of the data is present); false checks whatever is
    /// already on disk.
    no_recheck_incomplete_resume: bool;
    /// When true, the client tries to hide its identity to a certain
    /// degree: a generic user agent is sent to trackers (except for
    /// private torrents), the local IPv4/IPv6 addresses are not sent to
    /// private trackers as query parameters, a configured `announceIp`
    /// is not sent, and the client version is not sent to peers in the
    /// extension handshake.
    anonymous_mode: bool;
    /// Whether data downloaded from web seeds is reported to the
    /// tracker. Disabling this also excludes web-seed traffic from
    /// other transfer statistics and rate reporting.
    report_web_seed_downloads: bool;
    /// Whether seeding (and finished) torrents make outgoing peer
    /// connections. Disabling this only makes sense in very specific
    /// setups where outgoing connections are expensive and every peer is
    /// reachable without them (no firewalls or NATs).
    seeding_outgoing_connections: bool;
    /// When true, no outgoing connections are attempted to peers on
    /// ports below 1024 — a precaution against being used as part of a
    /// DDoS attack.
    no_connect_privileged_ports: bool;
    /// When true, connection attempts per second may be throttled below
    /// `connectionSpeed` when close to the connection limit, spreading
    /// attempts evenly over time instead of connecting — and timing out
    /// — in batches.
    smooth_connects: bool;
    /// When true, every web-seed request carries the user agent; when
    /// false, only the first request per HTTP connection does.
    always_send_user_agent: bool;
    /// Whether the session IP filter also applies to trackers, not just
    /// peers. Irrelevant when no IP filter is set.
    apply_ip_filter_to_trackers: bool;
    /// Whether web seeds that send bad data are banned.
    ban_web_seeds: bool;
    /// When false, share-mode support (the `SHARE_MODE` torrent flag) is
    /// not advertised to peers.
    support_share_mode: bool;
    /// Whether the number of redundant bytes downloaded is reported to
    /// the tracker.
    report_redundant_bytes: bool;
    /// When true, if binding the configured listen port fails, fall back
    /// to a port chosen by the operating system (bind to port 0). When
    /// false, a failed bind is reported as an error.
    listen_system_port_fallback: bool;
    /// Starts or stops the UPnP service. While running, the listen port
    /// and the DHT port are forwarded on local UPnP-capable routers.
    enable_upnp: bool;
    /// Starts or stops the NAT-PMP service. While running, the listen
    /// port and the DHT port are forwarded on the router via NAT-PMP.
    enable_natpmp: bool;
    /// Starts or stops Local Service Discovery, which broadcasts the
    /// info-hashes of all non-private torrents to the local network to
    /// find peers within multicast reach.
    enable_lsd: bool;
    /// Starts or stops the DHT node, which provides trackerless peer
    /// discovery.
    enable_dht: bool;
    /// When true, torrents with a very high availability of pieces (and
    /// seeds) are downloaded sequentially, which is more efficient for
    /// disk I/O; with many seeds the download order rarely matters
    /// anyway.
    auto_sequential: bool;
    /// Starts or stops the OS route-change notifier, which reacts to
    /// changes of the host's network configuration. Recommended on;
    /// disable it only if it is unreliable on the platform, and then
    /// call the `reopenNetworkSockets` mutation manually after network
    /// changes.
    enable_ip_notifier: bool;
    /// When true, DHT nodes whose IDs are correctly derived from their
    /// source IP per BEP 42 are preferred in the routing table.
    dht_prefer_verified_node_ids: bool;
    /// Restricts DHT routing-table entries to one per IP, and rejects
    /// nodes within a close CIDR distance (/24 for IPv4, /64 for IPv6)
    /// of an existing entry in the same bucket. Defaults to true;
    /// mitigates node-ID spoofing and related DHT attacks.
    dht_restrict_routing_ips: bool;
    /// Prevents DHT searches from adding nodes with IPs within a very
    /// close CIDR distance of nodes already in the search. Defaults to
    /// true; mitigates certain DHT attacks.
    dht_restrict_search_ips: bool;
    /// Sizes the first DHT routing-table buckets at 128, 64, 32, and 16
    /// nodes respectively, instead of the standard 8. All other buckets
    /// keep size 8.
    dht_extended_routing_table: bool;
    /// Keeps the full branch factor of outstanding requests to the
    /// closest nodes throughout a DHT lookup, querying closer nodes as
    /// soon as they are learned. Lowers lookup times at the cost of more
    /// outstanding queries.
    dht_aggressive_lookups: bool;
    /// When true, DHT lookups are performed in a slightly more expensive
    /// way that minimizes the amount of information leaked about this
    /// node.
    dht_privacy_lookups: bool;
    /// Ignores DHT nodes whose ID is not correctly derived from their
    /// external IP (BEP 42). Queries from such nodes are answered with
    /// an "invalid node ID" error message.
    dht_enforce_node_id: bool;
    /// Ignores DHT messages from parts of the address space no traffic
    /// is expected from.
    dht_ignore_dark_internet: bool;
    /// Puts the DHT node in read-only mode: it stops answering queries
    /// and marks its outgoing queries (a `ro` key) so other nodes do not
    /// add it to their routing tables. Meant for low-power, ephemeral,
    /// or traffic- and battery-sensitive devices.
    dht_read_only: bool;
    /// When true, the piece picker develops an affinity for 4 MiB
    /// extents of adjacent pieces, improving disk I/O throughput for
    /// torrents with small piece sizes.
    piece_extent_affinity: bool;
    /// Whether the TLS certificates of HTTPS trackers and HTTPS web
    /// seeds are validated against the system certificate store (as
    /// defined by OpenSSL/GnuTLS). May have to be disabled on systems
    /// without a certificate store for such trackers and web seeds to
    /// work.
    validate_https_trackers: bool;
    /// Restricts tracker and web-seed requests that reach the local
    /// network (SSRF mitigation). HTTP(S) tracker requests to loopback
    /// must have a request path starting with `/announce` (the
    /// conventional tracker path) — including after redirects. Web
    /// seeds that resolve to private address ranges may not carry query
    /// strings (including after redirects), and web seeds on global IPs
    /// may not redirect to local-network addresses.
    ssrf_mitigation: bool;
    /// Whether trackers and web seeds with internationalized (IDNA)
    /// hostnames are used. When disabled they are ignored, as a
    /// precaution against unicode-encoding attacks at the application
    /// level.
    allow_idna: bool;
    /// Linux only: set the no-copy-on-write flag (`FS_NOCOW_FL`) on
    /// downloaded files. This mitigates heavy fragmentation on
    /// copy-on-write filesystems such as btrfs, but also disables
    /// checksumming and compression for those files and restricts
    /// reflinks (a NOCOW file can only be reflinked into NOCOW
    /// directories), so it is disabled by default. Alternatively, set
    /// the NOCOW flag on the download directory itself: files created
    /// inside it inherit the flag.
    disk_disable_copy_on_write: bool;
    /// Whether to accept multiple connections carrying the same peer ID.
    /// Normally only one connection per peer is kept; for a peer with
    /// several IP addresses this can improve transfer efficiency, at the
    /// cost of extra network load.
    allow_multiple_connections_per_pid: bool;
    /// Seconds from sending a tracker request until the whole exchange
    /// is considered timed out.
    tracker_completion_timeout: i32 checked;
    /// Seconds without receiving any data from a tracker before the
    /// request is considered timed out. This is the timeout that
    /// triggers when a tracker is down.
    tracker_receive_timeout: i32 checked;
    /// Seconds to wait for the `stopped` announce when shutting a
    /// torrent down; kept short so the client can quit quickly. 0
    /// suppresses `stopped` announces entirely.
    stop_tracker_timeout: i32;
    /// Seconds from requesting a piece until the request times out when
    /// no part of the piece has been received.
    piece_timeout: i32;
    /// Seconds within which one requested 16 KiB block is expected to
    /// arrive; after that the block is requested from a different peer.
    request_timeout: i32;
    /// Length of the per-peer request queue, expressed as the number of
    /// seconds the queued requests should take the peer to send at its
    /// current rate — so the actual number of outstanding requests
    /// scales with the download rate.
    request_queue_time: i32;
    /// Number of outstanding block requests a peer may queue up in the
    /// client; requests beyond this are dropped. Higher values let a
    /// single peer reach faster upload speeds.
    max_allowed_in_request_queue: i32 checked;
    /// Maximum number of outstanding block requests to send to a peer.
    /// Takes precedence over `requestQueueTime`: the queue never exceeds
    /// this, regardless of download rate.
    max_out_request_queue: i32;
    /// If a whole piece can be downloaded from a peer within this many
    /// seconds, prefer requesting whole pieces from it. Localized
    /// accesses use disk caches better, and bad peers are easier to
    /// identify when a piece fails its hash check.
    whole_pieces_threshold: i32;
    /// Seconds without any activity on a peer connection before it is
    /// closed as timed out. The protocol specifies 120 seconds; a
    /// keep-alive message is sent after half the timeout.
    peer_timeout: i32 checked;
    /// Like `peerTimeout`, but for web seeds; usually lower, because web
    /// servers are expected to be more reliable.
    urlseed_timeout: i32;
    /// Seconds to wait before retrying a web seed, when the server does
    /// not provide a valid `retry-after` header.
    urlseed_wait_retry: i32;
    /// Upper limit on the number of files the session keeps open.
    /// Deferring file closes matters on systems where anti-virus
    /// software scans every closed file, and operating systems cap open
    /// file descriptors per process.
    file_pool_size: i32 checked;
    /// Maximum failed connection attempts to a peer before giving up on
    /// it. A successful connection resets the counter; rediscovering the
    /// peer from a source other than the DHT decrements it by one,
    /// allowing another try.
    max_failcount: i32 checked;
    /// Base number of seconds to wait before reconnecting to a peer,
    /// multiplied by the peer's failure count.
    min_reconnect_time: i32 checked;
    /// Seconds from initiating a peer connection attempt until it is
    /// considered timed out. Especially important when half-open
    /// connections are limited, since stale half-open connections delay
    /// other peers considerably.
    peer_connect_timeout: i32 checked;
    /// Outgoing connection attempts per second. 0 disables outgoing
    /// connections entirely; a negative value falls back to 200 per
    /// second.
    connection_speed: i32;
    /// Seconds a mutually uninterested and uninteresting peer connection
    /// is kept before it is disconnected.
    inactivity_timeout: i32;
    /// Seconds between choke/unchoke rounds, when peers are re-evaluated
    /// for choking. The protocol defines 30 seconds; it should be well
    /// above the time TCP needs to ramp up to its maximum rate.
    unchoke_interval: i32;
    /// Seconds between optimistic unchokes; on this timer, the
    /// optimistically unchoked peer is rotated.
    optimistic_unchoke_interval: i32;
    /// Number of peers to ask for in each tracker request (the
    /// `&num_want=` announce parameter).
    num_want: i32;
    /// Number of pieces to download before switching from random to
    /// rarest-first piece picking: the first this-many pieces of any
    /// torrent are picked at random.
    initial_picker_threshold: i32;
    /// Number of allowed-fast pieces to grant peers that support the
    /// fast extension.
    allowed_fast_set_size: i32;
    /// Maximum number of bytes waiting in the disk write queue. When the
    /// queue is full, peer connections stop reading from their sockets
    /// until the disk thread catches up. Setting this too low severely
    /// limits the download rate.
    max_queued_disk_bytes: i32 checked;
    /// Seconds to wait for a peer's handshake response; peers that do
    /// not respond in time are disconnected.
    handshake_timeout: i32 checked;
    /// Minimum send-buffer target size in bytes (the send buffer
    /// includes bytes pending being read from disk). Effectively the
    /// initial window size that determines how fast the send rate can
    /// ramp up; for good, snappy seeding performance, set it high enough
    /// to fit at least a few blocks.
    send_buffer_low_watermark: i32;
    /// Upper limit of the send-buffer watermark in bytes: whenever a
    /// send buffer holds fewer bytes than its current watermark, another
    /// 16 KiB block is read from disk onto it. Too small hurts upload
    /// capacity; too large wastes memory. The effective watermark is
    /// lower when the upload rate to the peer is low — see
    /// `sendBufferWatermarkFactor`.
    send_buffer_watermark: i32 checked;
    /// Percentage multiplied by the current upload rate to a peer to
    /// derive that peer's send-buffer watermark (50 means 0.5× the
    /// rate), clamped to at most `sendBufferWatermark`. Values above 100
    /// can improve upload performance and disk throughput on
    /// high-capacity connections; too high wastes RAM and biases disk
    /// work toward read jobs over write jobs.
    send_buffer_watermark_factor: i32;
    /// DSCP code point (0-63) set in the IP header of every packet sent
    /// to peers (including web seeds). 0 means no marking; 1 is Lower
    /// Effort. See RFC 8622 and IANA's DSCP registry for other values.
    peer_dscp: i32 dscp;
    /// Maximum number of auto-managed torrents allowed to actively
    /// download at once; the rest are queued (paused by
    /// auto-management). -1 means unlimited. The overall target of
    /// active torrents is min(`activeDownloads` + `activeSeeds`,
    /// `activeLimit`); torrents that are not auto-managed do not count
    /// against these limits.
    active_downloads: i32;
    /// Maximum number of auto-managed torrents allowed to actively seed
    /// at once; the rest are queued (paused by auto-management). -1
    /// means unlimited. The overall target of active torrents is
    /// min(`activeDownloads` + `activeSeeds`, `activeLimit`); torrents
    /// that are not auto-managed do not count against these limits.
    active_seeds: i32;
    /// Maximum number of torrents checking files simultaneously. -1
    /// means unlimited.
    active_checking: i32;
    /// Maximum number of auto-managed torrents announced to the DHT. -1
    /// means unlimited. Torrents beyond the limit stay active without
    /// being announced: a peer that knows about a torrent can still
    /// connect, unless the torrent is paused.
    active_dht_limit: i32;
    /// Maximum number of auto-managed torrents announced to their
    /// trackers. -1 means unlimited. Torrents beyond the limit stay
    /// active without being announced: a peer that knows about a torrent
    /// can still connect, unless the torrent is paused.
    active_tracker_limit: i32;
    /// Maximum number of auto-managed torrents announced over Local
    /// Service Discovery. -1 means unlimited. Torrents beyond the limit
    /// stay active without being announced: a peer that knows about a
    /// torrent can still connect, unless the torrent is paused.
    active_lsd_limit: i32;
    /// Hard cap on the number of active auto-managed torrents; also
    /// applies to slow torrents. -1 means unlimited. See
    /// `activeDownloads` and `activeSeeds` for how the active set is
    /// divided.
    active_limit: i32;
    /// Seconds between updates (and rotations) of the auto-managed
    /// torrent queue.
    auto_manage_interval: i32;
    /// Seconds a torrent may be an active seed before it is considered
    /// to have met its seeding goal and yields queue priority to other
    /// torrents (it may still seed, just without priority). See also
    /// `shareRatioLimit` and `seedTimeRatioLimit`.
    seed_time_limit: i32;
    /// Seconds between automatic scrapes of queued torrents (paused,
    /// auto-managed ones). Their downloader/seed ratios are tracked to
    /// decide which torrents to seed and which to pause.
    auto_scrape_interval: i32;
    /// Minimum seconds between any two automatic scrapes regardless of
    /// torrent, bounding scrape traffic when many auto-managed torrents
    /// are paused.
    auto_scrape_min_interval: i32;
    /// Maximum number of known peers kept per torrent. Known peers are
    /// not necessarily connected, so this should be much larger than the
    /// connection limit. Eviction starts when the list passes 90% of the
    /// limit; at the limit, no more peers are added. 0 means unlimited.
    max_peerlist_size: i32 checked;
    /// Like `maxPeerlistSize`, but for paused torrents, where a large
    /// peer list matters less; a lower value saves memory.
    max_paused_peerlist_size: i32 checked;
    /// Minimum announce interval in seconds accepted from a tracker
    /// response — a sanity check that avoids hammering misconfigured
    /// trackers.
    min_announce_interval: i32;
    /// Seconds a newly started torrent is considered active regardless
    /// of transfer rates, giving it a fair chance to start before the
    /// `dontCountSlowTorrents` logic can classify it as inactive.
    auto_manage_startup: i32;
    /// Number of pieces to send a peer while seeding before rotating
    /// another peer into the unchoke set.
    seeding_piece_quota: i32;
    /// Receive buffer size (`SO_RCVBUF`) set on peer sockets, in bytes;
    /// 0 keeps the OS default. Note that all uTP peers of a listen
    /// socket share one UDP socket buffer, along with DHT and UDP
    /// tracker traffic; a buffer too small for the combined traffic
    /// drops packets.
    recv_socket_buffer_size: i32;
    /// Send buffer size (`SO_SNDBUF`) set on peer sockets, in bytes; 0
    /// keeps the OS default. Note that all uTP peers of a listen socket
    /// share one UDP socket buffer, along with DHT and UDP tracker
    /// traffic; a buffer too small for the combined traffic drops
    /// packets.
    send_socket_buffer_size: i32;
    /// Best-effort cap in bytes on a single peer connection's receive
    /// buffer: growth stops here, but the buffer always accommodates
    /// the current message, so one large legal message (up to about
    /// 1 MiB) can exceed the cap.
    max_peer_recv_buffer_size: i32 checked;
    /// Seconds after a disk write error before an auto-managed torrent
    /// is taken out of upload mode again, to test whether the error
    /// condition has been fixed. Only auto-managed torrents leave upload
    /// mode automatically; clear the `UPLOAD_MODE` torrent flag to leave
    /// it explicitly.
    optimistic_disk_retry: i32;
    /// Maximum suggested piece indexes remembered per peer, bounding
    /// memory use when a peer floods `suggest` messages.
    max_suggest_pieces: i32 checked;
    /// Seconds between Local Service Discovery announces of a torrent.
    local_service_announce_interval: i32 checked;
    /// Seconds between DHT announces of a torrent.
    dht_announce_interval: i32 checked;
    /// Seconds to keep UDP tracker connection tokens around; the
    /// protocol specifies 60. Higher values need fewer packets, but
    /// require the tracker to be configured with a matching expiry.
    udp_tracker_token_expiry: i32;
    /// Number of optimistic unchoke slots. More slots find good peers
    /// faster but use more bandwidth. 0 means automatic: 20% of the
    /// allowed upload slots.
    num_optimistic_unchoke_slots: u16;
    /// Maximum peers accepted from a single peer's PEX message — a cap
    /// on how many concurrent peers any one peer may claim to be
    /// connected to. Entries beyond the limit are ignored.
    max_pex_peers: i32;
    /// Milliseconds between internal ticks — the frequency at which
    /// bandwidth quota is distributed to peers; at most 1000. A low
    /// value (around 100) gives finer-grained rate limiting; a higher
    /// value saves CPU cycles.
    ///
    /// A negative value removes the once-per-second gate on the heavier
    /// per-torrent work (the timer itself still fires every
    /// millisecond). That is intended for tests and fuzzing that need
    /// the session to react instantly; do not use it in production.
    tick_interval: i32 checked;
    /// Target share ratio for share-mode torrents: at 3, the client
    /// tries to upload three times as much as it downloads. Values below
    /// 2 make no sense, and too-high values are so conservative that
    /// nothing may be downloaded at all — a piece can only be uploaded
    /// as many times as there are peers who still need it.
    share_mode_target: i32;
    /// Session-global upload rate limit in bytes per second. 0 means
    /// unlimited. Peers on the local network are not rate limited by
    /// default.
    upload_rate_limit: i32;
    /// Session-global download rate limit in bytes per second. 0 means
    /// unlimited. Peers on the local network are not rate limited by
    /// default.
    download_rate_limit: i32;
    /// Average bytes per second the DHT node is allowed to send. When
    /// incoming requests would exceed the quota with their responses,
    /// requests are dropped until the quota is replenished.
    dht_upload_rate_limit: i32;
    /// Maximum number of unchoked peers in the session when
    /// `chokingAlgorithm` is `FIXED_SLOTS`. -1 means unlimited: all
    /// peers are always unchoked.
    unchoke_slots_limit: i32;
    /// Global limit on the number of open peer connections. A hard
    /// minimum of two connections per torrent still applies, so enough
    /// torrents can exceed a low limit.
    connections_limit: i32;
    /// Number of incoming connections accepted beyond
    /// `connectionsLimit`, as candidates that may replace existing
    /// connections.
    connections_slack: i32;
    /// Target one-way delay for uTP sockets, in milliseconds. Higher
    /// values are more aggressive and queue more at the upload
    /// bottleneck; too-low values drown in measurement noise and send
    /// too slowly.
    utp_target_delay: i32 checked;
    /// Maximum bytes the uTP congestion window may grow within one
    /// round-trip time. Too high, and the controller overreacts to noise
    /// and becomes unstable; too low, and it reacts slowly to congestion
    /// (and backs off slowly).
    utp_gain_factor: i32;
    /// Shortest allowed uTP socket timeout, in milliseconds. The actual
    /// timeout scales with the connection's round-trip time but never
    /// goes below this value. A connection times out when a whole window
    /// is lost, or one packet is lost twice in a row; a shorter timeout
    /// recovers faster, provided the round-trip time is low enough.
    utp_min_timeout: i32;
    /// Number of SYN packets sent — each timing out — before giving up
    /// and closing a uTP socket.
    utp_syn_resends: i32;
    /// Number of FIN packets sent — each timing out — before giving up
    /// and closing a uTP socket.
    utp_fin_resends: i32;
    /// Number of times one packet is resent (lost or timed out) before
    /// the uTP connection is considered broken and closed.
    utp_num_resends: i32;
    /// Timeout in milliseconds for the initial uTP SYN packet; each
    /// consecutive timeout doubles it.
    utp_connect_timeout: i32 checked;
    /// Percentage applied to the uTP congestion window when a packet
    /// loss is experienced. Do not change this unless you know what you
    /// are doing, and never set it above 100.
    utp_loss_multiplier: i32 checked;
    /// Backlog passed to `listen()` on listen sockets: the number of
    /// pending incoming connections queued while not actively accepting.
    /// 5 is sufficient for a normal client; raise it for
    /// high-performance servers expecting many connections. Takes effect
    /// the next time `listenInterfaces` is updated.
    listen_queue_size: i32;
    /// Number of peers to connect immediately when the first tracker
    /// response for a torrent arrives, instead of waiting for the
    /// once-per-second connect scheduler — a boost that accelerates new
    /// torrents. At most 255.
    torrent_connect_boost: u8;
    /// Maximum size in bytes of metadata (.torrent contents) accepted
    /// via the metadata extension, i.e. from magnet links.
    max_metadata_size: i32 checked;
    /// Number of disk I/O threads used for piece-hash verification
    /// during full torrent checking, in addition to the regular
    /// `aioThreads` (hash checks during normal download use the regular
    /// threads). Hasher threads also perform the disk reads; on storage
    /// optimized for sequential access, such as hard drives, 1 (the
    /// default) is best.
    hashing_threads: i32 checked;
    /// Number of 16 KiB blocks kept outstanding while checking torrents.
    /// Higher values give faster rechecks but use more memory.
    checking_mem_usage: i32 checked;
    /// When above 0, pieces are announced to peers this many
    /// milliseconds before they are expected to finish downloading — and
    /// before they are hash-checked — to gain up to 1.5 round trips per
    /// piece. 0 disables predictive announcing.
    predictive_piece_announce: i32;
    /// Number of disk I/O threads, for the asynchronous I/O back-ends
    /// that use a thread pool.
    aio_threads: i32 checked;
    /// How aggressively to back off from retrying failing trackers: the
    /// wait before retry number `fails` is `5 + 5 × trackerBackoff / 100
    /// × fails²` seconds.
    tracker_backoff: i32 checked;
    /// Share ratio (uploaded/downloaded) as a percentage at which a
    /// seeding torrent is considered done and yields queue priority to
    /// other torrents (it may still seed, just without priority). See
    /// also `seedTimeRatioLimit` and `seedTimeLimit`.
    share_ratio_limit: i32;
    /// Seed-time ratio (seconds as seed over seconds as downloader) as a
    /// percentage at which a seeding torrent is considered done and
    /// yields queue priority to other torrents (it may still seed, just
    /// without priority). See also `shareRatioLimit` and
    /// `seedTimeLimit`.
    seed_time_ratio_limit: i32;
    /// Percentage of a torrent's peers to disconnect in each turnover
    /// round (every `peerTurnoverInterval` seconds), to make room for
    /// trying other peers. Only applies while above the
    /// `peerTurnoverCutoff` fill grade.
    peer_turnover: i32 checked;
    /// Fill grade that triggers peer turnover, as a percentage of the
    /// torrent's connection limit: `peerTurnover` disconnects only
    /// happen while more peers than that are connected.
    peer_turnover_cutoff: i32;
    /// Seconds between peer-turnover rounds — the optimistic disconnects
    /// governed by `peerTurnover` and `peerTurnoverCutoff`.
    peer_turnover_interval: i32;
    /// Every n-th connection attempt is granted to a seeding or finished
    /// torrent instead of a downloading one; this is n. Connection
    /// attempts are a limited resource (`connectionSpeed`), and
    /// downloading torrents are prioritized for them by default.
    connect_seed_every_n_download: i32;
    /// Maximum size in bytes of an HTTP response accepted when
    /// announcing to trackers or downloading .torrent files.
    max_http_recv_buffer_size: i32;
    /// Number of times to retry binding a listen port that failed,
    /// incrementing the port by one each try.
    max_retry_port_bind: i32;
    /// Download rate in bytes per second below which a torrent may count
    /// as inactive for queuing: a torrent below both `inactiveDownRate`
    /// and `inactiveUpRate` for `autoManageStartup` seconds is
    /// considered inactive, and another queued torrent may start. Only
    /// applies when `dontCountSlowTorrents` is true.
    inactive_down_rate: i32;
    /// Upload rate in bytes per second below which a torrent may count
    /// as inactive for queuing; see `inactiveDownRate`.
    inactive_up_rate: i32;
    /// Maximum web-seed request range in bytes — the largest possible
    /// single sequential request. Values below the piece size are
    /// ignored. Relate it to the download speed to avoid creating too
    /// many expensive HTTP requests per second, but note that a request
    /// spanning the whole file precludes parallel requests, and that
    /// combining web seeds with rarest-first BitTorrent downloading
    /// splits requests around already-picked pieces.
    urlseed_max_request_bytes: i32;
    /// Seconds to wait before retrying a failed web-seed hostname
    /// lookup.
    web_seed_name_lookup_retry: i32;
    /// Seconds between closing the file that has been open the longest,
    /// nudging the operating system to flush its disk cache; observed to
    /// be necessary on Windows to keep the cache bounded. 0 disables the
    /// feature (the default everywhere except Windows, where it defaults
    /// to 240).
    close_file_interval: i32;
    /// Milliseconds after a uTP congestion-window reduction during which
    /// further packet losses do not reduce the window again.
    utp_cwnd_reduce_timer: i32;
    /// Maximum web-seed connections per torrent.
    max_web_seed_connections: i32;
    /// Seconds a hostname stays in the internal DNS cache before it is
    /// considered out of date and removed; negative values mean zero.
    /// Failed lookups are cached too, for one eighth of this time.
    resolver_cache_timeout: i32;
    /// Not-sent low watermark for socket send buffers — the
    /// Linux-specific `TCP_NOTSENT_LOWAT` TCP socket option.
    send_not_sent_low_watermark: i32;
    /// Starting threshold of the rate-based choker (`chokingAlgorithm:
    /// RATE_BASED`): peers are visited in decreasing upload rate, the
    /// threshold grows proportionally with each visited peer, and the
    /// peers whose upload rate exceeds it are unchoked. A higher start
    /// value yields fewer unchoke slots; a lower one, more.
    rate_choker_initial_threshold: i32;
    /// Requested expiration time of UPnP port mappings, in seconds; 0
    /// requests a permanent lease. Some routers mishandle expiring
    /// mappings without reporting an error — use 0 for those. Otherwise,
    /// do not set it below 5 minutes.
    upnp_lease_duration: i32 checked;
    /// Maximum concurrent HTTP tracker announces; further announces are
    /// queued and issued as outstanding ones complete.
    max_concurrent_http_announces: i32 checked;
    /// Maximum number of peers sent in a reply to a DHT `get_peers`
    /// query.
    dht_max_peers_reply: i32 checked;
    /// Number of concurrent search requests the DHT node sends when
    /// announcing and refreshing the routing table — the `alpha`
    /// parameter of the Kademlia paper.
    dht_search_branching: i32 checked;
    /// Failed contact attempts before a DHT node is removed from the
    /// routing table. Only relevant when no working replacement is
    /// known; a failing node with a known replacement is replaced
    /// immediately.
    dht_max_fail_count: i32;
    /// Maximum torrents tracked by the DHT node — an upper bound that
    /// keeps malicious nodes from forcing unbounded memory use.
    dht_max_torrents: i32;
    /// Maximum immutable/mutable items the DHT node stores.
    dht_max_dht_items: i32 checked;
    /// Maximum peers the DHT node stores per torrent.
    dht_max_peers: i32;
    /// Seconds a remote DHT node stays banned after exceeding the rate
    /// limit. The rate limit (`dhtBlockRatelimit`) is averaged over 10
    /// seconds to allow bursts.
    dht_block_timeout: i32;
    /// Maximum packets per second a remote DHT node may send before it
    /// is banned (see `dhtBlockTimeout`).
    dht_block_ratelimit: i32;
    /// Seconds until a stored immutable/mutable DHT item expires. 0 (the
    /// default) means it never expires.
    dht_item_lifetime: i32;
    /// Seconds between recomputations of the DHT info-hash sample: the
    /// node precomputes a subset of its tracked info-hashes and serves
    /// that instead of recomputing per request. Valid range 0..=21600.
    dht_sample_infohashes_interval: i32 checked;
    /// Maximum elements in the sampled info-hash subset. Very large
    /// values may be clamped by the DHT storage so replies still fit in
    /// UDP packets.
    dht_max_infohashes_sample_count: i32;
    /// Maximum number of pieces allowed in metadata received via magnet
    /// links.
    max_piece_count: i32 checked;
    /// Maximum bencoded tokens parsed from metadata (.torrent contents)
    /// received from peers — a denial-of-service guard that may need
    /// raising for very large torrents.
    metadata_token_limit: i32 checked;
    /// With the mmap disk I/O backend, files smaller than this many 16
    /// KiB blocks are not memory-mapped but accessed with plain
    /// pread/pwrite.
    mmap_file_size_cutoff: i32;
    /// Port reported to trackers and the DHT as the `port` announce
    /// parameter, without affecting the actual listen port or Local
    /// Service Discovery. 0 (the default) reports the real listening
    /// port.
    ///
    /// Only for special setups where the externally reachable port
    /// differs from the local one — for example, a local proxy providing
    /// a reverse tunnel through NAT-PMP, where peers must connect to the
    /// external NAT-PMP port.
    announce_port: u16;
    /// Requested lifetime of NAT-PMP and PCP port mappings, in seconds.
    natpmp_lease_duration: i32;
}
