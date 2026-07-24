// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The typed public settings surface.
//!
//! Every libtorrent setting the API exposes is an explicit, documented
//! GraphQL field on [`Settings`] (read) and [`SettingsInput`] (write):
//!
//! 1. **Scalar settings** (`catalog`) — libtorrent settings exposed
//!    unchanged, one documented field each.
//! 2. **Structured settings** (`structured`) — typed objects and enums
//!    translated to one or more backing libtorrent settings (e.g. `proxy`
//!    owns all ten `proxy_*`/`socks5_*` settings). The backing settings
//!    themselves are absent from the schema.
//! 3. **Blacklisted** settings ([`BLACKLIST`]) — absent from the schema
//!    entirely (daemon-owned or unsafe).
//!
//! The field tables partition the *entire* generated settings table; a
//! unit test enforces that a libtorrent upgrade cannot silently grow the
//! public surface.
//!
//! Write semantics: every [`SettingsInput`] field distinguishes *omitted*
//! (leave unchanged) from *null*. Null disables the nullable groups
//! (`proxy`, `i2p`, `outgoingPortRange`) by
//! resetting their backing settings to libtorrent defaults; on any other
//! field an explicit null is rejected.

mod catalog;
mod structured;
#[cfg(test)]
mod tests;

use async_graphql::{InputObject, MaybeUndefined, SimpleObject};
use rbtorrent::settings::all_settings;
use rbtorrent::{SettingKey, SettingKind, SettingsPack};

use catalog::{ScalarSettings, ScalarSettingsInput};
pub use structured::{
    ChokingAlgorithm, DiskIoCache, EncryptionMethods, EncryptionPolicy, EncryptionSettings,
    HostPort, I2pSettings, I2pTunnel, IoReadMode, IoWriteMode, ListenInterface, MixedModeAlgorithm,
    MmapWriteMode, PeerTransports, PortRange, ProxyProtocol, ProxySettings,
    QBITTORRENT_COMPAT_VERSION, SeedChokingAlgorithm, SuggestMode, TransportDirections, UserAgent,
};

/// Why a settings write (or read) was refused.
#[derive(Debug)]
pub struct SettingsError(String);

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SettingsError {}

impl From<SettingsError> for crate::engine::EngineError {
    fn from(err: SettingsError) -> Self {
        crate::engine::EngineError::Invalid(err.0)
    }
}

pub(crate) fn invalid(msg: impl Into<String>) -> SettingsError {
    SettingsError(msg.into())
}

/// A missing effective value means the daemon and libtorrent disagree
/// about the settings table; surface it rather than fabricating a value.
fn missing(name: &str) -> SettingsError {
    invalid(format!("{name}: effective value unavailable"))
}

/// Settings that are absent from the schema: neither readable nor
/// writable.
///
/// - `alert_mask` is pinned by the engine to exactly the categories its
///   events are generated from (`ALERT_MASK`); never more, never less.
/// - `alert_queue_size` is pinned by the engine to effectively unlimited
///   (`ALERT_QUEUE_SIZE`); the no-drop invariant depends on it — a
///   dropped alert aborts the daemon.
/// - `disable_hash_checks` is simulation/testing-only and compromises
///   downloaded-data integrity.
/// - `enable_set_file_valid_data` can expose previously deleted disk
///   contents on Windows.
/// - The remaining seven are inert in this build: `max_rejects`,
///   `tracker_maximum_response_length`, `urlseed_pipeline_size` and
///   `dht_max_torrent_search_reply` have no reader anywhere in the
///   vendored libtorrent, and the `webtorrent_*` settings are read only
///   inside `#if TORRENT_USE_RTC` bodies, compiled out (webtorrent=OFF).
///   Exposing them would suggest behavior that does not exist.
pub const BLACKLIST: &[&str] = &[
    "alert_mask",
    "alert_queue_size",
    "disable_hash_checks",
    "enable_set_file_valid_data",
    "max_rejects",
    "tracker_maximum_response_length",
    "urlseed_pipeline_size",
    "dht_max_torrent_search_reply",
    "webtorrent_stun_server",
    "min_websocket_announce_interval",
    "webtorrent_connection_timeout",
];

/// `(public field, backing libtorrent settings)` for every field of
/// [`Settings`], scalars first. Together with [`BLACKLIST`] this
/// partitions the generated settings table (enforced by a test).
pub fn backing() -> impl Iterator<Item = (&'static str, &'static [&'static str])> {
    catalog::SCALAR_BACKING
        .iter()
        .copied()
        .chain(structured::STRUCTURED_BACKING.iter().copied())
}

/// The daemon's effective configuration.
///
/// Every public setting is one explicit field; use GraphQL field
/// selection to read any subset. Change settings with `applySettings`.
#[derive(Debug, Clone, PartialEq, SimpleObject)]
pub struct Settings {
    #[graphql(flatten)]
    pub scalars: ScalarSettings,
    /// The user agent identity presented to trackers and in web-seed
    /// requests. The peer-ID prefix is controlled separately by
    /// `peerFingerprint`.
    ///
    /// `UNRECOGNIZED` reports an effective value set outside this API
    /// (e.g. state persisted by a different daemon version); it cannot
    /// be written back.
    pub user_agent: UserAgent,
    /// Proxy for outgoing connections. `null` = no proxy configured.
    pub proxy: Option<ProxySettings>,
    /// I2P SAM bridge configuration. `null` = I2P disabled.
    // Explicit name: the default rename turns "i2p" into "i2P".
    #[graphql(name = "i2p")]
    pub i2p: Option<I2pSettings>,
    /// BitTorrent protocol encryption policy.
    pub encryption: EncryptionSettings,
    /// Which peer transport protocols are enabled, per direction.
    pub peer_transports: PeerTransports,
    /// Source-port range bound for outgoing peer connections.
    /// `null` = OS-selected ephemeral ports.
    pub outgoing_port_range: Option<PortRange>,
    /// OS cache behavior of disk reads and writes.
    pub disk_io_cache: DiskIoCache,
    /// How to flush pieces to disk with the mmap disk I/O backend.
    pub mmap_write_mode: MmapWriteMode,
    /// Whether to send `suggest` messages to peers.
    pub suggest_mode: SuggestMode,
    /// Which algorithm chooses peers to unchoke.
    pub choking_algorithm: ChokingAlgorithm,
    /// Which algorithm chooses peers to unchoke while seeding.
    pub seed_choking_algorithm: SeedChokingAlgorithm,
    /// How rate limits are divided between TCP and uTP peers when both
    /// transports are in use.
    pub mixed_mode_algorithm: MixedModeAlgorithm,
    /// IP addresses or network device names outgoing peer connections
    /// bind to, round-robin (no hostnames; IPv6 goes unbracketed).
    /// Side effect: while non-empty, incoming connections and packets
    /// arriving on a local interface or IP *not* in this list are
    /// rejected. Empty = let the OS route.
    pub outgoing_interfaces: Vec<String>,
    /// Endpoints for incoming uTP and TCP peer connections, also used
    /// for *outgoing* uTP, UDP tracker, and DHT traffic. Applying a
    /// change closes and reopens the listen sockets. An empty list
    /// disables networking: no DHT, no incoming connections, no
    /// outgoing uTP or tracker traffic (outgoing TCP still works,
    /// subject to `outgoingInterfaces`).
    pub listen_interfaces: Vec<ListenInterface>,
    /// DHT bootstrap nodes.
    pub dht_bootstrap_nodes: Vec<HostPort>,
}

/// A settings delta for `applySettings`.
///
/// Omitted fields are left unchanged. `null` disables the nullable
/// groups (`proxy`, `i2p`, `outgoingPortRange`) by resetting their
/// backing settings to built-in defaults; on any other field an
/// explicit `null` is an error.
#[derive(InputObject)]
pub struct SettingsInput {
    #[graphql(flatten)]
    pub scalars: ScalarSettingsInput,
    /// The user agent identity to present. `UNRECOGNIZED` is read-only
    /// and rejected here.
    pub user_agent: MaybeUndefined<UserAgent>,
    /// Proxy for outgoing connections; `null` removes the proxy and
    /// resets its backing settings.
    pub proxy: MaybeUndefined<ProxySettings>,
    /// I2P SAM bridge configuration; `null` disables I2P.
    // Explicit name: the default rename turns "i2p" into "i2P".
    #[graphql(name = "i2p")]
    pub i2p: MaybeUndefined<I2pSettings>,
    /// BitTorrent protocol encryption policy.
    pub encryption: MaybeUndefined<EncryptionSettings>,
    /// Enabled peer transport protocols, per direction.
    pub peer_transports: MaybeUndefined<PeerTransports>,
    /// Source-port range for outgoing peer connections; `null` restores
    /// OS-selected ephemeral ports.
    pub outgoing_port_range: MaybeUndefined<PortRange>,
    /// OS cache behavior of disk reads and writes.
    pub disk_io_cache: MaybeUndefined<DiskIoCache>,
    /// How to flush pieces to disk with the mmap disk I/O backend.
    pub mmap_write_mode: MaybeUndefined<MmapWriteMode>,
    /// Whether to send `suggest` messages to peers.
    pub suggest_mode: MaybeUndefined<SuggestMode>,
    /// Which algorithm chooses peers to unchoke.
    pub choking_algorithm: MaybeUndefined<ChokingAlgorithm>,
    /// Which algorithm chooses peers to unchoke while seeding.
    pub seed_choking_algorithm: MaybeUndefined<SeedChokingAlgorithm>,
    /// How rate limits are divided between TCP and uTP peers.
    pub mixed_mode_algorithm: MaybeUndefined<MixedModeAlgorithm>,
    /// Interfaces/adapters for outgoing connections. An empty list lets
    /// the OS route.
    pub outgoing_interfaces: MaybeUndefined<Vec<String>>,
    /// Listen endpoints for incoming connections.
    pub listen_interfaces: MaybeUndefined<Vec<ListenInterface>>,
    /// DHT bootstrap nodes. An empty list means no bootstrap nodes.
    pub dht_bootstrap_nodes: MaybeUndefined<Vec<HostPort>>,
}

/// Builds the public settings view from the full effective pack.
pub fn read(effective: &SettingsPack) -> Result<Settings, SettingsError> {
    Ok(Settings {
        scalars: catalog::read_scalars(effective)?,
        user_agent: structured::read_user_agent(effective)?,
        proxy: structured::read_proxy(effective)?,
        i2p: structured::read_i2p(effective)?,
        encryption: structured::read_encryption(effective)?,
        peer_transports: structured::read_peer_transports(effective)?,
        outgoing_port_range: structured::read_outgoing_port_range(effective)?,
        disk_io_cache: structured::read_disk_io_cache(effective)?,
        mmap_write_mode: structured::read_mmap_write_mode(effective)?,
        suggest_mode: structured::read_suggest_mode(effective)?,
        choking_algorithm: structured::read_choking_algorithm(effective)?,
        seed_choking_algorithm: structured::read_seed_choking_algorithm(effective)?,
        mixed_mode_algorithm: structured::read_mixed_mode_algorithm(effective)?,
        outgoing_interfaces: structured::read_outgoing_interfaces(effective)?,
        listen_interfaces: structured::read_listen_interfaces(effective)?,
        dht_bootstrap_nodes: structured::read_dht_bootstrap_nodes(effective)?,
    })
}

/// Validates `input` and stages every change into `delta`. On error the
/// caller must discard `delta`: applying only a validated prefix would
/// break the all-or-nothing contract of `applySettings`.
pub fn write(delta: &mut SettingsPack, input: SettingsInput) -> Result<(), SettingsError> {
    let SettingsInput {
        scalars,
        user_agent,
        proxy,
        i2p,
        encryption,
        peer_transports,
        outgoing_port_range,
        disk_io_cache,
        mmap_write_mode,
        suggest_mode,
        choking_algorithm,
        seed_choking_algorithm,
        mixed_mode_algorithm,
        outgoing_interfaces,
        listen_interfaces,
        dht_bootstrap_nodes,
    } = input;
    catalog::write_scalars(delta, scalars)?;
    structured::write_user_agent(delta, user_agent)?;
    structured::write_proxy(delta, proxy)?;
    structured::write_i2p(delta, i2p)?;
    structured::write_encryption(delta, encryption)?;
    structured::write_peer_transports(delta, peer_transports)?;
    structured::write_outgoing_port_range(delta, outgoing_port_range)?;
    structured::write_disk_io_cache(delta, disk_io_cache)?;
    structured::write_mmap_write_mode(delta, mmap_write_mode)?;
    structured::write_suggest_mode(delta, suggest_mode)?;
    structured::write_choking_algorithm(delta, choking_algorithm)?;
    structured::write_seed_choking_algorithm(delta, seed_choking_algorithm)?;
    structured::write_mixed_mode_algorithm(delta, mixed_mode_algorithm)?;
    structured::write_outgoing_interfaces(delta, outgoing_interfaces)?;
    structured::write_listen_interfaces(delta, listen_interfaces)?;
    structured::write_dht_bootstrap_nodes(delta, dht_bootstrap_nodes)?;
    Ok(())
}

// ---- shared helpers ----------------------------------------------------

/// `(key, kind)` for every libtorrent setting, by name. Keys come from the
/// generated table (enum order), not `setting_by_name`, sidestepping the
/// upstream announce_to_all_tiers/trackers name-table swap.
fn lt_setting(name: &str) -> (SettingKey, SettingKind) {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_NAME: OnceLock<HashMap<&'static str, (SettingKey, SettingKind)>> = OnceLock::new();
    let map = BY_NAME.get_or_init(|| all_settings().map(|(k, n, kind)| (n, (k, kind))).collect());
    *map.get(name)
        .unwrap_or_else(|| panic!("{name} is not a libtorrent setting"))
}

/// The GraphQL (camelCase) name of a snake_case field, for error
/// messages that clients match against the schema.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Unwraps an input field that does not accept `null`: omitted means
/// "leave unchanged", a value means "set", and explicit null is an
/// error.
fn defined<T>(value: MaybeUndefined<T>, field: &str) -> Result<Option<T>, SettingsError> {
    match value {
        MaybeUndefined::Undefined => Ok(None),
        MaybeUndefined::Null => Err(invalid(format!(
            "{}: null is not a valid value; omit the field to leave it unchanged",
            camel(field)
        ))),
        MaybeUndefined::Value(v) => Ok(Some(v)),
    }
}

/// A change to one of the nullable setting groups.
enum Nullable<T> {
    /// Field omitted: leave the group unchanged.
    Unchanged,
    /// Explicit null: disable the group (reset backing settings).
    Disable,
    /// Set the group to this value.
    Set(T),
}

fn nullable<T>(value: MaybeUndefined<T>) -> Nullable<T> {
    match value {
        MaybeUndefined::Undefined => Nullable::Unchanged,
        MaybeUndefined::Null => Nullable::Disable,
        MaybeUndefined::Value(v) => Nullable::Set(v),
    }
}

// ---- scalar pack access (used by the catalog macro) ---------------------

fn get_str(pack: &SettingsPack, name: &'static str) -> Result<String, SettingsError> {
    let (key, _) = lt_setting(name);
    pack.get_str(key).ok_or_else(|| missing(name))
}

fn get_int(pack: &SettingsPack, name: &'static str) -> Result<i32, SettingsError> {
    let (key, _) = lt_setting(name);
    pack.get_int(key).ok_or_else(|| missing(name))
}

fn get_bool(pack: &SettingsPack, name: &'static str) -> Result<bool, SettingsError> {
    let (key, _) = lt_setting(name);
    pack.get_bool(key).ok_or_else(|| missing(name))
}

/// Maps a value-domain rejection from rbtorrent onto this module's
/// camelCase error convention. The rbtorrent message is value-centric
/// (no setting name), so the field path prefixes cleanly.
fn rb_err(field: &str, e: &rbtorrent::SettingsError) -> SettingsError {
    invalid(format!("{}: {}", camel(field), e.message()))
}

/// Lenient read-side narrowing for `u16`-typed fields: values written
/// through rbtorrent's typed setters are always in range; anything else
/// (a foreign state blob) saturates rather than failing the whole read.
fn sat_u16(value: i32) -> u16 {
    u16::try_from(value.clamp(0, i32::from(u16::MAX))).expect("clamped to u16 range")
}

/// As [`sat_u16`], for `u8`-typed fields.
fn sat_u8(value: i32) -> u8 {
    u8::try_from(value.clamp(0, i32::from(u8::MAX))).expect("clamped to u8 range")
}

/// The libtorrent default for an integer setting, used as the
/// guaranteed-in-domain sample value for constrained fields.
#[cfg(test)]
fn default_int(name: &'static str) -> i32 {
    let (key, _) = lt_setting(name);
    rbtorrent::SettingsPack::defaults()
        .get_int(key)
        .expect("integer setting has a default")
}
